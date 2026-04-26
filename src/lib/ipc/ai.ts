/**
 * AI-assist IPC wrappers — Phase 3-D.
 *
 * The D1 related-notes path is local-only; D2a functions below also expose
 * provider-backed embedding flows plus their structured failure surfaces.
 */
import { invoke } from '@tauri-apps/api/core';

// ── Types ─────────────────────────────────────────────────────────────────────

/** Breakdown of which heuristic signals fired for a related-note entry. */
export interface RelatedSignals {
  /** Shared-tag overlap fraction in [0, 1]. */
  tag_overlap: number;
  /** True if a wiki-link exists in either direction. */
  direct_link: boolean;
  /** True if at least one other note links to both. */
  co_cited: boolean;
  /** Cosine over note-level summed chunk embeddings, in [0, 1]. */
  embedding_cosine: number;
  /** Days-since-updated / 30, clamped to [0, 1]. Higher = older. */
  staleness: number;
}

/** One entry in the "related notes" result list. */
export interface RelatedNote {
  /** Vault-relative path. */
  path: string;
  /** Frontmatter title, or null. */
  title: string | null;
  /** Note type (inbox / note / moc / daily / project / …), or null. */
  note_type: string | null;
  /** ISO-8601 last-updated string, or null. */
  updated: string | null;
  /** Composite relevance score. Higher = more related. Display as sort key only. */
  score: number;
  /** Per-signal breakdown for debug / hover tooltip. */
  signals: RelatedSignals;
}

// ── Commands ──────────────────────────────────────────────────────────────────

/**
 * Return up to `limit` notes most related to `srcRelPath`, ranked by the
 * local-index scoring model (tag overlap · direct links · co-citation ·
 * embedding cosine · staleness when embeddings exist).
 *
 * Returns an empty array (not an error) when the vault has no index yet or
 * the source note isn't in the index. Only rejects on path-escape / DB errors.
 *
 * D2 will add embedding cosine similarity without changing this signature.
 */
export async function aiRelatedNotes(srcRelPath: string, limit = 10): Promise<RelatedNote[]> {
  return invoke<RelatedNote[]>('ai_related_notes', { srcRelPath, limit });
}

/**
 * Persist the AI-assist panel visibility toggle to `app-config.json`.
 * Returns the updated config snapshot (same shape as `appConfigGet`).
 */
export async function appConfigSetAiEnabled(enabled: boolean): Promise<void> {
  await invoke('app_config_set_ai_enabled', { enabled });
}

// ── D2a.2 provider-config commands ────────────────────────────────────────────

/** Error categories surfaced by `ai_provider_test_connection`. */
export type ProviderErrorKind = 'network' | 'auth' | 'rate_limit' | 'invalid_request' | 'other';

/**
 * Result of a `aiProviderTestConnection` round-trip. We use a struct
 * (not Promise-reject) so the UI can render both success + failure in
 * the same notice without branching at the IPC boundary.
 */
export interface ProviderTestResult {
  ok: boolean;
  /** Present on success: detected vector dimension. */
  dim?: number;
  /** Present on success: total tokens reported (may be 0 if provider omits). */
  total_tokens?: number;
  /** Present on failure. */
  error_kind?: ProviderErrorKind;
  /** Present on failure: human-readable detail. */
  error_message?: string;
  /** Present on rate-limit failure. */
  retry_after_secs?: number;
}

/**
 * Persist the provider config. Non-empty `apiKey` is stored in the OS
 * keystore under the provider kind; pass `""` to keep the existing key
 * untouched (useful when the user only edits `baseUrl` / `embedModel` /
 * `chatModel`).
 *
 * `chatModel` is optional — pass `null` (or omit) to leave it empty and
 * run in "embeddings-only" mode.
 */
export async function aiProviderSetConfig(
  kind: string,
  baseUrl: string,
  embedModel: string,
  chatModel: string | null,
  apiKey: string
): Promise<void> {
  await invoke('ai_provider_set_config', {
    kind,
    baseUrl,
    embedModel,
    chatModel,
    apiKey
  });
}

