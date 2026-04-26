//! OpenAI-compatible HTTP provider — Phase 3-D2a.2.
//!
//! One concrete implementation of [`super::provider::AiProvider`] that speaks
//! the OpenAI `POST /v1/embeddings` wire protocol. The same protocol is
//! accepted by a growing list of hosted + local backends:
//!
//! - **OpenAI** — `https://api.openai.com/v1`
//! - **OpenRouter** — `https://openrouter.ai/api/v1`
//! - **Ollama** — `http://localhost:11434/v1` (bundled OpenAI-compat shim)
//! - **LM Studio / vLLM / Together.ai / Groq / …** — all expose the same
//!   `/embeddings` endpoint shape.
//!
//! Users switch backend by editing `base_url` in Settings — **no new provider
//! code required** for the happy path.
//!
//! ## What this impl handles
//!
//! - `POST {base_url}/embeddings` with `{ model, input: [..] }`
//! - `Bearer {api_key}` header (empty string skips the header, which lets
//!   local Ollama work without a token)
//! - Response shape `{ data: [{ embedding: [f32] }], usage: { total_tokens } }`
//! - HTTP status → `ProviderError` mapping (401 / 429 / 4xx / 5xx)
//! - Dimension auto-detection on first successful call (first vector decides)
//!
//! ## What it does NOT handle
//!
//! - **Retries / backoff** — caller decides (the D2a.3 watcher wraps this
//!   with its own retry policy that respects the embed-batch schedule).
//! - **Chunk batching** — caller is responsible for splitting oversized
//!   `inputs` into provider-limit-safe batches. Good default: ≤ 96 inputs
//!   per call (OpenAI's current limit).
//! - **Streaming** — embeddings aren't a streaming API. `chat()` (D2b) will.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use super::provider::{
    AiProvider, ChatDelta, ChatRequest, ChatRole, ChatStream, EmbedRequest, EmbedResponse,
    ProviderError, ToolCall, ToolCallFragment,
};

/// Default request timeout. Large-ish because first-time Ollama model loads
/// can be slow; set conservatively enough that a genuine hang still errors.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

// ── Provider struct ──────────────────────────────────────────────────────────

/// OpenAI-compatible embedding provider.
///
/// Constructed per embed batch — the underlying `reqwest::Client` uses a
/// connection pool internally, so paying the ~10 µs Client build cost each
/// call is fine for the D2a.2 throughput (a few embeddings per minute).
/// D2a.3's watcher will share a long-lived Client if profiling warrants.
pub struct OpenAiProvider {
    base_url: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
    /// Assumed vector dim. Populated to 1536 (OpenAI `text-embedding-3-small`
    /// default) on construction; `embed()` will auto-update on first response
    /// so downstream calls can validate against the real dim.
    default_dim: std::sync::atomic::AtomicUsize,
}

impl OpenAiProvider {
    /// Build a provider. `base_url` should already be the full `/v1` root
    /// (no trailing slash). `api_key` may be empty for Ollama-style local
    /// backends that ignore auth.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .user_agent("mynotes/0.1 (+https://github.com)")
            .build()
            .expect("reqwest::Client::build with static config never fails");
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            api_key: api_key.into(),
            client,
            default_dim: std::sync::atomic::AtomicUsize::new(1536),
        }
    }

    /// Override the request timeout. Exposed for the `test_connection` flow
    /// which wants a faster failure (10 s) than a real embed batch.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("mynotes/0.1 (+https://github.com)")
            .build()
            .expect("reqwest::Client::build with static config never fails");
        self
    }

    fn embeddings_url(&self) -> String {
        format!("{}/embeddings", self.base_url)
    }

    fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

// ── Wire shapes ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct EmbeddingRequestBody<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbeddingResponseBody {
    data: Vec<EmbeddingDatum>,
    #[serde(default)]
    usage: Option<EmbeddingUsage>,
}

