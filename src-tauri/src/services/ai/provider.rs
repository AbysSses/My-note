//! AI provider abstraction — Phase 3-D2a.
//!
//! The `AiProvider` trait is the narrow waist between "what the app wants"
//! (embed a batch of text, chat a message stream) and "how a specific backend
//! actually does it" (OpenAI HTTP, Ollama local, Anthropic, …).
//!
//! D2a.1 scope: trait shape + `MockProvider` only. No HTTP impl yet — those
//! land in D2a.2. D2b adds the `chat()` method.
//!
//! ## Why a trait at all
//!
//! - Swap providers without touching call sites (Settings will let the user
//!   pick one of [OpenAI / Ollama / OpenRouter / …]).
//! - `MockProvider` lets us write determistic unit tests for downstream
//!   consumers (chunker → store → search pipeline) without network calls.
//! - Every call goes through `Box<dyn AiProvider>`, so we need `async_trait`
//!   to keep it dyn-compatible.
//!
//! ## Consumer status (D2a.2)
//!
//! - `AiProvider`, `EmbedRequest`, `EmbedResponse`, `ProviderError` are
//!   actively consumed by `commands/ai.rs::ai_provider_test_connection`.
//! - `MockProvider` and `trait AiProvider::{name, default_dim}` are
//!   exercised only by unit tests + future D2a.3 embed IPC. Module-level
//!   `allow(dead_code)` avoids noise; D2a.3 removes the attribute.
//!
//! ## What this module does NOT own
//!
//! - HTTP client / retry / rate-limit logic — lives inside each concrete impl
//!   (D2a.2 `openai.rs`).
//! - API-key storage (keychain) — D2a.2 will add a `providers::secrets` module.
//! - Token counting — uses `est_tokens()` in [`super::chunker`] as a cheap
//!   heuristic; real providers may return authoritative counts in their
//!   response.

#![allow(dead_code)]

use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures_util::Stream;
use serde::{Deserialize, Serialize};

// ── Core types ────────────────────────────────────────────────────────────────

/// Request shape accepted by all embedding providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedRequest {
    /// Provider-specific model identifier (e.g. `text-embedding-3-small`,
    /// `nomic-embed-text`). The caller is responsible for picking a model
    /// compatible with the chosen provider.
    pub model: String,
    /// Batch of input strings to embed. Ordering of the response must match.
    pub inputs: Vec<String>,
}

/// Response from a successful embed call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedResponse {
    /// One f32 vector per input, in the same order as `EmbedRequest::inputs`.
    pub vectors: Vec<Vec<f32>>,
    /// Total prompt tokens consumed (best-effort; 0 if the provider does
    /// not return it — e.g. Ollama / Mock).
    pub total_tokens: u32,
}

/// Errors that any provider may raise. Kept narrow so call sites can map
/// them to user-facing notices and retry policy without pattern-matching
/// on specific HTTP status codes.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProviderError {
    /// Network / DNS / TLS layer failure; usually retryable.
    #[error("network: {0}")]
    Network(String),
    /// 401/403 — API key missing, invalid, or revoked. Not retryable.
    #[error("auth: {0}")]
    Auth(String),
    /// 429 — rate limit / quota exhaustion. Carries both the suggested
    /// backoff seconds and the server's human-readable detail.
    #[error("rate limit: {message} (retry after {retry_after_secs}s)")]
    RateLimit {
        retry_after_secs: u64,
        message: String,
    },
    /// 4xx (not 401/429) — malformed request. Caller bug; not retryable.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// 5xx / unknown — retryable with caution.
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    Network,
    Auth,
    RateLimit,
    InvalidRequest,
    Other,
}

pub fn describe_provider_error(err: &ProviderError) -> (ProviderErrorKind, String, Option<u64>) {
    match err {
        ProviderError::Network(msg) => (ProviderErrorKind::Network, msg.clone(), None),
        ProviderError::Auth(msg) => (ProviderErrorKind::Auth, msg.clone(), None),
        ProviderError::RateLimit {
            retry_after_secs,
            message,
        } => (
            ProviderErrorKind::RateLimit,
            message.clone(),
            Some(*retry_after_secs),
        ),
        ProviderError::InvalidRequest(msg) => {
            (ProviderErrorKind::InvalidRequest, msg.clone(), None)
        }
        ProviderError::Other(msg) => (ProviderErrorKind::Other, msg.clone(), None),
    }
}

