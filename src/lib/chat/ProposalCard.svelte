<script lang="ts">
  import { diffLines, diffStats } from '$lib/ai/diffLines';
  import ToolCallCard from './ToolCallCard.svelte';
  import type { ProposalPayload, ToolCallViewModel } from './toolCallViewModel';

  interface Props {
    viewModel: ToolCallViewModel;
    onAccept?: (proposal: ProposalPayload) => void | Promise<void>;
    onReject?: (proposal: ProposalPayload) => void | Promise<void>;
    onAdjust?: (proposal: ProposalPayload) => void | Promise<void>;
    resolutionState?:
      | {
          kind: 'accepted' | 'rejected' | 'error';
          message: string;
        }
      | null;
  }

  let { viewModel, onAccept, onReject, onAdjust, resolutionState = null }: Props = $props();

  let diffOpen = $state(false);
  let actionBusy = $state(false);

  const proposal = $derived(viewModel.proposal);
  const parts = $derived(proposal ? diffLines(proposal.original_content, proposal.proposed_content) : []);
  const stats = $derived(diffStats(parts));
  const kindLabel = $derived.by(() => {
    switch (proposal?.proposal_kind) {
      case 'summary':
        return '摘要提案';
      case 'tag_update':
        return '标签提案';
      case 'moc':
        return 'MOC 提案';
      case 'note_edit':
        return '笔记编辑提案';
      case 'delete_note':
        return '删除提案';
      case 'rename_note':
        return '重命名提案';
      default:
        return '提案';
    }
  });
  const acceptLabel = $derived.by(() => {
    switch (proposal?.proposal_kind) {
      case 'delete_note':
        // Phase 4 Stage 3 — backend now uses `trash::delete`, so the
        // file lands in the OS recycle bin instead of being unlinked.
        // The label leans into "可恢复" so the user is not afraid to
        // confirm; the destructive 二次确认 dialog still fires.
        return '移至回收站';
      case 'rename_note':
        return '确认改名';
      default:
        return '接受';
    }
  });
  const adjustLabel = $derived.by(() => {
    return proposal?.proposal_kind === 'tag_update' ? '调整 / 重提' : '调整';
  });
  const actionsDisabled = $derived(
    actionBusy || resolutionState !== null || (!onAccept && !onReject && !onAdjust)
  );

  async function runAction(
    kind: 'accept' | 'reject' | 'adjust',
    handler: ((proposal: ProposalPayload) => void | Promise<void>) | undefined
  ): Promise<void> {
    if (!proposal || actionBusy) return;
    if (!handler) {
      console.warn(`[proposal-card] ${kind} not wired yet`, proposal);
      return;
    }
    actionBusy = true;
    try {
      await handler(proposal);
    } finally {
      actionBusy = false;
    }
  }
</script>