#[derive(Deserialize)]
struct EmbeddingDatum {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbeddingUsage {
    /// OpenAI returns this but we don't distinguish prompt-vs-total at the
    /// call site; keep the field so serde can parse the full response
    /// without failing on unknown fields.
    #[serde(default)]
    #[allow(dead_code)]
    prompt_tokens: u32,
    #[serde(default)]
    total_tokens: u32,
}

/// Body shape sent by OpenAI / OpenRouter / Azure OpenAI on error. Every
/// field is optional because local backends (Ollama) send plain-text
/// errors without this envelope.
#[derive(Deserialize)]
struct ErrorResponseBody {
    #[serde(default)]
    error: Option<ErrorBody>,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    message: Option<String>,
    #[serde(default, rename = "type")]
    _type: Option<String>,
}

/// Best-effort extraction of a user-facing error string from an HTTP body.
/// Tries the OpenAI-shaped envelope first, falls back to raw text.
fn extract_error_message(body: &str) -> String {
    if let Ok(envelope) = serde_json::from_str::<ErrorResponseBody>(body) {
        if let Some(msg) = envelope.error.and_then(|e| e.message) {
            return msg;
        }
    }
    // Strip trailing whitespace + bound the excerpt so a runaway server
    // can't flood our notice stack.
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "(empty response body)".to_string()
    } else if trimmed.len() > 400 {
        format!("{}…", &trimmed[..400])
    } else {
        trimmed.to_string()
    }
}

/// Map an HTTP status + response body into a [`ProviderError`].
///
/// Pulled out as a free function so unit tests can verify the classification
/// without any actual HTTP round-trip.
pub(crate) fn classify_http_error(status: StatusCode, body: &str) -> ProviderError {
    let msg = extract_error_message(body);
    match status.as_u16() {
        401 | 403 => ProviderError::Auth(msg),
        429 => {
            // `Retry-After` header would be more authoritative, but we don't
            // have access to it here. Default 30 s is a safe back-off.
            ProviderError::RateLimit {
                retry_after_secs: 30,
                message: msg,
            }
        }
        400..=499 => ProviderError::InvalidRequest(msg),
        500..=599 => ProviderError::Other(format!("server {status}: {msg}")),
        _ => ProviderError::Other(format!("unexpected status {status}: {msg}")),
    }
}

// ── Chat wire shapes (D2b.2) ─────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatCompletionRequestBody<'a> {
    model: &'a str,
    messages: Vec<ChatWireMessage<'a>>,
    stream: bool,
    /// Asks OpenAI to include one final `usage` chunk at the end of the
    /// stream. Ignored by Ollama / LM Studio / older vLLM, which simply
    /// never emit the chunk — harmless.
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptionsBody>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    /// P3-D5.1 tool-calling advert. Omitted entirely when empty so the
    /// wire payload stays identical to the pre-D5 shape for
    /// non-agentic chat flows.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAIToolSchema<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'static str>,
}

/// OpenAI-shaped wrapper around a [`ToolDefinition`]. The provider
/// protocol nests the function spec inside `{type, function: …}`; the
/// wrapper exists so `serde_json` can drive the shape directly from a
/// vec of borrowed [`ToolDefinition`]s without cloning.
#[derive(Serialize)]
struct OpenAIToolSchema<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OpenAIFunctionSpec<'a>,
}

#[derive(Serialize)]
struct OpenAIFunctionSpec<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

/// Serialized chat message. Tool-calling adds three conditionally-
/// serialized fields: `tool_calls` (assistant → wants to invoke), and
/// `tool_call_id` (tool → matches back to an assistant call). `content`
/// remains required even on tool-call turns (OpenAI expects it as empty
/// string, not omitted).
#[derive(Serialize)]
struct ChatWireMessage<'a> {
    role: &'static str,
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ChatWireToolCall<'a>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
}

/// Assistant → server: the historical tool-call request to replay on
/// every subsequent turn so the model can see its own earlier
/// decisions. Matches the shape OpenAI emits back in streaming deltas.
#[derive(Serialize)]
struct ChatWireToolCall<'a> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ChatWireToolCallFunction<'a>,
}