// ── Chat types (D2b.2 / extended in D5.1) ───────────────────────────────────

/// Role of a single chat turn on the wire. Matches the `chat_store::ChatRole`
/// shape; kept separate here so the provider layer doesn't depend on the
/// persistence layer (the command layer does the trivial conversion).
///
/// The `Tool` variant was added in P3-D5.1 so tool-calling round-trips can
/// persist a tool-result message in chat history without a second enum.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    #[default]
    User,
    Assistant,
    /// A function/tool call result. Only valid when the preceding assistant
    /// turn had a non-empty `tool_calls`; the `tool_call_id` field on the
    /// owning [`ChatTurn`] / [`super::chat_store::ChatMessage`] must match
    /// one of those call ids. Mirrors OpenAI's `"role": "tool"` wire shape.
    Tool,
}

/// One function/tool call requested by the assistant. Carried on a
/// [`ChatTurn`] with `role = Assistant`. The `arguments` field is the
/// raw JSON string produced by the model; validation happens only when
/// the tool registry actually executes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    /// Unique id the provider assigns so tool-result messages can be
    /// matched back to the originating call.
    pub id: String,
    pub name: String,
    /// JSON-encoded arguments string. Providers often emit this split
    /// across several streaming fragments — see [`ToolCallFragment`].
    pub arguments: String,
}

/// One incremental piece of a tool call as it streams in. Providers emit
/// many of these in sequence, all sharing the same `index` for one logical
/// call. The assembler at the command layer concatenates `arguments_delta`
/// into the final [`ToolCall::arguments`] string once `finish_reason`
/// arrives.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCallFragment {
    /// Position within `choices[0].delta.tool_calls` — OpenAI numbers them
    /// in emission order but re-sends fragments with the same index over
    /// many SSE frames.
    pub index: u32,
    /// Only present on the first fragment for a given index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Only present on the first fragment for a given index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Piece of the arguments JSON string; nullable because some vendors
    /// emit an "arguments-less" bootstrap fragment carrying only id+name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments_delta: Option<String>,
}

/// JSON-Schema-style declaration of a tool the model may call. Serialized
/// into the provider request (wire format differs per vendor; the OpenAI
/// mapper in `openai.rs` wraps this into the `{type:"function",function:…}`
/// envelope).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON-Schema object describing the argument shape. Kept as
    /// `serde_json::Value` so each tool can carry an arbitrary schema
    /// without the registry caring about the specifics.
    pub parameters: serde_json::Value,
}

/// Result of executing one tool call. The registry always returns one
/// of these — on failure, `is_error = true` and `content` carries a
/// human-readable message the next model turn can read and recover from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// One chat turn. `content` is plain text; tool-calling extensions added
/// in P3-D5.1 live in the two optional fields — both `None` on the vast
/// majority of turns so serialized form matches the pre-D5 shape exactly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChatTurn {
    pub role: ChatRole,
    pub content: String,
    /// Present only on `role = Assistant` turns where the model decided
    /// to invoke one or more tools. Empty-vec and `None` are semantically
    /// identical at the provider boundary; we prefer `None` so the
    /// serialized JSON omits the field entirely.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Present only on `role = Tool` turns; matches a `ToolCall.id`
    /// emitted by the preceding assistant turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl ChatTurn {
    /// Convenience constructor for text-only turns. Keeps existing call
    /// sites terse; the new optional fields default to `None`.
    pub fn text(role: ChatRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }
}

/// Chat request. `max_tokens` / `temperature` stay optional so providers
/// with different defaults (Ollama often ignores `max_tokens`) don't force
/// every caller to pick a number. `tools` added in P3-D5.1 — empty vec
/// (the default) means "tool calling disabled" and the provider must not
/// include the `tools` field on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatTurn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
}

