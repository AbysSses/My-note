//! AI-assist commands — Phase 3-D.
//!
//! ## Architecture notes
//!
//! P3-D1 is the **local / no-network** tier: no external API calls while the
//! command runs, no markdown writes, only reads from the local derivative
//! indexes (`index.sqlite`, and from D2a.5 onward `embeddings.sqlite`).
//!
//! Scoring model for `ai_related_notes` (v1):
//!
//! ```text
//! score(current, candidate) =
//!     2.0 * shared_tag_overlap      # shared tags / min(|tags|)
//!   + 1.5 * direct_link            # 1 if wiki-link exists in either direction
//!   + 1.0 * co_citation            # 1 if both linked to from the same note
//!   + 0.5 * embedding_cosine       # cosine over note-level summed embeddings
//!   - 0.3 * staleness              # days_since_updated / 30, clamped 0..=1
//! ```
//!
//! All signals are derived from SQLite; the scoring itself happens in Rust
//! after a small set of targeted queries and in-memory vector math. Total wall
//! time for a typical vault (<5k notes / <50k chunks) is still comfortably
//! interactive on laptop hardware.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::services::ai::chat_store::{
    self, ChatMessage, ChatSessionFull, ChatSessionSummary, ChatStore,
};
use crate::services::ai::embed_service::{
    self, failure_from_app_error, EmbedFailure, EmbedOutcome,
};
use crate::services::ai::embedding_store::EmbeddingStats;
use crate::services::ai::init_service::{self, VaultEmbedPreview, VaultEmbedRunResult};
use crate::services::ai::openai::OpenAiProvider;
use crate::services::ai::openai::ToolCallAccumulator;
use crate::services::ai::provider::{
    collect_chat_stream, describe_provider_error, AiProvider, ChatRequest, ChatRole, ChatTurn,
    EmbedRequest, ProviderError, ProviderErrorKind,
};
use crate::services::ai::runtime;
use crate::services::ai::secrets::{KeyringSecretStore, SecretStore};
use crate::services::ai::tool_registry::ToolContext;
use crate::services::config::AiProviderConfig;
use crate::AppState;

// ── Provider-config commands (D2a.2) ─────────────────────────────────────────

/// Result of a `ai_provider_test_connection` call.
///
/// Deliberately a struct (not a `Result<T, E>` at the IPC layer) so the
/// frontend can render **both** success state (dim / tokens) and failure
/// state (error kind / message) in the same notice-stack entry without
/// having to branch at the Tauri boundary.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderTestResult {
    /// Whether the provider successfully returned a non-empty embedding.
    pub ok: bool,
    /// On success: detected vector dimension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dim: Option<usize>,
    /// On success: total tokens reported by the provider (0 if not returned).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    /// On failure: categorical error kind
    /// (`"network" | "auth" | "rate_limit" | "invalid_request" | "other"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    /// On failure: human-readable detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// On rate-limit failure: suggested retry delay in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
}

/// Persist the AI provider configuration. `api_key` is routed to the OS
/// keystore separately and never written to disk in plaintext.
///
/// An empty `api_key` string means "don't touch the keystore" — callers use
/// this path to update `base_url` / `embed_model` / `chat_model` without
/// re-prompting for the key. Pass a non-empty string to overwrite the
/// stored key. `chat_model` is also optional (empty = chat disabled) so
/// users on "embeddings-only" setups don't have to invent a dummy model id.
#[tauri::command]
pub fn ai_provider_set_config(
    kind: String,
    base_url: String,
    embed_model: String,
    chat_model: Option<String>,
    api_key: String,
    state: State<AppState>,
) -> AppResult<()> {
    if kind.trim().is_empty() {
        return Err(AppError::Other("provider kind is empty".into()));
    }
    let config = AiProviderConfig {
        kind: kind.clone(),
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        embed_model: embed_model.trim().to_string(),
        chat_model: chat_model
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
    };
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.set_ai_provider(config)?;
    }
    if !api_key.is_empty() {
        KeyringSecretStore::new().set_api_key(&kind, &api_key)?;
    }
    Ok(())
}

/// Drop the persisted provider config AND any stored keychain entry.
/// Split into two steps deliberately so if the keychain wipe fails the
/// config still gets dropped — a half-clean state is preferable to users
/// being stuck with "I clicked forget but nothing happened".
#[tauri::command]
pub fn ai_provider_clear_config(state: State<AppState>) -> AppResult<()> {
    let existing_kind = {
        let mut cfg = state.config.lock().unwrap();
        let kind = cfg.ai_provider_kind();
        cfg.clear_ai_provider()?;
        kind
    };
    if let Some(kind) = existing_kind {
        // Keychain wipe is best-effort: user already intended to clear.
        let _ = KeyringSecretStore::new().delete_api_key(&kind);
    }
    Ok(())
}

/// Cheap existence check for the Settings UI badge ("key configured" / "not
/// configured") without ever returning the secret itself.
#[tauri::command]
pub fn ai_provider_has_api_key(state: State<AppState>) -> AppResult<bool> {
    let kind = match state.config.lock().unwrap().ai_provider_kind() {
        Some(k) => k,
        None => return Ok(false),
    };
    Ok(KeyringSecretStore::new().has_api_key(&kind)?)
}

/// Round-trip the provider with a one-token embed request. Uses the config
/// already persisted in `AppState` unless the caller passes override values
/// (so the "Test" button in Settings can validate **unsaved** edits).
///
/// `api_key_override`:
/// - `None` → read from keyring under the (saved) provider kind
/// - `Some("")` → treat as anonymous (Ollama style; no Authorization header)
/// - `Some(s)` → use `s` for this one call only; **never written to disk**
#[tauri::command]
pub async fn ai_provider_test_connection(
    kind: Option<String>,
    base_url: Option<String>,
    embed_model: Option<String>,
    api_key_override: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<ProviderTestResult> {
    // Resolve effective config: explicit args > persisted > error.
    let (eff_kind, eff_base, eff_model) = {
        let cfg = state.config.lock().unwrap();
        let persisted = cfg.snapshot().ai_provider;
        let kind = kind
            .or_else(|| persisted.as_ref().map(|p| p.kind.clone()))
            .unwrap_or_default();
        let base = base_url
            .or_else(|| persisted.as_ref().map(|p| p.base_url.clone()))
            .unwrap_or_default();
        let model = embed_model
            .or_else(|| persisted.as_ref().map(|p| p.embed_model.clone()))
            .unwrap_or_default();
        (kind, base, model)
    };

    if eff_kind.trim().is_empty() {
        return Err(AppError::Other("no provider configured".into()));
    }
    if eff_base.trim().is_empty() {
        return Err(AppError::Other("base_url is empty".into()));
    }
    if eff_model.trim().is_empty() {
        return Err(AppError::Other("embed_model is empty".into()));
    }

    // api_key: explicit override (possibly empty for anonymous local backends),
    // or read from keyring under the kind key.
    let api_key = match api_key_override {
        Some(k) => k,
        None => KeyringSecretStore::new()
            .get_api_key(&eff_kind)?
            .unwrap_or_default(),
    };

    let provider =
        OpenAiProvider::new(eff_base, &eff_model, api_key).with_timeout(Duration::from_secs(10));

    let result = provider
        .embed(EmbedRequest {
            model: eff_model,
            inputs: vec!["hello".to_string()],
        })
        .await;

    Ok(match result {
        Ok(resp) => ProviderTestResult {
            ok: true,
            dim: resp.vectors.first().map(|v| v.len()),
            total_tokens: Some(resp.total_tokens),
            error_kind: None,
            error_message: None,
            retry_after_secs: None,
        },
        Err(e) => {
            let (kind, msg, retry_after_secs) = classify_provider_error(&e);
            ProviderTestResult {
                ok: false,
                dim: None,
                total_tokens: None,
                error_kind: Some(kind),
                error_message: Some(msg),
                retry_after_secs,
            }
        }
    })
}

/// Result of a `ai_provider_test_chat_connection` call. Mirrors
/// [`ProviderTestResult`] but reports a sampled reply instead of a vector
/// dimension so Settings can show "✓ 模型回复: …" on success.
#[derive(Debug, Clone, Serialize)]
pub struct ChatProviderTestResult {
    pub ok: bool,
    /// On success: the aggregated reply content. Truncated to 200 chars
    /// by the caller layer so huge answers don't bloat Settings state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// On success: tokens consumed, if the backend reported `usage`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
}

/// Ping the chat endpoint by running a one-token conversation
/// (`"reply with OK"`). Used by the Settings "测试聊天" button.
///
/// Parameter resolution follows the same rules as
/// [`ai_provider_test_connection`]: explicit args > persisted > error.
/// Short reply truncation is intentional — we only need a signal that
/// the streaming transport works end-to-end.
#[tauri::command]
pub async fn ai_provider_test_chat_connection(
    kind: Option<String>,
    base_url: Option<String>,
    chat_model: Option<String>,
    api_key_override: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<ChatProviderTestResult> {
    let (eff_kind, eff_base, eff_model) = {
        let cfg = state.config.lock().unwrap();
        let persisted = cfg.snapshot().ai_provider;
        let kind = kind
            .or_else(|| persisted.as_ref().map(|p| p.kind.clone()))
            .unwrap_or_default();
        let base = base_url
            .or_else(|| persisted.as_ref().map(|p| p.base_url.clone()))
            .unwrap_or_default();
        let model = chat_model
            .or_else(|| persisted.as_ref().map(|p| p.chat_model.clone()))
            .unwrap_or_default();
        (kind, base, model)
    };

    if eff_kind.trim().is_empty() {
        return Err(AppError::Other("no provider configured".into()));
    }
    if eff_base.trim().is_empty() {
        return Err(AppError::Other("base_url is empty".into()));
    }
    if eff_model.trim().is_empty() {
        return Err(AppError::Other("chat_model is empty".into()));
    }

    let api_key = match api_key_override {
        Some(k) => k,
        None => KeyringSecretStore::new()
            .get_api_key(&eff_kind)?
            .unwrap_or_default(),
    };

    let provider =
        OpenAiProvider::new(eff_base, &eff_model, api_key).with_timeout(Duration::from_secs(20));

    let req = ChatRequest {
        model: eff_model,
        messages: vec![
            ChatTurn::text(
                ChatRole::System,
                "You are a test. Reply only with the word OK.",
            ),
            ChatTurn::text(ChatRole::User, "Say OK."),
        ],
        temperature: Some(0.0),
        max_tokens: Some(8),
        tools: Vec::new(),
    };

    let outcome = match provider.chat_stream(req).await {
        Ok(stream) => collect_chat_stream(stream).await,
        Err(e) => Err(e),
    };

    Ok(match outcome {
        Ok(aggregated) => {
            let reply = aggregated.content.trim().to_string();
            let reply = if reply.len() > 200 {
                reply.chars().take(200).collect()
            } else {
                reply
            };
            ChatProviderTestResult {
                ok: true,
                reply: Some(reply),
                input_tokens: aggregated.input_tokens,
                output_tokens: aggregated.output_tokens,
                error_kind: None,
                error_message: None,
                retry_after_secs: None,
            }
        }
        Err(e) => {
            let (kind, msg, retry_after_secs) = classify_provider_error(&e);
            ChatProviderTestResult {
                ok: false,
                reply: None,
                input_tokens: None,
                output_tokens: None,
                error_kind: Some(kind),
                error_message: Some(msg),
                retry_after_secs,
            }
        }
    })
}

fn classify_provider_error(e: &ProviderError) -> (String, String, Option<u64>) {
    let (kind, message, retry_after_secs) = describe_provider_error(e);
    let kind = match kind {
        ProviderErrorKind::Network => "network",
        ProviderErrorKind::Auth => "auth",
        ProviderErrorKind::RateLimit => "rate_limit",
        ProviderErrorKind::InvalidRequest => "invalid_request",
        ProviderErrorKind::Other => "other",
    };
    (kind.into(), message, retry_after_secs)
}

// ── Embed-note commands (D2a.3a) ────────────────────────────────────────────

/// Resolve the active vault's path and embedding store in one shot, or
/// return a user-visible error explaining what's missing. Used by every
/// `ai_embed_*` command below.
fn require_vault_and_store(
    state: &State<AppState>,
) -> AppResult<(
    std::path::PathBuf,
    std::sync::Arc<std::sync::Mutex<crate::services::ai::embedding_store::EmbeddingStore>>,
)> {
    let vault = state
        .active_vault
        .lock()
        .unwrap()
        .clone()
        .ok_or(AppError::NoActiveVault)?;
    let store = state
        .embeddings_handle()
        .ok_or_else(|| AppError::Other("embedding store unavailable".into()))?;
    Ok((vault, store))
}

/// Build a live `OpenAiProvider` from the persisted AI provider config +
/// keychain entry. Returns a typed error when the user hasn't configured
/// a provider yet so callers can surface a "configure in Settings" prompt
/// instead of a mystery failure.
fn build_configured_provider(
    state: &State<AppState>,
) -> AppResult<(crate::services::ai::openai::OpenAiProvider, String)> {
    let cfg = state.config.lock().unwrap();
    runtime::build_configured_provider(&cfg, &KeyringSecretStore::new())
}

fn configured_embed_model(state: &State<AppState>) -> Option<String> {
    state
        .config
        .lock()
        .unwrap()
        .snapshot()
        .ai_provider
        .and_then(|provider| {
            let model = provider.embed_model.trim().to_string();
            if model.is_empty() {
                None
            } else {
                Some(model)
            }
        })
}

#[derive(Debug, Clone, Serialize)]
pub struct EmbedNoteResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<EmbedOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<EmbedFailure>,
}

/// Embed one note. Returns a structured result so the frontend can surface
/// "retry later / fix API key / model not found" guidance without parsing
/// arbitrary strings.
///
/// The `rel_path` is rejected on traversal (matches the rest of the
/// command layer's invariants). Doesn't touch `index.sqlite` — embedding
/// state is isolated.
#[tauri::command]
pub async fn ai_embed_note(
    rel_path: String,
    state: State<'_, AppState>,
) -> AppResult<EmbedNoteResult> {
    let rel = std::path::Path::new(&rel_path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::PathEscape(rel_path));
    }
    let (vault, store) = match require_vault_and_store(&state) {
        Ok(ok) => ok,
        Err(err) => {
            return Ok(EmbedNoteResult {
                ok: false,
                outcome: None,
                failure: Some(failure_from_app_error(&err)),
            });
        }
    };
    let (provider, model) = match build_configured_provider(&state) {
        Ok(ok) => ok,
        Err(err) => {
            return Ok(EmbedNoteResult {
                ok: false,
                outcome: None,
                failure: Some(failure_from_app_error(&err)),
            });
        }
    };
    match embed_service::embed_note(store, &provider, &model, &vault, &rel_path).await {
        Ok(outcome) => Ok(EmbedNoteResult {
            ok: true,
            outcome: Some(outcome),
            failure: None,
        }),
        Err(failure) => Ok(EmbedNoteResult {
            ok: false,
            outcome: None,
            failure: Some(failure),
        }),
    }
}