#[derive(Serialize)]
struct ChatWireToolCallFunction<'a> {
    name: &'a str,
    arguments: &'a str,
}

#[derive(Serialize)]
struct StreamOptionsBody {
    include_usage: bool,
}

/// A single `data: {...}` chunk from OpenAI's chat SSE stream.
#[derive(Deserialize)]
struct ChatStreamChunk {
    #[serde(default)]
    choices: Vec<ChatStreamChoice>,
    #[serde(default)]
    usage: Option<ChatStreamUsage>,
}

#[derive(Deserialize)]
struct ChatStreamChoice {
    #[serde(default)]
    delta: ChatStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct ChatStreamDelta {
    #[serde(default)]
    content: Option<String>,
    /// `role` arrives on the first delta only; we don't need it since the
    /// caller owns the turn structure. Kept to make serde tolerate the field.
    #[serde(default, rename = "role")]
    _role: Option<String>,
    /// P3-D5.1: raw per-chunk tool-call fragments. A single logical
    /// tool call streams in as N entries here over several SSE frames
    /// (same `index`, growing `function.arguments`). We propagate
    /// them unreassembled up to the command layer via
    /// [`ChatDelta::tool_call_fragments`] so BTreeMap-keyed
    /// accumulation can happen there, closer to the chat-loop state.
    #[serde(default)]
    tool_calls: Option<Vec<OpenAIToolCallFragment>>,
}

/// Raw per-SSE-frame tool-call fragment as OpenAI emits it.
#[derive(Default, Deserialize)]
struct OpenAIToolCallFragment {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "type")]
    _kind: Option<String>,
    #[serde(default)]
    function: Option<OpenAIToolCallFragmentFunction>,
}

#[derive(Default, Deserialize)]
struct OpenAIToolCallFragmentFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct ChatStreamUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    #[allow(dead_code)]
    total_tokens: u32,
}

/// Parse one OpenAI-style `data: …` payload into a [`ChatDelta`].
///
/// `"[DONE]"` maps to `Ok(None)` so the caller terminates the stream.
/// Pulled out as `pub(crate)` so unit tests can drive it directly without
/// spinning up an HTTP server.
///
/// P3-D5.1: also translates per-frame `tool_calls[]` fragments into
/// [`ChatDelta::tool_call_fragments`]. This layer stays fragment-level —
/// per-call reassembly happens in the chat command loop, which needs
/// the BTreeMap across multiple deltas anyway.
pub(crate) fn parse_sse_data(payload: &str) -> Result<Option<ChatDelta>, ProviderError> {
    if payload == "[DONE]" {
        return Ok(None);
    }
    let chunk: ChatStreamChunk = serde_json::from_str(payload)
        .map_err(|e| ProviderError::Other(format!("invalid SSE data: {e}")))?;
    let mut delta = ChatDelta::default();
    if let Some(choice) = chunk.choices.into_iter().next() {
        if let Some(c) = choice.delta.content {
            delta.content = c;
        }
        delta.finish_reason = choice.finish_reason;
        if let Some(fragments) = choice.delta.tool_calls {
            let translated: Vec<ToolCallFragment> = fragments
                .into_iter()
                .map(|f| ToolCallFragment {
                    index: f.index,
                    id: f.id,
                    name: f.function.as_ref().and_then(|fn_| fn_.name.clone()),
                    arguments_delta: f.function.and_then(|fn_| fn_.arguments),
                })
                .collect();
            if !translated.is_empty() {
                delta.tool_call_fragments = Some(translated);
            }
        }
    }
    if let Some(u) = chunk.usage {
        delta.input_tokens = Some(u.prompt_tokens);
        delta.output_tokens = Some(u.completion_tokens);
    }
    Ok(Some(delta))
}