/// One incremental update produced by [`AiProvider::chat_stream`].
///
/// Concrete providers emit any number of `ChatDelta`s with a `content`
/// fragment, then a final delta that may carry `finish_reason` + usage.
/// Callers should accumulate `content` in order and treat
/// `finish_reason.is_some()` as the completion signal.
///
/// When the model emits a tool call instead of plain text, the provider
/// streams several deltas carrying `tool_call_fragments` (same `index`,
/// growing `arguments_delta`) and terminates with
/// `finish_reason = "tool_calls"`. Upstream assembles the fragments into
/// complete [`ToolCall`]s — the provider layer only guarantees ordered
/// fragment delivery, never rebuilds the full call itself.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChatDelta {
    /// Newly produced token text for this delta. May be empty when only
    /// usage / stop metadata / tool-call fragments arrive.
    #[serde(default)]
    pub content: String,
    /// `Some` on the final delta; `"stop"` / `"length"` / `"content_filter"`
    /// / `"tool_calls"` mirror OpenAI's enum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Prompt tokens, if the provider reports usage (OpenAI + `stream_options`;
    /// Ollama omits).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    /// Completion tokens, same caveats.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    /// Per-delta tool-call fragments. `None` on plain-text deltas so
    /// existing JSON roundtrips stay byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_fragments: Option<Vec<ToolCallFragment>>,
}

/// Dyn-compatible stream alias. We pin-box so the trait method can return
/// a single concrete type regardless of the provider's inner future/channel
/// choice.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatDelta, ProviderError>> + Send>>;

/// Common interface every AI provider must implement.
///
/// `async_trait` is used so we can hold `Box<dyn AiProvider>` inside
/// `AppState`; native `async fn` in trait (RFC 3185) would make the trait
/// non-dyn-compatible.
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// Short, human-readable provider name ("openai", "ollama", "mock").
    /// Used in logs / UI labels / `embedding_chunks.model` prefix when the
    /// caller wants to namespace vectors by provider.
    fn name(&self) -> &'static str;

    /// Dimension of vectors returned by this provider's default model.
    /// Callers use this to pre-allocate buffers and to validate dimension
    /// match against stored vectors.
    fn default_dim(&self) -> usize;

    /// Embed a batch of inputs. Must return `vectors.len() == inputs.len()`
    /// and `vectors[i].len() == default_dim()` on success.
    async fn embed(&self, req: EmbedRequest) -> Result<EmbedResponse, ProviderError>;

    /// Stream a chat completion. The returned stream yields zero-or-more
    /// [`ChatDelta`] with content fragments, then terminates (end-of-stream).
    /// Providers that don't support streaming should still implement this
    /// by emitting one delta with the full content + `finish_reason` set.
    ///
    /// Default returns [`ProviderError::InvalidRequest`] so embed-only test
    /// doubles don't have to opt into a no-op override they'll never
    /// reach. Real chat providers (OpenAI, mock chat harness) override.
    async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, ProviderError> {
        Err(ProviderError::InvalidRequest(
            "chat is not supported by this provider".into(),
        ))
    }
}

/// Helper: collect an entire `ChatStream` into one `ChatDelta`. Used by
/// the "test connection" IPC and by future non-streaming UIs (D2b.3 v1)
/// that don't want to reimplement delta aggregation.
pub async fn collect_chat_stream(mut stream: ChatStream) -> Result<ChatDelta, ProviderError> {
    use futures_util::StreamExt;
    let mut out = ChatDelta::default();
    while let Some(item) = stream.next().await {
        let delta = item?;
        out.content.push_str(&delta.content);
        if delta.finish_reason.is_some() {
            out.finish_reason = delta.finish_reason;
        }
        if let Some(t) = delta.input_tokens {
            out.input_tokens = Some(t);
        }
        if let Some(t) = delta.output_tokens {
            out.output_tokens = Some(t);
        }
    }
    Ok(out)
}

pub struct CompleteTextRequest<'a> {
    pub model: &'a str,
    pub system_prompt: Option<&'a str>,
    pub user_prompt: &'a str,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub cancel: Option<&'a AtomicBool>,
}

#[derive(Debug, Clone, Default)]
pub struct CompleteTextResponse {
    pub reply: String,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub cancelled: bool,
}