/// Return aggregate counters (chunks / notes / models) from the embedding
/// store. Returns zero-counters when no vault is open or no store yet;
/// this makes it safe to call unconditionally from the Settings UI.
#[tauri::command]
pub fn ai_embed_stats(state: State<AppState>) -> AppResult<EmbeddingStats> {
    let store = match state.embeddings_handle() {
        Some(s) => s,
        None => {
            return Ok(EmbeddingStats {
                chunk_count: 0,
                note_count: 0,
                model_count: 0,
            });
        }
    };
    let guard = store.lock().unwrap();
    guard.stats()
}

/// Delete every chunk that belongs to `rel_path`. Returns the number of
/// rows removed (0 if the note was never embedded). Also rejects path
/// traversal so callers can't wipe adjacent notes via `../` tricks.
#[tauri::command]
pub fn ai_embed_delete_note(rel_path: String, state: State<AppState>) -> AppResult<usize> {
    let rel = std::path::Path::new(&rel_path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::PathEscape(rel_path));
    }
    let store = state
        .embeddings_handle()
        .ok_or_else(|| AppError::Other("embedding store unavailable".into()))?;
    let g = store.lock().unwrap();
    g.delete_by_note(&rel_path)
}

/// Wipe **every** embedding from the store. Surfaced by the Settings
/// "清空 AI 索引" button — users expect a clean slate after "provider
/// changed" or "vault got reorganised". Returns the chunk count before
/// deletion so the UI can show `已清空 N chunks`.
#[tauri::command]
pub fn ai_embed_clear_all(state: State<AppState>) -> AppResult<u64> {
    let store = state
        .embeddings_handle()
        .ok_or_else(|| AppError::Other("embedding store unavailable".into()))?;
    let g = store.lock().unwrap();
    let before = g.stats()?.chunk_count;
    g.clear_all()?;
    Ok(before)
}

/// Dry-run a full-vault initialization run for the currently configured
/// embedding model. Walks every markdown note, chunks it, compares
/// model-scoped mtimes, and estimates chunk/token/cost impact without
/// calling the provider or mutating the store.
#[tauri::command]
pub fn ai_embed_vault_preview(state: State<AppState>) -> AppResult<VaultEmbedPreview> {
    let vault = state
        .active_vault
        .lock()
        .unwrap()
        .clone()
        .ok_or(AppError::NoActiveVault)?;
    let store = state
        .embeddings_handle()
        .ok_or_else(|| AppError::Other("embedding store unavailable".into()))?;
    let provider_cfg = state.config.lock().unwrap().snapshot().ai_provider;
    let guard = store.lock().unwrap();
    init_service::preview_vault_embed(&vault, &guard, provider_cfg.as_ref())
}

/// Embed every markdown note in the active vault for the currently saved
/// model. Per-note failures are collected into the returned summary so one
/// broken file or one transient provider error does not discard the progress
/// made on earlier notes.
#[tauri::command]
pub async fn ai_embed_vault_run(state: State<'_, AppState>) -> AppResult<VaultEmbedRunResult> {
    let (vault, store) = require_vault_and_store(&state)?;
    let (provider, model) = build_configured_provider(&state)?;
    init_service::embed_vault(store, &provider, &model, &vault).await
}

// ── Chat-session commands (D2b.1) ────────────────────────────────────────────
//
// Storage-only layer: these commands manipulate `<vault>/.mynotes/ai/chats/`.
// None of them call the AI provider — that arrives in D2b.2 (`ai_chat_stream`
// streaming endpoint) once the transport has a stable source-of-truth to
// persist deltas into. Keeping persistence in its own commit lets us add
// tests around session CRUD before the async streaming plumbing lands.

/// Resolve the active vault and hand back a `ChatStore` scoped to it.
/// Constructed fresh per call — `ChatStore` is cheap (a `PathBuf`) and
/// storing one in `AppState` would just invite stale-vault bugs on vault
/// switch.
fn chat_store(state: &State<AppState>) -> AppResult<ChatStore> {
    let vault = state
        .active_vault
        .lock()
        .unwrap()
        .clone()
        .ok_or(AppError::NoActiveVault)?;
    Ok(ChatStore::new(&vault))
}

/// List every chat session in the active vault, newest first. Message
/// counts and last-message timestamps are computed by streaming each
/// jsonl without copying message bodies into memory.
#[tauri::command]
pub fn ai_chat_session_list(state: State<AppState>) -> AppResult<Vec<ChatSessionSummary>> {
    chat_store(&state)?.list()
}

/// Create a new, empty chat session. The id is **generated by the
/// backend** — callers cannot pick their own id, which prevents hostile
/// frontends from aiming writes at arbitrary filenames.
#[tauri::command]
pub fn ai_chat_session_create(
    title: String,
    related_note: Option<String>,
    state: State<AppState>,
) -> AppResult<ChatSessionSummary> {
    // related_note, if given, must stay inside the vault. The store
    // doesn't resolve or read the file — it only persists the string —
    // but we still reject traversal to prevent persisting a poisoned
    // path that D2b.5 would later dereference.
    if let Some(p) = &related_note {
        let rp = std::path::Path::new(p);
        if rp.is_absolute()
            || rp
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(AppError::PathEscape(p.clone()));
        }
    }
    chat_store(&state)?.create(&title, related_note)
}

/// Read a session's full transcript. Returns an error when the session
/// doesn't exist or its first line is corrupt so the UI can offer a
/// "reset session" action instead of rendering garbage.
#[tauri::command]
pub fn ai_chat_session_load(
    session_id: String,
    state: State<AppState>,
) -> AppResult<ChatSessionFull> {
    chat_store(&state)?.load(&session_id)
}

/// Append one message to an existing session. Durability is enforced at
/// the store layer (`O_APPEND` + `sync_data`) so a crash mid-write loses
/// at most the in-flight turn.
#[tauri::command]
pub fn ai_chat_session_append(
    session_id: String,
    role: ChatRole,
    content: String,
    state: State<AppState>,
) -> AppResult<ChatMessage> {
    chat_store(&state)?.append(&session_id, role, &content)
}

/// Delete one session file. Idempotent: deleting an already-gone session
/// returns `false` rather than an error so the frontend can retry without
/// branching on "not found".
#[tauri::command]
pub fn ai_chat_session_delete(session_id: String, state: State<AppState>) -> AppResult<bool> {
    chat_store(&state)?.delete(&session_id)
}

// ── Non-streaming chat send (D2b.3) ─────────────────────────────────────────
//
// Synchronous one-shot send. The UI will be upgraded to a streaming
// `ai_chat_stream` endpoint in D2b.4 — this non-streaming variant exists so
// v1 of `ChatPanel.svelte` can ship a full "send message → persist user turn
// → wait for complete reply → persist assistant turn → render" loop without
// touching `emit_all` / cancel tokens.
//
// Ordering invariant: we append the user message **before** calling the
// provider. A provider-side failure therefore leaves the user turn on disk
// (and in the returned session history), so the user sees what they typed
// and can retry; the assistant turn is only appended on success. This
// matches what e.g. ChatGPT / Claude.ai do and avoids a dangling "was my
// message even sent?" UX.

/// Structured failure for a non-streaming chat send. Mirrors
/// [`EmbedFailure`] so the frontend has one shape to render AI-pipeline
/// failures against (banner + retry-after countdown + "configure
/// provider" CTA).
#[derive(Debug, Clone, Serialize)]
pub struct ChatSendFailure {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    /// Whether the caller's user turn was appended to the session before
    /// the failure. When `true`, the frontend should still show the user
    /// message in the transcript (so the "send" didn't "vanish"); when
    /// `false`, it was a pre-flight rejection (no vault / no provider).
    pub user_message_persisted: bool,
}

/// Structured response for [`ai_chat_send`]. `ok = true` carries the
/// persisted assistant [`ChatMessage`]; `ok = false` carries a
/// [`ChatSendFailure`]. Kept out-of-band from the Tauri `Result` channel
/// so the UI can render success + failure in the same message list without
/// branching at the IPC boundary.
#[derive(Debug, Clone, Serialize)]
pub struct ChatSendResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant: Option<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<ChatSendFailure>,
}

/// Convert a persisted [`ChatMessage`] to a provider-facing [`ChatTurn`].
/// Storage and transport share [`ChatRole`] (see D2b.2 §6.25.6), so this
/// is a simple field-by-field copy; the function exists to document the
/// direction and to have one edit point if we ever grow multi-modal
/// content.
fn message_to_turn(msg: &ChatMessage) -> ChatTurn {
    ChatTurn {
        role: msg.role,
        content: msg.content.clone(),
        tool_calls: msg.tool_calls.clone(),
        tool_call_id: msg.tool_call_id.clone(),
    }
}