/** Drop the persisted provider AND any stored keychain entry. */
export async function aiProviderClearConfig(): Promise<void> {
  await invoke('ai_provider_clear_config');
}

/**
 * Returns `true` if an API key is currently stored in the OS keystore for
 * the configured provider. Safe to call even when no provider is configured
 * (always returns `false` in that case).
 */
export async function aiProviderHasApiKey(): Promise<boolean> {
  return await invoke<boolean>('ai_provider_has_api_key');
}

/**
 * Validate the provider configuration with a one-token round-trip.
 *
 * All arguments are optional — when omitted, the persisted config is used.
 * Pass explicit values to validate **unsaved** edits in Settings without
 * touching disk or keyring state.
 *
 * `apiKeyOverride`:
 * - omitted → read key from keyring under the saved provider kind
 * - empty string → anonymous request (Ollama-style local backends)
 * - non-empty → use this key for this one call only; never persisted
 */
export async function aiProviderTestConnection(
  opts: {
    kind?: string;
    baseUrl?: string;
    embedModel?: string;
    apiKeyOverride?: string;
  } = {}
): Promise<ProviderTestResult> {
  return await invoke<ProviderTestResult>('ai_provider_test_connection', {
    kind: opts.kind ?? null,
    baseUrl: opts.baseUrl ?? null,
    embedModel: opts.embedModel ?? null,
    apiKeyOverride: opts.apiKeyOverride ?? null
  });
}

/**
 * Result of a `aiProviderTestChatConnection` round-trip. Same reason for
 * being a struct (not Promise-reject) as [`ProviderTestResult`]: Settings
 * wants to render both success + failure inline.
 */
export interface ChatProviderTestResult {
  ok: boolean;
  /** Present on success: truncated reply from the model (≤ 200 chars). */
  reply?: string;
  /** Present on success when the backend reported `usage`. */
  input_tokens?: number;
  output_tokens?: number;
  error_kind?: ProviderErrorKind;
  error_message?: string;
  retry_after_secs?: number;
}

/**
 * Validate the **chat** endpoint with a tiny one-shot conversation. All
 * arguments are optional; persisted config fills in anything omitted.
 * Useful for the Settings "测试聊天" button so users can spot-check that
 * their chat model is reachable without having to open the chat panel
 * and type something.
 */
export async function aiProviderTestChatConnection(
  opts: {
    kind?: string;
    baseUrl?: string;
    chatModel?: string;
    apiKeyOverride?: string;
  } = {}
): Promise<ChatProviderTestResult> {
  return await invoke<ChatProviderTestResult>('ai_provider_test_chat_connection', {
    kind: opts.kind ?? null,
    baseUrl: opts.baseUrl ?? null,
    chatModel: opts.chatModel ?? null,
    apiKeyOverride: opts.apiKeyOverride ?? null
  });
}

// ── D2a.3a embed commands ────────────────────────────────────────────────────

/**
 * Why a `ai_embed_note` call short-circuited without hitting the provider.
 * Both reasons are success states (no error banner needed).
 */
export type EmbedSkipReason = 'up_to_date' | 'empty';
export type EmbedFailureKind = ProviderErrorKind;

/** Result of a single-note embed run. */
export interface EmbedOutcome {
  /** Vault-relative path, as sent. */
  rel_path: string;
  /** Number of chunks written this run. `0` when skipped. */
  chunks_embedded: number;
  /** Provider-reported total tokens (may be `0` if provider omits usage). */
  tokens_used: number;
  /** Present only when the run was skipped — inspect before reading counts. */
  skipped?: EmbedSkipReason;
}

export interface EmbedFailure {
  kind: EmbedFailureKind;
  message: string;
  retry_after_secs?: number;
  store_unchanged: boolean;
}

export interface EmbedNoteResult {
  ok: boolean;
  outcome?: EmbedOutcome;
  failure?: EmbedFailure;
}

/** Aggregate counters surfaced to the Settings "AI index" section. */
export interface EmbeddingStats {
  chunk_count: number;
  note_count: number;
  model_count: number;
}

export type CostEstimateKind = 'local' | 'open_ai_public_pricing' | 'unknown';