{#if proposal}
  <article class="proposal-card">
    <ToolCallCard viewModel={viewModel} showResult={false} />

    <div class="proposal-card__body">
      <header class="proposal-card__header">
        <div class="proposal-card__headline">
          <span
            class="proposal-card__kind"
            class:proposal-card__kind--destructive={proposal.proposal_kind === 'delete_note' ||
              proposal.proposal_kind === 'rename_note'}>{kindLabel}</span
          >
          <code class="proposal-card__target">{proposal.target_rel_path}</code>
        </div>
        <button type="button" class="proposal-card__toggle" onclick={() => (diffOpen = !diffOpen)}>
          {diffOpen ? '收起 diff' : '展开 diff'}
        </button>
      </header>

      <p class="proposal-card__summary">{proposal.summary}</p>
      {#if resolutionState}
        <div
          class="proposal-card__resolution proposal-card__resolution--{resolutionState.kind}"
          role={resolutionState.kind === 'error' ? 'alert' : 'status'}
        >
          {resolutionState.message}
        </div>
      {/if}

      <div class="proposal-card__stats">
        <span class="proposal-card__stat proposal-card__stat--add">+{stats.added}</span>
        <span class="proposal-card__stat proposal-card__stat--remove">-{stats.removed}</span>
        {#if stats.added === 0 && stats.removed === 0}
          <span class="proposal-card__note">无可见改动</span>
        {/if}
      </div>

      {#if diffOpen}
        <div class="proposal-card__diff" role="region" aria-label="Proposal diff preview">
          {#each parts as part, i (i)}
            <div class="proposal-card__line proposal-card__line--{part.type}">
              <span class="proposal-card__marker" aria-hidden="true">
                {part.type === 'add' ? '+' : part.type === 'remove' ? '−' : ' '}
              </span>
              <span class="proposal-card__content">{part.value || '\u00a0'}</span>
            </div>
          {/each}
        </div>
      {/if}

      <footer class="proposal-card__actions" data-testid="proposal-card">
        <button
          type="button"
          class="proposal-card__btn"
          data-testid="proposal-reject"
          onclick={() => void runAction('reject', onReject)}
          disabled={actionsDisabled}
        >
          拒绝
        </button>
        <button
          type="button"
          class="proposal-card__btn"
          data-testid="proposal-adjust"
          onclick={() => void runAction('adjust', onAdjust)}
          disabled={actionsDisabled}
        >
          {adjustLabel}
        </button>
        <button
          type="button"
          class="proposal-card__btn proposal-card__btn--primary"
          data-testid="proposal-accept"
          onclick={() => void runAction('accept', onAccept)}
          disabled={actionsDisabled}
        >
          {acceptLabel}
        </button>
      </footer>
    </div>
  </article>
{:else}
  <ToolCallCard viewModel={viewModel} />
{/if}

<style>
  .proposal-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in oklch, var(--color-accent) 30%, var(--color-border));
    background: color-mix(in oklch, var(--color-surface-raised) 94%, transparent);
    box-shadow: var(--pane-border, 0 1px 2px rgba(0, 0, 0, 0.04));
  }
  .proposal-card__body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .proposal-card__header,
  .proposal-card__headline,
  .proposal-card__actions,
  .proposal-card__stats {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .proposal-card__header {
    justify-content: space-between;
  }
  .proposal-card__kind {
    padding: 2px 6px;
    border-radius: 999px;
    background: color-mix(in oklch, var(--color-accent) 14%, transparent);
    color: var(--color-accent);
    font-size: 10px;
    font-family: var(--font-mono);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .proposal-card__kind--destructive {
    background: color-mix(in oklch, var(--color-danger, #b54a4a) 14%, transparent);
    color: var(--color-danger, #b54a4a);
  }
  .proposal-card__target {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-fg);
    word-break: break-word;
  }
  .proposal-card__resolution {
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    line-height: 1.45;
  }
  .proposal-card__resolution--accepted {
    background: color-mix(in oklch, var(--color-accent) 10%, transparent);
    color: var(--color-accent);
  }
  .proposal-card__resolution--rejected {
    background: color-mix(in oklch, var(--color-fg-muted) 12%, transparent);
    color: var(--color-fg-muted);
  }
  .proposal-card__resolution--error {
    background: color-mix(in oklch, var(--color-danger, #b54a4a) 10%, transparent);
    color: var(--color-danger, #b54a4a);
  }
  .proposal-card__summary {
    margin: 0;
    color: var(--color-fg);
    font-size: var(--fs-sm);
    line-height: 1.5;
  }
  .proposal-card__toggle,
  .proposal-card__btn {
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-fg);
    border-radius: var(--radius-sm);
    padding: 6px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .proposal-card__toggle:hover,
  .proposal-card__btn:hover {
    border-color: var(--color-accent);
  }
  .proposal-card__btn--primary {
    background: var(--color-accent);
    color: white;
    border-color: var(--color-accent);
  }
  .proposal-card__btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .proposal-card__stat {
    padding: 2px 8px;
    border-radius: 999px;
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .proposal-card__stat--add {
    background: color-mix(in oklch, var(--color-accent) 12%, transparent);
    color: var(--color-accent);
  }
  .proposal-card__stat--remove {
    background: color-mix(in oklch, var(--color-danger, #b54a4a) 12%, transparent);
    color: var(--color-danger, #b54a4a);
  }
  .proposal-card__note {
    color: var(--color-fg-muted);
    font-size: 12px;
  }
  .proposal-card__diff {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    overflow: auto;
    max-height: 320px;
  }
  .proposal-card__line {
    display: grid;
    grid-template-columns: 18px 1fr;
    gap: 10px;
    padding: 2px 10px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .proposal-card__line--add {
    background: color-mix(in oklch, var(--color-accent) 8%, transparent);
  }
  .proposal-card__line--remove {
    background: color-mix(in oklch, var(--color-danger, #b54a4a) 8%, transparent);
  }
  .proposal-card__marker {
    color: var(--color-fg-muted);
  }
  .proposal-card__actions {
    justify-content: flex-end;
  }
</style>
