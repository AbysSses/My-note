//! `search_by_tag` — return every note tagged with a given tag.
//!
//! Thin wrapper around the SQL that `commands::index::index_notes_by_tag`
//! uses, adapted to return a list under the `notes` key so the chat
//! loop's transcript is self-describing ("tool said {notes:[…]}" vs.
//! "tool said [ … ]").
//!
//! **Ordering**: `COALESCE(n.updated, '') DESC` — newest first, same
//! as the command-layer sibling.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolContext};
use super::common;

pub struct SearchByTagTool;

#[derive(Serialize)]
struct SearchByTagPayload {
    notes: Vec<NoteRef>,
}

#[derive(Serialize)]
struct NoteRef {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated: Option<String>,
}

#[async_trait]
impl Tool for SearchByTagTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "search_by_tag".into(),
            description: "Find notes tagged with a specific tag, newest-first."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tag": { "type": "string" },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 100,
                        "default": 20
                    }
                },
                "required": ["tag"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let tag = match common::parse_str_field(&args, "tag") {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => return common::err("invalid args: 'tag' must be non-empty"),
            Err(r) => return r,
        };
        let limit = common::parse_uint_field(&args, "limit", 20, 100) as i64;

        let index = match ctx.index.as_ref() {
            Some(h) => h,
            None => return common::err("no index available (vault not opened?)"),
        };
        let conn = index.lock().unwrap();

        let mut stmt = match conn.prepare_cached(
            "SELECT n.path, n.title, n.updated
             FROM tags t
             JOIN notes n ON n.path = t.note_path
             WHERE t.tag = ?1
             ORDER BY COALESCE(n.updated, '') DESC
             LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(e) => return common::err(format!("database error: {e}")),
        };

        let rows = match stmt.query_map(rusqlite::params![tag, limit], |row| {
            Ok(NoteRef {
                path: row.get(0)?,
                title: row.get(1)?,
                updated: row.get(2)?,
            })
        }) {
            Ok(r) => r,
            Err(e) => return common::err(format!("database error: {e}")),
        };

        let mut notes = Vec::new();
        for row in rows {
            match row {
                Ok(n) => notes.push(n),
                Err(e) => return common::err(format!("database error: {e}")),
            }
        }

        common::ok(&SearchByTagPayload { notes })
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::testutil::{bare_ctx, fixture_ctx, in_memory_conn};
    use super::*;
    use serde_json::json;

    fn seed_tag_note(conn: &rusqlite::Connection, path: &str, title: &str, updated: &str, tag: &str) {
        conn.execute(
            "INSERT INTO notes (path, title, updated) VALUES (?1, ?2, ?3)",
            rusqlite::params![path, title, updated],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tags (note_path, tag) VALUES (?1, ?2)",
            rusqlite::params![path, tag],
        )
        .unwrap();
    }

    #[tokio::test]
    async fn missing_tag_arg_returns_error() {
        let ctx = bare_ctx();
        let r = SearchByTagTool.execute(json!({}), &ctx).await;
        assert!(r.is_error);
        assert!(r.content.contains("missing 'tag'"));
    }

    #[tokio::test]
    async fn empty_tag_returns_error() {
        let ctx = bare_ctx();
        let r = SearchByTagTool.execute(json!({"tag": "   "}), &ctx).await;
        assert!(r.is_error);
        assert!(r.content.contains("non-empty"));
    }

    #[tokio::test]
    async fn no_index_returns_error() {
        let ctx = bare_ctx();
        let r = SearchByTagTool.execute(json!({"tag": "x"}), &ctx).await;
        assert!(r.is_error);
        assert!(r.content.contains("no index"));
    }

    #[tokio::test]
    async fn happy_path_sorts_newest_first() {
        let conn = in_memory_conn();
        {
            let guard = conn.lock().unwrap();
            seed_tag_note(&guard, "a.md", "A", "2024-01-01", "project");
            seed_tag_note(&guard, "b.md", "B", "2026-03-15", "project");
            seed_tag_note(&guard, "c.md", "C", "2025-08-08", "project");
            seed_tag_note(&guard, "d.md", "D", "2026-04-20", "other");
        }
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = SearchByTagTool
            .execute(json!({"tag": "project"}), &ctx)
            .await;
        assert!(!r.is_error, "content: {}", r.content);
        // b.md > c.md > a.md; d.md filtered out.
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        let paths: Vec<&str> = v["notes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["b.md", "c.md", "a.md"]);
    }

    #[tokio::test]
    async fn limit_clamped_to_100() {
        let conn = in_memory_conn();
        {
            let guard = conn.lock().unwrap();
            for i in 0..150 {
                seed_tag_note(&guard, &format!("n{i}.md"), "T", "2026-01-01", "bulk");
            }
        }
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = SearchByTagTool
            .execute(json!({"tag": "bulk", "limit": 9999}), &ctx)
            .await;
        assert!(!r.is_error);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        assert_eq!(v["notes"].as_array().unwrap().len(), 100);
    }
}