/// Send one user message in the context of an existing session and wait
/// for the complete assistant reply (no streaming). Persists both turns.
///
/// Rejections the IPC layer short-circuits on (return `ok: false`,
/// `user_message_persisted: false`):
/// - no vault open,
/// - no provider configured / provider config incomplete (`chat_model`
///   empty → "chat disabled" — user has to set it in Settings first),
/// - session load fails (corrupt file / unknown id).
///
/// Failures after the user turn was persisted (return `ok: false`,
/// `user_message_persisted: true`):
/// - provider returned an error (network / auth / rate-limit / invalid
///   request / other) — the user can retry the send without re-typing.
#[tauri::command]
pub async fn ai_chat_send(
    session_id: String,
    content: String,
    state: State<'_, AppState>,
) -> AppResult<ChatSendResult> {
    let store = match chat_store(&state) {
        Ok(s) => s,
        Err(err) => {
            return Ok(ChatSendResult {
                ok: false,
                assistant: None,
                failure: Some(ChatSendFailure {
                    kind: "other".into(),
                    message: err.to_string(),
                    retry_after_secs: None,
                    user_message_persisted: false,
                }),
            });
        }
    };

    // Validate session exists + load historical turns BEFORE persisting
    // the new user message. Prevents "appended to a corrupt file then
    // the error surface is confusing" failure mode.
    let existing = match store.load(&session_id) {
        Ok(full) => full,
        Err(err) => {
            return Ok(ChatSendResult {
                ok: false,
                assistant: None,
                failure: Some(ChatSendFailure {
                    kind: "invalid_request".into(),
                    message: err.to_string(),
                    retry_after_secs: None,
                    user_message_persisted: false,
                }),
            });
        }
    };

    // Build a chat provider from the persisted config. Use a separate
    // helper from `build_configured_provider` because the user may have
    // `embed_model` set but `chat_model` empty (embeddings-only mode).
    let (provider, model) = {
        let cfg = state.config.lock().unwrap();
        match runtime::build_configured_chat_provider(&cfg, &KeyringSecretStore::new()) {
            Ok(ok) => ok,
            Err(err) => {
                return Ok(ChatSendResult {
                    ok: false,
                    assistant: None,
                    failure: Some(ChatSendFailure {
                        kind: "invalid_request".into(),
                        message: err.to_string(),
                        retry_after_secs: None,
                        user_message_persisted: false,
                    }),
                });
            }
        }
    };

    // Persist the user turn first — on provider failure we want the
    // user's input visible in the transcript so they can retry without
    // re-typing.
    let user_msg = match store.append(&session_id, ChatRole::User, &content) {
        Ok(m) => m,
        Err(err) => {
            return Ok(ChatSendResult {
                ok: false,
                assistant: None,
                failure: Some(ChatSendFailure {
                    kind: "other".into(),
                    message: err.to_string(),
                    retry_after_secs: None,
                    user_message_persisted: false,
                }),
            });
        }
    };

    // Assemble the full chat history for the provider.
    let mut messages: Vec<ChatTurn> = existing.messages.iter().map(message_to_turn).collect();
    messages.push(message_to_turn(&user_msg));

    let req = ChatRequest {
        model,
        messages,
        temperature: None,
        max_tokens: None,
        tools: Vec::new(),
    };

    let outcome = match provider.chat_stream(req).await {
        Ok(stream) => collect_chat_stream(stream).await,
        Err(e) => Err(e),
    };

    match outcome {
        Ok(aggregated) => {
            let reply = aggregated.content;
            if reply.trim().is_empty() {
                // Some providers return an empty reply on tool-call / stop
                // edge cases. Surface it as a failure rather than persist a
                // blank assistant turn that'd confuse the transcript.
                return Ok(ChatSendResult {
                    ok: false,
                    assistant: None,
                    failure: Some(ChatSendFailure {
                        kind: "other".into(),
                        message: "provider returned an empty reply".into(),
                        retry_after_secs: None,
                        user_message_persisted: true,
                    }),
                });
            }
            match store.append(&session_id, ChatRole::Assistant, &reply) {
                Ok(assistant_msg) => Ok(ChatSendResult {
                    ok: true,
                    assistant: Some(assistant_msg),
                    failure: None,
                }),
                Err(err) => Ok(ChatSendResult {
                    ok: false,
                    assistant: None,
                    failure: Some(ChatSendFailure {
                        kind: "other".into(),
                        message: err.to_string(),
                        retry_after_secs: None,
                        user_message_persisted: true,
                    }),
                }),
            }
        }
        Err(e) => {
            let (kind, msg, retry_after_secs) = classify_provider_error(&e);
            Ok(ChatSendResult {
                ok: false,
                assistant: None,
                failure: Some(ChatSendFailure {
                    kind,
                    message: msg,
                    retry_after_secs,
                    user_message_persisted: true,
                }),
            })
        }
    }
}

// ── Streaming chat (D2b.4) ──────────────────────────────────────────────────
//
// `ai_chat_stream_start` upgrades the non-streaming `ai_chat_send` from
// D2b.3 to per-token streaming. The control flow splits in two:
//
// 1. **Synchronous pre-flight** inside the command body: validate
//    `stream_id`, load session, build chat provider, truncate history,
//    **persist user turn**, register cancel token. All failures here
//    surface via the IPC `Result` so the frontend can show a banner
//    without first registering event listeners.
// 2. **Async streaming loop** in a spawned task: pull deltas from the
//    provider, emit them as Tauri events, and finally persist the
//    assistant turn. Failures here surface via events (since the command
//    has already returned by the time they happen).
//
// Three event names, documented wire-side so both the main window and
// a future D2b.6 standalone window listen to the same channel:
//
// - `ai:chat-stream:delta`      — one content fragment (or finish_reason flag)
// - `ai:chat-stream:done`       — stream finished, assistant message persisted
// - `ai:chat-stream:error`      — provider error or persist error mid-stream
//
// Every event carries `stream_id` so a panel that has N concurrent
// streams (future multi-window / multi-session pre-render cases) can
// dispatch to the right UI state. v1 the frontend only ever runs one
// stream at a time, but the wire shape is already multi-capable.

/// Approximate characters-per-token ratio used for the history budget.
/// English averages 4 c/t, code and CJK closer to 2. We pick 3.5 as a
/// pragmatic middle ground — cheap, no tokenizer crate, good enough
/// to keep us off the context-length edge for gpt-4o-mini / Qwen / Llama.
const CHARS_PER_TOKEN: f64 = 3.5;

/// Default history token budget. Chosen to fit comfortably in an 8k
/// context window while leaving room for the `max_tokens` cap on the
/// reply itself. Users can tune this later; for v1 it's a const.
const DEFAULT_HISTORY_TOKEN_BUDGET: usize = 4_000;

/// Hard cap on model ↔ tool loop iterations inside one
/// `ai_chat_stream_start` invocation (P3-D5.1). Each iteration is one
/// full `provider.chat_stream()` call; the loop exits early whenever
/// the model finishes with `stop` / `length` / `content_filter`.
/// Hitting the cap emits an `ai:chat-stream:error` with
/// `code = MAX_TOOL_ITERATIONS_EXCEEDED` so the frontend can surface
/// a "too many tool round-trips" banner.
const MAX_TOOL_ITERATIONS: u32 = 8;

/// Shape of the `ai:chat-stream:delta` event payload. One per content
/// fragment the provider yields; the content string can be empty when
/// the provider is only signalling `finish_reason`.
#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamDeltaEvent {
    pub stream_id: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Shape of the `ai:chat-stream:done` event payload. Emitted once the
/// stream terminates successfully; `cancelled` is true when the user
/// hit the cancel button (the partial assistant turn is still persisted
/// so the user can inspect / re-use what came through before the cancel).
#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamDoneEvent {
    pub stream_id: String,
    pub assistant: ChatMessage,
    pub cancelled: bool,
}

/// Shape of the `ai:chat-stream:error` event payload. Mirrors
/// [`ChatSendFailure`] so the frontend reuses the D2b.3 banner
/// renderer without another type-narrowing branch.
#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamErrorEvent {
    pub stream_id: String,
    pub failure: ChatSendFailure,
}

/// Shape of the `ai:chat-stream:tool_call_requested` event payload
/// (P3-D5.1). Fired once per tool call the model requested, **before**
/// the registry actually executes the tool. Carries the raw arguments
/// string as emitted by the provider — rendering is the frontend's
/// concern.
#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamToolCallRequestedEvent {
    pub stream_id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

/// Shape of the `ai:chat-stream:tool_call_result` event payload
/// (P3-D5.1). Fired once per tool call, **after** the registry returns
/// its [`super::services::ai::provider::ToolResult`]. `is_error = true`
/// means the tool reported a failure — the model still sees the content
/// and is expected to recover gracefully on the next turn.
#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamToolCallResultEvent {
    pub stream_id: String,
    pub call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// Return shape of `ai_chat_stream_start`. Pre-flight failure (no
/// vault, no provider, invalid session, duplicate stream_id) comes
/// back in `failure`; on `ok: true` the caller waits for terminal
/// events on the three event channels above.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ChatStreamStartResult {
    pub ok: bool,
    /// Present on success: the just-persisted user turn, same id the
    /// session file now has on disk. The frontend swaps its optimistic
    /// bubble for this authoritative id before waiting for deltas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<ChatMessage>,
    /// RAG citations used for this turn, in descending relevance order.
    /// Empty when no embeddings configured or retrieval returned zero
    /// matches. The backend prepends a synthetic system message to the
    /// provider request for each citation; the frontend uses this list
    /// to render a "Sources" footer on the assistant bubble.
    #[serde(default)]
    pub citations: Vec<crate::services::ai::rag::RagCitation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<ChatSendFailure>,
}

/// Best-effort RAG retrieval for a single user turn. Returns `None`
/// on any short-circuit (no embed provider / empty store / failed
/// embed call / zero hits). Never propagates errors — see module-level
/// rationale in `services::ai::rag`.
///
/// Split into three phases to keep the `EmbeddingStore` mutex out of
/// the `await` critical section (holding a `std::sync::MutexGuard`
/// across `.await` makes the spawned task non-`Send`):
///
///  1. acquire `Arc<Mutex<EmbeddingStore>>` (sync, brief).
///  2. build embed provider (sync, reads config).
///  3. embed the user query (async, **no lock held**).
///  4. re-acquire store lock, run top-K search synchronously, drop lock.
async fn try_build_rag_context(
    state: &tauri::State<'_, AppState>,
    query: &str,
) -> Option<crate::services::ai::rag::RagContext> {
    use crate::services::ai::rag::{embed_query, search_and_format, DEFAULT_TOP_K};

    let store_arc = state.embeddings.lock().unwrap().clone()?;

    let (provider, embed_model) = {
        let cfg = state.config.lock().unwrap();
        runtime::build_configured_provider(&cfg, &KeyringSecretStore::new()).ok()?
    };

    let query_vec = embed_query(query, &provider, &embed_model).await?;

    let store_guard = store_arc.lock().unwrap();
    search_and_format(&query_vec, &embed_model, &store_guard, DEFAULT_TOP_K)
}

fn append_usage_log_best_effort(vault_root: Option<&Path>, value: &impl Serialize) {
    let Some(vault_root) = vault_root else {
        return;
    };
    if let Err(err) = crate::services::ai::usage_log::append_usage_log(vault_root, value) {
        tracing::warn!(error = %err, "failed to append ai usage log");
    }
}

const RECENT_TOOL_CONTEXT_ROUNDS: usize = 2;