pub async fn complete_text(
    provider: &dyn AiProvider,
    req: CompleteTextRequest<'_>,
) -> Result<CompleteTextResponse, ProviderError> {
    let mut messages: Vec<ChatTurn> = Vec::with_capacity(2);
    if let Some(system_prompt) = req.system_prompt {
        let trimmed = system_prompt.trim();
        if !trimmed.is_empty() {
            messages.push(ChatTurn::text(ChatRole::System, trimmed.to_string()));
        }
    }
    messages.push(ChatTurn::text(ChatRole::User, req.user_prompt));

    let chat_req = ChatRequest {
        model: req.model.to_string(),
        messages,
        temperature: req.temperature,
        max_tokens: req.max_tokens,
        tools: Vec::new(),
    };

    let mut stream = provider.chat_stream(chat_req).await?;
    let mut accumulated = String::new();
    let mut input_tokens = None;
    let mut output_tokens = None;
    let mut cancelled = false;

    use futures_util::StreamExt;
    while let Some(item) = stream.next().await {
        if req
            .cancel
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            cancelled = true;
            break;
        }
        let delta = item?;
        accumulated.push_str(&delta.content);
        if let Some(tokens) = delta.input_tokens {
            input_tokens = Some(tokens);
        }
        if let Some(tokens) = delta.output_tokens {
            output_tokens = Some(tokens);
        }
    }

    if req
        .cancel
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        cancelled = true;
    }

    Ok(CompleteTextResponse {
        reply: accumulated.trim().to_string(),
        input_tokens,
        output_tokens,
        cancelled,
    })
}

// ── MockProvider ──────────────────────────────────────────────────────────────

/// One step in a mock chat script. Tests compose these into a sequence
/// the `MockProvider::chat_stream` loop emits deterministically — see
/// [`MockProvider::set_chat_script`].
#[derive(Debug, Clone)]
pub enum ChatScriptItem {
    /// Plain token fragment — becomes one `ChatDelta` with `content` set.
    Delta(String),
    /// Terminal delta carrying `finish_reason = "stop"` (+ optional
    /// trailing text).
    FinishText { content: String },
    /// Terminal delta carrying `finish_reason = "tool_calls"` and the
    /// complete rebuilt tool calls. The mock collapses the fragment
    /// reassembly step tests would otherwise have to emulate by hand.
    FinishToolCall { tool_calls: Vec<ToolCall> },
    /// Next `stream.next().await` surfaces this error.
    Error(ProviderError),
}

/// Deterministic, dependency-free provider for tests and offline dev.
///
/// The embedding is derived from a cheap rolling hash of the input — it is
/// NOT semantically meaningful, but two identical inputs produce identical
/// vectors (so `cosine(a, a) == 1.0`) and different inputs produce different
/// vectors (with high probability), which is enough to verify plumbing.
///
/// ## Chat scripting model (P3-D5.1)
///
/// Each `chat_stream` invocation consumes the **next** inner vec from
/// `chat_script: Vec<Vec<ChatScriptItem>>`. That lets a single test
/// orchestrate a multi-turn tool-calling loop — e.g. `[[tool_call], [text
/// finish]]` means turn-1 emits a tool_call terminator, the command
/// layer executes the tool + appends the result, and turn-2 replays
/// `chat_stream` to receive the final text answer.
///
/// Script exhaustion is a test-authoring bug, so we `panic!` rather than
/// silently echo the prompt: pairs well with `#[should_panic]` coverage
/// of the multi-turn cap.
pub struct MockProvider {
    dim: usize,
    /// Outer vec: one entry per call to `chat_stream`. Inner vec: the
    /// ordered script of steps for that single call.
    chat_script: Arc<Mutex<Option<Vec<Vec<ChatScriptItem>>>>>,
    /// Atomic cursor into the outer vec. `fetch_add` lets the mock stay
    /// thread-safe under a `&self` signature without needing to lock the
    /// whole script for read-only indexing.
    chat_iter: Arc<std::sync::atomic::AtomicUsize>,
    /// Next error to surface from `chat_stream` (consumed on emit). Lets
    /// tests assert classification without reaching through a real HTTP
    /// layer. Takes precedence over any configured script.
    chat_error: Arc<Mutex<Option<ProviderError>>>,
}

