//! Tool registry — Phase 3-D5.1 Agentic Chat protocol plumbing.
//!
//! The registry is a name → trait-object map the chat loop consults:
//!
//! - `definitions()` produces the JSON-schema list sent to the provider
//!   (OpenAI `tools` field, empty vec turns tool calling off).
//! - `execute(name, args, cancel)` dispatches a single tool call coming
//!   back from the model and returns a [`ToolResult`] the loop forwards
//!   to both the frontend (via `ai:chat-stream:tool_call_result` event)
//!   and the chat store (as a `role = Tool` message).
//!
//! ## Intentional non-features (P3-D5.1 scope)
//!
//! - **No real tools are registered.** D5.1 ships the plumbing only;
//!   search_notes / propose_edit_note / delete_note etc. land in
//!   D5.2+. A chat loop that hits `execute("search_notes", …)` against
//!   an empty registry gets back `is_error = true` + `content =
//!   "tool 'search_notes' not registered"`, which the next model turn
//!   sees and can recover from gracefully.
//! - **No permission tiers.** The 🟢 / 🟡 / 🔴 permission gating
//!   designed in plan_P3.md §4.2 lives in D5.4; D5.1 always executes
//!   whatever the registry accepts.
//! - **No concurrency.** Tool calls execute sequentially in the chat
//!   loop. A tool that needs to spawn background work owns its own
//!   `tokio::spawn` — the registry doesn't parallelise calls.
//!
//! ## Cancellation
//!
//! Each `execute` receives an `Arc<AtomicBool>` shared with the chat
//! stream's cancel flag. Long-running tools are expected to poll it
//! between async steps. A tool that ignores the flag will still
//! complete — the outer loop only observes cancel between tool calls —
//! but the user cancel UX degrades from "stop now" to "stop after
//! current tool finishes".

#![allow(dead_code)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rusqlite::Connection;

use super::embedding_store::EmbeddingStore;
use super::provider::{AiProvider, ToolDefinition, ToolResult};
use crate::services::config::AiToolPermissions;

/// Runtime handles a tool may consult, built fresh at each call site by
/// the chat streaming loop. All fields are `Option<_>` — any given
/// dependency may be missing (vault not open, embeddings not
/// initialised, config missing the embed model) and tools must
/// gracefully return `is_error: true` rather than panic.
///
/// Construction pattern (in `ai_chat_stream_start`):
///
/// ```ignore
/// let ctx = ToolContext {
///     vault_root: state.active_vault.lock().unwrap().clone(),
///     index: state.index_handle(),
///     embeddings: state.embeddings_handle(),
///     embed_model: configured_embed_model(&state),
///     cancel: cancel_flag.clone(),
/// };
/// tool_registry.execute(name, id, args, &ctx).await
/// ```
///
/// `ToolContext` is deliberately not `Clone`; pass by reference so the
/// streaming task owns the handles exactly once per iteration. Tools
/// that need to keep an `Arc` alive past a single call should clone
/// the specific `Arc` field themselves.
pub struct ToolContext {
    /// Absolute path of the currently-open vault. `None` means no
    /// vault is open — any tool touching the filesystem or index must
    /// treat this as a hard prerequisite failure.
    pub vault_root: Option<PathBuf>,
    /// Shared handle to the vault's SQLite index connection. `None`
    /// when the vault isn't opened or the index failed to initialise.
    pub index: Option<Arc<Mutex<Connection>>>,
    /// Shared handle to the per-vault embedding store. `None` when the
    /// vault has no embeddings initialised (AI disabled, or the user
    /// hasn't run the embed preview/run flow yet).
    pub embeddings: Option<Arc<Mutex<EmbeddingStore>>>,
    /// The embedding model name configured at call time (e.g.
    /// `"text-embedding-3-small"`). `None` when the AI provider is
    /// unconfigured. Tools pairing vectors with model-scoped queries
    /// must fall back gracefully when absent.
    pub embed_model: Option<String>,
    /// Live chat-capable provider handle for write-back / destructive
    /// tools that need an internal completion round-trip.
    pub provider: Option<Arc<dyn AiProvider>>,
    /// Chat model id used for internal completion calls.
    pub chat_model: Option<String>,
    /// Effective AI tool-permission snapshot for the current stream.
    pub tool_permissions: AiToolPermissions,
    /// The same cancel flag the chat streaming task watches; tools
    /// doing multi-step async work should poll it between awaits.
    pub cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    Readonly,
    Writeback,
    Destructive,
}

impl ToolCategory {
    pub fn allowed_by(self, permissions: &AiToolPermissions) -> bool {
        match self {
            ToolCategory::Readonly => permissions.allow_readonly,
            ToolCategory::Writeback => permissions.allow_writeback,
            ToolCategory::Destructive => permissions.allow_destructive,
        }
    }
}

/// Concrete tool the chat loop can invoke. Implementers own the JSON
/// schema they advertise via [`Tool::definition`] — the registry never
/// validates args against the schema, relying on the model to produce
/// conforming JSON. Malformed args are handled inside `execute` by
/// returning a `ToolResult` with `is_error = true`.
///
/// `Send + Sync` are required so the registry can store trait objects
/// behind an `Arc` and share them across the spawned streaming task.
#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    fn category(&self) -> ToolCategory {
        ToolCategory::Readonly
    }

    /// Run the tool with model-produced `args` (already parsed from the
    /// JSON string the provider emitted) and a borrowed [`ToolContext`]
    /// carrying whatever runtime handles the tool may need (vault
    /// root, index connection, embedding store, embed model, cancel
    /// flag).
    ///
    /// Implementations must never panic on bad input — return a
    /// `ToolResult` with `is_error = true` and a user-facing message
    /// instead. The chat loop forwards that same message into the
    /// transcript so the model can observe + recover.
    ///
    /// Tools should poll `ctx.cancel` between async steps when they
    /// have more than one. Single-shot SQL / IO tools may ignore it —
    /// the outer chat loop still observes cancel between tool calls.
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> ToolResult;
}