/** Dry-run summary for a full-vault initialization run. */
export interface VaultEmbedPreview {
  note_count_total: number;
  note_count_to_embed: number;
  note_count_up_to_date: number;
  note_count_empty: number;
  chunk_count_to_embed: number;
  token_count_estimated: number;
  model?: string | null;
  cost_estimate_kind: CostEstimateKind;
  cost_usd_estimate?: number;
  notes_preview: string[];
}

/** Result of executing a full-vault initialization run. */
export interface VaultEmbedRunResult {
  note_count_total: number;
  note_count_embedded: number;
  note_count_up_to_date: number;
  note_count_empty: number;
  note_count_failed: number;
  note_count_not_attempted: number;
  chunk_count_embedded: number;
  token_count_used: number;
  aborted_early: boolean;
  aborted_error_kind?: EmbedFailureKind;
  aborted_error_message?: string;
  aborted_retry_after_secs?: number;
  failure_preview: string[];
}

/**
 * Embed one note end-to-end: read → chunk → embed → persist. Provider and
 * config failures are returned structurally so the UI can render targeted
 * guidance instead of a generic red banner.
 */
export async function aiEmbedNote(relPath: string): Promise<EmbedNoteResult> {
  return await invoke<EmbedNoteResult>('ai_embed_note', { relPath });
}

/**
 * Return aggregate counters from the per-vault embedding store. Safe to
 * call with no vault open — returns all-zero counters instead of erroring.
 */
export async function aiEmbedStats(): Promise<EmbeddingStats> {
  return await invoke<EmbeddingStats>('ai_embed_stats');
}

/** Remove every chunk belonging to a single note. Returns rows deleted. */
export async function aiEmbedDeleteNote(relPath: string): Promise<number> {
  return await invoke<number>('ai_embed_delete_note', { relPath });
}

/**
 * Wipe the entire embedding store (all notes, all models). Returns the
 * chunk count that existed before deletion so the UI can say "已清空 N 个".
 */
export async function aiEmbedClearAll(): Promise<number> {
  return await invoke<number>('ai_embed_clear_all');
}

/**
 * Preview a full-vault initialization run. Pure dry-run: no provider calls,
 * no writes, no store mutation.
 */
export async function aiEmbedVaultPreview(): Promise<VaultEmbedPreview> {
  return await invoke<VaultEmbedPreview>('ai_embed_vault_preview');
}

/**
 * Execute a full-vault initialization run for the currently saved model.
 * Per-note failures are accumulated into the returned summary.
 */
export async function aiEmbedVaultRun(): Promise<VaultEmbedRunResult> {
  return await invoke<VaultEmbedRunResult>('ai_embed_vault_run');
}

// ── D2b.1 chat-session storage commands ──────────────────────────────────────

/**
 * Chat turn role. Matches the Rust enum; lowercase on the wire.
 * `'tool'` was added in P3-D5.1 to persist tool-result messages
 * inside the same `.jsonl` transcript as user / assistant turns.
 */
export type ChatRole = 'user' | 'assistant' | 'system' | 'tool';

/** First line of a `chats/<id>.jsonl` session file. */
export interface ChatMeta {
  v: number;
  session_id: string;
  title: string;
  /** Unix seconds. */
  created_at: number;
  /** Vault-relative path this session is "about", if any. */
  related_note?: string;
}

/**
 * One tool call the model requested. Carried on assistant turns whose
 * `tool_calls` array is non-empty. `arguments` is the raw JSON string
 * the provider emitted — validation happens inside the registry, not here.
 */
export interface ToolCall {
  id: string;
  name: string;
  /** JSON-encoded arguments string. */
  arguments: string;
}

/** One chat turn as persisted on disk. */
export interface ChatMessage {
  v: number;
  id: string;
  role: ChatRole;
  content: string;
  /** Unix seconds. */
  created_at: number;
  /** P3-D5.1: present on assistant turns that initiated one or more tool calls. */
  tool_calls?: ToolCall[];
  /** P3-D5.1: present on `role === 'tool'` turns; matches a `ToolCall.id`. */
  tool_call_id?: string;
}

