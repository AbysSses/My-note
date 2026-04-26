use std::collections::BTreeSet;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolCategory, ToolContext};
use super::common;
use super::writeback_support::{
    complete_via_tool, merge_tags_into_frontmatter, parse_existing_tags, parse_suggested_tags,
    proposal_ok, read_note_body, split_leading_frontmatter, ProposalPayload,
};

pub struct ProposeTagUpdateTool;

const SYSTEM_PROMPT: &str = "You are a tag curator for a personal knowledge base. Read the user's markdown note and pick 3 to 8 topical tags that best index it. Ground your picks in the existing tags list the user provides and prefer reusing those tags verbatim. You may add at most 2 brand-new tags when no existing tag captures a major theme. Every tag must be lowercase, words joined by '-' when needed, and must not include '#'. Output ONLY the comma-separated tag list.";

#[async_trait]
impl Tool for ProposeTagUpdateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "propose_tag_update".into(),
            description: "Draft a frontmatter.tags update proposal for a note based on its content and the vault's existing tag taxonomy.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" }
                },
                "required": ["rel_path"]
            }),
        }
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Writeback
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let rel_path = match common::parse_str_field(&args, "rel_path") {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => return common::err("invalid args: 'rel_path' must be non-empty"),
            Err(r) => return r,
        };

        let body = match read_note_body(ctx, &rel_path) {
            Ok(body) => body,
            Err(err) => return err,
        };
        let stripped = split_leading_frontmatter(&body)
            .map(|(_, rest)| rest)
            .unwrap_or(body.as_str())
            .trim();
        if stripped.is_empty() {
            return common::err("note body is empty after frontmatter stripping");
        }
        let existing_tags = parse_existing_tags(&body);
        let vault_tags = match load_vault_tags(ctx) {
            Ok(tags) => tags,
            Err(err) => return err,
        };

        let existing_line = if existing_tags.is_empty() {
            "(none)".to_string()
        } else {
            existing_tags.join(", ")
        };
        let vault_line = if vault_tags.is_empty() {
            "(none)".to_string()
        } else {
            vault_tags.join(", ")
        };
        let user_prompt = [
            format!("Existing tags on this note: {existing_line}"),
            format!("Most-used tags in the vault (for reuse preference): {vault_line}"),
            String::new(),
            "Suggest 3–8 tags for the note below. Prefer reusing tags from the two lists above; at most 2 genuinely new tags allowed.".into(),
            String::new(),
            "--- NOTE BEGIN ---".into(),
            stripped.to_string(),
            "--- NOTE END ---".into(),
        ]
        .join("\n");

        let reply =
            match complete_via_tool(ctx, SYSTEM_PROMPT, user_prompt, Some(0.2), Some(180)).await {
                Ok(reply) => reply,
                Err(err) => return err,
            };
        let suggested_tags = parse_suggested_tags(&reply);
        if suggested_tags.is_empty() {
            return common::err("provider returned no parseable tag suggestions");
        }

        let final_tags = merge_tag_lists(&existing_tags, &suggested_tags);
        let proposed = merge_tags_into_frontmatter(&body, &final_tags);

        proposal_ok(&ProposalPayload {
            proposal_kind: "tag_update".into(),
            target_rel_path: rel_path,
            original_content: body,
            proposed_content: proposed,
            summary: format!("建议标签: {}", final_tags.join(", ")),
            metadata: json!({
                "existing_tags": existing_tags,
                "suggested_tags": suggested_tags,
                "final_tags": final_tags,
                "vault_tags": vault_tags
            }),
        })
    }
}

fn load_vault_tags(ctx: &ToolContext) -> Result<Vec<String>, ToolResult> {
    let index = match ctx.index.as_ref() {
        Some(index) => index,
        None => return Ok(Vec::new()),
    };
    let conn = index.lock().unwrap();
    let mut stmt = conn
        .prepare_cached(
            "SELECT tag, COUNT(*) AS c
             FROM tags
             GROUP BY tag
             ORDER BY c DESC, tag ASC
             LIMIT 40",
        )
        .map_err(|e| common::err(format!("database error: {e}")))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| common::err(format!("database error: {e}")))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| common::err(format!("database error: {e}")))?);
    }
    Ok(out)
}

fn merge_tag_lists(existing: &[String], suggested: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for tag in existing.iter().chain(suggested.iter()) {
        if seen.insert(tag.clone()) {
            out.push(tag.clone());
        }
    }
    out
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
        conn.execute("INSERT INTO notes (path) VALUES ('foo.md')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO tags (note_path, tag) VALUES ('foo.md', 'existing')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO notes (path) VALUES ('bar.md')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO tags (note_path, tag) VALUES ('bar.md', 'vault-tag')",
            [],
        )
        .unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn ctx(vault: std::path::PathBuf, index: Arc<Mutex<Connection>>) -> ToolContext {
        let provider = Arc::new(MockProvider::new());
        provider.set_chat_script(vec![vec![ChatScriptItem::FinishText {
            content: "existing, vault-tag, fresh".into(),
        }]]);
        ToolContext {
            vault_root: Some(vault),
            index: Some(index),
            embeddings: None,
            embed_model: None,
            provider: Some(provider),
            chat_model: Some("mock-chat".into()),
            tool_permissions: AiToolPermissions::default(),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    #[tokio::test]
    async fn builds_tag_update_proposal() {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("foo.md"),
            "---\ntags: [existing]\n---\n\n这里是正文",
        )
        .unwrap();
        let out = ProposeTagUpdateTool
            .execute(json!({"rel_path": "foo.md"}), &ctx(tmp.path().to_path_buf(), seeded_conn()))
            .await;
        assert!(!out.is_error, "{}", out.content);
        let payload: ProposalPayload = serde_json::from_str(&out.content).unwrap();
        assert_eq!(payload.proposal_kind, "tag_update");
        assert!(payload.proposed_content.contains("tags: [existing, vault-tag, fresh]"));
    }

    #[test]
    fn merge_tag_lists_preserves_order() {
        let out = merge_tag_lists(
            &["a".into(), "b".into()],
            &["b".into(), "c".into(), "a".into()],
        );
        assert_eq!(out, vec!["a", "b", "c"]);
    }
}