impl MockProvider {
    /// Default mock dimension (192) is small enough that unit tests stay
    /// fast but large enough to catch obvious indexing bugs.
    pub fn new() -> Self {
        Self {
            dim: 192,
            chat_script: Arc::new(Mutex::new(None)),
            chat_iter: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            chat_error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_dim(dim: usize) -> Self {
        Self {
            dim,
            ..Self::new()
        }
    }

    /// Pre-D5.1 helper kept for legacy tests that only care about a
    /// single-turn token stream. Converts the flat token list into a
    /// one-iteration script ending with a `stop` finish.
    pub fn set_chat_script_tokens(&self, tokens: Vec<String>) {
        let mut items: Vec<ChatScriptItem> = tokens.into_iter().map(ChatScriptItem::Delta).collect();
        // Convert the trailing delta into a terminal `FinishText` so the
        // emitted stream carries `finish_reason = "stop"`.
        let trailing = match items.pop() {
            Some(ChatScriptItem::Delta(s)) => s,
            // Empty input: terminate with an empty-content stop delta
            // rather than produce a stream the caller can never finish.
            _ => String::new(),
        };
        items.push(ChatScriptItem::FinishText { content: trailing });
        *self.chat_script.lock().unwrap() = Some(vec![items]);
        self.chat_iter.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Pre-load a multi-iteration script. Each inner vec is consumed
    /// fully by one `chat_stream` invocation; the outer index advances
    /// on every call. Calls beyond the outer length panic so a
    /// mis-built test fails loudly instead of silently hanging.
    pub fn set_chat_script(&self, script: Vec<Vec<ChatScriptItem>>) {
        *self.chat_script.lock().unwrap() = Some(script);
        self.chat_iter.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// Next `chat_stream` call errors with this. Consumed on the first
    /// emit so subsequent calls fall back to script / default behaviour.
    pub fn set_chat_error(&self, err: ProviderError) {
        *self.chat_error.lock().unwrap() = Some(err);
    }

    /// Hash one input into a unit-norm `dim`-vector. Public so unit tests
    /// can cross-check expected values.
    pub fn mock_embed(&self, input: &str) -> Vec<f32> {
        let mut vec = vec![0.0_f32; self.dim];
        // FNV-1a 64-bit — deterministic, dependency-free.
        let mut hash: u64 = 0xcbf29ce484222325;
        for (i, b) in input.bytes().enumerate() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(0x100000001b3);
            // Spread bytes across the vector.
            let idx = (i + (hash as usize)) % self.dim;
            // Range [-1, 1].
            vec[idx] += ((hash >> 16) as i32 as f32) / (i32::MAX as f32);
        }
        // Unit-normalize so cosine == dot product on retrieval.
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        vec
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AiProvider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn default_dim(&self) -> usize {
        self.dim
    }

    async fn embed(&self, req: EmbedRequest) -> Result<EmbedResponse, ProviderError> {
        if req.inputs.is_empty() {
            return Err(ProviderError::InvalidRequest(
                "embed called with empty inputs".to_string(),
            ));
        }
        let vectors: Vec<Vec<f32>> = req.inputs.iter().map(|s| self.mock_embed(s)).collect();
        Ok(EmbedResponse {
            vectors,
            total_tokens: 0,
        })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        use std::sync::atomic::Ordering;

        // A pre-installed error takes precedence and short-circuits before we
        // emit any content. Callers use this to assert failure classification.
        if let Some(err) = self.chat_error.lock().unwrap().take() {
            return Err(err);
        }
        if req.messages.is_empty() {
            return Err(ProviderError::InvalidRequest(
                "chat called with empty messages".to_string(),
            ));
        }

        // Pick the script for *this* iteration. If no script is set,
        // synthesize a single-turn echo so the existing default-behaviour
        // tests keep working without explicit scripting. If a script IS
        // set and we've walked past the last iteration, panic — it means
        // a test asked for more turns than it declared.
        let items: Vec<ChatScriptItem> = {
            let guard = self.chat_script.lock().unwrap();
            match guard.as_ref() {
                Some(outer) => {
                    let idx = self.chat_iter.fetch_add(1, Ordering::Relaxed);
                    match outer.get(idx) {
                        Some(inner) => inner.clone(),
                        None => panic!(
                            "MockProvider script exhausted: iteration {idx} requested but only {len} configured",
                            idx = idx,
                            len = outer.len()
                        ),
                    }
                }
                None => {
                    // Unscripted fallback: echo the last user turn as a
                    // single `FinishText` so callers still observe a
                    // clean `stop` terminator.
                    let last_user = req
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.role == ChatRole::User)
                        .map(|m| m.content.clone())
                        .unwrap_or_default();
                    vec![ChatScriptItem::FinishText {
                        content: format!("echo: {last_user}"),
                    }]
                }
            }
        };

        let iter = items.into_iter().map(|step| match step {
            ChatScriptItem::Delta(content) => Ok(ChatDelta {
                content,
                ..Default::default()
            }),
            ChatScriptItem::FinishText { content } => Ok(ChatDelta {
                content,
                finish_reason: Some("stop".into()),
                input_tokens: Some(0),
                output_tokens: Some(0),
                ..Default::default()
            }),
            ChatScriptItem::FinishToolCall { tool_calls } => {
                // Collapse the whole tool call into a single fragment
                // per call-id: the command-layer reassembler must still
                // tolerate the "everything arrives in one fragment" shape.
                let fragments: Vec<ToolCallFragment> = tool_calls
                    .into_iter()
                    .enumerate()
                    .map(|(i, tc)| ToolCallFragment {
                        index: i as u32,
                        id: Some(tc.id),
                        name: Some(tc.name),
                        arguments_delta: Some(tc.arguments),
                    })
                    .collect();
                Ok(ChatDelta {
                    content: String::new(),
                    finish_reason: Some("tool_calls".into()),
                    input_tokens: Some(0),
                    output_tokens: Some(0),
                    tool_call_fragments: Some(fragments),
                })
            }
            ChatScriptItem::Error(err) => Err(err),
        });

        Ok(Box::pin(futures_util::stream::iter(
            iter.collect::<Vec<Result<ChatDelta, ProviderError>>>(),
        )))
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_embed_roundtrip_shape() {
        let p = MockProvider::new();
        let resp = p
            .embed(EmbedRequest {
                model: "mock-v1".to_string(),
                inputs: vec!["hello".to_string(), "world".to_string()],
            })
            .await
            .unwrap();
        assert_eq!(resp.vectors.len(), 2);
        assert_eq!(resp.vectors[0].len(), p.default_dim());
        assert_eq!(resp.vectors[1].len(), p.default_dim());
    }

    #[tokio::test]
    async fn mock_embed_is_deterministic() {
        let p = MockProvider::new();
        let a = p.mock_embed("stable input");
        let b = p.mock_embed("stable input");
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn mock_embed_different_inputs_differ() {
        let p = MockProvider::new();
        let a = p.mock_embed("alpha");
        let b = p.mock_embed("beta");
        // Not asserting full inequality — with 192 dims a byte-level diff is
        // astronomically likely.
        assert_ne!(a, b);
    }

    #[tokio::test]
    async fn mock_embed_is_unit_norm() {
        let p = MockProvider::new();
        let v = p.mock_embed("norm check");
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        // Allow ~1e-5 slack for f32 accumulation.
        assert!((norm - 1.0).abs() < 1e-5, "norm = {norm}");
    }

    #[tokio::test]
    async fn empty_inputs_returns_invalid_request() {
        let p = MockProvider::new();
        let err = p
            .embed(EmbedRequest {
                model: "mock-v1".to_string(),
                inputs: vec![],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    // ── chat (D2b.2) ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn mock_chat_uses_script_when_set() {
        let p = MockProvider::new();
        p.set_chat_script_tokens(vec!["hello ".into(), "world".into()]);
        let stream = p
            .chat_stream(ChatRequest {
                model: "mock".into(),
                messages: vec![ChatTurn::text(ChatRole::User, "hi")],
                ..Default::default()
            })
            .await
            .unwrap();

        let collected = collect_chat_stream(stream).await.unwrap();
        assert_eq!(collected.content, "hello world");
        assert_eq!(collected.finish_reason.as_deref(), Some("stop"));
    }

    #[tokio::test]
    async fn mock_chat_echoes_last_user_turn_without_script() {
        let p = MockProvider::new();
        let stream = p
            .chat_stream(ChatRequest {
                model: "mock".into(),
                messages: vec![
                    ChatTurn::text(ChatRole::System, "ignored"),
                    ChatTurn::text(ChatRole::User, "hello"),
                ],
                ..Default::default()
            })
            .await
            .unwrap();
        let collected = collect_chat_stream(stream).await.unwrap();
        assert!(collected.content.contains("hello"));
        assert_eq!(collected.finish_reason.as_deref(), Some("stop"));
    }

    #[tokio::test]
    async fn mock_chat_surfaces_preloaded_error() {
        let p = MockProvider::new();
        p.set_chat_error(ProviderError::RateLimit {
            retry_after_secs: 5,
            message: "too many".into(),
        });
        let result = p
            .chat_stream(ChatRequest {
                model: "mock".into(),
                messages: vec![ChatTurn::text(ChatRole::User, "hi")],
                ..Default::default()
            })
            .await;
        // `ChatStream` isn't `Debug`, so use a manual match instead of
        // `.unwrap_err()`.
        match result {
            Ok(_) => panic!("expected RateLimit error"),
            Err(ProviderError::RateLimit {
                retry_after_secs,
                message,
            }) => {
                assert_eq!(retry_after_secs, 5);
                assert!(message.contains("too many"));
            }
            Err(other) => panic!("expected RateLimit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn mock_chat_rejects_empty_messages() {
        let p = MockProvider::new();
        let result = p
            .chat_stream(ChatRequest {
                model: "mock".into(),
                messages: vec![],
                ..Default::default()
            })
            .await;
        match result {
            Ok(_) => panic!("expected InvalidRequest"),
            Err(ProviderError::InvalidRequest(_)) => {}
            Err(other) => panic!("expected InvalidRequest, got {other:?}"),
        }
    }

    // ── P3-D5.1 tool-calling protocol types ─────────────────────────────────

    #[test]
    fn chat_turn_text_only_serialization_omits_tool_fields() {
        let turn = ChatTurn::text(ChatRole::Assistant, "hi");
        let json = serde_json::to_string(&turn).unwrap();
        assert!(!json.contains("tool_calls"), "unexpected tool_calls: {json}");
        assert!(
            !json.contains("tool_call_id"),
            "unexpected tool_call_id: {json}"
        );
        // Roundtrip parity.
        let back: ChatTurn = serde_json::from_str(&json).unwrap();
        assert_eq!(back.role, ChatRole::Assistant);
        assert_eq!(back.content, "hi");
        assert!(back.tool_calls.is_none());
        assert!(back.tool_call_id.is_none());
    }

    #[test]
    fn chat_turn_with_tool_calls_roundtrips() {
        let turn = ChatTurn {
            role: ChatRole::Assistant,
            content: String::new(),
            tool_calls: Some(vec![ToolCall {
                id: "call_1".into(),
                name: "search_notes".into(),
                arguments: r#"{"query":"foo"}"#.into(),
            }]),
            tool_call_id: None,
        };
        let json = serde_json::to_string(&turn).unwrap();
        assert!(json.contains("\"tool_calls\""));
        assert!(json.contains("search_notes"));
        let back: ChatTurn = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tool_calls.as_ref().unwrap().len(), 1);
        assert_eq!(back.tool_calls.unwrap()[0].name, "search_notes");
    }

    #[test]
    fn chat_role_tool_serializes_as_lowercase() {
        assert_eq!(serde_json::to_string(&ChatRole::Tool).unwrap(), "\"tool\"");
        let back: ChatRole = serde_json::from_str("\"tool\"").unwrap();
        assert_eq!(back, ChatRole::Tool);
    }

    #[tokio::test]
    async fn mock_chat_per_iteration_script_advances() {
        let p = MockProvider::new();
        p.set_chat_script(vec![
            vec![ChatScriptItem::FinishToolCall {
                tool_calls: vec![ToolCall {
                    id: "call_a".into(),
                    name: "fake".into(),
                    arguments: "{}".into(),
                }],
            }],
            vec![ChatScriptItem::FinishText {
                content: "final answer".into(),
            }],
        ]);

        let req = || ChatRequest {
            model: "mock".into(),
            messages: vec![ChatTurn::text(ChatRole::User, "go")],
            ..Default::default()
        };

        let turn1 = collect_chat_stream(p.chat_stream(req()).await.unwrap())
            .await
            .unwrap();
        assert_eq!(turn1.finish_reason.as_deref(), Some("tool_calls"));

        let turn2 = collect_chat_stream(p.chat_stream(req()).await.unwrap())
            .await
            .unwrap();
        assert_eq!(turn2.finish_reason.as_deref(), Some("stop"));
        assert_eq!(turn2.content, "final answer");
    }

    #[tokio::test]
    #[should_panic(expected = "script exhausted")]
    async fn mock_chat_exhausted_script_panics() {
        let p = MockProvider::new();
        p.set_chat_script(vec![vec![ChatScriptItem::FinishText {
            content: "only one".into(),
        }]]);
        let req = || ChatRequest {
            model: "mock".into(),
            messages: vec![ChatTurn::text(ChatRole::User, "go")],
            ..Default::default()
        };
        // First call consumes the only iteration; second call must panic.
        let _ = p.chat_stream(req()).await;
        let _ = p.chat_stream(req()).await;
    }
}
