<script lang="ts">
  /**
   * Streaming chat panel with RAG + wiki-link resolution (P3-D2b.5).
   *
   * Runs a full "send a message → stream tokens → render" loop against
   * the persisted chat-session store (D2b.1) and the configured provider's
   * chat endpoint (D2b.2). D2b.4 added event-based streaming + cancel;
   * this slice (D2b.5) adds two quality features:
   *
   * - **RAG context injection.** The backend (`ai_chat_stream_start`)
   *   silently embeds the user's prompt and pulls top-K chunks from the
   *   embedding store. The resulting `citations` come back in the
   *   pre-flight result; we render them as a compact "Sources" chip
   *   row below the latest assistant bubble.
   * - **`[[wiki-link]]` resolution.** The markdown renderer detects
   *   `[[target]]` tokens and emits clickable chips; click routes
   *   through `onOpenNote` after an IPC-side title→path resolution.
   *
   * D2b.5 also replaces the old `window.prompt` + `window.confirm`
   * new-session flow with an inline modal (title input + "link this
   * note" checkbox), so session creation is no longer a native dialog.
   *
   * What it intentionally does NOT do in this slice:
   * - No inline session rename (D2b.5+): create → use → delete lifecycle
   *   only. The session title is taken from the title input in the new-
   *   session modal, or falls back to a timestamp.
  */
  import { onDestroy, tick } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { performProposalAction } from '$lib/chat/acceptProposal';
  import {
    clearResolutionsForSession,
    loadResolutionsForSession,
    persistResolution
  } from '$lib/chat/proposalResolutionStore';
  import {
    mockCreateSession,
    mockSessionSummary,
    runMockSend,
    type MockChatHandles
  } from '$lib/e2e/mockChatScripts';
  import ProposalCard from '$lib/chat/ProposalCard.svelte';
  import ToolCallCard from '$lib/chat/ToolCallCard.svelte';
  import { toolTraceLabel } from '$lib/chat/toolLabels';
  import {
    buildInlineToolCallViewModels,
    pairPersistedToolCalls,
    type InlineToolEvent,
    type ProposalPayload,
    type ToolCallViewModel
  } from '$lib/chat/toolCallViewModel';
  import { indexResolveWikiLink } from '$lib/ipc/index';
  import {
    aiChatSessionCreate,
    aiChatSessionDelete,
    aiChatSessionList,
    aiChatSessionLoad,
    aiRecordProposalResolution,
    aiChatStreamCancel,
    aiChatStreamStart,
    CHAT_STREAM_DELTA_EVENT,
    CHAT_STREAM_DONE_EVENT,
    CHAT_STREAM_ERROR_EVENT,
    CHAT_STREAM_TOOL_CALL_REQUESTED_EVENT,
    CHAT_STREAM_TOOL_CALL_RESULT_EVENT,
    type ChatMessage,
    type ChatSessionFull,
    type ChatSessionSummary,
    type ChatSendFailure,
    type ChatStreamDeltaEvent,
    type ChatStreamDoneEvent,
    type ChatStreamErrorEvent,
    type ChatStreamToolCallRequestedEvent,
    type ChatStreamToolCallResultEvent,
    type RagCitation
  } from '$lib/ipc/ai';

  interface Props {
    /**
     * Vault-relative path of the currently open file, or `null`. When a
     * note is open, the "新建会话" button offers to link the new session
     * to that path — what D2b.5 uses to seed RAG context.
     */
    filePath: string | null;
    /** Click target so the panel can open, reload, or clear a vault path. */
    onOpenNote: (relPath: string | null, opts?: { forceReload?: boolean }) => void;
    /**
     * Render variant (D2b.6). `docked` (default) = right panel tab inside
     * the main window; `standalone` = dedicated window where the panel
     * owns the whole viewport. The prop only tweaks layout (hide "pop-out"
     * button, stretch to 100vh, slightly roomier padding); session data /
     * IPC / event plumbing is identical in both modes.
     */
    variant?: 'docked' | 'standalone';
  }

  const { filePath, onOpenNote, variant = 'docked' }: Props = $props();

  /**
   * Phase 4 Stage 1 — production bundles must NOT ship the in-panel mock
   * provider. We gate `e2eMockMode` on a build-time literal
   * (`import.meta.env.PUBLIC_E2E === '1'`) AND the runtime URL flag
   * (`?e2eMock=1`). In normal `pnpm build` / `pnpm tauri:build` runs,
   * `PUBLIC_E2E` is unset → Vite folds the constant to `false`, the
   * `&&` short-circuits, and Rollup dead-code-eliminates every
   * `if (e2eMockMode) { … }` branch plus the supporting `mock*`
   * functions in this file. Only the dedicated Playwright build
   * (`PUBLIC_E2E=1 pnpm build && pnpm preview`) keeps the mock alive.
   */
  const E2E_BUILD = import.meta.env.PUBLIC_E2E === '1';
  const e2eMockMode =
    E2E_BUILD &&
    typeof window !== 'undefined' &&
    new URLSearchParams(window.location.search).get('e2eMock') === '1';

  /**
   * Per-assistant-message citation list. Keyed by the authoritative
   * assistant message id (from `done` event → session reload). We
   * record the citations that the backend used when generating the
   * reply so a "Sources" footer can render underneath that bubble.
   *
   * Keyed by id rather than stored on the message itself so we don't
   * pollute the on-disk JSONL shape; citations are derivative data.
   */
  let citationsByAssistantId = $state<Record<string, RagCitation[]>>({});
  /** Citations for the currently streaming (not yet persisted) reply. */
  let pendingCitations = $state<RagCitation[]>([]);

  /** All sessions in the active vault, newest first. */
  let sessions = $state<ChatSessionSummary[]>([]);
  /** The session currently rendered in the message list. `null` before first load / when empty. */
  let activeSessionId = $state<string | null>(null);
  /** Full transcript of the active session. Separate from `sessions` because list entries are summary-only. */
  let activeSession = $state<ChatSessionFull | null>(null);
  /** True while the first sessions-list fetch is in flight. */
  let loadingList = $state(false);
  /** True while loading transcript for the active session. */
  let loadingSession = $state(false);
  /** True between "user clicks 发送" and either the assistant reply arrives or a failure comes back. */
  let sending = $state(false);
  /**
   * Stream id of the currently running send, or `null` when idle. Holds
   * the id used to route `ai:chat-stream:*` events + `aiChatStreamCancel`.
   * Separate from `sending` so cancel can flip `sending → false` while
   * still keeping the id around long enough for a trailing delta / done
   * event to be routed correctly (the terminal event handler nulls it).
   */
  let activeStreamId = $state<string | null>(null);
  /**
   * Live accumulator for the streaming assistant bubble. Rendered as a
   * `pending` bubble at the tail of the transcript; swapped for the
   * authoritative `ChatMessage` from the `done` event once it persists.
   */
  let streamingContent = $state('');
  /**
   * Transient in-flight tool-call log for the currently active stream
   * (P3-D5.1). Each entry is either a "requested" marker or a "result"
   * marker, rendered as a one-line chip above the streaming assistant
   * bubble. Cleared when the stream terminates (we reload the session
   * from disk afterwards, which replays the persisted Assistant+Tool
   * messages from the `.jsonl`).
   *
   * D5.1 renders everything as plain text — D5.3 upgrades
   * `propose_edit_note` results into an inline diff card; the inline
   * log stays as the fallback for non-diff tools.
   */
  let inlineToolEvents = $state<InlineToolEvent[]>([]);
  /** Text in the compose textarea. */
  let composeText = $state('');
  /** Last send failure, displayed as an inline banner above the composer. Cleared on next successful send or session switch. */
  let lastFailure = $state<ChatSendFailure | null>(null);
  /** Non-provider errors (list failed / load failed). */
  let uiError = $state<string | null>(null);
  /** DOM ref for the scrollable transcript — used to auto-scroll to bottom after a message arrives. */
  let transcriptEl: HTMLDivElement | null = $state(null);
  let composeEl: HTMLTextAreaElement | null = $state(null);
  let proposalResolutionByKey = $state<
    Record<string, { kind: 'accepted' | 'rejected' | 'error'; message: string }>
  >({});
  // Phase 4 Stage 1.5 — the mock session counter, helpers, and the
  // streaming send loop all live in `src/lib/e2e/mockChatScripts.ts`
  // so the panel file is no longer 250 lines fatter just to host
  // them. We keep `mockCancelRequested` here because it's tied to the
  // panel's `cancel()` button click handler; runMockSend reads it
  // through the `MockChatHandles` adapter.
  let mockCancelRequested = $state(false);

  // ── New-session modal state (D2b.5) ───────────────────────────────
  //
  // Replaces the pre-D2b.5 `window.prompt` + `window.confirm` flow
  // with an inline form that can show a proper error line and keep
  // focus trapped inside the panel. The modal is rendered at the
  // bottom of the template (not a portal) — `.modal-backdrop` uses
  // `position: fixed` so the overlay still covers the whole viewport.
  let newSessionModalOpen = $state(false);
  let newSessionTitle = $state('');
  let newSessionLinkNote = $state(false);
  let newSessionError = $state<string | null>(null);
  let newSessionBusy = $state(false);
  let newSessionInputEl: HTMLInputElement | null = $state(null);

  // Monotonic counter so a slow session-load can't overwrite the display
  // after the user has already switched sessions.
  let loadReqSeq = 0;
  /**
   * Which session id `activeSession` currently reflects. Plain `let`
   * (not `$state`) on purpose: this is an internal bookkeeping value
   * that should NOT wake the `$effect` below. When the send() path
   * optimistically populates `activeSession` before the effect fires,
   * it bumps this tracker so the effect's change-detection short-
   * circuits the reload. Without it, a freshly auto-created session
   * would briefly wipe out the user's typed message before the backend
   * reload replayed it.
   */
  let lastResolvedSessionId: string | null = null;
  const inlineToolCallCards = $derived(buildInlineToolCallViewModels(inlineToolEvents));
  const persistedToolPairing = $derived(pairPersistedToolCalls(activeSession?.messages ?? []));
  const currentToolTrace = $derived.by(() => {
    const latest = [...inlineToolCallCards]
      .reverse()
      .find((toolVm) => toolVm.status === 'pending') ?? inlineToolCallCards.at(-1);
    if (!latest) return null;
    return toolTraceLabel(latest.name, latest.arguments);
  });

  async function refreshSessions(preferSelectId?: string | null): Promise<void> {
    if (e2eMockMode) {
      if (!activeSession && !activeSessionId) {
        sessions = [];
        return;
      }
      if (activeSession) {
        sessions = [mockSessionSummary(activeSession)];
        activeSessionId = activeSession.meta.session_id;
      }
      return;
    }
    loadingList = true;
    try {
      const list = await aiChatSessionList();
      sessions = list;
      // Auto-select policy: (1) prefer explicit id if present in the
      // refreshed list, (2) keep current selection if still present,
      // (3) fall back to newest, (4) null when the list is empty.
      const prefer = preferSelectId ?? activeSessionId;
      const preferFound = prefer ? list.find((s) => s.session_id === prefer) : undefined;
      const next = preferFound?.session_id ?? list[0]?.session_id ?? null;
      if (next !== activeSessionId) {
        activeSessionId = next;
      }
      if (!next) {
        activeSession = null;
      }
    } catch (e) {
      uiError = String(e);
    } finally {
      loadingList = false;
    }
  }

  async function loadActiveSession(id: string | null): Promise<void> {
    if (e2eMockMode) {
      if (!id || activeSession?.meta.session_id !== id) {
        activeSession = null;
      }
      lastResolvedSessionId = id;
      // Phase 4 Stage 4 — even in mock mode, rehydrate so the
      // standalone-window E2E case sees the same chips as the docked
      // panel (covered by the consistency hardening sub-task).
      rehydrateResolutionsFor(id);
      return;
    }
    if (!id) {
      activeSession = null;
      lastResolvedSessionId = null;
      rehydrateResolutionsFor(null);
      return;
    }
    const myReq = ++loadReqSeq;
    loadingSession = true;
    uiError = null;
    try {
      const full = await aiChatSessionLoad(id);
      if (myReq !== loadReqSeq) return;
      activeSession = full;
      lastResolvedSessionId = id;
      // Phase 4 Stage 4 — pull the resolution mirror once the
      // transcript is in hand. Order matters: do it AFTER
      // `activeSession` is assigned so the derived
      // `pairPersistedToolCalls` has the messages to attach chips to.
      rehydrateResolutionsFor(id);
    } catch (e) {
      if (myReq !== loadReqSeq) return;
      uiError = String(e);
      activeSession = null;
      lastResolvedSessionId = null;
      rehydrateResolutionsFor(null);
    } finally {
      if (myReq === loadReqSeq) loadingSession = false;
    }
  }

  // Drive "whenever the active session id genuinely changes, refresh the
  // transcript + clear any stale per-session banner". Skip when we've
  // already resolved this id (e.g. after an auto-create in send(), which
  // populates `activeSession` synchronously). Without that short-circuit
  // the effect would reload an empty transcript from disk and wipe the
  // just-pushed optimistic user bubble.
  $effect(() => {
    const id = activeSessionId;
    lastFailure = null;
    if (id === lastResolvedSessionId) return;
    void loadActiveSession(id);
  });

  // Auto-scroll the transcript to the bottom when the message list grows
  // or the active session changes. Using a $effect keyed on the message
  // count lets the DOM settle before we read `scrollHeight`.
  $effect(() => {
    const count = activeSession?.messages.length ?? 0;
    void count;
    // next microtask, after Svelte flushes the DOM mutation
    queueMicrotask(() => {
      if (transcriptEl) {
        transcriptEl.scrollTop = transcriptEl.scrollHeight;
      }
    });
  });

  // Initial load.
  $effect(() => {
    void refreshSessions();
  });

  // ── Stream event plumbing ─────────────────────────────────────────
  //
  // We register three listeners lazily on the first send so we don't
  // pay the Tauri event-bus cost on users who never open the chat
  // panel. Handlers filter by `stream_id` so a stale listener from a
  // previous send can't mis-route a delta into the new session.
  let unlistenDelta: UnlistenFn | null = null;
  let unlistenDone: UnlistenFn | null = null;
  let unlistenError: UnlistenFn | null = null;
  let unlistenToolCallRequested: UnlistenFn | null = null;
  let unlistenToolCallResult: UnlistenFn | null = null;

  async function ensureStreamListeners(): Promise<void> {
    if (
      unlistenDelta &&
      unlistenDone &&
      unlistenError &&
      unlistenToolCallRequested &&
      unlistenToolCallResult
    )
      return;
    unlistenDelta = await listen<ChatStreamDeltaEvent>(CHAT_STREAM_DELTA_EVENT, (ev) => {
      const payload = ev.payload;
      if (payload.stream_id !== activeStreamId) return;
      if (payload.content) {
        streamingContent += payload.content;
      }
    });
    unlistenDone = await listen<ChatStreamDoneEvent>(CHAT_STREAM_DONE_EVENT, (ev) => {
      const payload = ev.payload;
      if (payload.stream_id !== activeStreamId) return;
      void onStreamTerminal(payload.assistant.id, /*ok*/ true, null, payload.cancelled);
    });
    unlistenError = await listen<ChatStreamErrorEvent>(CHAT_STREAM_ERROR_EVENT, (ev) => {
      const payload = ev.payload;
      if (payload.stream_id !== activeStreamId) return;
      void onStreamTerminal(null, /*ok*/ false, payload.failure, false);
    });
    // P3-D5.1 — append a placeholder chip per tool call round-trip.
    // The chip stays until the stream terminates; reload-from-disk in
    // `onStreamTerminal` then displays the persisted Tool message in
    // its canonical position in the transcript.
    unlistenToolCallRequested = await listen<ChatStreamToolCallRequestedEvent>(
      CHAT_STREAM_TOOL_CALL_REQUESTED_EVENT,
      (ev) => {
        const payload = ev.payload;
        if (payload.stream_id !== activeStreamId) return;
        inlineToolEvents = [
          ...inlineToolEvents,
          {
            kind: 'requested',
            call_id: payload.call_id,
            name: payload.name,
            arguments: payload.arguments
          }
        ];
      }
    );
    unlistenToolCallResult = await listen<ChatStreamToolCallResultEvent>(
      CHAT_STREAM_TOOL_CALL_RESULT_EVENT,
      (ev) => {
        const payload = ev.payload;
        if (payload.stream_id !== activeStreamId) return;
        inlineToolEvents = [
          ...inlineToolEvents,
          {
            kind: 'result',
            call_id: payload.call_id,
            content: payload.content,
            is_error: payload.is_error
          }
        ];
      }
    );
  }

  onDestroy(() => {
    unlistenDelta?.();
    unlistenDone?.();
    unlistenError?.();
    unlistenToolCallRequested?.();
    unlistenToolCallResult?.();
  });

  // ── Session create / delete / switch ──────────────────────────────

  /**
   * Open the "新建会话" modal. Seeds the form with the current note's
   * path as the default "link this note" choice when a note is open.
   *
   * The modal handles its own confirm/cancel — this just flips the
   * flag and focuses the title input on the next tick so typing
   * works immediately.
   */
  async function newSession(): Promise<void> {
    if (e2eMockMode) {
      const created = mockCreateSession(defaultTitleForNow(), filePath ?? null);
      activeSession = created;
      activeSessionId = created.meta.session_id;
      lastResolvedSessionId = created.meta.session_id;
      sessions = [mockSessionSummary(created)];
      return;
    }
    newSessionTitle = '';
    newSessionLinkNote = !!filePath; // default ON when a note is open
    newSessionError = null;
    newSessionBusy = false;
    newSessionModalOpen = true;
    await tick();
    newSessionInputEl?.focus();
  }

  function cancelNewSession(): void {
    if (newSessionBusy) return; // don't close while create IPC in flight
    newSessionModalOpen = false;
    newSessionError = null;
  }

  async function confirmNewSession(): Promise<void> {
    if (newSessionBusy) return;
    const title = newSessionTitle.trim() || defaultTitleForNow();
    const relatedNote = newSessionLinkNote && filePath ? filePath : null;
    newSessionBusy = true;
    newSessionError = null;
    try {
      const created = await aiChatSessionCreate(title, relatedNote);
      newSessionModalOpen = false;
      await refreshSessions(created.session_id);
    } catch (e) {
      newSessionError = String(e);
    } finally {
      newSessionBusy = false;
    }
  }

  function onNewSessionKey(e: KeyboardEvent): void {
    if (e.key === 'Enter' && !e.isComposing) {
      e.preventDefault();
      void confirmNewSession();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      cancelNewSession();
    }
  }

  function defaultTitleForNow(): string {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, '0');
    return `会话 ${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  async function deleteActiveSession(): Promise<void> {
    if (e2eMockMode) {
      activeSession = null;
      activeSessionId = null;
      lastResolvedSessionId = null;
      sessions = [];
      return;
    }
    const id = activeSessionId;
    if (!id) return;
    const summary = sessions.find((s) => s.session_id === id);
    const title = summary?.title ?? id;
    const ok = window.confirm(`确定删除会话「${title}」？\n\n此操作不可撤销。`);
    if (!ok) return;
    try {
      await aiChatSessionDelete(id);
      // Phase 4 Stage 4 — flush the per-session resolution cache so
      // that creating a new session that happens to reuse this id
      // (theoretically impossible since ids are uuids, but safe) does
      // not pick up stale chips.
      clearResolutionsForSession(id);
      // Refresh; when the deleted session was active, the first session
      // (or none) becomes active.
      activeSessionId = null;
      await refreshSessions();
    } catch (e) {
      uiError = String(e);
    }
  }

  function switchSession(id: string): void {
    if (id === activeSessionId) return;
    activeSessionId = id;
    // Phase 4 Stage 4 — pull the resolution mirror for this session
    // so accepted / rejected chips render immediately (without it
    // they'd flicker back to "pending" until the next user action).
    rehydrateResolutionsFor(id);
  }

  function proposalResolutionFor(
    viewModel: ToolCallViewModel
  ): { kind: 'accepted' | 'rejected' | 'error'; message: string } | null {
    return proposalResolutionByKey[viewModel.key] ?? null;
  }

  function setProposalResolution(
    key: string,
    kind: 'accepted' | 'rejected' | 'error',
    message: string
  ): void {
    proposalResolutionByKey = {
      ...proposalResolutionByKey,
      [key]: { kind, message }
    };
    // Phase 4 Stage 4 — mirror to localStorage so closing/reopening
    // the panel (or popping out the standalone window) doesn't reset
    // every accepted card to "pending". The backend audit log is
    // still the source of truth; this is a frontend UX cache.
    if (activeSessionId) {
      persistResolution(activeSessionId, key, { kind, message });
    }
  }

  /**
   * Phase 4 Stage 4 — when switching to a session (cold load, popout
   * bring-back, panel re-mount), seed `proposalResolutionByKey` from
   * the localStorage mirror keyed by session_id. Without this the
   * accepted/rejected chip on every past proposal card flickers back
   * to "pending" the moment ChatPanel re-mounts, which the user reads
   * as "did my accept actually go through?".
   */
  function rehydrateResolutionsFor(sessionId: string | null): void {
    if (!sessionId) {
      proposalResolutionByKey = {};
      return;
    }
    proposalResolutionByKey = loadResolutionsForSession(sessionId);
  }

  async function recordProposalResolution(
    viewModel: ToolCallViewModel,
    proposal: ProposalPayload,
    acceptedByUser: boolean,
    result: string
  ): Promise<void> {
    if (!activeSessionId) return;
    if (e2eMockMode) return;
    try {
      await aiRecordProposalResolution({
        session_id: activeSessionId,
        tool_call_id: viewModel.callId,
        tool_name: viewModel.name,
        proposal_kind: proposal.proposal_kind,
        target_rel_path: proposal.target_rel_path,
        accepted_by_user: acceptedByUser,
        modified_before_accept: false,
        result,
        metadata: proposal.metadata ?? null
      });
    } catch (err) {
      console.warn('[proposal] audit log failed:', err);
    }
  }

  async function refreshAfterProposal(
    proposal: ProposalPayload,
    nextOpenPath: string | null
  ): Promise<void> {
    if (proposal.proposal_kind === 'delete_note' && nextOpenPath === null) {
      onOpenNote(null);
    } else if (nextOpenPath) {
      onOpenNote(nextOpenPath, { forceReload: true });
    } else if (filePath === proposal.target_rel_path) {
      onOpenNote(proposal.target_rel_path, { forceReload: true });
    }

    if (activeSessionId) {
      await loadActiveSession(activeSessionId);
      await refreshSessions(activeSessionId);
    }
  }

  async function acceptProposal(
    viewModel: ToolCallViewModel,
    proposal: ProposalPayload
  ): Promise<void> {
    const destructive =
      proposal.proposal_kind === 'delete_note' || proposal.proposal_kind === 'rename_note';
    if (destructive) {
      const confirmed = window.confirm(
        proposal.proposal_kind === 'delete_note'
          ? `确认永久删除 ${proposal.target_rel_path} 吗？\n\n此操作不可撤销。`
          : `确认将笔记改名为 ${proposal.target_rel_path} 吗？\n\n链接会一并重写。`
      );
      if (!confirmed) {
        await recordProposalResolution(viewModel, proposal, false, 'cancelled_by_user');
        setProposalResolution(viewModel.key, 'rejected', '已取消');
        return;
      }
    }

    if (e2eMockMode) {
      setProposalResolution(viewModel.key, 'accepted', acceptedMessage(proposal, 'written'));
      return;
    }

    try {
      const outcome = await performProposalAction(proposal, filePath);
      await recordProposalResolution(viewModel, proposal, true, outcome.resultText);
      setProposalResolution(viewModel.key, 'accepted', acceptedMessage(proposal, outcome.resultText));
      await refreshAfterProposal(proposal, outcome.nextOpenPath);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      await recordProposalResolution(viewModel, proposal, true, `error:${message}`);
      setProposalResolution(viewModel.key, 'error', `执行失败：${message}`);
      uiError = `提案执行失败：${message}`;
    }
  }

  async function rejectProposal(
    viewModel: ToolCallViewModel,
    proposal: ProposalPayload
  ): Promise<void> {
    await recordProposalResolution(viewModel, proposal, false, 'rejected');
    setProposalResolution(viewModel.key, 'rejected', '已拒绝');
  }

  async function adjustProposal(
    _viewModel: ToolCallViewModel,
    proposal: ProposalPayload
  ): Promise<void> {
    const target =
      proposal.proposal_kind === 'rename_note'
        ? (proposal.metadata?.source_rel_path as string | undefined) ?? proposal.target_rel_path
        : proposal.target_rel_path;
    composeText = `请基于刚才对 ${target} 的 ${proposal.proposal_kind} 提案继续调整：`;
    await tick();
    composeEl?.focus();
    composeEl?.setSelectionRange(composeText.length, composeText.length);
  }

  function acceptedMessage(proposal: ProposalPayload, result: string): string {
    switch (proposal.proposal_kind) {
      case 'moc':
        return result === 'created' ? 'MOC 已写入' : 'MOC 已更新';
      case 'rename_note':
        return '已重命名并重写引用';
      case 'delete_note':
        // Phase 4 Stage 3 — file moved to OS Trash, not unlinked.
        // Tell the user where it went so recovery is obvious.
        return '已移至系统回收站（可在 Finder/资源管理器中恢复）';
      default:
        return '已写入';
    }
  }

  // ── Send loop ─────────────────────────────────────────────────────

  async function send(): Promise<void> {
    const text = composeText.trim();
    if (!text || sending) return;
    if (e2eMockMode) {
      await sendMock(text);
      return;
    }
    let sessionId = activeSessionId;
    // Auto-create a session on the very first send if the user skipped
    // "新建会话". Better than popping a prompt from the send path —
    // we take the first 60 chars of the message as title.
    if (!sessionId) {
      try {
        const autoTitle = deriveTitle(text);
        const created = await aiChatSessionCreate(autoTitle, filePath ?? null);
        sessions = [created, ...sessions];
        // Populate activeSession BEFORE assigning activeSessionId so the
        // $effect short-circuit sees consistent state and skips a wasteful
        // reload of the (empty) session from disk.
        activeSession = {
          meta: {
            v: 1,
            session_id: created.session_id,
            title: created.title,
            created_at: created.created_at,
            related_note: created.related_note
          },
          messages: []
        };
        lastResolvedSessionId = created.session_id;
        activeSessionId = created.session_id;
        sessionId = created.session_id;
      } catch (e) {
        uiError = String(e);
        return;
      }
    }
    sending = true;
    lastFailure = null;
    streamingContent = '';
    inlineToolEvents = [];
    // Optimistic user bubble so the input feels responsive. The real
    // `ChatMessage` arrives synchronously from `aiChatStreamStart`
    // (pre-flight persists the user turn before spawning the stream);
    // we swap ids then so downstream refreshes don't create duplicates.
    const optimisticUserId = `optimistic-${Date.now()}`;
    const optimistic: ChatMessage = {
      v: 1,
      id: optimisticUserId,
      role: 'user',
      content: text,
      created_at: Math.floor(Date.now() / 1000)
    };
    if (activeSession) {
      activeSession = {
        ...activeSession,
        messages: [...activeSession.messages, optimistic]
      };
    }
    composeText = '';

    await ensureStreamListeners();

    const streamId = generateStreamId();
    activeStreamId = streamId;

    try {
      const res = await aiChatStreamStart(streamId, sessionId, text);
      if (!res.ok) {
        // Pre-flight failure: no events will arrive, so we short-circuit
        // here. Restore the typed text so the user can retry without
        // re-typing. Keep the optimistic user bubble only if the backend
        // said it was persisted.
        if (!res.failure?.user_message_persisted && activeSession) {
          activeSession = {
            ...activeSession,
            messages: activeSession.messages.filter((m) => m.id !== optimisticUserId)
          };
        }
        if (!res.failure?.user_message_persisted) {
          composeText = text;
        }
        lastFailure = res.failure ?? null;
        activeStreamId = null;
        sending = false;
        return;
      }
      // Swap the optimistic user bubble for the authoritative one.
      if (res.user_message && activeSession) {
        activeSession = {
          ...activeSession,
          messages: activeSession.messages.map((m) =>
            m.id === optimisticUserId ? (res.user_message as ChatMessage) : m
          )
        };
      }
      // Stash retrieved citations for when `done` arrives — we'll
      // key them by the authoritative assistant id then.
      pendingCitations = res.citations ?? [];
    } catch (e) {
      composeText = text;
      uiError = String(e);
      // Roll back the optimistic bubble — the backend never saw it.
      if (activeSession) {
        activeSession = {
          ...activeSession,
          messages: activeSession.messages.filter((m) => m.id !== optimisticUserId)
        };
      }
      activeStreamId = null;
      sending = false;
    }
  }

  /**
   * Phase 4 Stage 1.5 — sendMock is now a one-liner that hands the
   * message to `runMockSend` in `src/lib/e2e/mockChatScripts.ts`. The
   * `MockChatHandles` adapter exposes the slice of panel state that
   * the script needs (active session, sessions list, streaming
   * buffer, …) without leaking the entire component API. Production
   * builds drop the import via Vite tree-shake (see e2eMockMode docs
   * above).
   */
  function makeMockHandles(): MockChatHandles {
    return {
      getActiveSession: () => activeSession,
      getFilePath: () => filePath,
      isCancelRequested: () => mockCancelRequested,
      setActiveSession: (s) => {
        activeSession = s;
      },
      setActiveSessionId: (id) => {
        activeSessionId = id;
      },
      setLastResolvedSessionId: (id) => {
        lastResolvedSessionId = id;
      },
      setSessions: (list) => {
        sessions = list;
      },
      setSending: (busy) => {
        sending = busy;
      },
      setLastFailure: (f) => {
        lastFailure = f;
      },
      setComposeText: (t) => {
        composeText = t;
      },
      setStreamingContent: (t) => {
        streamingContent = t;
      },
      appendStreamingContent: (chunk) => {
        streamingContent += chunk;
      },
      setInlineToolEvents: (evts) => {
        inlineToolEvents = evts;
      },
      appendInlineToolEvent: (evt) => {
        inlineToolEvents = [...inlineToolEvents, evt];
      },
      setPendingCitations: (c) => {
        pendingCitations = c;
      },
      resetCancelRequested: () => {
        mockCancelRequested = false;
      },
      tick
    };
  }

  async function sendMock(text: string): Promise<void> {
    await runMockSend(text, makeMockHandles());
  }

  async function cancelStreaming(): Promise<void> {
    const id = activeStreamId;
    if (e2eMockMode) {
      mockCancelRequested = true;
      return;
    }
    if (!id) return;
    try {
      await aiChatStreamCancel(id);
    } catch (e) {
      uiError = String(e);
    }
  }

  /**
   * Shared handler for both `done` and `error` terminal events. We
   * always reload the session from disk so the transcript reflects
   * whatever the backend actually persisted (including partial-on-
   * cancel / no-assistant-on-error cases) without hand-merging.
   */
  async function onStreamTerminal(
    assistantId: string | null,
    ok: boolean,
    failure: ChatSendFailure | null,
    _cancelled: boolean
  ): Promise<void> {
    const sessionId = activeSessionId;
    activeStreamId = null;
    sending = false;
    streamingContent = '';
    inlineToolEvents = [];
    if (!ok && failure) {
      lastFailure = failure;
    }
    // Pin the citations we accumulated during streaming to the
    // authoritative assistant message id, then discard the pending
    // bucket. On error we drop them silently — there's no bubble to
    // attach them to.
    if (ok && assistantId && pendingCitations.length > 0) {
      citationsByAssistantId = {
        ...citationsByAssistantId,
        [assistantId]: pendingCitations
      };
    }
    pendingCitations = [];
    if (sessionId) {
      await loadActiveSession(sessionId);
      await refreshSessions(sessionId);
    }
  }

  /** Short, collision-resistant-enough id for per-send routing. */
  function generateStreamId(): string {
    const ts = Date.now().toString(36);
    const rand = Math.random().toString(36).slice(2, 10);
    return `s-${ts}-${rand}`;
  }

  function deriveTitle(text: string): string {
    const trimmed = text.trim();
    if (trimmed.length <= 60) return trimmed;
    return `${trimmed.slice(0, 60)}…`;
  }

  function onComposeKeydown(ev: KeyboardEvent): void {
    // Enter = send (fires only when neither shift/meta/ctrl is held).
    // Shift+Enter inserts a newline (default browser behaviour).
    if (ev.key === 'Enter' && !ev.shiftKey && !ev.metaKey && !ev.ctrlKey) {
      ev.preventDefault();
      void send();
    } else if ((ev.metaKey || ev.ctrlKey) && ev.key === 'Enter') {
      // Cmd/Ctrl+Enter: send regardless.
      ev.preventDefault();
      void send();
    }
  }

  // ── Rendering helpers ─────────────────────────────────────────────

  function formatTime(unixSecs: number): string {
    const d = new Date(unixSecs * 1000);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  }

  function formatRelative(unixSecs: number | undefined): string {
    if (!unixSecs) return '';
    const diffSec = Math.floor(Date.now() / 1000 - unixSecs);
    if (diffSec < 60) return '刚刚';
    if (diffSec < 3600) return `${Math.floor(diffSec / 60)} 分钟前`;
    if (diffSec < 86_400) return `${Math.floor(diffSec / 3600)} 小时前`;
    const days = Math.floor(diffSec / 86_400);
    if (days < 7) return `${days} 天前`;
    const d = new Date(unixSecs * 1000);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  }

  function roleLabel(role: ChatMessage['role']): string {
    switch (role) {
      case 'user':
        return '你';
      case 'assistant':
        return 'AI';
      case 'tool':
        return '工具';
      case 'system':
      default:
        return '系统';
    }
  }

  /**
   * Minimal markdown → HTML renderer for v1 chat bubbles. Supports:
   *   - Fenced code blocks (```lang? ... ```)
   *   - Inline code (`…`)
   *   - Bold (**…**), italic (*…* / _…_)
   *   - `[[wiki-link]]` → clickable `<span data-wiki-target="…">`
   *     (D2b.5). Resolution to a concrete path happens lazily on click
   *     via `indexResolveWikiLink` — this avoids a burst of N IPC
   *     calls during render.
   *   - Auto-link bare URLs
   *   - Preserves line breaks inside paragraphs
   *
   * Runs HTML-escape first so untrusted provider output can't inject
   * tags. Full-featured markdown (headings, lists, tables) will land
   * when a real renderer is pulled in; v1 optimises for "readable at a
   * glance" over "spec-complete".
   */
  function renderMarkdown(src: string): string {
    // 1) HTML-escape everything — this is the security boundary.
    const esc = (s: string) =>
      s
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');

    // 2) Slice out fenced code blocks first so their contents are not
    //    touched by the inline transforms. Replace with placeholders,
    //    restore at the end.
    const codeBlocks: string[] = [];
    let working = src.replace(/```([^\n]*)\n([\s\S]*?)```/g, (_m, lang, body) => {
      const language = String(lang || '').trim();
      const escaped = esc(String(body));
      const className = language ? ` data-lang="${esc(language)}"` : '';
      codeBlocks.push(`<pre class="chat-codeblock"${className}><code>${escaped}</code></pre>`);
      return `\u0000CODEBLOCK${codeBlocks.length - 1}\u0000`;
    });

    // 3) Escape remaining text.
    working = esc(working);

    // 4) `[[wiki-link]]` → data-wiki-target span. Runs BEFORE inline
    //    code so code spans containing `[[…]]` stay literal (code
    //    blocks were already pulled out in step 2). `target` is
    //    HTML-escaped because step 3 already ran.
    working = working.replace(/\[\[([^\]\n|]+)(?:\|([^\]\n]+))?\]\]/g, (_m, rawTarget, rawLabel) => {
      const target = String(rawTarget).trim();
      const label = (rawLabel ? String(rawLabel).trim() : target) || target;
      // Target went through the HTML-escape pass; unescape the
      // attribute (so the data attribute holds the raw text) but
      // keep the visible label escaped.
      const attrSafe = target
        .replace(/&amp;/g, '&')
        .replace(/&lt;/g, '<')
        .replace(/&gt;/g, '>')
        .replace(/&quot;/g, '"')
        .replace(/&#39;/g, "'")
        .replace(/"/g, '&quot;');
      return `<span class="chat-wiki-link" role="link" tabindex="0" data-wiki-target="${attrSafe}">[[${label}]]</span>`;
    });

    // 5) Inline code, then bold/italic. Ordering matters so `**_x_**`
    //    doesn't mis-parse.
    working = working.replace(/`([^`\n]+)`/g, '<code class="chat-inlinecode">$1</code>');
    working = working.replace(/\*\*([^*\n][^*]*?)\*\*/g, '<strong>$1</strong>');
    working = working.replace(/(?:^|[\s(])\*([^*\n][^*]*?)\*(?=[\s).,!?:;]|$)/g, (m, g) => {
      const prefix = m.charAt(0) === '*' ? '' : m.charAt(0);
      return `${prefix}<em>${g}</em>`;
    });
    working = working.replace(/(?:^|[\s(])_([^_\n][^_]*?)_(?=[\s).,!?:;]|$)/g, (m, g) => {
      const prefix = m.charAt(0) === '_' ? '' : m.charAt(0);
      return `${prefix}<em>${g}</em>`;
    });

    // 6) Auto-link bare URLs. Keep this simple — greedy URL regex would
    //    swallow trailing punctuation, so we carve off a trailing
    //    `.,;:!?)` run when present.
    working = working.replace(/\bhttps?:\/\/[^\s<]+/g, (url) => {
      const trailing = url.match(/[.,;:!?)]+$/)?.[0] ?? '';
      const bare = trailing ? url.slice(0, -trailing.length) : url;
      return `<a href="${bare}" target="_blank" rel="noopener noreferrer">${bare}</a>${trailing}`;
    });

    // 7) Collapse paragraphs: split on blank lines, wrap each block, and
    //    turn single newlines inside a block into <br>.
    const paragraphs = working
      .split(/\n{2,}/)
      .map((p) => p.trim())
      .filter((p) => p.length > 0)
      .map((p) => {
        // Don't wrap a pure codeblock placeholder paragraph.
        if (/^\u0000CODEBLOCK\d+\u0000$/.test(p)) return p;
        return `<p>${p.replace(/\n/g, '<br>')}</p>`;
      });

    let html = paragraphs.join('\n');

    // 8) Restore code blocks.
    html = html.replace(/\u0000CODEBLOCK(\d+)\u0000/g, (_m, idx) => codeBlocks[Number(idx)] ?? '');

    return html;
  }

  /**
   * Event delegate for `[[wiki-link]]` chips inside the transcript.
   * Delegation avoids attaching N click handlers on every render pass
   * (the markdown is re-rendered whenever a delta arrives during
   * streaming; per-bubble listeners would churn badly).
   *
   * Resolution uses the same two-pass precedence as the indexer's
   * link resolver (title match → stem match). Unresolved links
   * surface a tiny toast-style banner rather than silently swallowing
   * the click.
   */
  async function onTranscriptClick(ev: MouseEvent | KeyboardEvent): Promise<void> {
    const target = ev.target as HTMLElement | null;
    if (!target) return;
    const chip = target.closest<HTMLElement>('.chat-wiki-link');
    if (!chip) return;
    if (ev.type === 'keydown') {
      const k = (ev as KeyboardEvent).key;
      if (k !== 'Enter' && k !== ' ') return;
      ev.preventDefault();
    }
    const rawTarget = chip.getAttribute('data-wiki-target');
    if (!rawTarget) return;
    try {
      const resolved = await indexResolveWikiLink(rawTarget);
      if (resolved) {
        onOpenNote(resolved.path);
      } else {
        uiError = `链接未解析：[[${rawTarget}]]（未在当前 vault 找到匹配的笔记标题或文件名）`;
      }
    } catch (e) {
      uiError = String(e);
    }
  }

  function failureHint(f: ChatSendFailure): string {
    switch (f.kind) {
      case 'network':
        return '检查网络连接或 provider base URL。';
      case 'auth':
        return 'API key 无效或已过期，请在「设置 → AI 辅助」重新配置。';
      case 'rate_limit':
        return f.retry_after_secs
          ? `请 ${f.retry_after_secs} 秒后重试。`
          : '已触发 provider 限流，稍后重试。';
      case 'invalid_request':
        return '请求被 provider 拒绝，检查 chat model 名称与上下文长度。';
      default:
        return '';
    }
  }
