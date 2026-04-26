import type { ChatMessage, ToolCall } from '$lib/ipc/ai';

export type ProposalKind =
  | 'summary'
  | 'tag_update'
  | 'moc'
  | 'note_edit'
  | 'delete_note'
  | 'rename_note';

export interface ProposalPayload {
  proposal_kind: ProposalKind;
  target_rel_path: string;
  original_content: string;
  proposed_content: string;
  summary: string;
  metadata?: Record<string, unknown>;
}

export type InlineToolEvent =
  | { kind: 'requested'; call_id: string; name: string; arguments: string }
  | { kind: 'result'; call_id: string; content: string; is_error: boolean };

export interface ToolCallViewModel {
  key: string;
  source: 'inline' | 'persisted';
  callId: string;
  name: string;
  arguments: string;
  status: 'pending' | 'completed';
  resultContent: string | null;
  isError: boolean;
  proposal: ProposalPayload | null;
  summary: string;
}

export interface PersistedToolCallPairing {
  toolCallsByAssistantId: Record<string, ToolCallViewModel[]>;
  consumedToolMessageIds: Set<string>;
}

export function compactText(value: string | null | undefined, limit = 160): string {
  const text = (value ?? '').replace(/\s+/g, ' ').trim();
  if (text.length <= limit) return text;
  return `${text.slice(0, limit)}…`;
}

export function prettyPrintJson(raw: string): string {
  if (!raw.trim()) return raw;
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}

export function buildInlineToolCallViewModels(events: InlineToolEvent[]): ToolCallViewModel[] {
  const order: string[] = [];
  const byCallId = new Map<string, ToolCallViewModel>();

  for (const event of events) {
    let vm = byCallId.get(event.call_id);
    if (!vm) {
      vm = {
        key: `inline:${event.call_id}`,
        source: 'inline',
        callId: event.call_id,
        name: '(unknown tool)',
        arguments: '',
        status: 'pending',
        resultContent: null,
        isError: false,
        proposal: null,
        summary: '等待工具结果…'
      };
      byCallId.set(event.call_id, vm);
      order.push(event.call_id);
    }

    if (event.kind === 'requested') {
      byCallId.set(event.call_id, {
        ...vm,
        name: event.name,
        arguments: event.arguments,
        summary: compactText(event.arguments, 140) || '等待工具结果…'
      });
      continue;
    }

    const proposal = parseProposalPayload(event.content);
    byCallId.set(event.call_id, {
      ...vm,
      status: 'completed',
      resultContent: event.content,
      isError: event.is_error,
      proposal,
      summary: summarizeResult(vm.name, vm.arguments, event.content, event.is_error, proposal)
    });
  }

  return order.map((callId) => byCallId.get(callId)!);
}

export function pairPersistedToolCalls(messages: ChatMessage[]): PersistedToolCallPairing {
  const toolMessagesByCallId = new Map<string, ChatMessage>();
  for (const message of messages) {
    if (message.role !== 'tool' || !message.tool_call_id) continue;
    if (!toolMessagesByCallId.has(message.tool_call_id)) {
      toolMessagesByCallId.set(message.tool_call_id, message);
    }
  }

  const consumedToolMessageIds = new Set<string>();
  const toolCallsByAssistantId: Record<string, ToolCallViewModel[]> = {};

  for (const message of messages) {
    if (message.role !== 'assistant' || !message.tool_calls?.length) continue;
    toolCallsByAssistantId[message.id] = message.tool_calls.map((toolCall) => {
      const resultMessage = toolMessagesByCallId.get(toolCall.id) ?? null;
      if (resultMessage) consumedToolMessageIds.add(resultMessage.id);
      return buildPersistedViewModel(message.id, toolCall, resultMessage);
    });
  }

  return { toolCallsByAssistantId, consumedToolMessageIds };
}

function buildPersistedViewModel(
  assistantId: string,
  toolCall: ToolCall,
  resultMessage: ChatMessage | null
): ToolCallViewModel {
  const resultContent = resultMessage?.content ?? null;
  const proposal = parseProposalPayload(resultContent);

  return {
    key: `persisted:${assistantId}:${toolCall.id}`,
    source: 'persisted',
    callId: toolCall.id,
    name: toolCall.name,
    arguments: toolCall.arguments,
    status: resultMessage ? 'completed' : 'pending',
    resultContent,
    isError: false,
    proposal,
    summary: summarizeResult(
      toolCall.name,
      toolCall.arguments,
      resultContent,
      false,
      proposal,
      !resultMessage
    )
  };
}

function summarizeResult(
  name: string,
  argumentsRaw: string,
  resultContent: string | null,
  isError: boolean,
  proposal: ProposalPayload | null,
  pending = false
): string {
  if (proposal) {
    return `${proposal.target_rel_path} · ${proposal.summary}`;
  }
  if (pending) return '等待工具结果…';
  if (isError && resultContent) return compactText(resultContent, 180);
  if (resultContent) return compactText(resultContent, 180);
  if (argumentsRaw) return compactText(argumentsRaw, 140);
  return compactText(name, 80);
}

function parseProposalPayload(raw: string | null): ProposalPayload | null {
  if (!raw) return null;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!isProposalPayload(parsed)) return null;
    return parsed;
  } catch {
    return null;
  }
}

function isProposalPayload(value: unknown): value is ProposalPayload {
  if (!isRecord(value)) return false;
  if (!isProposalKind(value.proposal_kind)) return false;
  if (typeof value.target_rel_path !== 'string') return false;
  if (typeof value.original_content !== 'string') return false;
  if (typeof value.proposed_content !== 'string') return false;
  if (typeof value.summary !== 'string') return false;
  if (
    value.metadata !== undefined &&
    value.metadata !== null &&
    !isRecord(value.metadata)
  ) {
    return false;
  }
  return true;
}

function isProposalKind(value: unknown): value is ProposalKind {
  return (
    value === 'summary' ||
    value === 'tag_update' ||
    value === 'moc' ||
    value === 'note_edit' ||
    value === 'delete_note' ||
    value === 'rename_note'
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