/** Sidebar entry — everything needed to list sessions without loading them. */
export interface ChatSessionSummary {
  session_id: string;
  title: string;
  /** Unix seconds. */
  created_at: number;
  message_count: number;
  /** Unix seconds of the most recent message, when any. */
  last_message_at?: number;
  related_note?: string;
}

/** Full session payload: meta + ordered message list. */
export interface ChatSessionFull {
  meta: ChatMeta;
  messages: ChatMessage[];
}

/**
 * List every chat session under the active vault, newest first. Returns an
 * empty array (not an error) when no vault is open or the directory hasn't
 * been created yet.
 */
export async function aiChatSessionList(): Promise<ChatSessionSummary[]> {
  return await invoke<ChatSessionSummary[]>('ai_chat_session_list');
}

/**
 * Create an empty chat session. The `session_id` is generated by the
 * backend — callers must use the returned id for all subsequent
 * append/load/delete calls. `relatedNote`, when provided, must be a
 * vault-relative path (backend rejects traversal).
 */
export async function aiChatSessionCreate(
  title: string,
  relatedNote: string | null = null
): Promise<ChatSessionSummary> {
  return await invoke<ChatSessionSummary>('ai_chat_session_create', {
    title,
    relatedNote
  });
}

/** Read a session's full transcript. Rejects on corrupt / unknown-schema files. */
export async function aiChatSessionLoad(sessionId: string): Promise<ChatSessionFull> {
  return await invoke<ChatSessionFull>('ai_chat_session_load', { sessionId });
}

/**
 * Append one turn to an existing session. Durable before returning
 * (`O_APPEND` + `sync_data` at the store layer).
 */
export async function aiChatSessionAppend(
  sessionId: string,
  role: ChatRole,
  content: string
): Promise<ChatMessage> {
  return await invoke<ChatMessage>('ai_chat_session_append', {
    sessionId,
    role,
    content
  });
}

/**
 * Delete a session. Returns `true` on first delete, `false` on an
 * already-gone session — idempotent by design so the frontend can retry
 * without branching on "not found".
 */
export async function aiChatSessionDelete(sessionId: string): Promise<boolean> {
  return await invoke<boolean>('ai_chat_session_delete', { sessionId });
}

// ── D2b.3 non-streaming chat send ───────────────────────────────────────────

/**
 * Structured failure for a non-streaming chat send. Mirrors `EmbedFailure`
 * so the ChatPanel can render AI-pipeline failures using the same banner
 * component as the Settings embed flow.
 */
export interface ChatSendFailure {
  kind: ProviderErrorKind | 'other';
  message: string;
  retry_after_secs?: number;
  /**
   * Whether the user's message was written to disk before the failure.
   * When `true`, the frontend should keep the user bubble visible in the
   * transcript so the send didn't "disappear" — users can retry without
   * re-typing. When `false`, it was a pre-flight rejection (no vault, no
   * provider, corrupt session) and nothing was persisted.
   */
  user_message_persisted: boolean;
}

/**
 * Result of [`aiChatSend`]. Success carries the persisted assistant turn;
 * failure carries a [`ChatSendFailure`]. Kept out-of-band from the
 * Tauri `Result` channel so callers can render success + failure states
 * in the same transcript view without branching at the IPC boundary.
 */
export interface ChatSendResult {
  ok: boolean;
  assistant?: ChatMessage;
  failure?: ChatSendFailure;
}

/**
 * Send one user message in the context of an existing session and wait
 * for the complete assistant reply (no streaming). Persists both turns
 * on success; on a provider-level failure, persists only the user turn
 * so the user can retry without re-typing.
 *
 * This is the non-streaming v1 consumed by `ChatPanel.svelte` (D2b.3);
 * D2b.4 adds a streaming `ai_chat_stream` equivalent that emits tokens
 * via Tauri events. Switching between the two only changes the transport
 * — persisted session files stay identical on disk.
 */
export async function aiChatSend(
  sessionId: string,
  content: string
): Promise<ChatSendResult> {
  return await invoke<ChatSendResult>('ai_chat_send', { sessionId, content });
}

// ── D2b.4 streaming chat ────────────────────────────────────────────────────

