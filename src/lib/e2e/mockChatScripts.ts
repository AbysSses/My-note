/**
 * Phase 4 Stage 1.5 — extracted ChatPanel mock script.
 *
 * Why this lives in its own file:
 *
 * - ChatPanel.svelte is already ~2330 lines; the mock used to add
 *   ~250 of those, all interleaved with production code paths. Moving
 *   the mock out makes the component readable again and lets us fuzz /
 *   storybook-style exercise the chat UX without touching the panel.
 * - The build-time `import.meta.env.PUBLIC_E2E === '1'` gate is read
 *   in ChatPanel.svelte, so production bundles continue to drop the
 *   entire `if (e2eMockMode) { … }` branch — the imports here become
 *   unreferenced and Rollup tree-shakes the module out completely.
 * - Tests can import this module directly to assert the script the
 *   mock should produce (e.g. "given '请生成删除提案', the script must
 *   emit a destructive proposal payload").
 *
 * Design notes:
 *
 * - The mock manipulates ~10 pieces of ChatPanel state (active
 *   session, sessions list, streaming buffer, inline tool events,
 *   …). Rather than re-create those as module-level mutable state
 *   (which would split the panel's view model in two and invite
 *   subtle desyncs), we accept a `MockChatHandles` object that
 *   exposes typed getters / setters / appenders. ChatPanel constructs
 *   one wrapper around its own `$state` slots and passes it in.
 * - `runMockSend` is the hot loop. The smaller helpers
 *   (`mockCreateSession`, `mockSessionSummary`, `mockSleep`,
 *   `mockNow`) are exported individually so the few callers outside
 *   the send loop (`refreshSessions`, `createNewSession`,
 *   `deleteActiveSession`, …) can use the same primitives.
 */

import type { InlineToolEvent } from '$lib/chat/toolCallViewModel';
import type {
  ChatMessage,
  ChatSendFailure,
  ChatSessionFull,
  ChatSessionSummary,
  RagCitation
} from '$lib/ipc/ai';

let mockSessionSeq = 0;

/** Seconds-since-epoch — matches the chat_store timestamps. */
export function mockNow(): number {
  return Math.floor(Date.now() / 1000);
}

/** Yield to the timer queue between streamed chunks. */
export function mockSleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Synthesize a fresh ChatSessionFull with a unique mock id. */
export function mockCreateSession(
  title: string,
  relatedNote: string | null
): ChatSessionFull {
  mockSessionSeq += 1;
  const sessionId = `mock-session-${mockSessionSeq}`;
  const createdAt = mockNow();
  return {
    meta: {
      v: 1,
      session_id: sessionId,
      title,
      created_at: createdAt,
      related_note: relatedNote ?? undefined
    },
    messages: []
  };
}

/** Project a ChatSessionFull into the lighter ChatSessionSummary view. */
export function mockSessionSummary(full: ChatSessionFull): ChatSessionSummary {
  return {
    session_id: full.meta.session_id,
    title: full.meta.title,
    created_at: full.meta.created_at,
    message_count: full.messages.length,
    last_message_at: full.messages.at(-1)?.created_at,
    related_note: full.meta.related_note
  };
}

/**
 * View into ChatPanel's state that `runMockSend` is allowed to read /
 * write. Exposes only the slots the mock actually needs — keeps the
 * surface narrow so future ChatPanel refactors don't ripple here.
 */
export interface MockChatHandles {
  /** Current session, or null when there isn't one yet. */
  getActiveSession(): ChatSessionFull | null;
  /** Vault-relative path of the file the panel is bound to, or null. */
  getFilePath(): string | null;
  /** Whether the cancel button has been pressed since the last send. */
  isCancelRequested(): boolean;

  setActiveSession(session: ChatSessionFull | null): void;
  setActiveSessionId(id: string | null): void;
  setLastResolvedSessionId(id: string | null): void;
  setSessions(list: ChatSessionSummary[]): void;
  setSending(busy: boolean): void;
  setLastFailure(failure: ChatSendFailure | null): void;
  setComposeText(text: string): void;

  setStreamingContent(text: string): void;
  appendStreamingContent(chunk: string): void;

  setInlineToolEvents(events: InlineToolEvent[]): void;
  appendInlineToolEvent(event: InlineToolEvent): void;