fn compact_history_tool_context(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let preserved_assistant_ids: HashSet<String> = messages
        .iter()
        .filter(|msg| msg.role == ChatRole::Assistant && msg.tool_calls.as_ref().is_some_and(|c| !c.is_empty()))
        .rev()
        .take(RECENT_TOOL_CONTEXT_ROUNDS)
        .map(|msg| msg.id.clone())
        .collect();

    let preserved_call_ids: HashSet<String> = messages
        .iter()
        .filter(|msg| preserved_assistant_ids.contains(&msg.id))
        .flat_map(|msg| {
            msg.tool_calls
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(|call| call.id)
                .collect::<Vec<_>>()
        })
        .collect();

    let mut out = Vec::with_capacity(messages.len());
    for msg in messages {
        match msg.role {
            ChatRole::Tool => {
                if msg
                    .tool_call_id
                    .as_ref()
                    .is_some_and(|id| preserved_call_ids.contains(id))
                {
                    out.push(msg.clone());
                }
            }
            ChatRole::Assistant
                if msg.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty())
                    && !preserved_assistant_ids.contains(&msg.id) =>
            {
                let mut stripped = msg.clone();
                stripped.tool_calls = None;
                out.push(stripped);
            }
            _ => out.push(msg.clone()),
        }
    }
    out
}

fn build_message_units(messages: &[ChatMessage]) -> Vec<Vec<ChatMessage>> {
    let mut units = Vec::new();
    let mut i = 0usize;
    while i < messages.len() {
        let msg = &messages[i];
        if msg.role == ChatRole::Assistant
            && msg.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty())
        {
            let call_ids: HashSet<&str> = msg
                .tool_calls
                .as_ref()
                .into_iter()
                .flatten()
                .map(|call| call.id.as_str())
                .collect();
            let mut unit = vec![msg.clone()];
            i += 1;
            while i < messages.len()
                && messages[i].role == ChatRole::Tool
                && messages[i]
                    .tool_call_id
                    .as_deref()
                    .is_some_and(|id| call_ids.contains(id))
            {
                unit.push(messages[i].clone());
                i += 1;
            }
            units.push(unit);
            continue;
        }
        units.push(vec![msg.clone()]);
        i += 1;
    }
    units
}

/// Truncate a chat history so the aggregate character count stays
/// under `max_chars`. Always keeps a leading system message (when
/// present) + the newest turns; drops the oldest user / assistant
/// pairs in between. Used to keep streaming requests off the
/// provider's context-length rejection path without asking the user
/// to manage history manually.
/// Length of one message's `content` plus each tool-call's `arguments`
/// payload. P3-D5.1: tool-calling messages carry most of their useful
/// context in `tool_calls[].arguments` (often a JSON blob many kB
/// long) — counting only `content.len()` would let a history with a
/// 50 kB `arguments` blob slip under a 4 kB budget and then explode
/// the provider request.
fn message_weight_chars(msg: &ChatMessage) -> usize {
    let mut w = msg.content.len();
    if let Some(calls) = msg.tool_calls.as_ref() {
        for c in calls {
            w = w.saturating_add(c.arguments.len());
        }
    }
    w
}

/// Truncate chat history to `max_chars` of cumulative content while
/// **never splitting an Assistant-with-tool_calls turn from its matching
/// Tool replies**. A `role = Tool` message that lands in the kept
/// window but whose parent Assistant turn got dropped would confuse the
/// provider (Tool messages only make sense when the immediately
/// preceding context explains which `tool_call_id` they answer).
///
/// Algorithm:
/// 1. Pop a leading `System` prefix (always kept — it's the RAG /
///    persona seed).
/// 2. Walk the remainder newest → oldest. Keep a running `selected_rev`
///    plus its cumulative char count, bail on the first message that
///    would push past the budget.
/// 3. Before returning, "heal" the boundary: if the oldest kept
///    message is `role = Tool`, walk forward and drop it (and any
///    further leading Tool messages) until we either hit the matching
///    Assistant-with-tool_calls or run out. This preserves the
///    property "every kept Tool has a visible parent".
///
/// The function retains the pre-D5.1 name + signature so the callers at
/// `ai_chat_stream_start` and the two old tests can still use it.
fn truncate_history_to_budget(messages: &[ChatMessage], max_chars: usize) -> Vec<ChatTurn> {
    let compacted = compact_history_tool_context(messages);
    let system_prefix_len = compacted
        .iter()
        .take_while(|msg| msg.role == ChatRole::System)
        .count();
    let (system_prefix, rest): (&[ChatMessage], &[ChatMessage]) =
        compacted.split_at(system_prefix_len);
    let prefix_chars: usize = system_prefix.iter().map(message_weight_chars).sum();
    let units = build_message_units(rest);
    let mut selected_rev: Vec<&Vec<ChatMessage>> = Vec::new();
    let mut running: usize = 0;
    for unit in units.iter().rev() {
        let w: usize = unit.iter().map(message_weight_chars).sum();
        let next_total = prefix_chars
            .saturating_add(running)
            .saturating_add(w);
        if next_total > max_chars && !selected_rev.is_empty() {
            break;
        }
        selected_rev.push(unit);
        running = running.saturating_add(w);
        if w >= max_chars && selected_rev.len() == 1 {
            // One giant final message — keep it and bail; provider
            // will reject, we'll surface via the error channel.
            break;
        }
    }
    selected_rev.reverse();
    let mut selected: Vec<&ChatMessage> = Vec::new();
    for unit in selected_rev {
        for msg in unit {
            selected.push(msg);
        }
    }
    // Heal the leading boundary: drop any orphaned Tool turns that
    // lost their parent Assistant-with-tool_calls to truncation. Walk
    // forward from the start of `selected` until we hit either a
    // non-Tool message or exhaust the slice.
    while let Some(first) = selected.first() {
        if first.role == ChatRole::Tool {
            selected.remove(0);
        } else {
            break;
        }
    }

    let mut out = Vec::with_capacity(system_prefix.len() + selected.len());
    out.extend(system_prefix.iter().map(message_to_turn));
    out.extend(selected.into_iter().map(message_to_turn));
    out
}

