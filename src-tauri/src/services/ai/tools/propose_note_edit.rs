use async_trait::async_trait;
use serde_json::{json, Value};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolCategory, ToolContext};
use super::common;
use super::writeback_support::{
    complete_via_tool, note_edit_too_large, proposal_ok, read_note_body, ProposalPayload,
};

pub struct ProposeNoteEditTool;

const SYSTEM_PROMPT: &str = "You are revising a markdown note in a personal knowledge base. Apply the user's instruction to the note and return ONLY the full revised markdown document. Preserve existing frontmatter and structure unless the instruction explicitly changes them. Do not add explanations, headings about the edit, or code fences.";

#[async_trait]
impl Tool for ProposeNoteEditTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "propose_note_edit".into(),
            description: "Draft a full-note edit proposal for one markdown note based on a user instruction. Returns a proposal payload; it does not write the file.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "instruction": { "type": "string" }
                },
                "required": ["rel_path", "instruction"]
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
        let instruction = match common::parse_str_field(&args, "instruction") {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => return common::err("invalid args: 'instruction' must be non-empty"),
            Err(r) => return r,
        };

        let body = match read_note_body(ctx, &rel_path) {
            Ok(body) => body,
            Err(err) => return err,
        };
        if note_edit_too_large(&body) {
            return common::err(
                "note is too large for propose_note_edit; ask the user to narrow the section or split the note first",
            );
        }

        let user_prompt = format!(
            "User instruction:\n{instruction}\n\n--- NOTE BEGIN ---\n{body}\n--- NOTE END ---"
        );
        let reply =
            match complete_via_tool(ctx, SYSTEM_PROMPT, user_prompt, Some(0.2), Some(2400)).await
            {
                Ok(reply) => reply,
                Err(err) => return err,
            };
        let proposed = strip_markdown_fence(&reply);
        if proposed.trim().is_empty() {
            return common::err("provider returned an empty revised note");
        }

        proposal_ok(&ProposalPayload {
            proposal_kind: "note_edit".into(),
            target_rel_path: rel_path,
            original_content: body,
            proposed_content: proposed,
            summary: format!("按指令修改: {}", compact_instruction(&instruction)),
            metadata: json!({
                "instruction": instruction
            }),
        })
    }
}

fn strip_markdown_fence(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") || !trimmed.ends_with("```") {
        return trimmed.to_string();
    }
    trimmed
        .trim_start_matches("```markdown")
        .trim_start_matches("```md")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

fn compact_instruction(text: &str) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 100 {
        compact
    } else {
        compact.chars().take(100).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::services::ai::provider::{ChatScriptItem, MockProvider};
    use crate::services::config::AiToolPermissions;

    fn ctx(vault: std::path::PathBuf, reply: &str) -> ToolContext {
        let provider = Arc::new(MockProvider::new());
        provider.set_chat_script(vec![vec![ChatScriptItem::FinishText {
            content: reply.into(),
        }]]);
        ToolContext {
            vault_root: Some(vault),
            index: None,
            embeddings: None,
            embed_model: None,
            provider: Some(provider),
            chat_model: Some("mock-chat".into()),
            tool_permissions: AiToolPermissions::default(),
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    #[tokio::test]
    async fn returns_note_edit_proposal() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("foo.md"), "# Title\n\nold body").unwrap();
        let out = ProposeNoteEditTool
            .execute(
                json!({"rel_path": "foo.md", "instruction": "make it shorter"}),
                &ctx(tmp.path().to_path_buf(), "# Title\n\nnew body"),
            )
            .await;
        assert!(!out.is_error, "{}", out.content);
        let payload: ProposalPayload = serde_json::from_str(&out.content).unwrap();
        assert_eq!(payload.proposal_kind, "note_edit");
        assert!(payload.proposed_content.contains("new body"));
    }
}