  setPendingCitations(citations: RagCitation[]): void;
  resetCancelRequested(): void;

  /** Caller-supplied micro-tick (typically Svelte's `tick()`). */
  tick(): Promise<void>;
}

/**
 * Fall-through derivation of a session title from the first user
 * message. Mirrors what the production path does on its first send;
 * extracted here so ChatPanel can drop the duplicated helper. Kept
 * deliberately simple — first 32 chars, single line.
 */
function deriveSessionTitle(text: string): string {
  const first = text.trim().split(/\r?\n/)[0] ?? text;
  return first.length <= 32 ? first || '新对话' : `${first.slice(0, 32)}…`;
}

/**
 * Run the keyword-routed mock send. Drives the same observable chain
 * the real provider would: streaming chunks → optional tool call →
 * optional proposal → final assistant message. Branches:
 *
 * - text contains "失败" / "超时" → failure banner only, no stream
 * - text contains "取消测试"     → long stream that respects cancel
 * - text contains "删除提案" / "rename提案" → destructive proposal
 * - text contains "提案" / "summary"      → summary proposal
 * - text contains "搜索" / "tool"         → search_by_tag tool trace
 * - default                              → short three-chunk reply
 *
 * Cancellation: while streaming, we poll `handles.isCancelRequested()`
 * after every chunk. On cancel we still persist whatever was
 * streamed (so the panel matches the real "cancel saves partial"
 * semantic) and surface a `lastFailure` of kind `'other'`.
 */
