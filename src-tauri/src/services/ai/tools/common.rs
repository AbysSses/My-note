//! Shared helpers for the D5.2 read-only tools.
//!
//! Three categories:
//!
//! 1. **Result builders** (`err` / `ok`) — every tool returns
//!    `ToolResult` via these so the `tool_call_id` field is always
//!    `String::new()` (registry backfills) and the JSON serialization
//!    path is identical across tools.
//! 2. **Argument parsers** (`parse_str_field` / `parse_uint_field`) —
//!    uniform error messages for missing / wrong-type fields and a
//!    single clamp policy for integer limits.
//! 3. **Test scaffolding** (`#[cfg(test)]`) — an in-memory SQLite
//!    helper using `db::apply_schema` so every tool test fixture
//!    builds the real schema rather than drifting copies.

use serde::Serialize;
use serde_json::Value;

use super::super::provider::ToolResult;

/// Shorthand for "something went wrong inside the tool". Matches
/// the shape the chat loop forwards to the transcript so the next
/// model turn can see + recover.
pub(super) fn err(msg: impl Into<String>) -> ToolResult {
    ToolResult {
        tool_call_id: String::new(),
        content: msg.into(),
        is_error: true,
    }
}

/// Serialize `payload` to a JSON string and wrap it in a successful
/// [`ToolResult`]. The result of `serde_json::to_string` on a
/// derive-Serialize struct is infallible in practice — but we still
/// fall back to `err` rather than panic, to keep the "never panic"
/// invariant of `Tool::execute`.
pub(super) fn ok<T: Serialize>(payload: &T) -> ToolResult {
    match serde_json::to_string(payload) {
        Ok(s) => ToolResult {
            tool_call_id: String::new(),
            content: s,
            is_error: false,
        },
        Err(e) => err(format!("serialize error: {e}")),
    }
}

/// Extract a required string field. Returns `Err(ToolResult)` on any
/// shape mismatch so callers can early-return with `?`-style flow:
///
/// ```ignore
/// let tag = match common::parse_str_field(&args, "tag") {
///     Ok(s) if !s.is_empty() => s,
///     Ok(_) => return common::err("tag must be non-empty"),
///     Err(r) => return r,
/// };
/// ```
///
/// Empty strings pass through — the caller decides whether empty is
/// meaningful (e.g. `search_fulltext` treats `""` as an error; a
/// future `search_by_author` with `author=""` might treat it as "any").
pub(super) fn parse_str_field(args: &Value, key: &str) -> Result<String, ToolResult> {
    match args.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(err(format!("invalid args: '{key}' must be a string"))),
        None => Err(err(format!("invalid args: missing '{key}'"))),
    }
}

/// Extract an optional unsigned integer field, falling back to
/// `default` when absent and clamping to `[1, max]` when present.
///
/// - Missing → `default`.
/// - Non-integer / negative → `default` (silent; we treat bad shape
///   as "caller didn't try" rather than erroring out — the model
///   often emits floating-point limits which we happily round).
/// - `> max` → clamped down to `max`.
/// - `< 1` → clamped up to `1`.
pub(super) fn parse_uint_field(args: &Value, key: &str, default: u32, max: u32) -> u32 {
    let v = args.get(key);
    let raw = match v {
        Some(Value::Number(n)) => {
            if let Some(u) = n.as_u64() {
                u as u32
            } else if let Some(i) = n.as_i64() {
                if i < 0 { return default; }
                i as u32
            } else if let Some(f) = n.as_f64() {
                if !f.is_finite() || f < 0.0 { return default; }
                f as u32
            } else {
                return default;
            }
        }
        _ => return default,
    };
    raw.clamp(1, max)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) mod testutil {
    //! In-memory DB + bare `ToolContext` helpers shared by every
    //! tool's inline test module.

    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    use rusqlite::Connection;

    use super::super::super::embedding_store::EmbeddingStore;
    use super::super::super::tool_registry::ToolContext;
    use crate::services::config::AiToolPermissions;

    /// Open a fresh `:memory:` connection and apply the real
    /// production schema. Keeps tool tests honest — a schema drift
    /// that breaks a real tool will break the corresponding test too.
    pub(crate) fn in_memory_conn() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().expect("open :memory:");
        // Pragmas are best-effort on in-memory DBs (foreign_keys ON is
        // still useful; WAL is a no-op).
        let _ = crate::db::apply_pragmas(&conn);
        crate::db::apply_schema(&conn).expect("apply_schema");
        Arc::new(Mutex::new(conn))
    }

    /// Build a `ToolContext` with only the given bits wired; the
    /// rest stays absent. Tool tests construct exactly what the tool
    /// reads and leave the rest `None` to prove the tool degrades.
    pub(crate) fn fixture_ctx(
        conn: Option<Arc<Mutex<Connection>>>,
        vault: Option<PathBuf>,
        embeddings: Option<Arc<Mutex<EmbeddingStore>>>,
        embed_model: Option<String>,
    ) -> ToolContext {
        ToolContext {
            vault_root: vault,
            index: conn,
            embeddings,
            embed_model,
            provider: None,
            chat_model: None,
            tool_permissions: AiToolPermissions::default(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Shortest-path builder used by tests that don't care about any
    /// handle (registry-dispatch tests, arg-validation tests).
    pub(crate) fn bare_ctx() -> ToolContext {
        fixture_ctx(None, None, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn err_produces_is_error_true() {
        let r = err("boom");
        assert!(r.is_error);
        assert_eq!(r.content, "boom");
        assert!(r.tool_call_id.is_empty());
    }

    #[test]
    fn ok_serializes_json_payload() {
        #[derive(Serialize)]
        struct P { n: u32 }
        let r = ok(&P { n: 7 });
        assert!(!r.is_error);
        assert_eq!(r.content, r#"{"n":7}"#);
    }

    #[test]
    fn parse_str_field_missing() {
        let args = json!({});
        let r = parse_str_field(&args, "tag").unwrap_err();
        assert!(r.content.contains("missing 'tag'"));
    }

    #[test]
    fn parse_str_field_wrong_type() {
        let args = json!({"tag": 3});
        let r = parse_str_field(&args, "tag").unwrap_err();
        assert!(r.content.contains("must be a string"));
    }

    #[test]
    fn parse_str_field_ok() {
        let args = json!({"tag": "inbox"});
        let s = parse_str_field(&args, "tag").unwrap();
        assert_eq!(s, "inbox");
    }

    #[test]
    fn parse_uint_defaults_when_missing() {
        assert_eq!(parse_uint_field(&json!({}), "limit", 10, 100), 10);
    }

    #[test]
    fn parse_uint_clamps_up_to_max() {
        assert_eq!(parse_uint_field(&json!({"limit": 9999}), "limit", 10, 100), 100);
    }

    #[test]
    fn parse_uint_clamps_up_to_one() {
        assert_eq!(parse_uint_field(&json!({"limit": 0}), "limit", 10, 100), 1);
    }

    #[test]
    fn parse_uint_rejects_negative_and_falls_back() {
        assert_eq!(parse_uint_field(&json!({"limit": -5}), "limit", 10, 100), 10);
    }

    #[test]
    fn parse_uint_accepts_float() {
        assert_eq!(parse_uint_field(&json!({"limit": 12.7}), "limit", 10, 100), 12);
    }
}
