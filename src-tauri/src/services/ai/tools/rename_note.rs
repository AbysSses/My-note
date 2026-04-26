use async_trait::async_trait;
use serde_json::{json, Value};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolCategory, ToolContext};
use super::common;
use super::writeback_support::{
    proposal_ok, read_note_body, validate_rel_path, write_target_exists, ProposalPayload,
};

pub struct RenameNoteTool;

#[async_trait]
impl Tool for RenameNoteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "rename_note".into(),
            description: "Prepare a destructive proposal to rename or move one note. This tool never renames the file itself; the user must explicitly confirm.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "new_rel_path": { "type": "string" }
                },
                "required": ["rel_path", "new_rel_path"]
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
        let new_rel_path = match common::parse_str_field(&args, "new_rel_path") {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => return common::err("invalid args: 'new_rel_path' must be non-empty"),
            Err(r) => return r,
        };
        if rel_path == new_rel_path {
            return common::err("new_rel_path must differ from rel_path");
        }
        if let Err(err) = validate_rel_path(&new_rel_path) {
            return err;
        }
        match write_target_exists(ctx, &new_rel_path) {
            Ok(true) => return common::err(format!("destination already exists: {new_rel_path}")),
            Ok(false) => {}
            Err(err) => return err,
        }
        let body = match read_note_body(ctx, &rel_path) {
            Ok(body) => body,
            Err(err) => return err,
        };

        proposal_ok(&ProposalPayload {
            proposal_kind: "rename_note".into(),
            target_rel_path: new_rel_path.clone(),
            original_content: body.clone(),
            proposed_content: body,
            summary: format!("重命名: {rel_path} → {new_rel_path}"),
            metadata: json!({
                "destructive": true,
                "action": "rename",
                "source_rel_path": rel_path,
                "new_rel_path": new_rel_path
            }),
        })
    }
}