export async function runMockSend(
  text: string,
  handles: MockChatHandles
): Promise<void> {
  const filePath = handles.getFilePath();
  let session = handles.getActiveSession();
  if (!session) {
    const created = mockCreateSession(deriveSessionTitle(text), filePath ?? null);
    session = created;
    handles.setActiveSession(created);
    handles.setActiveSessionId(created.meta.session_id);
    handles.setLastResolvedSessionId(created.meta.session_id);
    handles.setSessions([mockSessionSummary(created)]);
  }

  const userMsg: ChatMessage = {
    v: 1,
    id: `mock-user-${Date.now()}`,
    role: 'user',
    content: text,
    created_at: mockNow()
  };
  const sessionWithUser: ChatSessionFull = {
    ...session,
    messages: [...session.messages, userMsg]
  };
  handles.setActiveSession(sessionWithUser);
  handles.setSessions([mockSessionSummary(sessionWithUser)]);

  handles.setSending(true);
  handles.setLastFailure(null);
  handles.resetCancelRequested();
  handles.setComposeText('');
  handles.setStreamingContent('');
  handles.setInlineToolEvents([]);
  handles.setPendingCitations([]);
  await handles.tick();

  const wantsFailure = text.includes('失败') || text.includes('超时');
  const wantsDestructiveProposal =
    text.includes('删除提案') || text.includes('rename提案');
  const wantsProposal =
    wantsDestructiveProposal || text.includes('提案') || text.includes('summary');
  const wantsTool =
    wantsProposal || text.includes('搜索') || text.includes('tool');
  const wantsLongStream = text.includes('取消测试');

  if (wantsFailure) {
    handles.setLastFailure({
      kind: text.includes('超时') ? 'network' : 'other',
      message: text.includes('超时') ? 'mock timeout' : 'mock provider failure',
      user_message_persisted: true
    });
    handles.setSending(false);
    return;
  }

  let mockToolCallId: string | null = null;
  if (wantsTool) {
    const callId = `mock-call-${Date.now()}`;
    mockToolCallId = callId;
    handles.appendInlineToolEvent({
      kind: 'requested',
      call_id: callId,
      name: wantsProposal
        ? wantsDestructiveProposal
          ? 'delete_note'
          : 'propose_summary'
        : 'search_by_tag',
      arguments: wantsProposal
        ? wantsDestructiveProposal
          ? JSON.stringify({ target_rel_path: filePath ?? '1-notes/mock-note.md' })
          : JSON.stringify({
              target_rel_path: filePath ?? '1-notes/mock-note.md',
              target: 'frontmatter'
            })
        : JSON.stringify({ tag: 'project', limit: 5 })
    });
    await mockSleep(120);
    const proposalPayload = JSON.stringify(
      {
        proposal_kind: wantsDestructiveProposal ? 'delete_note' : 'summary',
        target_rel_path: filePath ?? '1-notes/mock-note.md',
        original_content: '---\ntitle: Mock Note\n---\n\n原始内容',
        proposed_content: wantsDestructiveProposal
          ? ''
          : '---\ntitle: Mock Note\nsummary: 这是一条 Mock 摘要\n---\n\n原始内容',
        summary: wantsDestructiveProposal ? '删除目标笔记' : '为当前笔记补充 frontmatter.summary'
      },
      null,
      2
    );
    handles.appendInlineToolEvent({
      kind: 'result',
      call_id: callId,
      content: wantsProposal
        ? proposalPayload
        : JSON.stringify(
            {
              hits: [{ path: '1-notes/mock-note.md', title: 'Mock Note' }]
            },
            null,
            2
          ),
      is_error: false
    });
  }

  const chunks = wantsProposal
    ? wantsDestructiveProposal
      ? ['我已生成删除提案，请二次确认。']
      : ['我已生成摘要提案，请确认是否接受。']
    : wantsLongStream
    ? ['这是 ', '一个 ', '可取消 ', '的 ', 'Mock ', '长回复。']
    : wantsTool
    ? ['已为你搜索 ', '#project', '，找到 1 条结果。']
    : ['这是 ', '一条 ', 'Mock 流式回复。'];

  let streamingContent = '';
  for (const chunk of chunks) {
    if (handles.isCancelRequested()) break;
    streamingContent += chunk;
    handles.appendStreamingContent(chunk);
    await mockSleep(90);
  }

  const sessionAfterStream = handles.getActiveSession();
  if (sessionAfterStream) {
    if (wantsProposal && mockToolCallId) {
      const proposalAssistantId = `mock-assistant-tool-${Date.now()}`;
      const toolCall = {
        id: mockToolCallId,
        name: wantsDestructiveProposal ? 'delete_note' : 'propose_summary',
        arguments: JSON.stringify({
          target_rel_path: filePath ?? '1-notes/mock-note.md'
        })
      };
      const toolMsg: ChatMessage = {
        v: 1,
        id: `mock-tool-${Date.now()}`,
        role: 'tool',
        content: JSON.stringify(
          {
            proposal_kind: wantsDestructiveProposal ? 'delete_note' : 'summary',
            target_rel_path: filePath ?? '1-notes/mock-note.md',
            original_content: '---\ntitle: Mock Note\n---\n\n原始内容',
            proposed_content: wantsDestructiveProposal
              ? ''
              : '---\ntitle: Mock Note\nsummary: 这是一条 Mock 摘要\n---\n\n原始内容',
            summary: wantsDestructiveProposal
              ? '删除目标笔记'
              : '为当前笔记补充 frontmatter.summary'
          },
          null,
          2
        ),
        created_at: mockNow(),
        tool_call_id: mockToolCallId
      };
      const assistantMsg: ChatMessage = {
        v: 1,
        id: `mock-assistant-${Date.now()}`,
        role: 'assistant',
        content: streamingContent || '已取消，保留部分内容。',
        created_at: mockNow()
      };
      const next: ChatSessionFull = {
        ...sessionAfterStream,
        messages: [
          ...sessionAfterStream.messages,
          {
            v: 1,
            id: proposalAssistantId,
            role: 'assistant',
            content: '',
            created_at: mockNow(),
            tool_calls: [toolCall]
          },
          toolMsg,
          assistantMsg
        ]
      };
      handles.setActiveSession(next);
      handles.setSessions([mockSessionSummary(next)]);
    } else {
      const assistantMsg: ChatMessage = {
        v: 1,
        id: `mock-assistant-${Date.now()}`,
        role: 'assistant',
        content: streamingContent || '已取消，保留部分内容。',
        created_at: mockNow()
      };
      const next: ChatSessionFull = {
        ...sessionAfterStream,
        messages: [...sessionAfterStream.messages, assistantMsg]
      };
      handles.setActiveSession(next);
      handles.setSessions([mockSessionSummary(next)]);
    }
  }

  if (handles.isCancelRequested()) {
    handles.setLastFailure({
      kind: 'other',
      message: 'mock cancelled',
      user_message_persisted: true
    });
  }
  handles.setStreamingContent('');
  handles.setInlineToolEvents([]);
  handles.setSending(false);
}
