use async_trait::async_trait;
use serde_json::{json, Value};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolCategory, ToolContext};
use super::common;
use super::writeback_support::{proposal_ok, read_note_body, ProposalPayload};

pub struct DeleteNoteTool;

#[async_trait]
impl Tool for DeleteNoteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "delete_note".into(),
            description: "Prepare a destructive proposal to move one note to the system Trash (Finder Trash on macOS / Recycle Bin on Windows / freedesktop Trash on Linux). This tool never moves the file itself; the user must explicitly confirm. Phase 4 Stage 3 changed this from permanent delete to Trash semantics — recovery is one user-side click after acceptance.".into(),
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
        ToolCategory::Destructive
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

        proposal_ok(&ProposalPayload {
            proposal_kind: "delete_note".into(),
            target_rel_path: rel_path,
            original_content: body,
            proposed_content: String::new(),
            summary: "移至系统回收站（需二次确认 · 可恢复）".into(),
            metadata: json!({
                "destructive": true,
                "action": "delete",
                "recovery": "system_trash"
            }),
        })
    }
}