/// Kick off a streaming chat send. Persists the user turn synchronously
/// so the caller's optimistic bubble can be replaced with an authoritative
/// `ChatMessage` immediately; the assistant turn lands asynchronously via
/// the `ai:chat-stream:*` events and the corresponding store append.
///
/// `stream_id` is caller-generated (nanoid / uuid); the command rejects
/// collisions so event listeners are never ambiguous about which stream
/// owns a given delta.
#[tauri::command]
pub async fn ai_chat_stream_start(
    stream_id: String,
    session_id: String,
    content: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> AppResult<ChatStreamStartResult> {
    use std::sync::atomic::Ordering;

    // Empty / obviously invalid stream ids get rejected up front.
    if stream_id.trim().is_empty() || stream_id.len() > 128 {
        return Ok(ChatStreamStartResult {
            ok: false,
            user_message: None,
            citations: Vec::new(),
            failure: Some(ChatSendFailure {
                kind: "invalid_request".into(),
                message: "invalid stream_id".into(),
                retry_after_secs: None,
                user_message_persisted: false,
            }),
        });
    }

    // Duplicate-id guard: if the frontend somehow fires the same id
    // twice, we reject the second call so the cancel channel can't
    // accidentally kill the first.
    {
        let guard = state.chat_streams.lock().unwrap();
        if guard.contains_key(&stream_id) {
            return Ok(ChatStreamStartResult {
                ok: false,
                user_message: None,
                citations: Vec::new(),
                failure: Some(ChatSendFailure {
                    kind: "invalid_request".into(),
                    message: "stream_id already in use".into(),
                    retry_after_secs: None,
                    user_message_persisted: false,
                }),
            });
        }
    }

    // Pre-flight: vault + session + provider. Each failure here surfaces
    // via the Result so the frontend can show a banner without needing
    // to wait for an event.
    let store = match chat_store(&state) {
        Ok(s) => s,
        Err(err) => {
            return Ok(ChatStreamStartResult {
                ok: false,
                user_message: None,
                citations: Vec::new(),
                failure: Some(ChatSendFailure {
                    kind: "other".into(),
                    message: err.to_string(),
                    retry_after_secs: None,
                    user_message_persisted: false,
                }),
            });
        }
    };

    let existing = match store.load(&session_id) {
        Ok(full) => full,
        Err(err) => {
            return Ok(ChatStreamStartResult {
                ok: false,
                user_message: None,
                citations: Vec::new(),
                failure: Some(ChatSendFailure {
                    kind: "invalid_request".into(),
                    message: err.to_string(),
                    retry_after_secs: None,
                    user_message_persisted: false,
                }),
            });
        }
    };

    let tool_vault_root = state.active_vault.lock().unwrap().clone();
    let (provider, model, tool_permissions) = {
        let cfg = state.config.lock().unwrap();
        let permissions = cfg.snapshot().ai_tool_permissions.clone();
        match runtime::build_configured_chat_provider(&cfg, &KeyringSecretStore::new()) {
            Ok((provider, model)) => (Arc::new(provider) as Arc<dyn AiProvider>, model, permissions),
            Err(err) => {
                return Ok(ChatStreamStartResult {
                    ok: false,
                    user_message: None,
                    citations: Vec::new(),
                    failure: Some(ChatSendFailure {
                        kind: "invalid_request".into(),
                        message: err.to_string(),
                        retry_after_secs: None,
                        user_message_persisted: false,
                    }),
                });
            }
        }
    };

    let user_msg = match store.append(&session_id, ChatRole::User, &content) {
        Ok(m) => m,
        Err(err) => {
            return Ok(ChatStreamStartResult {
                ok: false,
                user_message: None,
                citations: Vec::new(),
                failure: Some(ChatSendFailure {
                    kind: "other".into(),
                    message: err.to_string(),
                    retry_after_secs: None,
                    user_message_persisted: false,
                }),
            });
        }
    };

    // RAG pass (D2b.5). Best-effort: if embeddings aren't configured or
    // retrieval fails for any reason, we silently proceed with no
    // context. The user-visible effect of "no RAG available" should
    // be "answers that don't quote my notes" — never "chat broken".
    let rag_ctx = try_build_rag_context(&state, &content).await;
    let citations = rag_ctx
        .as_ref()
        .map(|c| c.citations.clone())
        .unwrap_or_default();

    let tool_defs = state.tool_registry.definitions_filtered(&tool_permissions);
    let vault_name = tool_vault_root
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "vault".into());
    let agent_system_prompt = crate::services::ai::system_prompt::build_agent_system_prompt(
        &vault_name,
        existing.meta.related_note.as_deref(),
        &tool_defs,
    );

    // Compose the message array: agent system turn + optional RAG system turn + full
    // existing history + new user turn, then truncate to the char
    // budget. `truncate_history_to_budget` already preserves a leading
    // system-message prefix, so prepending one or two here is transparent to it.
    let max_chars = ((DEFAULT_HISTORY_TOKEN_BUDGET as f64) * CHARS_PER_TOKEN) as usize;
    let mut full_messages: Vec<ChatMessage> = Vec::new();
    full_messages.push(ChatMessage {
        v: chat_store::SCHEMA_VERSION,
        id: "agent-system".into(),
        role: ChatRole::System,
        content: agent_system_prompt,
        created_at: user_msg.created_at,
        tool_calls: None,
        tool_call_id: None,
    });
    if let Some(ref ctx) = rag_ctx {
        full_messages.push(ChatMessage {
            v: chat_store::SCHEMA_VERSION,
            id: "rag-system".into(),
            role: ChatRole::System,
            content: ctx.system_turn.content.clone(),
            created_at: user_msg.created_at,
            tool_calls: None,
            tool_call_id: None,
        });
    }
    full_messages.extend(existing.messages.iter().cloned());
    full_messages.push(user_msg.clone());
    let turns = truncate_history_to_budget(&full_messages, max_chars);

    // Register cancel flag.
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut guard = state.chat_streams.lock().unwrap();
        guard.insert(stream_id.clone(), cancel.clone());
    }

    // Snapshot the bits the spawn task needs. No borrows into `state`
    // survive past this point so the task is trivially `'static`.
    let streams_registry = state.chat_streams.clone();
    let tool_registry = state.tool_registry.clone();
    let app_handle = app.clone();
    let store_for_task = store;
    let stream_id_task = stream_id.clone();
    let session_id_task = session_id.clone();
    let model_task = model;
    let initial_turns = turns;
    let tool_permissions_task = tool_permissions.clone();
    let provider_for_tools = provider.clone();
    // Handles the tools need at call time (P3-D5.2 / D5.4). Captured
    // now, before `spawn`, so the task doesn't borrow into `state`.
    // Each is `Option<_>` because the dependency may be absent (no
    // vault, no embeddings, no embed model) — the tool honours that
    // by returning a structured "missing prerequisite" error.
    let tool_index = state.index_handle();
    let tool_embeddings = state.embeddings_handle();
    let tool_embed_model = configured_embed_model(&state);

    tauri::async_runtime::spawn(async move {
        use futures_util::StreamExt;
        use tauri::Emitter;

        let cleanup = || {
            let mut guard = streams_registry.lock().unwrap();
            guard.remove(&stream_id_task);
        };
        append_usage_log_best_effort(
            tool_vault_root.as_deref(),
            &json!({
                "ts": chrono::Utc::now().timestamp(),
                "kind": "chat_stream_start",
                "stream_id": stream_id_task.clone(),
                "session_id": session_id_task.clone(),
                "tool_count": tool_defs.len()
            }),
        );

        // Working transcript for the multi-turn loop. Starts with the
        // truncated history + new user turn; each iteration may append
        // one Assistant-with-tool_calls + N Tool turns before looping
        // again.
        let mut loop_messages: Vec<ChatTurn> = initial_turns;

        // Assistant text for *this* iteration only. Reset per-turn so a
        // tool round-trip never accidentally leaks text from the first
        // turn into the second.
        let mut assistant_text = String::new();
        let mut cancelled_flag = false;
        let mut done_emitted = false;

        'outer: for iter in 0..MAX_TOOL_ITERATIONS {
            // Cancel check at the top of each iteration so cancels
            // between tool execution and the next provider call are
            // observed before we burn another round-trip. This path
            // only triggers AFTER at least one previous turn finished
            // (so the transcript already has a persisted Assistant or
            // Tool reply); surface a "cancelled between turns" error
            // so the UI drops the spinner.
            if cancel.load(Ordering::Relaxed) {
                let _ = app_handle.emit(
                    "ai:chat-stream:error",
                    ChatStreamErrorEvent {
                        stream_id: stream_id_task.clone(),
                        failure: ChatSendFailure {
                            kind: "other".into(),
                            message: "cancelled between tool iterations".into(),
                            retry_after_secs: None,
                            user_message_persisted: true,
                        },
                    },
                );
                // Mark so the fallthrough MAX_TOOL_ITERATIONS branch
                // doesn't also fire after this emit.
                done_emitted = true;
                break 'outer;
            }

            let req = ChatRequest {
                model: model_task.clone(),
                messages: loop_messages.clone(),
                temperature: None,
                max_tokens: None,
                tools: tool_defs.clone(),
            };

            let mut stream = match provider.chat_stream(req).await {
                Ok(s) => s,
                Err(e) => {
                    let (kind, msg, retry) = classify_provider_error(&e);
                    let _ = app_handle.emit(
                        "ai:chat-stream:error",
                        ChatStreamErrorEvent {
                            stream_id: stream_id_task.clone(),
                            failure: ChatSendFailure {
                                kind,
                                message: msg,
                                retry_after_secs: retry,
                                user_message_persisted: true,
                            },
                        },
                    );
                    cleanup();
                    return;
                }
            };

            assistant_text.clear();
            let mut accumulator = ToolCallAccumulator::new();
            // Track the *last* non-null finish_reason the provider
            // emits — some OpenAI-compatible endpoints fire an interim
            // `null` before the real terminator, so "latest wins" is
            // the right interpretation.
            let mut finish_reason: Option<String> = None;
            let mut error_seen: Option<ProviderError> = None;

            while let Some(item) = stream.next().await {
                if cancel.load(Ordering::Relaxed) {
                    cancelled_flag = true;
                    break;
                }
                match item {
                    Ok(delta) => {
                        if !delta.content.is_empty() || delta.finish_reason.is_some() {
                            append_usage_log_best_effort(
                                tool_vault_root.as_deref(),
                                &json!({
                                    "ts": chrono::Utc::now().timestamp(),
                                    "kind": "chat_delta",
                                    "stream_id": stream_id_task.clone(),
                                    "session_id": session_id_task.clone(),
                                    "content": delta.content.clone(),
                                    "finish_reason": delta.finish_reason.clone(),
                                    "input_tokens": delta.input_tokens,
                                    "output_tokens": delta.output_tokens
                                }),
                            );
                        }
                        if !delta.content.is_empty() {
                            assistant_text.push_str(&delta.content);
                        }
                        if let Some(frags) = delta.tool_call_fragments.as_ref() {
                            accumulator.ingest(frags);
                        }
                        let _ = app_handle.emit(
                            "ai:chat-stream:delta",
                            ChatStreamDeltaEvent {
                                stream_id: stream_id_task.clone(),
                                content: delta.content,
                                finish_reason: delta.finish_reason.clone(),
                            },
                        );
                        if let Some(r) = delta.finish_reason {
                            finish_reason = Some(r);
                        }
                    }
                    Err(e) => {
                        error_seen = Some(e);
                        break;
                    }
                }
            }

            // Late cancel reconciliation (same race as D2b.4).
            if cancel.load(Ordering::Relaxed) {
                cancelled_flag = true;
            }

            if let Some(e) = error_seen {
                let (kind, msg, retry) = classify_provider_error(&e);
                let _ = app_handle.emit(
                    "ai:chat-stream:error",
                    ChatStreamErrorEvent {
                        stream_id: stream_id_task.clone(),
                        failure: ChatSendFailure {
                            kind,
                            message: msg,
                            retry_after_secs: retry,
                            user_message_persisted: true,
                        },
                    },
                );
                cleanup();
                return;
            }

            // A cancel signal during a tool-calling turn skips tool
            // execution but still persists the Assistant-with-tool_calls
            // message if the model produced one — otherwise the next
            // turn would see an orphaned Tool in its context. The
            // `done` event fires with `cancelled = true` so the UI
            // clears its spinner.
            match finish_reason.as_deref() {
                Some("tool_calls") => {
                    let calls = accumulator.finish();
                    if calls.is_empty() {
                        // Malformed provider output: finish_reason was
                        // tool_calls but no fragments survived. Surface
                        // as an error rather than loop forever.
                        let _ = app_handle.emit(
                            "ai:chat-stream:error",
                            ChatStreamErrorEvent {
                                stream_id: stream_id_task.clone(),
                                failure: ChatSendFailure {
                                    kind: "other".into(),
                                    message:
                                        "provider emitted finish_reason=tool_calls with no complete calls"
                                            .into(),
                                    retry_after_secs: None,
                                    user_message_persisted: true,
                                },
                            },
                        );
                        cleanup();
                        return;
                    }

                    // **Persist-before-execute**: on cancel mid-tool
                    // we never want a tool-result without its parent
                    // Assistant on disk, so the Assistant-with-tool_calls
                    // gets appended first. `content` is whatever text
                    // the model emitted alongside the tool call (often
                    // empty but some models reason in the open).
                    let assistant_msg = match store_for_task.append_rich(
                        &session_id_task,
                        ChatRole::Assistant,
                        assistant_text.trim(),
                        Some(calls.clone()),
                        None,
                    ) {
                        Ok(m) => m,
                        Err(err) => {
                            let _ = app_handle.emit(
                                "ai:chat-stream:error",
                                ChatStreamErrorEvent {
                                    stream_id: stream_id_task.clone(),
                                    failure: ChatSendFailure {
                                        kind: "other".into(),
                                        message: err.to_string(),
                                        retry_after_secs: None,
                                        user_message_persisted: true,
                                    },
                                },
                            );
                            cleanup();
                            return;
                        }
                    };

                    // Append into the loop transcript so the next turn
                    // sees the tool_calls context.
                    loop_messages.push(message_to_turn(&assistant_msg));

                    // If cancel flipped during streaming, do NOT execute
                    // tools and do NOT loop — emit `done` with partial
                    // assistant turn so the UI drops its spinner.
                    if cancelled_flag {
                        let _ = app_handle.emit(
                            "ai:chat-stream:done",
                            ChatStreamDoneEvent {
                                stream_id: stream_id_task.clone(),
                                assistant: assistant_msg,
                                cancelled: true,
                            },
                        );
                        done_emitted = true;
                        break 'outer;
                    }

                    // Execute each tool call in order. Cancellation
                    // between calls aborts remaining calls — the tool
                    // itself may also honour the `cancel` flag mid-
                    // execution if it wants fine-grained granularity.
                    for call in &calls {
                        if cancel.load(Ordering::Relaxed) {
                            cancelled_flag = true;
                            break;
                        }
                        // Parse arguments defensively — malformed JSON
                        // is reported to the tool (as an empty object)
                        // and the registry's `execute` decides how to
                        // react. We still emit `tool_call_requested`
                        // with the raw string so the UI can show what
                        // the model actually tried.
                        let _ = app_handle.emit(
                            "ai:chat-stream:tool_call_requested",
                            ChatStreamToolCallRequestedEvent {
                                stream_id: stream_id_task.clone(),
                                call_id: call.id.clone(),
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            },
                        );
                        append_usage_log_best_effort(
                            tool_vault_root.as_deref(),
                            &json!({
                                "ts": chrono::Utc::now().timestamp(),
                                "kind": "tool_call_requested",
                                "stream_id": stream_id_task.clone(),
                                "session_id": session_id_task.clone(),
                                "tool_call_id": call.id.clone(),
                                "tool_name": call.name.clone(),
                                "arguments": call.arguments.clone()
                            }),
                        );

                        let parsed_args: serde_json::Value =
                            serde_json::from_str(&call.arguments)
                                .unwrap_or(serde_json::Value::Object(Default::default()));

                        let ctx = ToolContext {
                            vault_root: tool_vault_root.clone(),
                            index: tool_index.clone(),
                            embeddings: tool_embeddings.clone(),
                            embed_model: tool_embed_model.clone(),
                            provider: Some(provider_for_tools.clone()),
                            chat_model: Some(model_task.clone()),
                            tool_permissions: tool_permissions_task.clone(),
                            cancel: cancel.clone(),
                        };
                        let result = tool_registry
                            .execute(
                                &call.name,
                                call.id.clone(),
                                parsed_args,
                                &ctx,
                            )
                            .await;

                        let _ = app_handle.emit(
                            "ai:chat-stream:tool_call_result",
                            ChatStreamToolCallResultEvent {
                                stream_id: stream_id_task.clone(),
                                call_id: result.tool_call_id.clone(),
                                content: result.content.clone(),
                                is_error: result.is_error,
                            },
                        );
                        append_usage_log_best_effort(
                            tool_vault_root.as_deref(),
                            &json!({
                                "ts": chrono::Utc::now().timestamp(),
                                "kind": "tool_call_result",
                                "stream_id": stream_id_task.clone(),
                                "session_id": session_id_task.clone(),
                                "tool_call_id": result.tool_call_id.clone(),
                                "tool_name": call.name.clone(),
                                "is_error": result.is_error,
                                "content": result.content.clone()
                            }),
                        );

                        let tool_msg = match store_for_task.append_rich(
                            &session_id_task,
                            ChatRole::Tool,
                            &result.content,
                            None,
                            Some(result.tool_call_id.clone()),
                        ) {
                            Ok(m) => m,
                            Err(err) => {
                                let _ = app_handle.emit(
                                    "ai:chat-stream:error",
                                    ChatStreamErrorEvent {
                                        stream_id: stream_id_task.clone(),
                                        failure: ChatSendFailure {
                                            kind: "other".into(),
                                            message: err.to_string(),
                                            retry_after_secs: None,
                                            user_message_persisted: true,
                                        },
                                    },
                                );
                                cleanup();
                                return;
                            }
                        };
                        loop_messages.push(message_to_turn(&tool_msg));
                    }

                    if cancelled_flag {
                        // Partial tool execution cancelled — no more
                        // iterations. We don't emit `done` here because
                        // no final assistant text was produced; surface
                        // as a "cancelled" error so the UI knows to
                        // close the spinner.
                        let _ = app_handle.emit(
                            "ai:chat-stream:error",
                            ChatStreamErrorEvent {
                                stream_id: stream_id_task.clone(),
                                failure: ChatSendFailure {
                                    kind: "other".into(),
                                    message: "cancelled during tool execution".into(),
                                    retry_after_secs: None,
                                    user_message_persisted: true,
                                },
                            },
                        );
                        cleanup();
                        return;
                    }

                    // Keep looping — next iteration replays with the
                    // new Assistant + Tool[] context appended.
                    let _ = iter; // silence unused-var in release builds
                    continue 'outer;
                }
                Some(_other) => {
                    // "stop" / "length" / "content_filter" — model is
                    // done, persist its text (if any) and emit `done`.
                    let trimmed = assistant_text.trim().to_string();
                    if trimmed.is_empty() {
                        let _ = app_handle.emit(
                            "ai:chat-stream:error",
                            ChatStreamErrorEvent {
                                stream_id: stream_id_task.clone(),
                                failure: ChatSendFailure {
                                    kind: "other".into(),
                                    message: if cancelled_flag {
                                        "cancelled before any content arrived".into()
                                    } else {
                                        "provider returned an empty reply".into()
                                    },
                                    retry_after_secs: None,
                                    user_message_persisted: true,
                                },
                            },
                        );
                        cleanup();
                        return;
                    }
                    match store_for_task.append(
                        &session_id_task,
                        ChatRole::Assistant,
                        &trimmed,
                    ) {
                        Ok(assistant_msg) => {
                            append_usage_log_best_effort(
                                tool_vault_root.as_deref(),
                                &json!({
                                    "ts": chrono::Utc::now().timestamp(),
                                    "kind": "assistant_done",
                                    "stream_id": stream_id_task.clone(),
                                    "session_id": session_id_task.clone(),
                                    "assistant_message_id": assistant_msg.id.clone(),
                                    "cancelled": cancelled_flag,
                                    "content": trimmed
                                }),
                            );
                            let _ = app_handle.emit(
                                "ai:chat-stream:done",
                                ChatStreamDoneEvent {
                                    stream_id: stream_id_task.clone(),
                                    assistant: assistant_msg,
                                    cancelled: cancelled_flag,
                                },
                            );
                            done_emitted = true;
                        }
                        Err(err) => {
                            let _ = app_handle.emit(
                                "ai:chat-stream:error",
                                ChatStreamErrorEvent {
                                    stream_id: stream_id_task.clone(),
                                    failure: ChatSendFailure {
                                        kind: "other".into(),
                                        message: err.to_string(),
                                        retry_after_secs: None,
                                        user_message_persisted: true,
                                    },
                                },
                            );
                        }
                    }
                    break 'outer;
                }
                None => {
                    // Stream ended without a finish_reason — rare but
                    // observed when providers close the HTTP stream
                    // on keep-alive timeout. Treat as error.
                    let _ = app_handle.emit(
                        "ai:chat-stream:error",
                        ChatStreamErrorEvent {
                            stream_id: stream_id_task.clone(),
                            failure: ChatSendFailure {
                                kind: "other".into(),
                                message: "provider stream ended without finish_reason".into(),
                                retry_after_secs: None,
                                user_message_persisted: true,
                            },
                        },
                    );
                    cleanup();
                    return;
                }
            }
        }

        // Fell off the end of the for loop without emitting `done`: we
        // hit MAX_TOOL_ITERATIONS. Surface a distinct code so the UI
        // can render a "too many tool round-trips" banner.
        if !done_emitted {
            let _ = app_handle.emit(
                "ai:chat-stream:error",
                ChatStreamErrorEvent {
                    stream_id: stream_id_task.clone(),
                    failure: ChatSendFailure {
                        kind: "MAX_TOOL_ITERATIONS_EXCEEDED".into(),
                        message: format!(
                            "exceeded MAX_TOOL_ITERATIONS ({MAX_TOOL_ITERATIONS}) without a terminal response"
                        ),
                        retry_after_secs: None,
                        user_message_persisted: true,
                    },
                },
            );
        }

        cleanup();
    });

    Ok(ChatStreamStartResult {
        ok: true,
        user_message: Some(user_msg),
        citations,
        failure: None,
    })
}

