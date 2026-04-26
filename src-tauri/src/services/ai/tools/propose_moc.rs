use async_trait::async_trait;
use chrono::Local;
use serde_json::{json, Value};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolCategory, ToolContext};
use super::common;
use super::writeback_support::{
    build_flat_entries_markdown, complete_via_tool, inject_moc_entries, load_moc_template,
    proposal_ok, read_note_body, rewrite_frontmatter_scalar, sanitize_moc_reply, slugify_title,
    stem_from_path, write_target_exists, NoteRef, ProposalPayload,
};

pub struct ProposeMocTool;

const SYSTEM_PROMPT: &str = "You are a MOC (Map Of Content) curator for a personal knowledge base. Given a set of markdown note titles under a single tag, organise them into 2-6 themed sections and output a markdown block.\n\nStrict output rules:\n- Output ONLY the body of the '核心笔记' section.\n- Each theme is a second-level heading ('## <theme>') followed by '- [[title]]' bullets.\n- Every title MUST come verbatim from the provided list.\n- Every title from the provided list MUST appear exactly once across all sections.\n- Output ONLY the markdown block, no explanation and no code fences.";

#[async_trait]
impl Tool for ProposeMocTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "propose_moc".into(),
            description: "Draft a MOC note for a tag by grouping tagged notes into themed sections. Returns a proposal payload; it does not write files.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tag": { "type": "string" },
                    "title": { "type": "string" }
                },
                "required": ["tag"]
            }),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Writeback
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let tag = match common::parse_str_field(&args, "tag") {
            Ok(s) if !s.trim().is_empty() => s.trim().to_string(),
            Ok(_) => return common::err("invalid args: 'tag' must be non-empty"),
            Err(r) => return r,
        };
        let requested_title = args
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .unwrap_or(tag.as_str())
            .to_string();
        let slug = slugify_title(&requested_title);
        if slug.is_empty() {
            return common::err("title cannot be converted into a valid MOC filename");
        }
        let target_rel_path = format!("2-moc/{slug}.md");

        let notes = match load_notes_for_tag(ctx, &tag) {
            Ok(notes) => notes,
            Err(err) => return err,
        };
        if notes.is_empty() {
            return common::err(format!("no notes found for tag '{tag}'"));
        }

        let template = match load_moc_template(ctx) {
            Ok(template) => render_moc_template(&template, &requested_title),
            Err(err) => return err,
        };
        let flat_entries = build_flat_entries_markdown(&notes);
        let (flat_body, flat_strategy) = inject_moc_entries(&template, &flat_entries);
        let flat_body = rewrite_frontmatter_scalar(&flat_body, "moc_source_tag", &tag);

        let allowed_titles: Vec<String> = notes
            .iter()
            .map(|note| {
                note.title
                    .clone()
                    .unwrap_or_else(|| stem_from_path(&note.path))
            })
            .collect();
        let user_prompt = [
            format!("Tag: #{tag}"),
            format!("MOC title: {requested_title}"),
            String::new(),
            "Titles to group (one per line):".into(),
            allowed_titles
                .iter()
                .map(|title| format!("- {title}"))
                .collect::<Vec<_>>()
                .join("\n"),
            String::new(),
            "Output the themed section markdown now.".into(),
        ]
        .join("\n");
        let reply =
            match complete_via_tool(ctx, SYSTEM_PROMPT, user_prompt, Some(0.4), Some(700)).await {
                Ok(reply) => reply,
                Err(err) => return err,
            };
        let grouped_entries = sanitize_moc_reply(&reply, &allowed_titles);
        if grouped_entries.trim().is_empty() {
            return common::err("provider returned an empty MOC grouping");
        }
        let (grouped_body, grouped_strategy) = inject_moc_entries(&template, &grouped_entries);
        let grouped_body = rewrite_frontmatter_scalar(&grouped_body, "moc_source_tag", &tag);

        let target_exists = match write_target_exists(ctx, &target_rel_path) {
            Ok(exists) => exists,
            Err(err) => return err,
        };
        let original_content = if target_exists {
            match read_note_body(ctx, &target_rel_path) {
                Ok(body) => body,
                Err(err) => return err,
            }
        } else {
            flat_body
        };

        proposal_ok(&ProposalPayload {
            proposal_kind: "moc".into(),
            target_rel_path,
            original_content,
            proposed_content: grouped_body,
            summary: format!("为 #{tag} 起草 MOC（{} 篇笔记）", allowed_titles.len()),
            metadata: json!({
                "tag": tag,
                "title": requested_title,
                "target_exists": target_exists,
                "note_count": allowed_titles.len(),
                "flat_injection_strategy": flat_strategy,
                "grouped_injection_strategy": grouped_strategy,
                "allowed_titles": allowed_titles
            }),
        })
    }
}

