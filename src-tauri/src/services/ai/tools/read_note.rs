//! `read_note` — return the full markdown body of a vault note.
//!
//! Uses the same `resolve_in_vault` path-traversal check as
//! `file_read`, so `../etc/passwd` or absolute paths are rejected
//! with a structured error rather than leaking outside the vault.
//!
//! **No size cap in D5.2.** A 5 MB note will happily round-trip into
//! the model context. D5.4 lands a max-bytes guard + optional range
//! reads; tracked in plan_P3.md §4.4.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use crate::commands::vault::resolve_in_vault;

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolContext};
use super::common;

pub struct ReadNoteTool;

#[derive(Serialize)]
struct ReadNotePayload {
    rel_path: String,
    content: String,
}

#[async_trait]
impl Tool for ReadNoteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_note".into(),
            description:
                "Return full markdown content of a vault note, given its vault-relative path."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" }
                },
                "required": ["rel_path"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let rel_path = match common::parse_str_field(&args, "rel_path") {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => return common::err("invalid args: 'rel_path' must be non-empty"),
            Err(r) => return r,
        };

        if ctx.vault_root.is_none() {
            return common::err("no vault open");
        }

        let abs = match resolve_in_vault(&ctx.vault_root, &rel_path) {
            Ok(p) => p,
            Err(_) => {
                return common::err(format!(
                    "invalid path: '{rel_path}' (path escapes vault or vault not open)"
                ))
            }
        };

        let content = match std::fs::read_to_string(&abs) {
            Ok(s) => s,
            Err(e) => return common::err(format!("read error: {e}")),
        };

        common::ok(&ReadNotePayload {
            rel_path,
            content,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::testutil::{bare_ctx, fixture_ctx};
    use super::*;
    use serde_json::json;

    fn tmp_vault() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[tokio::test]
    async fn missing_rel_path_arg_returns_error() {
        let ctx = bare_ctx();
        let r = ReadNoteTool.execute(json!({}), &ctx).await;
        assert!(r.is_error);
        assert!(r.content.contains("missing 'rel_path'"));
    }

    #[tokio::test]
    async fn empty_rel_path_returns_error() {
        let ctx = bare_ctx();
        let r = ReadNoteTool
            .execute(json!({"rel_path": "  "}), &ctx)
            .await;
        assert!(r.is_error);
    }

    #[tokio::test]
    async fn no_vault_returns_error() {
        let ctx = bare_ctx();
        let r = ReadNoteTool
            .execute(json!({"rel_path": "foo.md"}), &ctx)
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("no vault"));
    }

    #[tokio::test]
    async fn path_traversal_rejected() {
        let vault = tmp_vault();
        let ctx = fixture_ctx(None, Some(vault.path().to_path_buf()), None, None);
        let r = ReadNoteTool
            .execute(json!({"rel_path": "../../etc/passwd"}), &ctx)
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("invalid path"));
    }

    #[tokio::test]
    async fn happy_path_returns_content() {
        let vault = tmp_vault();
        let note = vault.path().join("foo.md");
        std::fs::write(&note, "# Hello\n\nbody").unwrap();
        let ctx = fixture_ctx(None, Some(vault.path().to_path_buf()), None, None);
        let r = ReadNoteTool
            .execute(json!({"rel_path": "foo.md"}), &ctx)
            .await;
        assert!(!r.is_error, "content: {}", r.content);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        assert_eq!(v["rel_path"], "foo.md");
        assert_eq!(v["content"], "# Hello\n\nbody");
    }

    #[tokio::test]
    async fn nonexistent_file_returns_error() {
        let vault = tmp_vault();
        let ctx = fixture_ctx(None, Some(vault.path().to_path_buf()), None, None);
        let r = ReadNoteTool
            .execute(json!({"rel_path": "ghost.md"}), &ctx)
            .await;
        assert!(r.is_error);
        // Could be `read error: …` (file missing) — the resolve_in_vault
        // allows non-existent leaves.
        assert!(r.content.contains("read error") || r.content.contains("invalid path"));
    }
}