/// Name → tool lookup. Construction is cheap; pass around via `Arc`.
///
/// `definitions()` snapshots the full set of advertised tools; the
/// order is arbitrary (HashMap iteration). That's fine because the
/// provider sees it as a set too — no tool's behaviour depends on
/// `tools[0]` being a specific function.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty registry. D5.2+ wires concrete tools via
    /// [`ToolRegistry::register`]; D5.1 leaves this empty so the chat
    /// loop exercises the end-to-end "tool not found" error path.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites any previous registration under
    /// the same name; callers should treat re-registration as a bug.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.definition().name;
        self.tools.insert(name, tool);
    }

    /// Advertise every registered tool to the provider. Returns an
    /// empty vec on an empty registry — the chat layer translates that
    /// into "don't send the `tools` field at all".
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    pub fn definitions_filtered(&self, permissions: &AiToolPermissions) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .filter(|t| t.category().allowed_by(permissions))
            .map(|t| t.definition())
            .collect()
    }

    /// Dispatch a single call. Unknown names return a structured error
    /// the chat loop forwards into the transcript so the next model
    /// turn can observe + recover from the mistake.
    pub async fn execute(
        &self,
        name: &str,
        tool_call_id: String,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> ToolResult {
        match self.tools.get(name) {
            Some(tool) => {
                if !tool.category().allowed_by(&ctx.tool_permissions) {
                    return ToolResult {
                        tool_call_id,
                        content: format!(
                            "tool '{name}' is disabled by current AI tool permissions"
                        ),
                        is_error: true,
                    };
                }
                let mut result = tool.execute(args, ctx).await;
                // Tools generally leave `tool_call_id` empty since the
                // registry is the only authority on per-call identity.
                // Backfill here so upstream never sees an anonymous
                // result.
                if result.tool_call_id.is_empty() {
                    result.tool_call_id = tool_call_id;
                }
                result
            }
            None => ToolResult {
                tool_call_id,
                content: format!("tool '{name}' not registered"),
                is_error: true,
            },
        }
    }

    /// True when the registry has at least one tool registered.
    /// Used by the chat loop to decide whether to send a non-empty
    /// `tools` field on the wire at all.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Trivial tool used only to verify dispatch + backfill behaviour.
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "echo".to_string(),
                description: "Echo the `text` argument back verbatim.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"],
                }),
            }
        }

        async fn execute(
            &self,
            args: serde_json::Value,
            _ctx: &ToolContext,
        ) -> ToolResult {
            let text = args
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("(missing)");
            ToolResult {
                tool_call_id: String::new(),
                content: format!("you said: {text}"),
                is_error: false,
            }
        }
    }

    /// Build a minimal `ToolContext` with every dependency absent. The
    /// registry-level tests don't exercise DB/vault/embeddings; they
    /// only need a context shape the Tool signature accepts.
    pub(crate) fn bare_ctx() -> ToolContext {
        ToolContext {
            vault_root: None,
            index: None,
            embeddings: None,
            embed_model: None,
            provider: None,
            chat_model: None,
            tool_permissions: AiToolPermissions::default(),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    struct WriteTool;

    #[async_trait]
    impl Tool for WriteTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "write".to_string(),
                description: "Test write tool".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {},
                    "required": [],
                }),
            }
        }

        fn category(&self) -> ToolCategory {
            ToolCategory::Writeback
        }

        async fn execute(
            &self,
            _args: serde_json::Value,
            _ctx: &ToolContext,
        ) -> ToolResult {
            ToolResult {
                tool_call_id: String::new(),
                content: "ok".into(),
                is_error: false,
            }
        }
    }

    #[tokio::test]
    async fn empty_registry_returns_not_registered_error() {
        let reg = ToolRegistry::new();
        assert!(reg.is_empty());
        assert!(reg.definitions().is_empty());

        let ctx = bare_ctx();
        let result = reg
            .execute("search_notes", "call_1".into(), json!({}), &ctx)
            .await;
        assert!(result.is_error);
        assert!(
            result.content.contains("not registered"),
            "msg: {}",
            result.content
        );
        assert_eq!(result.tool_call_id, "call_1");
    }

    #[tokio::test]
    async fn registered_tool_executes_and_tool_call_id_is_backfilled() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        assert!(!reg.is_empty());
        assert_eq!(reg.definitions().len(), 1);

        let ctx = bare_ctx();
        let result = reg
            .execute("echo", "call_42".into(), json!({"text": "hi"}), &ctx)
            .await;
        assert!(!result.is_error);
        assert_eq!(result.tool_call_id, "call_42");
        assert!(result.content.contains("you said: hi"));
    }

    #[tokio::test]
    async fn blocked_tool_returns_permission_error() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(WriteTool));

        let mut ctx = bare_ctx();
        ctx.tool_permissions = AiToolPermissions {
            allow_readonly: true,
            allow_writeback: false,
            allow_destructive: false,
        };

        let result = reg
            .execute("write", "call_2".into(), json!({}), &ctx)
            .await;
        assert!(result.is_error);
        assert!(result.content.contains("disabled"));
    }

    #[test]
    fn definitions_filtered_hides_disallowed_categories() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        reg.register(Arc::new(WriteTool));

        let names: Vec<String> = reg
            .definitions_filtered(&AiToolPermissions {
                allow_readonly: true,
                allow_writeback: false,
                allow_destructive: false,
            })
            .into_iter()
            .map(|d| d.name)
            .collect();

        assert_eq!(names, vec!["echo".to_string()]);
    }
}