/// Request cancellation of an in-flight chat stream. Returns `true`
/// when the stream existed and was flagged, `false` when the stream id
/// had already ended (or never started). The spawned task observes
/// the flag on its next poll and persists whatever it has accumulated
/// so far before emitting a `done { cancelled: true }` event.
#[tauri::command]
pub fn ai_chat_stream_cancel(
    stream_id: String,
    state: tauri::State<AppState>,
) -> AppResult<bool> {
    use std::sync::atomic::Ordering;
    let guard = state.chat_streams.lock().unwrap();
    match guard.get(&stream_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            Ok(true)
        }
        None => Ok(false),
    }
}

// ── Single-shot completion (D3.1) ───────────────────────────────────────────
//
// `ai_complete` is the narrow backend channel shared by all three P3-D3
// write-back commands (summarize / suggest tags / MOC AI draft). It is
// deliberately **not** the chat pipeline:
//
// - no chat-session jsonl persistence — the caller frames the prompt and
//   owns what happens with the reply (diff-preview modal, direct write-back
//   after confirmation, clipboard copy, …);
// - no RAG injection — these commands already have the full note body in
//   the prompt, retrieval on top would waste tokens and risk confusing the
//   model with unrelated chunks;
// - caller picks `temperature` / `max_tokens` (summarization wants low
//   temperature, MOC structuring may want a bit more);
// - cancel token lives in its own `AppState::complete_requests` registry
//   so a chat-side cancel can't abort a write-back mid-flight and vice
//   versa.
//
// Cancellation still reuses the atomic-flag pattern from D2b.4: the command
// internally pulls from `provider.chat_stream()` and polls the flag between
// deltas. That keeps one transport path for every call site; the only
// user-visible difference is "one non-streaming reply" vs "per-token UI".

/// Structured failure for a single-shot completion. Intentionally narrower
/// than [`ChatSendFailure`]: no `user_message_persisted` because the
/// write-back commands never touch the chat store.
#[derive(Debug, Clone, Serialize)]
pub struct CompleteFailure {
    pub kind: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
}

/// Result of [`ai_complete`]. Success carries the trimmed reply + usage
/// counts; failure carries a typed [`CompleteFailure`]. Kept out-of-band
/// from the Tauri `Result` channel so the frontend diff-preview modal
/// can render both paths (loaded / error banner) in the same UI without
/// another `try/catch` layer.
#[derive(Debug, Clone, Serialize, Default)]
pub struct CompleteResult {
    pub ok: bool,
    /// On success: the provider reply with leading/trailing whitespace
    /// trimmed. Empty replies are converted into a failure rather than
    /// an empty success to avoid a silent "summarizer returned nothing"
    /// UX.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// Prompt tokens when the provider reports usage (OpenAI with
    /// `stream_options.include_usage`; Ollama omits).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    /// Completion tokens, same caveats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    /// `true` iff the request was cancelled via [`ai_complete_cancel`]
    /// before the provider finished. When combined with `ok: true` it
    /// means the partial reply is what had accumulated before the
    /// cancel took effect; the UI can offer "keep partial / discard".
    #[serde(default)]
    pub cancelled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<CompleteFailure>,
}

/// Kick off a single-shot completion. Blocks until the provider has
/// finished (or the caller invokes [`ai_complete_cancel`] with the same
/// `request_id`).
///
/// Arguments:
/// - `request_id`: caller-generated (nanoid / uuid). Must be unique
///   across in-flight calls; rejected on collision so a stray cancel
///   can't target the wrong request.
/// - `system_prompt`: optional role-priming message (e.g. "You write
///   concise TL;DRs in the user's language."). Skipped when empty.
/// - `user_prompt`: the actual task payload (prompt template + note
///   body). Required; empty strings are rejected up front.
/// - `temperature` / `max_tokens`: forwarded verbatim to the provider.
///   `None` → provider default.
///
/// Pre-flight rejections (no provider / empty prompt / duplicate id)
/// surface via `failure`; provider errors surface the same way once the
/// network round-trip fails.
#[tauri::command]
pub async fn ai_complete(
    request_id: String,
    system_prompt: Option<String>,
    user_prompt: String,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> AppResult<CompleteResult> {
    use std::sync::atomic::{AtomicBool, Ordering};

    // Validate request_id up front so we never pollute the registry
    // with unusable keys.
    if request_id.trim().is_empty() || request_id.len() > 128 {
        return Ok(CompleteResult {
            ok: false,
            failure: Some(CompleteFailure {
                kind: "invalid_request".into(),
                message: "invalid request_id".into(),
                retry_after_secs: None,
            }),
            ..Default::default()
        });
    }

    // Trim + validate the caller-provided prompt. An empty prompt would
    // just burn tokens on a one-word reply, so we reject rather than
    // forwarding it.
    let user_prompt = user_prompt;
    if user_prompt.trim().is_empty() {
        return Ok(CompleteResult {
            ok: false,
            failure: Some(CompleteFailure {
                kind: "invalid_request".into(),
                message: "user_prompt is empty".into(),
                retry_after_secs: None,
            }),
            ..Default::default()
        });
    }

    // Duplicate-id guard.
    {
        let guard = state.complete_requests.lock().unwrap();
        if guard.contains_key(&request_id) {
            return Ok(CompleteResult {
                ok: false,
                failure: Some(CompleteFailure {
                    kind: "invalid_request".into(),
                    message: "request_id already in use".into(),
                    retry_after_secs: None,
                }),
                ..Default::default()
            });
        }
    }

    // Build the chat provider. Same validation as `ai_chat_send`: need
    // a configured provider with a non-empty `chat_model`.
    let (provider, model) = {
        let cfg = state.config.lock().unwrap();
        match runtime::build_configured_chat_provider(&cfg, &KeyringSecretStore::new()) {
            Ok(ok) => ok,
            Err(err) => {
                return Ok(CompleteResult {
                    ok: false,
                    failure: Some(CompleteFailure {
                        kind: "invalid_request".into(),
                        message: err.to_string(),
                        retry_after_secs: None,
                    }),
                    ..Default::default()
                });
            }
        }
    };

    // Assemble messages: optional system + required user. We deliberately
    // don't reuse `truncate_history_to_budget` — callers own the prompt
    // length for write-back tasks and know whether to chunk the note.
    let mut messages: Vec<ChatTurn> = Vec::with_capacity(2);
    if let Some(sys) = system_prompt.as_ref() {
        let s = sys.trim();
        if !s.is_empty() {
            messages.push(ChatTurn::text(ChatRole::System, s.to_string()));
        }
    }
    messages.push(ChatTurn::text(ChatRole::User, user_prompt));

    let req = ChatRequest {
        model,
        messages,
        temperature,
        max_tokens,
        tools: Vec::new(),
    };

    // Register cancel flag.
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut guard = state.complete_requests.lock().unwrap();
        guard.insert(request_id.clone(), cancel.clone());
    }
    let registry = state.complete_requests.clone();
    let cleanup = || {
        let mut guard = registry.lock().unwrap();
        guard.remove(&request_id);
    };

    // Kick off the stream.
    let mut stream = match provider.chat_stream(req).await {
        Ok(s) => s,
        Err(e) => {
            cleanup();
            let (kind, msg, retry_after_secs) = classify_provider_error(&e);
            return Ok(CompleteResult {
                ok: false,
                failure: Some(CompleteFailure {
                    kind,
                    message: msg,
                    retry_after_secs,
                }),
                ..Default::default()
            });
        }
    };

    let mut accumulated = String::new();
    let mut input_tokens: Option<u32> = None;
    let mut output_tokens: Option<u32> = None;
    let mut cancelled_flag = false;
    let mut error_seen: Option<ProviderError> = None;

    {
        use futures_util::StreamExt;
        while let Some(item) = stream.next().await {
            if cancel.load(Ordering::Relaxed) {
                cancelled_flag = true;
                break;
            }
            match item {
                Ok(delta) => {
                    if !delta.content.is_empty() {
                        accumulated.push_str(&delta.content);
                    }
                    if let Some(t) = delta.input_tokens {
                        input_tokens = Some(t);
                    }
                    if let Some(t) = delta.output_tokens {
                        output_tokens = Some(t);
                    }
                    // `finish_reason` is observed but doesn't force a
                    // `break` — the provider may still emit a trailing
                    // usage chunk after "stop", and we want it.
                }
                Err(e) => {
                    error_seen = Some(e);
                    break;
                }
            }
        }
    }

    // Cancel may have flipped right after we exited the loop.
    if cancel.load(Ordering::Relaxed) {
        cancelled_flag = true;
    }
    cleanup();

    if let Some(e) = error_seen {
        let (kind, msg, retry_after_secs) = classify_provider_error(&e);
        return Ok(CompleteResult {
            ok: false,
            failure: Some(CompleteFailure {
                kind,
                message: msg,
                retry_after_secs,
            }),
            ..Default::default()
        });
    }

    let trimmed = accumulated.trim().to_string();
    if trimmed.is_empty() {
        return Ok(CompleteResult {
            ok: false,
            cancelled: cancelled_flag,
            failure: Some(CompleteFailure {
                kind: "other".into(),
                message: if cancelled_flag {
                    "cancelled before any content arrived".into()
                } else {
                    "provider returned an empty reply".into()
                },
                retry_after_secs: None,
            }),
            ..Default::default()
        });
    }

    Ok(CompleteResult {
        ok: true,
        reply: Some(trimmed),
        input_tokens,
        output_tokens,
        cancelled: cancelled_flag,
        failure: None,
    })
}