</script>

<div class="chat-panel chat-panel--{variant}">
  {#if variant === 'standalone'}
    <!-- Standalone header: matches Second-design's chat column head.
         Docked variant skips this — Panel.svelte's tab bar already labels it. -->
    <header class="chat-head">
      <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
        <path
          d="M3 5a2 2 0 012-2h8a2 2 0 012 2v5a2 2 0 01-2 2H8l-4 3v-3H5a2 2 0 01-2-2V5z"
          stroke="currentColor"
          stroke-width="1.3"
          stroke-linejoin="round"
        />
      </svg>
      <span class="chat-head-title">AI Chat</span>
    </header>
    <div class="chat-head-divider" aria-hidden="true"></div>
  {/if}

  <!-- ── Session sidebar / selector ────────────────────────────── -->
  <!-- Hide the chooser row when in standalone variant with only the default
       session — the chat-head above already labels this surface. The row
       re-appears automatically once a second session exists. -->
  {#if !(variant === 'standalone' && sessions.length <= 1 && !loadingList)}
    <header class="session-header">
      {#if loadingList}
        <span class="muted">加载会话…</span>
      {:else if sessions.length === 0}
        <span class="muted">还没有会话</span>
      {:else}
        <select
          class="session-select"
          value={activeSessionId ?? ''}
          onchange={(e) => switchSession((e.currentTarget as HTMLSelectElement).value)}
          aria-label="切换会话"
        >
          {#each sessions as s (s.session_id)}
            <option value={s.session_id}>
              {s.title} · {s.message_count} 条
            </option>
          {/each}
        </select>
      {/if}
      <div class="session-actions">
        <button
          class="icon-btn"
          title="新建会话"
          onclick={newSession}
          disabled={sending}
          aria-label="新建会话"
        >
          +
        </button>
        <button
          class="icon-btn danger"
          title="删除当前会话"
          onclick={deleteActiveSession}
          disabled={!activeSessionId || sending}
          aria-label="删除会话"
        >
          ×
        </button>
      </div>
    </header>
  {/if}

  <!-- "关联笔记" pill — only in the docked variant, where the chat IS about
       the currently-open note. The standalone chat is a global agent surface;
       showing a specific file path there conflicts with its role. -->
  {#if variant !== 'standalone' && activeSession?.meta.related_note}
    <div class="session-meta" title={activeSession.meta.related_note}>
      关联笔记 · <span class="mono">{activeSession.meta.related_note}</span>
    </div>
  {/if}

  <!-- ── Transcript ────────────────────────────────────────────── -->
  <!-- svelte-ignore a11y_no_static_element_interactions — event delegate on the transcript reaches wiki-link chips rendered via @html; attaching per-chip handlers would churn on every streaming delta. -->
  <div
    class="transcript"
    data-testid="chat-transcript"
    bind:this={transcriptEl}
    onclick={(e) => { void onTranscriptClick(e); }}
    onkeydown={(e) => { void onTranscriptClick(e); }}
  >
    {#if loadingSession}
      <p class="empty">加载中…</p>
    {:else if !activeSession || activeSession.messages.length === 0}
      <p class="empty">
        {#if activeSessionId}
          发一条消息开始对话。
        {:else}
          点击 <strong>+</strong> 新建会话，或直接在下方输入。
        {/if}
      </p>
    {:else}
      {#each activeSession.messages as m (m.id)}
        {#if !(m.role === 'tool' && persistedToolPairing.consumedToolMessageIds.has(m.id))}
          <article class="bubble {m.role}">
            <header class="bubble-header">
              <span class="role">{roleLabel(m.role)}</span>
              <span class="time" title={new Date(m.created_at * 1000).toLocaleString()}>
                {formatTime(m.created_at)}
              </span>
            </header>
            {#if m.content.trim().length > 0}
              <div class="bubble-body">
                <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                {@html renderMarkdown(m.content)}
              </div>
            {:else if m.role === 'assistant' && m.tool_calls?.length}
              <div class="bubble-body bubble-body--empty">调用了工具，结果见下方卡片。</div>
            {/if}
            {#if m.role === 'assistant' && citationsByAssistantId[m.id]?.length}
              <footer class="bubble-citations" aria-label="参考来源">
                <span class="citations-label">来源</span>
                {#each citationsByAssistantId[m.id] as c, i (c.note_rel_path + '#' + c.chunk_index)}
                  <button
                    type="button"
                    class="citation-chip"
                    title={`${c.note_rel_path} · 相似度 ${c.score.toFixed(2)}\n\n${c.preview}`}
                    onclick={() => onOpenNote(c.note_rel_path)}
                  >
                    <span class="citation-idx">[{i + 1}]</span>
                    <span class="citation-path">{c.note_rel_path}</span>
                  </button>
                {/each}
              </footer>
            {/if}
          </article>

          {#if m.role === 'assistant' && persistedToolPairing.toolCallsByAssistantId[m.id]?.length}
            <div class="tool-card-stack" aria-label="持久化工具调用">
              {#each persistedToolPairing.toolCallsByAssistantId[m.id] as toolVm (toolVm.key)}
                {#if toolVm.proposal}
                  <ProposalCard
                    viewModel={toolVm}
                    resolutionState={proposalResolutionFor(toolVm)}
                    onAccept={(proposal) => acceptProposal(toolVm, proposal)}
                    onReject={(proposal) => rejectProposal(toolVm, proposal)}
                    onAdjust={(proposal) => adjustProposal(toolVm, proposal)}
                  />
                {:else}
                  <ToolCallCard viewModel={toolVm} />
                {/if}
              {/each}
            </div>
          {/if}
        {/if}
      {/each}
    {/if}

    {#if sending && currentToolTrace}
      <div class="tool-trace" role="status" data-testid="chat-tool-trace">{currentToolTrace}</div>
    {/if}

    {#if sending && inlineToolCallCards.length > 0}
      <div
        class="tool-card-stack tool-card-stack--inline"
        aria-label="工具调用日志"
        data-testid="chat-inline-tool-cards"
      >
        {#each inlineToolCallCards as toolVm (toolVm.key)}
          {#if toolVm.proposal}
            <ProposalCard viewModel={toolVm} />
          {:else}
            <ToolCallCard viewModel={toolVm} />
          {/if}
        {/each}
      </div>
    {/if}

    {#if sending}
      <article class="bubble assistant pending" data-testid="chat-streaming-bubble">
        <header class="bubble-header">
          <span class="role">AI</span>
          <span class="time">流式中…</span>
        </header>
        <div class="bubble-body">
          {#if streamingContent.length > 0}
            <!-- eslint-disable-next-line svelte/no-at-html-tags -->
            {@html renderMarkdown(streamingContent)}
            <span class="streaming-cursor" aria-hidden="true"></span>
          {:else}
            <span class="typing-dots"><span></span><span></span><span></span></span>
          {/if}
        </div>
        {#if pendingCitations.length > 0}
          <footer class="bubble-citations" aria-label="参考来源">
            <span class="citations-label">来源</span>
            {#each pendingCitations as c, i (c.note_rel_path + '#' + c.chunk_index)}
              <button
                type="button"
                class="citation-chip"
                title={`${c.note_rel_path} · 相似度 ${c.score.toFixed(2)}\n\n${c.preview}`}
                onclick={() => onOpenNote(c.note_rel_path)}
              >
                <span class="citation-idx">[{i + 1}]</span>
                <span class="citation-path">{c.note_rel_path}</span>
              </button>
            {/each}
          </footer>
        {/if}
      </article>
    {/if}
  </div>

  <!-- ── Error banners ─────────────────────────────────────────── -->
  {#if uiError}
    <div class="banner error" role="alert">
      <span>{uiError}</span>
      <button class="banner-close" onclick={() => (uiError = null)} aria-label="关闭">×</button>
    </div>
  {/if}
  {#if lastFailure}
    <div class="banner failure" role="alert" data-testid="chat-failure-banner">
      <div class="banner-row">
        <strong>发送失败</strong>
        <span class="banner-kind" data-testid="chat-failure-kind">{lastFailure.kind}</span>
        <button class="banner-close" onclick={() => (lastFailure = null)} aria-label="关闭">×</button>
      </div>
      <p class="banner-msg" title={lastFailure.message}>{lastFailure.message}</p>
      {#if failureHint(lastFailure)}
        <p class="banner-hint">{failureHint(lastFailure)}</p>
      {/if}
      {#if !lastFailure.user_message_persisted}
        <p class="banner-hint">你的消息未发送，可修改后重试。</p>
      {/if}
    </div>
  {/if}

  <!-- ── Composer ──────────────────────────────────────────────── -->
  <footer class="composer">
    <div class="composer-pane">
      <textarea
        bind:this={composeEl}
        bind:value={composeText}
        onkeydown={onComposeKeydown}
        placeholder="Describe your agenda, tasks, or project…"
        rows="1"
        disabled={sending}
        aria-label="输入消息"
        data-testid="chat-compose"
      ></textarea>
      {#if sending && activeStreamId}
        <button
          class="send-btn cancel-btn"
          onclick={cancelStreaming}
          title="中断当前回复（已生成内容会保留）"
          data-testid="chat-cancel"
          aria-label="中断"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
            <rect x="2" y="2" width="6" height="6" rx="1" fill="currentColor" />
          </svg>
        </button>
      {:else}
        <button
          class="send-btn"
          class:is-empty={composeText.trim().length === 0}
          onclick={send}
          disabled={sending || composeText.trim().length === 0}
          data-testid="chat-send"
          aria-label="发送"
          title="发送 (Enter)"
        >
          <svg width="12" height="12" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <path
              d="M7 12V2M3 6l4-4 4 4"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>
      {/if}
    </div>
    <div class="composer-foot">
      {#if variant !== 'standalone' && activeSession?.meta.related_note}
        <span class="composer-hint mono">{activeSession.meta.related_note}</span>
      {:else}
        <span class="composer-caption">
          AI automatically organizes content into notes, tasks, and events in your Vault.
        </span>
      {/if}
    </div>
  </footer>

  {#if activeSession?.messages.length}
    <div class="session-footer">
      共 {activeSession.messages.length} 条 · 最新：{formatRelative(
        activeSession.messages.at(-1)?.created_at
      )}
    </div>
  {/if}
</div>

{#if newSessionModalOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events — backdrop click dismiss is a click-only UX; modal body has its own keyboard handling. -->
  <!-- svelte-ignore a11y_no_static_element_interactions — backdrop is a passive dismiss surface; the dialog body below handles focus + keys. -->
  <div
    class="ns-backdrop"
    onclick={cancelNewSession}
  >
    <div
      class="ns-modal"
      role="dialog"
      aria-modal="true"
      aria-label="新建 AI 会话"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
    >
      <h3 class="ns-title">新建 AI 会话</h3>
      <p class="ns-hint">
        会话将保存到 <code>.my-notes/chat-sessions/</code>，可随时切换。留空则使用时间戳标题。
      </p>
      <label class="ns-field">
        <span class="ns-label">标题</span>
        <input
          bind:this={newSessionInputEl}
          bind:value={newSessionTitle}
          onkeydown={onNewSessionKey}
          placeholder={defaultTitleForNow()}
          class="ns-input"
          maxlength="120"
          disabled={newSessionBusy}
        />
      </label>
      {#if filePath}
        <label class="ns-checkbox">
          <input
            type="checkbox"
            bind:checked={newSessionLinkNote}
            disabled={newSessionBusy}
          />
          <span>
            关联当前笔记
            <code class="ns-path">{filePath}</code>
            <span class="ns-checkbox-hint">（RAG 会优先检索该笔记的相关片段）</span>
          </span>
        </label>
      {:else}
        <p class="ns-none-hint">当前未打开笔记，若需要关联笔记请先在编辑器中打开一个。</p>
      {/if}
      {#if newSessionError}
        <p class="ns-error">{newSessionError}</p>
      {/if}
      <div class="ns-actions">
        <button type="button" onclick={cancelNewSession} disabled={newSessionBusy}>取消</button>
        <button
          type="button"
          class="ns-primary"
          onclick={confirmNewSession}
          disabled={newSessionBusy}
        >
          {newSessionBusy ? '创建中…' : '创建'}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .chat-panel {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
    font-size: var(--fs-sm);
  }
  /* Standalone: a dedicated webview owns the whole viewport. Widen the
     transcript, give the composer a touch more breathing room. No other
     behavior changes — IPC / event routing is identical. */
  .chat-panel--standalone {
    padding: 0;
    max-width: none;
  }

  /* ── Standalone head (Second-design chat column) ───────────────────── */
  .chat-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 26px 10px;
    flex-shrink: 0;
    color: var(--color-fg-muted);
  }
  .chat-head-title {
    flex: 1;
    font-size: 13px;
    font-weight: 500;
    color: var(--color-fg);
    letter-spacing: -0.1px;
  }
  .chat-head-divider {
    height: 0.5px;
    background: var(--color-border);
    margin: 0 26px;
    flex-shrink: 0;
  }

  /* ── Session header ─────────────────────────────────────────────────── */
  .session-header {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 10px 26px;
    background: transparent;
    box-shadow: inset 0 -0.5px 0 var(--color-border);
  }
  .chat-panel--docked .session-header {
    padding: 8px 14px;
  }
  .session-select {
    flex: 1;
    min-width: 0;
    padding: 6px 8px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-fg);
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .session-select:focus {
    outline: 2px solid var(--color-accent);
    outline-offset: 1px;
  }
  .session-actions {
    display: flex;
    gap: 4px;
  }
  .icon-btn {
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid var(--color-border);
    background: var(--color-surface-raised);
    color: var(--color-fg);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 16px;
    line-height: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition:
      background 0.15s ease,
      border-color 0.15s ease;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--color-bg-hover);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .icon-btn.danger:hover:not(:disabled) {
    border-color: var(--color-danger, #c94f4f);
    color: var(--color-danger, #c94f4f);
  }
  .session-meta {
    padding: 4px 12px;
    background: var(--color-surface);
    font-size: var(--fs-xs);
    color: var(--color-fg-muted);
    border-bottom: 1px solid var(--color-border);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mono {
    font-family: var(--font-mono);
  }

  /* ── Transcript ─────────────────────────────────────────────────────── */
  .transcript {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px 26px 8px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .chat-panel--docked .transcript {
    padding: 12px 14px 6px;
    gap: 12px;
  }
  .empty {
    margin: auto;
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
    text-align: center;
    padding: 32px 16px;
  }

  /* Bubbles: user = raised surface on the right; assistant = transparent on
     the left with a glow-dot label on top. Matches Second-design:Message
     (app-core.jsx:444-512). Compact variant swaps in smaller padding/radius
     so the docked right-panel chat stays readable at 300px wide. */
  .bubble {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 18px;
    border-radius: var(--radius-lg);
    max-width: 78%;
    word-wrap: break-word;
    overflow-wrap: break-word;
  }
  .bubble.user {
    align-self: flex-end;
    background: var(--color-surface-raised);
    box-shadow: var(--pane-border);
    color: var(--color-fg);
  }
  .bubble.assistant {
    align-self: flex-start;
    background: transparent;
    box-shadow: none;
    border: none;
    padding-left: 0;
    padding-right: 0;
    max-width: 82%;
  }
  .bubble.tool {
    align-self: flex-start;
    background: color-mix(in oklch, var(--color-surface) 70%, transparent);
    border: 1px dashed var(--color-border);
  }
  .bubble.system {
    align-self: stretch;
    background: transparent;
    border: 1px dashed var(--color-border);
    color: var(--color-fg-muted);
    font-size: var(--fs-xs);
    padding: 8px 12px;
  }
  .bubble.pending {
    opacity: 0.85;
  }
  /* Compact density (right-panel / docked). */
  .chat-panel--docked .bubble {
    padding: 10px 14px;
    border-radius: var(--radius-md);
    max-width: 92%;
  }
  .chat-panel--docked .bubble.assistant {
    padding-left: 0;
    padding-right: 0;
  }

  .bubble-header {
    display: flex;
    align-items: center;
    gap: 8px;
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-muted);
  }
  .bubble.assistant .bubble-header {
    opacity: 0.85;
  }
  /* Glow dot in front of the assistant role label (Second-design). */
  .bubble.assistant .bubble-header .role::before {
    content: '';
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: var(--accent-glow);
    margin-right: 8px;
    vertical-align: middle;
    transform: translateY(-1px);
  }
  .bubble-header .role {
    font-weight: 500;
  }
  .bubble-header .time {
    color: var(--color-fg-dim);
  }
  /* User bubbles: hide the redundant "USER" role label — alignment + surface
     already convey who spoke. Keep the timestamp. */
  .bubble.user .bubble-header .role {
    display: none;
  }
  .bubble-body {
    font-size: 14.5px;
    line-height: 1.55;
    letter-spacing: -0.1px;
    color: var(--color-fg);
  }
  .chat-panel--docked .bubble-body {
    font-size: 14px;
  }
  .bubble-body--empty {
    color: var(--color-fg-muted);
    font-style: italic;
  }
  .bubble-body :global(p) {
    margin: 0 0 8px;
  }
  .bubble-body :global(p:last-child) {
    margin-bottom: 0;
  }
  .bubble-body :global(a) {
    color: var(--color-accent);
    text-decoration: underline;
  }
  .bubble-body :global(.chat-codeblock) {
    margin: 8px 0;
    padding: 8px 10px;
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    overflow-x: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.45;
  }
  .bubble-body :global(.chat-codeblock code) {
    font-family: inherit;
    background: transparent;
    padding: 0;
  }
  .bubble-body :global(.chat-inlinecode) {
    padding: 1px 5px;
    background: var(--color-bg-hover);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 0.92em;
  }
  .bubble-body :global(.chat-wiki-link) {
    color: var(--color-accent);
    background: color-mix(in oklch, var(--color-accent) 10%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-accent) 30%, transparent);
    border-radius: 4px;
    padding: 0 4px;
    cursor: pointer;
    transition:
      background 0.12s ease,
      border-color 0.12s ease;
  }
  .bubble-body :global(.chat-wiki-link:hover),
  .bubble-body :global(.chat-wiki-link:focus) {
    background: color-mix(in oklch, var(--color-accent) 22%, transparent);
    border-color: var(--color-accent);
    outline: none;
  }

  /* ── Citations footer ─────────────────────────────────────────────── */
  .bubble-citations {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px dashed var(--color-border);
  }
  .citations-label {
    font-size: 10px;
    color: var(--color-fg-muted);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-family: var(--font-mono);
    margin-right: 2px;
  }
  .citation-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-fg-muted);
    border-radius: 4px;
    font-size: 10px;
    font-family: var(--font-mono);
    cursor: pointer;
    max-width: 260px;
    transition:
      background 0.12s ease,
      color 0.12s ease,
      border-color 0.12s ease;
  }
  .citation-chip:hover {
    color: var(--color-fg);
    background: var(--color-bg-hover);
    border-color: var(--color-accent);
  }
  .citation-idx {
    color: var(--color-accent);
    font-weight: 600;
  }
  .citation-path {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tool-trace {
    margin: 4px 0 2px 18px;
    padding: 8px 12px;
    border-radius: 999px;
    background: color-mix(in oklch, var(--color-accent) 10%, transparent);
    color: var(--color-accent);
    font-size: 12px;
    line-height: 1.4;
    align-self: flex-start;
  }
  .tool-card-stack {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: -4px 0 0 18px;
    padding-left: 10px;
    border-left: 2px solid color-mix(in oklch, var(--color-border) 80%, transparent);
  }
  .tool-card-stack--inline {
    margin-top: 6px;
  }

  .typing-dots {
    display: inline-flex;
    gap: 4px;
    padding: 4px 0;
  }
  .typing-dots span {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--color-fg-muted);
    animation: typing-bounce 1.2s infinite ease-in-out;
  }
  .typing-dots span:nth-child(2) {
    animation-delay: 0.15s;
  }
  .typing-dots span:nth-child(3) {
    animation-delay: 0.3s;
  }
  @keyframes typing-bounce {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.4;
    }
    30% {
      transform: translateY(-4px);
      opacity: 1;
    }
  }

  /* ── Banners ────────────────────────────────────────────────────────── */
  .banner {
    margin: 0 12px 8px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    line-height: 1.5;
  }
  .banner.failure {
    background: color-mix(in oklch, var(--color-danger, #c94f4f) 14%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-danger, #c94f4f) 40%, transparent);
    color: var(--color-fg);
  }
  .banner.error {
    background: color-mix(in oklch, var(--color-warn, #b58e3a) 14%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-warn, #b58e3a) 40%, transparent);
    color: var(--color-fg);
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
  }
  .banner-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .banner-kind {
    font-family: var(--font-mono);
    font-size: 10px;
    padding: 1px 6px;
    background: var(--color-bg-hover);
    border-radius: 4px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .banner-close {
    margin-left: auto;
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 2px 6px;
  }
  .banner-msg {
    margin: 4px 0 0;
    word-break: break-word;
  }
  .banner-hint {
    margin: 4px 0 0;
    color: var(--color-fg-muted);
  }

  /* ── Composer (Second-design) ──────────────────────────────────────── */
  .composer {
    padding: 10px 26px 18px;
    background: transparent;
    border-top: none;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .chat-panel--docked .composer {
    padding: 8px 14px 14px;
  }
  /* Raised inner pane; textarea is borderless inside. Focus-within lights up
     a 2px accent ring + accent glow. */
  .composer-pane {
    background: var(--color-surface-raised);
    border-radius: var(--radius-lg);
    box-shadow: var(--pane-border);
    padding: 10px 14px;
    display: flex;
    align-items: flex-end;
    gap: 10px;
    transition: box-shadow 0.25s ease;
  }
  .chat-panel--docked .composer-pane {
    padding: 8px 12px;
    border-radius: var(--radius-md);
  }
  .composer-pane:focus-within {
    box-shadow:
      var(--pane-border),
      0 0 0 2px var(--color-accent-weak),
      var(--accent-glow);
  }
  .composer textarea {
    flex: 1;
    min-height: 28px;
    max-height: 220px;
    padding: 4px 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--color-fg);
    font-family: inherit;
    font-size: 14px;
    line-height: 1.5;
    letter-spacing: -0.1px;
    resize: none;
    caret-color: var(--color-accent);
  }
  .composer textarea::placeholder {
    color: var(--color-fg-dim);
  }
  .composer textarea:disabled {
    opacity: 0.6;
  }
  /* Circular send: accent-filled + glow when the input has content; neutral
     otherwise. No pane-border on the button itself (sits inside the pane). */
  .send-btn {
    width: 30px;
    height: 30px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: var(--color-accent);
    color: #fff;
    box-shadow: var(--accent-glow);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    transition: background 0.2s ease, box-shadow 0.2s ease, color 0.2s ease;
  }
  .send-btn:hover:not(:disabled) {
    filter: brightness(1.06);
    transform: none;
  }
  .send-btn.is-empty {
    background: var(--color-bg-hover);
    box-shadow: none;
    color: var(--color-fg-dim);
  }
  .send-btn:disabled {
    cursor: not-allowed;
  }
  .send-btn.cancel-btn {
    background: var(--color-danger, #c94f4f);
    box-shadow: none;
    color: #fff;
  }
  .composer-foot {
    margin-top: 10px;
    display: flex;
    justify-content: center;
  }
  .composer-caption {
    font-size: 11px;
    color: var(--color-fg-dim);
    text-align: center;
    letter-spacing: -0.1px;
  }
  .composer-hint {
    font-size: 10px;
    color: var(--color-fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 80%;
  }
  /* Docked variant: drop the caption — vertical space is scarce at 300px. */
  .chat-panel--docked .composer-foot {
    margin-top: 6px;
  }
  .chat-panel--docked .composer-caption {
    display: none;
  }

  .streaming-cursor {
    display: inline-block;
    width: 2px;
    height: 1em;
    margin-left: 2px;
    vertical-align: text-bottom;
    background: var(--color-accent);
    animation: cursor-blink 1s steps(2, start) infinite;
  }
  @keyframes cursor-blink {
    to {
      visibility: hidden;
    }
  }

  .session-footer {
    padding: 4px 12px 8px;
    font-size: 10px;
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    text-align: center;
  }
  .hint {
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60%;
  }
  .muted {
    color: var(--color-fg-muted);
  }

  /* ── New-session modal ────────────────────────────────────────────── */
  .ns-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.32);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 24px;
  }
  .ns-modal {
    width: min(460px, 100%);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    padding: 20px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .ns-title {
    margin: 0;
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .ns-hint {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--color-fg-muted);
    line-height: 1.5;
  }
  .ns-hint code {
    font-family: var(--font-mono);
    font-size: 0.9em;
    background: var(--color-bg-hover);
    padding: 0 4px;
    border-radius: 3px;
  }
  .ns-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ns-label {
    font-size: var(--fs-xs);
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .ns-input {
    padding: 8px 10px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-fg);
    border-radius: 4px;
    font-size: var(--fs-sm);
    font-family: inherit;
  }
  .ns-input:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 2px color-mix(in oklch, var(--color-accent) 25%, transparent);
  }
  .ns-checkbox {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--color-fg);
    cursor: pointer;
  }
  .ns-checkbox input {
    margin-top: 3px;
  }
  .ns-path {
    font-family: var(--font-mono);
    background: var(--color-bg-hover);
    padding: 0 4px;
    border-radius: 3px;
    font-size: 0.92em;
  }
  .ns-checkbox-hint {
    display: block;
    color: var(--color-fg-muted);
    font-size: 10px;
    margin-top: 2px;
  }
  .ns-none-hint {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--color-fg-muted);
    font-style: italic;
  }
  .ns-error {
    margin: 0;
    padding: 6px 10px;
    border: 1px solid var(--color-danger, #c94a4a);
    background: color-mix(in oklch, var(--color-danger, #c94a4a) 12%, transparent);
    color: var(--color-danger, #c94a4a);
    border-radius: 4px;
    font-size: var(--fs-xs);
  }
  .ns-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
  .ns-actions button {
    padding: 6px 14px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-fg);
    border-radius: 4px;
    cursor: pointer;
    font-size: var(--fs-sm);
    transition:
      background 0.12s ease,
      border-color 0.12s ease;
  }
  .ns-actions button:hover:not(:disabled) {
    background: var(--color-bg-hover);
    border-color: var(--color-accent);
  }
  .ns-actions button:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .ns-actions .ns-primary {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-bg);
    font-weight: 600;
  }
  .ns-actions .ns-primary:hover:not(:disabled) {
    background: color-mix(in oklch, var(--color-accent) 85%, white);
    border-color: color-mix(in oklch, var(--color-accent) 85%, white);
  }
</style>