/**
 * Event name constants for the streaming-chat transport. Centralized so
 * the main ChatPanel and the D2b.6 detached window listen on identical
 * channels without a typo introducing a silent "no tokens ever arrive".
 */
export const CHAT_STREAM_DELTA_EVENT = 'ai:chat-stream:delta';
export const CHAT_STREAM_DONE_EVENT = 'ai:chat-stream:done';
export const CHAT_STREAM_ERROR_EVENT = 'ai:chat-stream:error';

// P3-D5.1 tool-calling events. Fired once per model-initiated tool call
// during the multi-turn loop inside `ai_chat_stream_start`. The frontend
// uses these to render inline "▸ request" / "◂ result" placeholders in
// the chat timeline (D5.3 upgrades the result into a diff card for
// propose_edit_note etc.).
export const CHAT_STREAM_TOOL_CALL_REQUESTED_EVENT = 'ai:chat-stream:tool_call_requested';
export const CHAT_STREAM_TOOL_CALL_RESULT_EVENT = 'ai:chat-stream:tool_call_result';

/** One token fragment. `content` may be empty when only `finish_reason` changes. */
export interface ChatStreamDeltaEvent {
  stream_id: string;
  content: string;
  finish_reason?: string;
}

/** Terminal success event; the full assistant turn has been persisted. */
export interface ChatStreamDoneEvent {
  stream_id: string;
  assistant: ChatMessage;
  /** True when this stream terminated via an `aiChatStreamCancel` call. */
  cancelled: boolean;
}

/** Terminal failure event; mirrors {@link ChatSendFailure}. */
export interface ChatStreamErrorEvent {
  stream_id: string;
  failure: ChatSendFailure;
}

/**
 * Fired before the backend executes a tool the model asked for. `arguments`
 * is the raw JSON string emitted by the provider — may be malformed or empty.
 * Rendering this as a user-facing status chip is the frontend's job.
 */
export interface ChatStreamToolCallRequestedEvent {
  stream_id: string;
  call_id: string;
  name: string;
  /** Raw JSON string as emitted by the provider; may be malformed. */
  arguments: string;
}

/**
 * Fired after the tool registry returns a result. `is_error = true`
 * means the tool reported failure (most commonly "tool not registered"
 * while the registry is empty in D5.1); the model will see `content` on
 * the next turn and is expected to recover gracefully.
 */
export interface ChatStreamToolCallResultEvent {
  stream_id: string;
  call_id: string;
  content: string;
  is_error: boolean;
}

/**
 * One retrieved chunk used as RAG context for a chat turn. Shape
 * mirrors `SearchHit` from the embedding store but with the raw
 * vector stripped and a pre-truncated `preview` (≤ ~160 chars) for
 * inline "Sources" UI.
 *
 * `offset_start` / `offset_end` are byte offsets into the source
 * note; future "jump-to chunk" navigation can seek straight to the
 * quoted range.
 */
export interface RagCitation {
  note_rel_path: string;
  chunk_index: number;
  offset_start: number;
  offset_end: number;
  /** Cosine similarity in `[0, 1]`. Higher = more similar. */
  score: number;
  /** First ~160 chars of the chunk text. */
  preview: string;
}

/** Synchronous pre-flight result of {@link aiChatStreamStart}. */
export interface ChatStreamStartResult {
  ok: boolean;
  /** On success: the already-persisted user turn (swap optimistic bubble). */
  user_message?: ChatMessage;
  /**
   * RAG citations retrieved for this turn, newest → least relevant.
   * Empty when no embeddings are configured or retrieval found zero
   * matches. Frontend renders these as a "Sources" chip list under
   * the assistant bubble once the stream terminates.
   */
  citations?: RagCitation[];
  /** On failure: pre-flight error (no events will follow for this stream_id). */
  failure?: ChatSendFailure;
}

/**
 * Kick off a streaming chat send. `streamId` is caller-generated (nanoid /
 * uuid); the command rejects collisions so event listeners are never
 * ambiguous about which stream owns a given delta.
 *
 * On `ok: true`, listen on {@link CHAT_STREAM_DELTA_EVENT},
 * {@link CHAT_STREAM_DONE_EVENT}, and {@link CHAT_STREAM_ERROR_EVENT}
 * for tokens / terminal events. Pre-flight failures return synchronously
 * in `failure` and emit no events.
 */