/// Request cancellation of an in-flight `ai_complete` call. Returns
/// `true` when the request existed and was flagged, `false` when it
/// had already ended. The in-flight command observes the flag on its
/// next delta tick and returns whatever it has accumulated so far
/// (or an "empty reply" failure when no content arrived yet).
#[tauri::command]
pub fn ai_complete_cancel(
    request_id: String,
    state: tauri::State<AppState>,
) -> AppResult<bool> {
    use std::sync::atomic::Ordering;
    let guard = state.complete_requests.lock().unwrap();
    match guard.get(&request_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            Ok(true)
        }
        None => Ok(false),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProposalResolutionRequest {
    pub session_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub proposal_kind: String,
    pub target_rel_path: String,
    pub accepted_by_user: bool,
    #[serde(default)]
    pub modified_before_accept: bool,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[tauri::command]
pub fn ai_record_proposal_resolution(
    req: ProposalResolutionRequest,
    state: State<AppState>,
) -> AppResult<()> {
    let vault_root = state
        .active_vault
        .lock()
        .unwrap()
        .clone()
        .ok_or(AppError::NoActiveVault)?;
    let destructive = matches!(req.proposal_kind.as_str(), "delete_note" | "rename_note");
    let payload = json!({
        "ts": chrono::Utc::now().timestamp(),
        "kind": "proposal_resolution",
        "session_id": req.session_id,
        "tool_call_id": req.tool_call_id,
        "tool_name": req.tool_name,
        "proposal_kind": req.proposal_kind,
        "target_rel_path": req.target_rel_path,
        "accepted_by_user": req.accepted_by_user,
        "modified_before_accept": req.modified_before_accept,
        "result": req.result,
        "metadata": req.metadata
    });
    crate::services::ai::usage_log::append_usage_log(&vault_root, &payload)?;
    if destructive {
        crate::services::ai::usage_log::append_audit_log(&vault_root, &payload)?;
    }
    Ok(())
}

// ── Public response type ──────────────────────────────────────────────────────

/// One entry in the "related notes" list returned to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct RelatedNote {
    /// Vault-relative path of the candidate note.
    pub path: String,
    /// Frontmatter title, or `None` when the note has no title.
    pub title: Option<String>,
    /// Note type (inbox / note / moc / daily / project / …).
    pub note_type: Option<String>,
    /// ISO-8601 string of the note's last update, or `None`.
    pub updated: Option<String>,
    /// Composite relevance score (higher = more related). Not normalised —
    /// callers should display it as a sort key only, not a percentage.
    pub score: f64,
    /// Which heuristic signals fired (for debug / hover tooltip).
    pub signals: RelatedSignals,
}

/// Breakdown of which scoring signals contributed to `score`.
#[derive(Debug, Clone, Serialize)]
pub struct RelatedSignals {
    /// `shared_tags / min(|tags_current|, |tags_candidate|)` ∈ [0, 1].
    pub tag_overlap: f64,
    /// `true` if a wiki-link exists in either direction between the two notes.
    pub direct_link: bool,
    /// `true` if at least one *other* note links to both.
    pub co_cited: bool,
    /// Cosine over note-level summed chunk embeddings ∈ [0, 1].
    pub embedding_cosine: f64,
    /// `days_since_updated / 30`, clamped to [0, 1].
    pub staleness: f64,
}

// ── Command ───────────────────────────────────────────────────────────────────

/// Return up to `limit` notes most related to `src_rel_path`.
///
/// Returns an empty list (not an error) when:
/// - No vault is open.
/// - `src_rel_path` is not in the index yet (watcher hasn't caught up).
///
/// Returns an error only on DB / path sanity failures.
#[tauri::command]
pub fn ai_related_notes(
    src_rel_path: String,
    limit: Option<u32>,
    state: State<AppState>,
) -> AppResult<Vec<RelatedNote>> {
    let conn_arc = match state.index_handle() {
        Some(a) => a,
        None => return Ok(vec![]),
    };
    let conn = conn_arc.lock().unwrap();

    // Reject path escapes.
    let rel = std::path::Path::new(&src_rel_path);
    if rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::PathEscape(src_rel_path));
    }

    let limit = limit.unwrap_or(10).min(50) as usize;
    let embedding_scores = {
        match state.embeddings_handle() {
            Some(store) => {
                let configured_model = configured_embed_model(&state);
                let guard = store.lock().unwrap();
                match configured_model.or(guard.only_model_name()?) {
                    Some(model) => guard
                        .note_cosine_scores(&src_rel_path, &model)?
                        .into_iter()
                        .map(|(path, score)| (path, score as f64))
                        .collect::<HashMap<String, f64>>(),
                    None => HashMap::new(),
                }
            }
            None => HashMap::new(),
        }
    };

    related_notes_core(&conn, &embedding_scores, &src_rel_path, limit)
}

/// Pure-SQL related-notes scoring kernel. Shared between the
/// `ai_related_notes` Tauri command and the `get_related_notes` tool
/// (P3-D5.2). The caller owns the connection lock + the pre-computed
/// per-note cosine scores; this fn only issues SELECTs, scores every
/// candidate, filters `score > 0`, sorts descending, and truncates to
/// `limit`.
///
/// `embedding_scores` may be empty — the embedding-cosine signal then
/// contributes `0` for every candidate (same semantics as "no embed
/// model configured"). `src_rel_path` not being in `notes` results in
/// an empty `src_tags` / `direct_links` / `co_citers`, so every
/// candidate scores `0` and the fn returns an empty vec.
pub(crate) fn related_notes_core(
    conn: &rusqlite::Connection,
    embedding_scores: &HashMap<String, f64>,
    src_rel_path: &str,
    limit: usize,
) -> AppResult<Vec<RelatedNote>> {
    // ── 1. Gather current-note context ───────────────────────────────────────

    // Tags of the current note.
    let src_tags: HashSet<String> = {
        let mut stmt = conn.prepare_cached("SELECT tag FROM tags WHERE note_path = ?1")?;
        let rows: Vec<rusqlite::Result<String>> = stmt
            .query_map([src_rel_path], |r| r.get::<_, String>(0))?
            .collect();
        rows.into_iter().filter_map(|r| r.ok()).collect()
    };

    // Notes that directly link to or from the current note (resolved paths).
    let direct_links: HashSet<String> = {
        let mut stmt = conn.prepare_cached(
            "SELECT DISTINCT dst_resolved FROM links
             WHERE src = ?1 AND dst_resolved IS NOT NULL
             UNION
             SELECT DISTINCT src FROM links
             WHERE dst_resolved = ?1",
        )?;
        let rows: Vec<rusqlite::Result<String>> = stmt
            .query_map([src_rel_path], |r| r.get::<_, String>(0))?
            .collect();
        rows.into_iter().filter_map(|r| r.ok()).collect()
    };

    // Notes that co-cite the current note (another note links to both).
    // Co-citer C → current AND C → candidate.
    let co_citers: HashSet<String> = {
        let mut stmt = conn.prepare_cached(
            "SELECT DISTINCT l2.dst_resolved
             FROM links l1
             JOIN links l2 ON l1.src = l2.src
             WHERE l1.dst_resolved = ?1
               AND l2.dst_resolved IS NOT NULL
               AND l2.dst_resolved != ?1",
        )?;
        let rows: Vec<rusqlite::Result<String>> = stmt
            .query_map([src_rel_path], |r| r.get::<_, String>(0))?
            .collect();
        rows.into_iter().filter_map(|r| r.ok()).collect()
    };

    // ── 2. Enumerate candidate notes ─────────────────────────────────────────

    // Candidate pool: all notes except the current one.
    // We fetch title / type / updated in the same pass to avoid N+1 queries.
    struct Candidate {
        path: String,
        title: Option<String>,
        note_type: Option<String>,
        updated: Option<String>,
        tags: HashSet<String>,
    }

    let mut candidates: Vec<Candidate> = {
        let mut stmt = conn.prepare_cached(
            "SELECT n.path, n.title, n.type, n.updated
             FROM notes n
             WHERE n.path != ?1",
        )?;
        let rows: Vec<rusqlite::Result<Candidate>> = stmt
            .query_map([src_rel_path], |r| {
                Ok(Candidate {
                    path: r.get(0)?,
                    title: r.get(1)?,
                    note_type: r.get(2)?,
                    updated: r.get(3)?,
                    tags: HashSet::new(),
                })
            })?
            .collect();
        rows.into_iter().filter_map(|r| r.ok()).collect()
    };

    // Bulk-load tags for all candidates in one query, then assign.
    let mut tag_map: HashMap<String, HashSet<String>> = HashMap::new();
    {
        let mut stmt =
            conn.prepare_cached("SELECT note_path, tag FROM tags WHERE note_path != ?1")?;
        let rows: Vec<rusqlite::Result<(String, String)>> = stmt
            .query_map([src_rel_path], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect();
        for row in rows.into_iter().filter_map(|r| r.ok()) {
            tag_map.entry(row.0).or_default().insert(row.1);
        }
    }
    for c in &mut candidates {
        if let Some(tags) = tag_map.remove(&c.path) {
            c.tags = tags;
        }
    }

    // ── 3. Score every candidate ─────────────────────────────────────────────

    let today_days = unix_days_now();

    let mut scored: Vec<RelatedNote> = candidates
        .into_iter()
        .map(|c| {
            // --- tag overlap ---
            let shared = src_tags.intersection(&c.tags).count();
            let min_tags = src_tags.len().min(c.tags.len());
            let tag_overlap = if min_tags == 0 {
                0.0
            } else {
                shared as f64 / min_tags as f64
            };

            // --- direct link ---
            let direct_link = direct_links.contains(&c.path);

            // --- co-citation ---
            let co_cited = co_citers.contains(&c.path);

            // --- embedding cosine ---
            let embedding_cosine = embedding_scores.get(&c.path).copied().unwrap_or(0.0);

            // --- staleness ---
            let staleness = staleness_score(c.updated.as_deref(), today_days);

            // --- composite ---
            let score = 2.0 * tag_overlap
                + if direct_link { 1.5 } else { 0.0 }
                + if co_cited { 1.0 } else { 0.0 }
                + 0.5 * embedding_cosine
                - 0.3 * staleness;

            RelatedNote {
                path: c.path,
                title: c.title,
                note_type: c.note_type,
                updated: c.updated,
                score,
                signals: RelatedSignals {
                    tag_overlap,
                    direct_link,
                    co_cited,
                    embedding_cosine,
                    staleness,
                },
            }
        })
        .collect();

    // ── 4. Sort + filter + truncate ──────────────────────────────────────────

    // Only surface notes with at least one positive signal — a score ≤ 0
    // means no tags in common, no links, no co-citation, and no embedding
    // proximity: the note is objectively unrelated. Staleness can push a
    // borderline note below zero, but not one that has any genuine signal.
    scored.retain(|n| n.score > 0.0);

    // Descending score.
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(limit);

    Ok(scored)
}

// ── Scoring helpers ───────────────────────────────────────────────────────────

/// Current time as integer "days since Unix epoch". Used for staleness.
fn unix_days_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 / 86_400)
        .unwrap_or(0)
}