/// Streaming reassembler for tool-call fragments. Feed it each
/// [`ToolCallFragment`] slice that arrives on `ChatDelta.tool_call_fragments`;
/// call [`ToolCallAccumulator::finish`] once `finish_reason = "tool_calls"`
/// lands to rebuild the complete per-call [`ToolCall`] vec in `index`
/// order.
///
/// BTreeMap keyed by `index` is defensive: OpenAI currently emits
/// fragments in-order, but spec-wise two call indices can interleave
/// across frames. BTreeMap gives us ordered iteration for free once
/// all fragments are in.
#[derive(Default)]
pub(crate) struct ToolCallAccumulator {
    slots: std::collections::BTreeMap<u32, ToolCallSlot>,
}

#[derive(Default)]
struct ToolCallSlot {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl ToolCallAccumulator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn ingest(&mut self, fragments: &[ToolCallFragment]) {
        for frag in fragments {
            let slot = self.slots.entry(frag.index).or_default();
            if let Some(id) = &frag.id {
                slot.id = Some(id.clone());
            }
            if let Some(name) = &frag.name {
                slot.name = Some(name.clone());
            }
            if let Some(args) = &frag.arguments_delta {
                slot.arguments.push_str(args);
            }
        }
    }

    /// Rebuild complete [`ToolCall`]s in `index` order. Slots that
    /// never received an `id` are skipped — OpenAI promises the id in
    /// the first fragment, so a missing one means the stream broke
    /// mid-fragment and we'd rather drop the incomplete call than
    /// synthesize a fake id the model later can't correlate.
    pub(crate) fn finish(self) -> Vec<ToolCall> {
        self.slots
            .into_values()
            .filter_map(|slot| {
                Some(ToolCall {
                    id: slot.id?,
                    name: slot.name.unwrap_or_default(),
                    arguments: slot.arguments,
                })
            })
            .collect()
    }

    /// True when no fragments have been ingested yet.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// Find the byte offset + delimiter length of the next SSE event boundary
/// (`\n\n` or `\r\n\r\n`) in `buf`, or `None` if no complete event has
/// arrived yet.
pub(crate) fn find_event_end(buf: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some((i, 2));
        }
        if i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
            return Some((i, 4));
        }
        i += 1;
    }
    None
}

fn wire_role(role: ChatRole) -> &'static str {
    match role {
        ChatRole::System => "system",
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
    }
}

/// Build the vector of tool-call wire fragments OpenAI expects replayed
/// on every subsequent request whose history contains an assistant
/// turn that previously requested tools. Returns `None` for turns with
/// no tool calls so the serialized JSON omits the field cleanly.
fn wire_tool_calls<'a>(calls: Option<&'a Vec<ToolCall>>) -> Option<Vec<ChatWireToolCall<'a>>> {
    let calls = calls?;
    if calls.is_empty() {
        return None;
    }
    Some(
        calls
            .iter()
            .map(|c| ChatWireToolCall {
                id: &c.id,
                kind: "function",
                function: ChatWireToolCallFunction {
                    name: &c.name,
                    arguments: &c.arguments,
                },
            })
            .collect(),
    )
}

