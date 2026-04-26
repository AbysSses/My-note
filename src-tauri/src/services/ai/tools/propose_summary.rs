use async_trait::async_trait;
use serde_json::{json, Value};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolCategory, ToolContext};
use super::common;
use super::writeback_support::{
    complete_via_tool, insert_tldr_at_top, proposal_ok, read_note_body,
    rewrite_frontmatter_scalar, split_leading_frontmatter, summary_target_from_args,
    ProposalPayload,
};

pub struct ProposeSummaryTool;

const SYSTEM_PROMPT: &str = "You are a concise-summary writer for a personal knowledge base. Write a faithful, information-dense TL;DR in the user's language. If the note is Chinese, reply in Chinese; otherwise English. Output ONLY the summary paragraph, without any heading, bullet, quote, or markdown decoration. Keep it to 1-3 sentences, under 120 characters when possible.";

#[async_trait]
impl Tool for ProposeSummaryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "propose_summary".into(),
            description: "Draft a summary proposal for a note, either writing frontmatter.summary or inserting a TL;DR block near the top.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "target": {
                        "type": "string",
                        "enum": ["frontmatter", "top"],
                        "default": "frontmatter"
                    }
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
        let target = summary_target_from_args(&args);

        let body = match read_note_body(ctx, &rel_path) {
            Ok(body) => body,
            Err(err) => return err,
        };
        let note_body = split_leading_frontmatter(&body)
            .map(|(_, rest)| rest)
            .unwrap_or(body.as_str())
            .trim();
        if note_body.is_empty() {
            return common::err("note body is empty after frontmatter stripping");
        }

        let user_prompt = format!(
            "Summarize the following markdown note into a single TL;DR paragraph:\n\n{}",
            note_body
        );
        let reply =
            match complete_via_tool(ctx, SYSTEM_PROMPT, user_prompt, Some(0.2), Some(180)).await {
                Ok(reply) => reply,
                Err(err) => return err,
            };

        let cleaned = reply.split_whitespace().collect::<Vec<_>>().join(" ");
        if cleaned.is_empty() {
            return common::err("provider returned an empty summary");
        }

        let proposed = if target == "top" {
            insert_tldr_at_top(&body, &cleaned)
        } else {
            rewrite_frontmatter_scalar(&body, "summary", &cleaned)
        };
        let target_label = if target == "top" {
            "插入 TL;DR"
        } else {
            "更新 frontmatter.summary"
        };

        proposal_ok(&ProposalPayload {
            proposal_kind: "summary".into(),
            target_rel_path: rel_path,
            original_content: body,
            proposed_content: proposed,
            summary: format!("{target_label} · {cleaned}"),
            metadata: json!({
                "target": target,
                "generated_summary": cleaned
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tempfile::tempdir;

    use super::*;
    use crate::services::ai::provider::{ChatScriptItem, MockProvider};
    use crate::services::ai::tools::common::testutil::fixture_ctx;
    use crate::services::config::AiToolPermissions;

    fn ctx_with_provider(vault: std::path::PathBuf, reply: &str) -> ToolContext {
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
    async fn builds_frontmatter_summary_proposal() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("foo.md");
        std::fs::write(&path, "# 标题\n\n正文内容").unwrap();
        let ctx = ctx_with_provider(tmp.path().to_path_buf(), "这是一段摘要。");
        let out = ProposeSummaryTool
            .execute(json!({"rel_path": "foo.md"}), &ctx)
            .await;
        assert!(!out.is_error, "{}", out.content);
        let payload: ProposalPayload = serde_json::from_str(&out.content).unwrap();
        assert_eq!(payload.proposal_kind, "summary");
        assert!(payload.proposed_content.contains("summary: 这是一段摘要。"));
    }

    #[tokio::test]
    async fn top_target_inserts_tldr_block() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("foo.md");
        std::fs::write(&path, "---\ntitle: Foo\n---\n\n正文内容").unwrap();
        let ctx = ctx_with_provider(tmp.path().to_path_buf(), "TLDR");
        let out = ProposeSummaryTool
            .execute(json!({"rel_path": "foo.md", "target": "top"}), &ctx)
            .await;
        assert!(!out.is_error, "{}", out.content);
        let payload: ProposalPayload = serde_json::from_str(&out.content).unwrap();
        assert!(payload.proposed_content.contains("> **TL;DR** TLDR"));
    }

    #[tokio::test]
    async fn missing_rel_path_errors() {
        let ctx = fixture_ctx(None, None, None, None);
        let out = ProposeSummaryTool.execute(json!({}), &ctx).await;
        assert!(out.is_error);
        assert!(out.content.contains("missing 'rel_path'"));
    }
}