export async function aiChatStreamStart(
  streamId: string,
  sessionId: string,
  content: string
): Promise<ChatStreamStartResult> {
  return await invoke<ChatStreamStartResult>('ai_chat_stream_start', {
    streamId,
    sessionId,
    content
  });
}

/**
 * Flag an in-flight stream for cancellation. Returns `true` when the
 * stream existed and was flagged, `false` when it had already ended.
 * The streaming task persists whatever assistant content it has
 * accumulated so far before emitting `done { cancelled: true }`.
 */
export async function aiChatStreamCancel(streamId: string): Promise<boolean> {
  return await invoke<boolean>('ai_chat_stream_cancel', { streamId });
}

// ── D3.1 single-shot completion ─────────────────────────────────────────────

/**
 * Structured failure for a single-shot completion. Narrower than
 * {@link ChatSendFailure}: no `user_message_persisted` because the
 * write-back commands never touch the chat-session store.
 */
export interface CompleteFailure {
  kind: ProviderErrorKind | 'other';
  message: string;
  retry_after_secs?: number;
}

/** Result of {@link aiComplete}. Success carries a trimmed reply; failure
 * carries a typed banner-shaped error. `cancelled` reflects whether the
 * call was interrupted via {@link aiCompleteCancel}; `ok && cancelled` is
 * possible when tokens arrived before the cancel took effect and the
 * caller may want to offer "keep partial / discard" UX. */
export interface CompleteResult {
  ok: boolean;
  /** On success: trimmed reply from the model. */
  reply?: string;
  /** Prompt tokens when the provider reports usage. */
  input_tokens?: number;
  /** Completion tokens when the provider reports usage. */
  output_tokens?: number;
  /** `true` iff cancelled before the provider finished. */
  cancelled?: boolean;
  failure?: CompleteFailure;
}

/**
 * Run a single-shot, non-streaming completion against the configured
 * chat provider. Used by the three P3-D3 write-back commands
 * (summarize / suggest tags / MOC AI draft) — the full reply lands in
 * the diff-preview modal, which the user must confirm before any
 * markdown changes on disk.
 *
 * `requestId` is caller-generated (nanoid / uuid). Reusing a live id is
 * rejected so a stray cancel can't target the wrong request.
 *
 * `systemPrompt` is optional. `temperature` / `maxTokens` are forwarded
 * verbatim (null/undefined → provider default).
 *
 * No RAG injection, no chat-session persistence: the caller owns the
 * entire prompt.
 */
export async function aiComplete(
  requestId: string,
  opts: {
    systemPrompt?: string | null;
    userPrompt: string;
    temperature?: number | null;
    maxTokens?: number | null;
  }
): Promise<CompleteResult> {
  return await invoke<CompleteResult>('ai_complete', {
    requestId,
    systemPrompt: opts.systemPrompt ?? null,
    userPrompt: opts.userPrompt,
    temperature: opts.temperature ?? null,
    maxTokens: opts.maxTokens ?? null
  });
}

/**
 * Flag an in-flight {@link aiComplete} call for cancellation. Returns
 * `true` when the request existed and was flagged, `false` when it had
 * already ended. The in-flight command observes the flag on its next
 * delta tick and returns whatever it accumulated so far.
 */
export async function aiCompleteCancel(requestId: string): Promise<boolean> {
  return await invoke<boolean>('ai_complete_cancel', { requestId });
}

export interface ProposalResolutionRequest {
  session_id: string;
  tool_call_id: string;
  tool_name: string;
  proposal_kind: string;
  target_rel_path: string;
  accepted_by_user: boolean;
  modified_before_accept?: boolean;
  result?: string | null;
  metadata?: Record<string, unknown> | null;
}

export async function aiRecordProposalResolution(
  req: ProposalResolutionRequest
): Promise<void> {
  await invoke('ai_record_proposal_resolution', { req });
}
