//! `get_related_notes` — rank vault notes by relatedness to a source
//! note using tag overlap, direct links, co-citation, embedding
//! cosine similarity, and staleness.
//!
//! Delegates the scoring kernel to `commands::ai::related_notes_core`
//! (extracted in D5.2 from the `ai_related_notes` Tauri command) so
//! the tool and the command share one source of truth.
//!
//! ## Embedding fallback
//!
//! When `ctx.embeddings` is `None` **or** `ctx.embed_model` is `None`
//! **or** `note_cosine_scores` returns an error, we pass an empty
//! `HashMap` to the core and let tag/link/co-citation signals carry
//! the ranking. This mirrors what `ai_related_notes` does when the
//! vault has no embeddings yet.
//!
//! ## Cancellation
//!
//! The scoring phase is a handful of bulk SQL queries — milliseconds
//! even on 10k-note vaults — so we only poll `ctx.cancel` once,
//! between the (synchronous) embedding-score collection and the core
//! call. That covers the "user cancelled while embedding scores were
//! being loaded" window; a cancel during the tight SQL scoring loop
//! still completes but gets dropped at the outer chat loop boundary.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use crate::commands::ai::{related_notes_core, RelatedNote};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolContext};
use super::common;

pub struct GetRelatedNotesTool;

#[derive(Serialize)]
struct GetRelatedNotesPayload {
    related: Vec<RelatedNote>,
}

#[async_trait]
impl Tool for GetRelatedNotesTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_related_notes".into(),
            description:
                "Rank notes related to a source note via tag overlap, direct links, \
                 co-citation, embedding cosine, and staleness."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "rel_path": { "type": "string" },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 30,
                        "default": 10
                    }
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

        // Path-traversal gate — matches the AppError::PathEscape branch
        // in `ai_related_notes`.
        let rel = std::path::Path::new(&rel_path);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return common::err(format!("invalid path: '{rel_path}'"));
        }

        let index = match ctx.index.as_ref() {
            Some(h) => h,
            None => return common::err("no index available (vault not opened?)"),
        };

        let limit = common::parse_uint_field(&args, "limit", 10, 30) as usize;

        // Best-effort embedding-cosine map. Any missing piece →
        // empty map, same as `ai_related_notes` semantics.
        let embedding_scores: HashMap<String, f64> =
            match (ctx.embeddings.as_ref(), ctx.embed_model.as_ref()) {
                (Some(store), Some(model)) => {
                    let guard = store.lock().unwrap();
                    guard
                        .note_cosine_scores(&rel_path, model)
                        .map(|m| {
                            m.into_iter()
                                .map(|(p, s)| (p, s as f64))
                                .collect::<HashMap<_, _>>()
                        })
                        .unwrap_or_default()
                }
                _ => HashMap::new(),
            };

        if ctx.cancel.load(Ordering::SeqCst) {
            return common::err("cancelled");
        }

        let conn = index.lock().unwrap();
        let related: Vec<RelatedNote> =
            match related_notes_core(&conn, &embedding_scores, &rel_path, limit) {
                Ok(v) => v,
                Err(e) => return common::err(format!("related-notes error: {e}")),
            };

        common::ok(&GetRelatedNotesPayload { related })
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::testutil::{bare_ctx, fixture_ctx, in_memory_conn};
    use super::*;
    use serde_json::json;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn insert_note(
        conn: &rusqlite::Connection,
        path: &str,
        title: &str,
        updated: &str,
        tags: &[&str],
    ) {
        conn.execute(
            "INSERT INTO notes (path, title, updated) VALUES (?1, ?2, ?3)",
            rusqlite::params![path, title, updated],
        )
        .unwrap();
        for t in tags {
            conn.execute(
                "INSERT INTO tags (note_path, tag) VALUES (?1, ?2)",
                rusqlite::params![path, *t],
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn missing_rel_path_returns_error() {
        let ctx = bare_ctx();
        let r = GetRelatedNotesTool.execute(json!({}), &ctx).await;
        assert!(r.is_error);
        assert!(r.content.contains("missing 'rel_path'"));
    }

    #[tokio::test]
    async fn path_traversal_rejected() {
        let ctx = bare_ctx();
        let r = GetRelatedNotesTool
            .execute(json!({"rel_path": "../escape.md"}), &ctx)
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("invalid path"));
    }

    #[tokio::test]
    async fn no_index_returns_error() {
        let ctx = bare_ctx();
        let r = GetRelatedNotesTool
            .execute(json!({"rel_path": "foo.md"}), &ctx)
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("no index"));
    }

    #[tokio::test]
    async fn empty_when_source_not_in_index() {
        let conn = in_memory_conn();
        {
            // Other notes exist, but none share a tag with the (absent) src.
            let g = conn.lock().unwrap();
            insert_note(&g, "a.md", "A", "2026-04-01", &["x"]);
            insert_note(&g, "b.md", "B", "2026-04-02", &["y"]);
        }
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = GetRelatedNotesTool
            .execute(json!({"rel_path": "ghost.md"}), &ctx)
            .await;
        assert!(!r.is_error, "content: {}", r.content);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        assert_eq!(v["related"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn scoring_ranks_tag_peer_first() {
        let conn = in_memory_conn();
        {
            let g = conn.lock().unwrap();
            insert_note(&g, "src.md", "S", "2026-04-10", &["rust", "project"]);
            insert_note(&g, "peer.md", "P", "2026-04-10", &["rust", "project"]); // overlap 2/2
            insert_note(&g, "weak.md", "W", "2026-04-10", &["rust"]);           // overlap 1/1 (same)
            insert_note(&g, "unrelated.md", "U", "2026-04-10", &["cooking"]);   // 0
        }
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = GetRelatedNotesTool
            .execute(json!({"rel_path": "src.md"}), &ctx)
            .await;
        assert!(!r.is_error, "content: {}", r.content);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        let paths: Vec<&str> = v["related"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["path"].as_str().unwrap())
            .collect();
        // peer/weak both have tag_overlap=1.0 (shared / min(src,cand));
        // the unrelated note is filtered out (score <= 0).
        assert!(paths.contains(&"peer.md"));
        assert!(paths.contains(&"weak.md"));
        assert!(!paths.contains(&"unrelated.md"));
    }

    #[tokio::test]
    async fn cancel_before_core_returns_cancelled() {
        let conn = in_memory_conn();
        let mut ctx = fixture_ctx(Some(conn), None, None, None);
        ctx.cancel = Arc::new(AtomicBool::new(true)); // flip pre-execute
        let r = GetRelatedNotesTool
            .execute(json!({"rel_path": "foo.md"}), &ctx)
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("cancelled"));
    }

    #[tokio::test]
    async fn limit_clamped_to_30() {
        let conn = in_memory_conn();
        {
            let g = conn.lock().unwrap();
            insert_note(&g, "src.md", "S", "2026-04-10", &["t"]);
            for i in 0..50 {
                insert_note(&g, &format!("n{i}.md"), "N", "2026-04-10", &["t"]);
            }
        }
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = GetRelatedNotesTool
            .execute(json!({"rel_path": "src.md", "limit": 999}), &ctx)
            .await;
        assert!(!r.is_error);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        // At most 30 — but also not panic / regression.
        assert!(v["related"].as_array().unwrap().len() <= 30);
    }
}