// ── AiProvider impl ──────────────────────────────────────────────────────────

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn default_dim(&self) -> usize {
        self.default_dim.load(std::sync::atomic::Ordering::Relaxed)
    }

    async fn embed(&self, req: EmbedRequest) -> Result<EmbedResponse, ProviderError> {
        if req.inputs.is_empty() {
            return Err(ProviderError::InvalidRequest(
                "embed called with empty inputs".into(),
            ));
        }

        let model = if req.model.is_empty() {
            self.model.clone()
        } else {
            req.model.clone()
        };

        let url = self.embeddings_url();
        let mut request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }

        let body = EmbeddingRequestBody {
            model: &model,
            input: &req.inputs,
        };
        let resp = request.json(&body).send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                ProviderError::Network(e.to_string())
            } else {
                ProviderError::Other(e.to_string())
            }
        })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(classify_http_error(status, &text));
        }

        let parsed: EmbeddingResponseBody = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("invalid response body: {e}")))?;

        if parsed.data.len() != req.inputs.len() {
            return Err(ProviderError::Other(format!(
                "provider returned {} vectors for {} inputs",
                parsed.data.len(),
                req.inputs.len()
            )));
        }

        // Auto-update default_dim on the first well-formed vector we see.
        if let Some(first) = parsed.data.first() {
            if !first.embedding.is_empty() {
                self.default_dim
                    .store(first.embedding.len(), std::sync::atomic::Ordering::Relaxed);
            }
        }

        let vectors: Vec<Vec<f32>> = parsed.data.into_iter().map(|d| d.embedding).collect();
        let total_tokens = parsed.usage.map(|u| u.total_tokens).unwrap_or(0);

        Ok(EmbedResponse {
            vectors,
            total_tokens,
        })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        if req.messages.is_empty() {
            return Err(ProviderError::InvalidRequest(
                "chat called with empty messages".into(),
            ));
        }
        let model = if req.model.is_empty() {
            self.model.clone()
        } else {
            req.model.clone()
        };

        let url = self.chat_completions_url();
        let mut request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream");
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }

        let messages: Vec<ChatWireMessage<'_>> = req
            .messages
            .iter()
            .map(|m| ChatWireMessage {
                role: wire_role(m.role),
                content: &m.content,
                tool_calls: wire_tool_calls(m.tool_calls.as_ref()),
                tool_call_id: m.tool_call_id.as_deref(),
            })
            .collect();

        let tools: Vec<OpenAIToolSchema<'_>> = req
            .tools
            .iter()
            .map(|t| OpenAIToolSchema {
                kind: "function",
                function: OpenAIFunctionSpec {
                    name: &t.name,
                    description: &t.description,
                    parameters: &t.parameters,
                },
            })
            .collect();
        let tool_choice: Option<&'static str> = if tools.is_empty() { None } else { Some("auto") };

        let body = ChatCompletionRequestBody {
            model: &model,
            messages,
            stream: true,
            stream_options: Some(StreamOptionsBody {
                include_usage: true,
            }),
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            tools,
            tool_choice,
        };

        let resp = request.json(&body).send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                ProviderError::Network(e.to_string())
            } else {
                ProviderError::Other(e.to_string())
            }
        })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(classify_http_error(status, &text));
        }

        // Bridge reqwest's byte stream onto a tokio mpsc channel. Parsing
        // runs in a spawned task so we can own a mutable byte buffer
        // without fighting the `Stream` trait's borrow shape. The channel
        // also gives natural cancellation: dropping the returned stream
        // closes `tx` and the spawned task exits on its next send.
        let (tx, rx) = tokio::sync::mpsc::channel::<Result<ChatDelta, ProviderError>>(16);
        let mut bytes_stream = resp.bytes_stream();
        tokio::spawn(async move {
            use futures_util::StreamExt;

            let mut buf: Vec<u8> = Vec::with_capacity(4096);
            let mut terminated = false;

            while let Some(chunk) = bytes_stream.next().await {
                let chunk = match chunk {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Err(ProviderError::Network(e.to_string()))).await;
                        return;
                    }
                };
                buf.extend_from_slice(&chunk);

                while let Some((end, delim_len)) = find_event_end(&buf) {
                    let event_bytes: Vec<u8> = buf.drain(..end).collect();
                    buf.drain(..delim_len);
                    for line in event_bytes.split(|&b| b == b'\n') {
                        let line = std::str::from_utf8(line).unwrap_or("");
                        let line = line.trim_end_matches('\r');
                        let Some(rest) = line.strip_prefix("data:") else {
                            // Ignore SSE comments, event: / id: / retry:
                            // lines — OpenAI only uses `data:`.
                            continue;
                        };
                        let payload = rest.trim_start();
                        if payload.is_empty() {
                            continue;
                        }
                        match parse_sse_data(payload) {
                            Ok(Some(delta)) => {
                                if tx.send(Ok(delta)).await.is_err() {
                                    return;
                                }
                            }
                            Ok(None) => {
                                terminated = true;
                                break;
                            }
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                return;
                            }
                        }
                    }
                    if terminated {
                        return;
                    }
                }
            }

            // Flush a trailing event that wasn't followed by a delimiter
            // (spec-legal but rare in practice).
            if !buf.is_empty() {
                for line in buf.split(|&b| b == b'\n') {
                    let line = std::str::from_utf8(line).unwrap_or("");
                    let line = line.trim_end_matches('\r');
                    let Some(rest) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let payload = rest.trim_start();
                    if payload.is_empty() || payload == "[DONE]" {
                        return;
                    }
                    if let Ok(Some(delta)) = parse_sse_data(payload) {
                        let _ = tx.send(Ok(delta)).await;
                    }
                }
            }
        });

        let stream = futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        });
        Ok(Box::pin(stream))
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// We exercise the pure pieces (URL composition, error body parsing, status
// classification) without a real HTTP round-trip. Live-network calls are
// verified manually by the user via the `ai_provider_test_connection` IPC;
// adding a mock HTTP server would add dev dependencies (`wiremock` etc.)
// for a payoff dominated by manual smoke test.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeddings_url_strips_trailing_slash() {
        let p = OpenAiProvider::new("https://api.example.com/v1/", "m", "k");
        assert_eq!(p.embeddings_url(), "https://api.example.com/v1/embeddings");
    }

    #[test]
    fn embeddings_url_preserves_path() {
        let p = OpenAiProvider::new("http://localhost:11434/v1", "m", "k");
        assert_eq!(p.embeddings_url(), "http://localhost:11434/v1/embeddings");
    }

    #[test]
    fn classify_401_is_auth() {
        let err = classify_http_error(
            StatusCode::UNAUTHORIZED,
            r#"{"error":{"message":"Incorrect API key"}}"#,
        );
        match err {
            ProviderError::Auth(msg) => assert!(msg.contains("Incorrect")),
            _ => panic!("expected Auth, got {err:?}"),
        }
    }

    #[test]
    fn classify_403_is_also_auth() {
        let err = classify_http_error(StatusCode::FORBIDDEN, "");
        assert!(matches!(err, ProviderError::Auth(_)));
    }

    #[test]
    fn classify_429_is_rate_limit() {
        let err = classify_http_error(
            StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"rate limited"}}"#,
        );
        match err {
            ProviderError::RateLimit {
                retry_after_secs,
                message,
            } => {
                assert!(retry_after_secs > 0);
                assert!(message.contains("rate limited"));
            }
            _ => panic!("expected RateLimit, got {err:?}"),
        }
    }

    #[test]
    fn classify_400_is_invalid_request() {
        let err = classify_http_error(
            StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"model not found"}}"#,
        );
        match err {
            ProviderError::InvalidRequest(msg) => assert!(msg.contains("model")),
            _ => panic!("expected InvalidRequest, got {err:?}"),
        }
    }

    #[test]
    fn classify_500_is_other() {
        let err = classify_http_error(StatusCode::INTERNAL_SERVER_ERROR, "oops");
        match err {
            ProviderError::Other(msg) => assert!(msg.contains("500")),
            _ => panic!("expected Other, got {err:?}"),
        }
    }

    #[test]
    fn extract_error_from_openai_envelope() {
        let msg = extract_error_message(r#"{"error":{"message":"hello","type":"foo"}}"#);
        assert_eq!(msg, "hello");
    }

    #[test]
    fn extract_error_from_plain_text_fallback() {
        let msg = extract_error_message("ollama: model not pulled");
        assert!(msg.contains("ollama"));
    }

    #[test]
    fn extract_error_bounds_giant_body() {
        let big = "x".repeat(5000);
        let msg = extract_error_message(&big);
        // 400 ASCII chars + `…` (which is 1 char but 3 UTF-8 bytes).
        assert_eq!(msg.chars().count(), 401);
        assert!(msg.ends_with('…'));
    }

    #[test]
    fn extract_error_handles_empty() {
        assert_eq!(extract_error_message(""), "(empty response body)");
        assert_eq!(extract_error_message("   \n\t"), "(empty response body)");
    }

    #[test]
    fn response_body_parses_openai_shape() {
        let raw = r#"{
            "object": "list",
            "data": [
                { "object": "embedding", "index": 0, "embedding": [0.1, 0.2, 0.3] },
                { "object": "embedding", "index": 1, "embedding": [0.4, 0.5, 0.6] }
            ],
            "model": "text-embedding-3-small",
            "usage": { "prompt_tokens": 5, "total_tokens": 5 }
        }"#;
        let parsed: EmbeddingResponseBody = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.data.len(), 2);
        assert_eq!(parsed.data[0].embedding.len(), 3);
        assert_eq!(parsed.usage.unwrap().total_tokens, 5);
    }

    #[test]
    fn response_body_parses_without_usage() {
        // Ollama omits usage; we must still parse.
        let raw = r#"{"data":[{"embedding":[1.0,2.0]}]}"#;
        let parsed: EmbeddingResponseBody = serde_json::from_str(raw).unwrap();
        assert!(parsed.usage.is_none());
        assert_eq!(parsed.data[0].embedding, vec![1.0, 2.0]);
    }

    #[tokio::test]
    async fn empty_inputs_errors_without_http() {
        let p = OpenAiProvider::new("http://127.0.0.1:1", "m", "k");
        let err = p
            .embed(EmbedRequest {
                model: "m".into(),
                inputs: vec![],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, ProviderError::InvalidRequest(_)));
    }

    #[tokio::test]
    async fn chat_empty_messages_errors_without_http() {
        let p = OpenAiProvider::new("http://127.0.0.1:1", "m", "k");
        let result = p
            .chat_stream(ChatRequest {
                model: "m".into(),
                messages: vec![],
                ..Default::default()
            })
            .await;
        // `ChatStream` isn't `Debug`; match manually.
        match result {
            Ok(_) => panic!("expected InvalidRequest"),
            Err(ProviderError::InvalidRequest(msg)) => assert!(msg.contains("empty")),
            Err(other) => panic!("expected InvalidRequest, got {other:?}"),
        }
    }

    #[test]
    fn chat_completions_url_is_appended_once() {
        let p = OpenAiProvider::new("https://api.example.com/v1/", "m", "k");
        assert_eq!(
            p.chat_completions_url(),
            "https://api.example.com/v1/chat/completions"
        );
    }

    // ── SSE parser (D2b.2) ──────────────────────────────────────────────────

    #[test]
    fn sse_parser_extracts_content_delta() {
        let payload = r#"{"id":"x","choices":[{"index":0,"delta":{"role":"assistant","content":"Hel"},"finish_reason":null}]}"#;
        let delta = parse_sse_data(payload).unwrap().unwrap();
        assert_eq!(delta.content, "Hel");
        assert!(delta.finish_reason.is_none());
        assert!(delta.input_tokens.is_none());
    }

    #[test]
    fn sse_parser_extracts_finish_reason_chunk() {
        let payload = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let delta = parse_sse_data(payload).unwrap().unwrap();
        assert_eq!(delta.content, "");
        assert_eq!(delta.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn sse_parser_extracts_usage_trailer() {
        let payload = r#"{"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":34,"total_tokens":46}}"#;
        let delta = parse_sse_data(payload).unwrap().unwrap();
        assert_eq!(delta.input_tokens, Some(12));
        assert_eq!(delta.output_tokens, Some(34));
    }

    #[test]
    fn sse_parser_handles_done_sentinel() {
        assert!(parse_sse_data("[DONE]").unwrap().is_none());
    }

    #[test]
    fn sse_parser_rejects_invalid_json() {
        let err = parse_sse_data("not-json").unwrap_err();
        match err {
            ProviderError::Other(msg) => assert!(msg.contains("invalid SSE data")),
            _ => panic!("expected Other, got {err:?}"),
        }
    }

    #[test]
    fn find_event_end_matches_double_newline() {
        let buf = b"data: a\ndata: b\n\ntail";
        let (end, delim) = find_event_end(buf).unwrap();
        assert_eq!(delim, 2);
        assert_eq!(&buf[..end], b"data: a\ndata: b");
    }

    #[test]
    fn find_event_end_matches_crlf_crlf() {
        let buf = b"data: a\r\n\r\ntail";
        let (end, delim) = find_event_end(buf).unwrap();
        assert_eq!(delim, 4);
        assert_eq!(&buf[..end], b"data: a");
    }

    #[test]
    fn find_event_end_returns_none_when_incomplete() {
        assert!(find_event_end(b"data: partial").is_none());
        assert!(find_event_end(b"data: a\n").is_none());
    }

    // ── P3-D5.1 tool-calling SSE plumbing ───────────────────────────────────

    #[test]
    fn sse_parser_extracts_tool_call_fragment_first_frame() {
        // First frame carries id + name + opening args.
        let payload = r#"{"id":"x","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"search_notes","arguments":"{\""}}]},"finish_reason":null}]}"#;
        let delta = parse_sse_data(payload).unwrap().unwrap();
        let frags = delta.tool_call_fragments.as_ref().unwrap();
        assert_eq!(frags.len(), 1);
        assert_eq!(frags[0].index, 0);
        assert_eq!(frags[0].id.as_deref(), Some("call_abc"));
        assert_eq!(frags[0].name.as_deref(), Some("search_notes"));
        assert_eq!(frags[0].arguments_delta.as_deref(), Some("{\""));
        assert!(delta.finish_reason.is_none());
    }

    #[test]
    fn sse_parser_extracts_tool_calls_finish_reason() {
        let payload = r#"{"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#;
        let delta = parse_sse_data(payload).unwrap().unwrap();
        assert_eq!(delta.finish_reason.as_deref(), Some("tool_calls"));
        assert!(delta.tool_call_fragments.is_none());
    }

    #[test]
    fn tool_call_accumulator_reassembles_fragments_across_frames() {
        // Three frames: {id, name, "{"} then {"\"q\":"} then {"\"x\"}"}
        let frames: Vec<&str> = vec![
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"search","arguments":"{"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"q\":"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"x\"}"}}]},"finish_reason":null}]}"#,
        ];
        let mut acc = ToolCallAccumulator::new();
        assert!(acc.is_empty());
        for f in frames {
            let delta = parse_sse_data(f).unwrap().unwrap();
            if let Some(fragments) = delta.tool_call_fragments.as_ref() {
                acc.ingest(fragments);
            }
        }
        let calls = acc.finish();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[0].name, "search");
        assert_eq!(calls[0].arguments, r#"{"q":"x"}"#);
    }

    #[test]
    fn tool_call_accumulator_drops_slots_missing_id() {
        // Fragments arrived for index 0 but no id ever set → drop
        // that slot rather than synthesize a bogus id.
        let mut acc = ToolCallAccumulator::new();
        acc.ingest(&[ToolCallFragment {
            index: 0,
            id: None,
            name: Some("orphan".into()),
            arguments_delta: Some("{}".into()),
        }]);
        assert!(acc.finish().is_empty());
    }

    #[test]
    fn tool_call_accumulator_orders_by_index() {
        // Fragments arrive in reverse index order; the accumulator
        // must still emit by ascending index at `finish()`.
        let mut acc = ToolCallAccumulator::new();
        acc.ingest(&[
            ToolCallFragment {
                index: 1,
                id: Some("call_b".into()),
                name: Some("second".into()),
                arguments_delta: Some("{}".into()),
            },
            ToolCallFragment {
                index: 0,
                id: Some("call_a".into()),
                name: Some("first".into()),
                arguments_delta: Some("{}".into()),
            },
        ]);
        let calls = acc.finish();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[1].id, "call_b");
    }
}