fn load_notes_for_tag(ctx: &ToolContext, tag: &str) -> Result<Vec<NoteRef>, ToolResult> {
    let index = match ctx.index.as_ref() {
        Some(index) => index,
        None => return Err(common::err("no index available (vault not opened?)")),
    };
    let conn = index.lock().unwrap();
    let mut stmt = conn
        .prepare_cached(
            "SELECT n.path, n.title, n.updated
             FROM tags t
             JOIN notes n ON n.path = t.note_path
             WHERE t.tag = ?1
             ORDER BY COALESCE(n.updated, '') DESC, n.path ASC
             LIMIT 120",
        )
        .map_err(|e| common::err(format!("database error: {e}")))?;
    let rows = stmt
        .query_map([tag], |row| {
            Ok(NoteRef {
                path: row.get(0)?,
                title: row.get(1)?,
            })
        })
        .map_err(|e| common::err(format!("database error: {e}")))?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| common::err(format!("database error: {e}")))?);
    }
    Ok(out)
}

fn render_moc_template(template: &str, title: &str) -> String {
    let now = Local::now().format("%Y-%m-%d %H:%M").to_string();
    template
        .replace("{{title}}", title)
        .replace("{{now}}", &now)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rusqlite::Connection;
    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::db::{apply_pragmas, apply_schema};
    use crate::services::ai::provider::{ChatScriptItem, MockProvider};
    use crate::services::config::AiToolPermissions;

    fn seeded_conn() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        let _ = apply_pragmas(&conn);
        apply_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO notes (path, title, updated) VALUES ('a.md', 'Alpha', '2026-04-20')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO notes (path, title, updated) VALUES ('b.md', 'Beta', '2026-04-19')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO tags (note_path, tag) VALUES ('a.md', 'topic')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (note_path, tag) VALUES ('b.md', 'topic')", [])
            .unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn ctx(vault: std::path::PathBuf) -> ToolContext {
        let provider = Arc::new(MockProvider::new());
        provider.set_chat_script(vec![vec![ChatScriptItem::FinishText {
            content: "## 核心分组\n\n- [[Alpha]]\n- [[Beta]]".into(),
        }]]);
        ToolContext {
            vault_root: Some(vault),
            index: Some(seeded_conn()),
            embeddings: None,
            embed_model: None,
            provider: Some(provider),
            chat_model: Some("mock-chat".into()),
            tool_permissions: AiToolPermissions::default(),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    #[tokio::test]
    async fn builds_moc_proposal() {
        let tmp = tempdir().unwrap();
        let templates_dir = tmp.path().join("templates");
        std::fs::create_dir_all(&templates_dir).unwrap();
        std::fs::write(
            templates_dir.join("moc.md"),
            "---\ntitle: \"{{title}}\"\n---\n\n## 核心笔记\n\n<!-- moc:entries-insertion-point -->",
        )
        .unwrap();
        let out = ProposeMocTool
            .execute(json!({"tag": "topic", "title": "Topic"}), &ctx(tmp.path().to_path_buf()))
            .await;
        assert!(!out.is_error, "{}", out.content);
        let payload: ProposalPayload = serde_json::from_str(&out.content).unwrap();
        assert_eq!(payload.proposal_kind, "moc");
        assert!(payload.target_rel_path.ends_with("topic.md"));
        assert!(payload.proposed_content.contains("[[Alpha]]"));
        assert!(payload.proposed_content.contains("moc_source_tag: topic"));
    }
}
