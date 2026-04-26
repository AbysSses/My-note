//! `list_tags` — enumerate every tag in the vault with its frequency.
//!
//! Same query shape as `commands::index::index_tags`: GROUP BY tag,
//! ordered by `count DESC, tag ASC`.

use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Value};

use super::super::provider::{ToolDefinition, ToolResult};
use super::super::tool_registry::{Tool, ToolContext};
use super::common;

pub struct ListTagsTool;

#[derive(Serialize)]
struct ListTagsPayload {
    tags: Vec<TagCount>,
}

#[derive(Serialize)]
struct TagCount {
    tag: String,
    count: i64,
}

#[async_trait]
impl Tool for ListTagsTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_tags".into(),
            description:
                "List every tag in the vault with the number of notes carrying it."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
        }
    }

    async fn execute(&self, _args: Value, ctx: &ToolContext) -> ToolResult {
        let index = match ctx.index.as_ref() {
            Some(h) => h,
            None => return common::err("no index available (vault not opened?)"),
        };
        let conn = index.lock().unwrap();

        let mut stmt = match conn.prepare_cached(
            "SELECT tag, COUNT(*) as c FROM tags
             GROUP BY tag
             ORDER BY c DESC, tag ASC",
        ) {
            Ok(s) => s,
            Err(e) => return common::err(format!("database error: {e}")),
        };

        let rows = match stmt.query_map([], |row| {
            Ok(TagCount {
                tag: row.get(0)?,
                count: row.get(1)?,
            })
        }) {
            Ok(r) => r,
            Err(e) => return common::err(format!("database error: {e}")),
        };

        let mut tags = Vec::new();
        for row in rows {
            match row {
                Ok(t) => tags.push(t),
                Err(e) => return common::err(format!("database error: {e}")),
            }
        }

        common::ok(&ListTagsPayload { tags })
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::testutil::{bare_ctx, fixture_ctx, in_memory_conn};
    use super::*;
    use serde_json::json;

    fn seed(conn: &rusqlite::Connection, path: &str, tags: &[&str]) {
        conn.execute("INSERT INTO notes (path) VALUES (?1)", rusqlite::params![path])
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
    async fn empty_vault_returns_empty_list() {
        let conn = in_memory_conn();
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = ListTagsTool.execute(json!({}), &ctx).await;
        assert!(!r.is_error);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        assert_eq!(v["tags"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn returns_counts_desc_then_tag_asc() {
        let conn = in_memory_conn();
        {
            let g = conn.lock().unwrap();
            seed(&g, "a.md", &["zeta", "alpha"]);
            seed(&g, "b.md", &["alpha"]);
            seed(&g, "c.md", &["alpha", "beta"]);
        }
        let ctx = fixture_ctx(Some(conn), None, None, None);
        let r = ListTagsTool.execute(json!({}), &ctx).await;
        assert!(!r.is_error);
        let v: serde_json::Value = serde_json::from_str(&r.content).unwrap();
        let tags = v["tags"].as_array().unwrap();
        // alpha(3), beta(1), zeta(1)  — beta before zeta alphabetically.
        assert_eq!(tags[0]["tag"], "alpha");
        assert_eq!(tags[0]["count"], 3);
        assert_eq!(tags[1]["tag"], "beta");
        assert_eq!(tags[2]["tag"], "zeta");
    }

    #[tokio::test]
    async fn no_index_returns_error() {
        let ctx = bare_ctx();
        let r = ListTagsTool.execute(json!({}), &ctx).await;
        assert!(r.is_error);
        assert!(r.content.contains("no index"));
    }
}
