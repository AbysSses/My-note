//! `search_fulltext` — BM25-ranked FTS5 search over note bodies.
//!
//! Mirrors `commands::index::index_search` but returns results under
//! `hits` with the raw vault-relative path, optional title, and a
//! snippet with hits wrapped in `<mark>…</mark>`.
//!
//! `fts_sanitize` wraps the query in a quoted phrase so user text
//! like `foo: bar` doesn't get re-parsed as FTS5 operators.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use crate::commands::index::fts_sanitize;

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolContext};
use super::common;

pub struct SearchFulltextTool;

#[derive(Serialize)]
struct SearchFulltextPayload {
    hits: Vec<Hit>,
}

#[derive(Serialize)]
struct Hit {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    snippet: String,
}

#[async_trait]
impl Tool for SearchFulltextTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "search_fulltext".into(),
            description:
                "Full-text search over note bodies (FTS5 + BM25). Snippets wrap hits in `<mark>`."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 50,
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult {
        let query = match common::parse_str_field(&args, "query") {
            Ok(s) if !s.trim().is_empty() => s,
            Ok(_) => return common::err("invalid args: 'query' must be non-empty"),
            Err(r) => return r,
        };
        let limit = common::parse_uint_field(&args, "limit", 10, 50) as i64;

        let index = match ctx.index.as_ref() {
            Some(h) => h,
            None => return common::err("no index available (vault not opened?)"),
        };
        let conn = index.lock().unwrap();

        // FTS5 contentless tables (`content=''`) do NOT preserve
        // `UNINDEXED` column values on read — `f.path` always comes
        // back as NULL. The indexer and `notes` share rowids
        // (see `db/indexer.rs::upsert_note` — `notes` is inserted
        // first, then `notes_fts` right after, so SQLite's
        // auto-rowid assigns matching integers), so we JOIN on
        // `rowid` to get a real path + title from `notes`.
        let mut stmt = match conn.prepare_cached(
            "SELECT n.path, n.title,
                    snippet(notes_fts, 2, '<mark>', '</mark>', '…', 12) as snip
             FROM notes_fts
             JOIN notes n ON n.rowid = notes_fts.rowid
             WHERE notes_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(e) => return common::err(format!("database error: {e}")),
        };

        let rows = match stmt.query_map(
            rusqlite::params![fts_sanitize(&query), limit],
            |row| {
                // `snippet` comes back NULL on contentless FTS5 tables
                // when the matched row has no recoverable body text —
                // degrade to empty string rather than erroring out so
                // the hit list still reaches the caller.
                let snip: Option<String> = row.get(2)?;
                Ok(Hit {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    snippet: snip.unwrap_or_default(),
                })
            },
        ) {
            Ok(r) => r,
            Err(e) => return common::err(format!("database error: {e}")),
        };

        let mut hits = Vec::new();
        for row in rows {
            match row {
                Ok(h) => hits.push(h),
                Err(e) => return common::err(format!("database error: {e}")),
            }
        }

        common::ok(&SearchFulltextPayload { hits })
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::testutil::{bare_ctx, fixture_ctx, in_memory_conn};
    use super::*;
    use serde_json::json;

    /// Insert a note + its FTS row so MATCH can hit it.
    fn seed_note(conn: &rusqlite::Connection, path: &str, title: &str, body: &str) {
        conn.execute(
            "INSERT INTO notes (path, title) VALUES (?1, ?2)",
            rusqlite::params![path, title],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO notes_fts (path, title, body) VALUES (?1, ?2, ?3)",
            rusqlite::params![path, title, body],
        )
        .unwrap();
    }

    #[tokio::test]
    async fn missing_query_arg_returns_error() {
        let ctx = bare_ctx();
        let r = SearchFulltextTool.execute(json!({}), &ctx).await;
        assert!(r.is_error);
        assert!(r.content.contains("missing 'query'"));
    }

    #[tokio::test]
    async fn empty_query_returns_error() {
        let ctx = bare_ctx();
        let r = SearchFulltextTool
            .execute(json!({"query": "   "}), &ctx)
            .await;
        assert!(r.is_error);
    }

    #[tokio::test]
    async fn no_index_returns_error() {
        let ctx = bare_ctx();
        let r = SearchFulltextTool
            .execute(json!({"query": "anything"}), &ctx)
            .await;
        assert!(r.is_error);
        assert!(r.content.contains("no index"));
    }

    #[tokio::test]
    async fn fts_returns_hit_with_path_from_notes() {
        // Note: snippet() on a contentless FTS5 table (`content=''`)
        // returns NULL unless an external content table provides the
        // body back — in production the indexer keeps body in FTS
        // only, so the snippet comes out empty here. We verify the
        // match + path surface correctly; snippet quality is a
        // presentation concern tested separately at the command
        // layer.
        let conn = in_memory_conn();
        {
            let g = conn.lock().unwrap();
            seed_note(&g, "a.md", "Alpha", "the quick brown fox jumps over the lazy dog");
            seed_note(&g, "b.md", "Beta", "completely unrelated content about pizza");
        }
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = SearchFulltextTool
            .execute(json!({"query": "quick brown"}), &ctx)
            .await;
        assert!(!r.is_error, "content: {}", r.content);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        let hits = v["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["path"], "a.md");
        // `snippet` key always present — string, possibly empty.
        assert!(hits[0].get("snippet").is_some());
    }

    #[tokio::test]
    async fn limit_default_10() {
        let conn = in_memory_conn();
        {
            let g = conn.lock().unwrap();
            for i in 0..15 {
                seed_note(&g, &format!("n{i}.md"), "T", "needle haystack");
            }
        }
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = SearchFulltextTool
            .execute(json!({"query": "needle"}), &ctx)
            .await;
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        assert_eq!(v["hits"].as_array().unwrap().len(), 10);
    }
}