/// Convert an ISO-8601 `updated` string ("YYYY-MM-DD" or "YYYY-MM-DDTHH:…")
/// to a staleness score in [0, 1].
///
/// `0` = updated today, `1` = updated ≥ 30 days ago.
fn staleness_score(updated: Option<&str>, today_days: i64) -> f64 {
    let Some(s) = updated else { return 1.0 };
    // Parse "YYYY-MM-DD" — only need the first 10 chars.
    let date_str = s.get(..10).unwrap_or(s);
    let parts: Vec<i64> = date_str
        .splitn(3, '-')
        .filter_map(|p| p.parse().ok())
        .collect();
    if parts.len() != 3 {
        return 1.0;
    }
    // Very simple Gregorian approximation: just use days in the year.
    // Accurate to within ±1 day for our purposes (staleness is a soft signal).
    let year = parts[0];
    let month = parts[1];
    let day = parts[2];
    // Days since epoch approximation: use the Julian Day Number difference.
    let note_days = date_to_julian(year, month, day);
    let raw = (today_days - note_days).max(0) as f64 / 30.0;
    raw.min(1.0)
}

/// Days since Unix epoch (1970-01-01 = 0) for a given date.
///
/// Uses the Julian Day Number algorithm internally, then subtracts the
/// Julian Day for the Unix epoch (2440588) so the result is directly
/// comparable with `unix_days_now()`.
fn date_to_julian(y: i64, m: i64, d: i64) -> i64 {
    // Julian Day Number (integer arithmetic from Wikipedia).
    let a = (14 - m) / 12;
    let y2 = y + 4800 - a;
    let m2 = m + 12 * a - 3;
    let jdn = d + (153 * m2 + 2) / 5 + 365 * y2 + y2 / 4 - y2 / 100 + y2 / 400 - 32_045;
    // Unix epoch 1970-01-01 has JDN = 2440588.
    jdn - 2_440_588
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- staleness ---

    #[test]
    fn staleness_none_updated_is_max() {
        assert_eq!(staleness_score(None, 0), 1.0);
    }

    #[test]
    fn staleness_recent_date_is_less_than_one() {
        // 2026-04-21 is a known recent date; staleness should be < 1.0 even
        // with today's reference point (unless the tests are run in 2027+).
        let stale = staleness_score(Some("2026-04-21"), unix_days_now());
        assert!(stale < 1.0, "expected <1.0, got {stale}");
    }

    #[test]
    fn staleness_old_date_is_one() {
        // A note last updated 10 years ago should have staleness = 1.
        let stale = staleness_score(Some("2015-01-01"), unix_days_now());
        assert_eq!(stale, 1.0);
    }

    #[test]
    fn staleness_bad_format_is_max() {
        assert_eq!(staleness_score(Some("not-a-date"), 0), 1.0);
        assert_eq!(staleness_score(Some(""), 0), 1.0);
    }

    // --- scoring composition ---

    #[test]
    fn score_with_all_positive_signals_equals_five() {
        // tag_overlap=1, direct=1, co_cited=1, embedding_cosine=1, staleness=0
        // = 2.0 + 1.5 + 1.0 + 0.5 - 0.0 = 5.0
        let score: f64 = 2.0 * 1.0 + 1.5 * 1.0 + 1.0 * 1.0 + 0.5 * 1.0 - 0.3 * 0.0;
        assert!((score - 5.0_f64).abs() < 1e-9_f64);
    }

    #[test]
    fn score_with_no_signals_is_negative() {
        // tag_overlap=0, direct=0, co_cited=0, embedding_cosine=0, staleness=1
        // = 0 + 0 + 0 + 0 - 0.3 = -0.3
        let score: f64 = 2.0 * 0.0 + 1.5 * 0.0 + 1.0 * 0.0 + 0.5 * 0.0 - 0.3 * 1.0;
        assert!(score < 0.0_f64);
    }

    // --- history truncation (D2b.4) ---

    fn msg(role: ChatRole, content: &str) -> ChatMessage {
        ChatMessage {
            v: 1,
            id: "x".into(),
            role,
            content: content.into(),
            created_at: 0,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    #[test]
    fn truncate_preserves_all_when_under_budget() {
        let msgs = vec![
            msg(ChatRole::User, "hi"),
            msg(ChatRole::Assistant, "hello"),
            msg(ChatRole::User, "how are you"),
        ];
        let out = truncate_history_to_budget(&msgs, 1_000);
        assert_eq!(out.len(), 3);
        assert_eq!(out[2].content, "how are you");
    }

    #[test]
    fn truncate_drops_oldest_over_budget() {
        // Build 5 messages, each 100 chars; budget 250 → keep newest 2.
        let long = "x".repeat(100);
        let msgs: Vec<ChatMessage> = (0..5)
            .map(|i| {
                let role = if i % 2 == 0 {
                    ChatRole::User
                } else {
                    ChatRole::Assistant
                };
                msg(role, &long)
            })
            .collect();
        let out = truncate_history_to_budget(&msgs, 250);
        // 2 × 100 = 200 fits, 3 × 100 = 300 > 250, so newest 2.
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn truncate_always_keeps_system_prefix() {
        let long = "x".repeat(100);
        let mut msgs = vec![msg(ChatRole::System, "SYSTEM PROMPT")];
        for i in 0..5 {
            let role = if i % 2 == 0 {
                ChatRole::User
            } else {
                ChatRole::Assistant
            };
            msgs.push(msg(role, &long));
        }
        let out = truncate_history_to_budget(&msgs, 250);
        assert_eq!(out[0].role, ChatRole::System);
        assert_eq!(out[0].content, "SYSTEM PROMPT");
        // Rest is whatever fit under 250 - 13 (system).
        assert!(out.len() >= 2);
    }

    #[test]
    fn truncate_keeps_sole_oversized_newest_message() {
        // One huge message that by itself exceeds the budget — we still
        // include it so the call isn't empty; the provider will reject
        // with InvalidRequest and we'll surface that.
        let huge = "x".repeat(1000);
        let msgs = vec![msg(ChatRole::User, &huge)];
        let out = truncate_history_to_budget(&msgs, 100);
        assert_eq!(out.len(), 1);
    }

    // --- P3-D5.1 atomic-unit truncation ---

    fn msg_assistant_with_tools(content: &str, tool_names: &[&str]) -> ChatMessage {
        use crate::services::ai::provider::ToolCall;
        ChatMessage {
            v: 2,
            id: "asst".into(),
            role: ChatRole::Assistant,
            content: content.into(),
            created_at: 0,
            tool_calls: Some(
                tool_names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| ToolCall {
                        id: format!("call_{i}"),
                        name: (*n).to_string(),
                        arguments: "{}".into(),
                    })
                    .collect(),
            ),
            tool_call_id: None,
        }
    }

    fn msg_tool(content: &str, call_id: &str) -> ChatMessage {
        ChatMessage {
            v: 2,
            id: format!("tool-{call_id}"),
            role: ChatRole::Tool,
            content: content.into(),
            created_at: 0,
            tool_calls: None,
            tool_call_id: Some(call_id.into()),
        }
    }

    #[test]
    fn truncate_drops_orphan_tool_when_parent_assistant_evicted() {
        use crate::services::ai::provider::ToolCall;
        // Shape: User(big) → Assistant(heavy tool_call args) →
        // Tool("hit") → User("final"). Budget tight: only the tail
        // two messages fit (Assistant's 150-char args blow past).
        // Atomic-unit healing must drop the orphan Tool so the
        // provider never sees a Tool without its parent.
        let big = "x".repeat(200);
        let heavy_args = "a".repeat(150);
        let heavy_assistant = ChatMessage {
            v: 2,
            id: "asst".into(),
            role: ChatRole::Assistant,
            content: String::new(),
            created_at: 0,
            tool_calls: Some(vec![ToolCall {
                id: "call_0".into(),
                name: "search".into(),
                arguments: heavy_args,
            }]),
            tool_call_id: None,
        };
        let msgs = vec![
            msg(ChatRole::User, &big),
            heavy_assistant,
            msg_tool("hit", "call_0"),
            msg(ChatRole::User, "final"),
        ];
        // Budget 30: "final" (5) + "hit" (3) = 8 fits, Assistant
        // weight ≥ 150 > 30 → evicted. Tool becomes the oldest kept
        // message; healing drops it.
        let out = truncate_history_to_budget(&msgs, 30);
        assert!(
            out.iter().all(|t| t.role != ChatRole::Tool),
            "orphan Tool should have been healed out: {out:?}"
        );
        assert_eq!(out.last().unwrap().role, ChatRole::User);
        assert_eq!(out.last().unwrap().content, "final");
    }

    #[test]
    fn truncate_keeps_assistant_with_tool_calls_and_its_tool_together() {
        // Whole history fits — atomic pair survives intact in order.
        let msgs = vec![
            msg(ChatRole::User, "question"),
            msg_assistant_with_tools("calling", &["search_notes"]),
            msg_tool("hit A", "call_0"),
            msg(ChatRole::Assistant, "final answer"),
        ];
        let out = truncate_history_to_budget(&msgs, 10_000);
        assert_eq!(out.len(), 4);
        assert_eq!(out[1].role, ChatRole::Assistant);
        assert!(out[1].tool_calls.is_some());
        assert_eq!(out[2].role, ChatRole::Tool);
        assert_eq!(out[2].tool_call_id.as_deref(), Some("call_0"));
    }

    #[test]
    fn truncate_counts_tool_call_arguments_toward_budget() {
        // A single tool-call with a 5 kB arguments JSON must count —
        // otherwise a heavy history would slip past the budget.
        use crate::services::ai::provider::ToolCall;
        let big_args = "a".repeat(5_000);
        let heavy = ChatMessage {
            v: 2,
            id: "heavy".into(),
            role: ChatRole::Assistant,
            content: "light".into(),
            created_at: 0,
            tool_calls: Some(vec![ToolCall {
                id: "call_0".into(),
                name: "x".into(),
                arguments: big_args,
            }]),
            tool_call_id: None,
        };
        let weight = message_weight_chars(&heavy);
        assert!(weight >= 5_000, "expected weight ≥ 5000, got {weight}");
    }
}
