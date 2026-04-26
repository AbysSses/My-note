<script lang="ts">
  import {
    compactText,
    prettyPrintJson,
    type ToolCallViewModel
  } from './toolCallViewModel';

  interface Props {
    viewModel: ToolCallViewModel;
    showResult?: boolean;
    defaultOpenArgs?: boolean;
    defaultOpenResult?: boolean;
  }

  let {
    viewModel,
    showResult = true,
    defaultOpenArgs = false,
    defaultOpenResult = false
  }: Props = $props();

  let argsOpen = $state(false);
  let resultOpen = $state(false);

  $effect(() => {
    argsOpen = defaultOpenArgs;
  });

  $effect(() => {
    resultOpen = defaultOpenResult;
  });

  const prettyArguments = $derived(prettyPrintJson(viewModel.arguments));
  const prettyResult = $derived(prettyPrintJson(viewModel.resultContent ?? ''));
  const resultPreview = $derived(compactText(viewModel.resultContent ?? '', 240));
  const summaryText = $derived(
    viewModel.summary || compactText(viewModel.arguments, 140) || compactText(viewModel.name, 80)
  );
  const statusLabel = $derived.by(() => {
    if (viewModel.isError) return '错误';
    if (viewModel.status === 'pending') return '运行中';
    return viewModel.proposal ? '提案' : '完成';
  });
</script>

<article
  class="tool-card"
  class:is-error={viewModel.isError}
  class:is-pending={viewModel.status === 'pending'}
  class:is-proposal={!!viewModel.proposal}
>
  <header class="tool-card__header">
    <div class="tool-card__title-row">
      <span class="tool-card__status">{statusLabel}</span>
      <code class="tool-card__name">{viewModel.name}</code>
    </div>
    <span class="tool-card__call-id" title={viewModel.callId}>{viewModel.callId}</span>
  </header>

  {#if summaryText}
    <p class="tool-card__summary">{summaryText}</p>
  {/if}

  <div class="tool-card__actions">
    {#if viewModel.arguments.trim()}
      <button type="button" class="tool-card__toggle" onclick={() => (argsOpen = !argsOpen)}>
        {argsOpen ? '收起参数' : '展开参数'}
      </button>
    {/if}
    {#if showResult && viewModel.status === 'completed' && viewModel.resultContent !== null}
      <button type="button" class="tool-card__toggle" onclick={() => (resultOpen = !resultOpen)}>
        {resultOpen ? '收起结果' : '展开结果'}
      </button>
    {/if}
  </div>

  {#if argsOpen}
    <pre class="tool-card__code" aria-label="Tool arguments">{prettyArguments}</pre>
  {/if}

  {#if viewModel.status === 'pending'}
    <div class="tool-card__note" role="status">等待工具执行完成…</div>
  {:else if showResult && viewModel.resultContent !== null}
    {#if resultOpen}
      <pre class="tool-card__code tool-card__code--result" aria-label="Tool result">
        {prettyResult}
      </pre>
    {:else if resultPreview}
      <p class="tool-card__preview">{resultPreview}</p>
    {/if}
  {/if}
</article>

<style>
  .tool-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--color-border);
    border-left: 3px solid var(--color-accent);
    border-radius: var(--radius-md);
    background: color-mix(in oklch, var(--color-surface-raised) 92%, transparent);
    box-shadow: var(--pane-border, 0 1px 2px rgba(0, 0, 0, 0.04));
  }
  .tool-card.is-pending {
    border-left-color: color-mix(in oklch, var(--color-accent) 55%, var(--color-border));
  }
  .tool-card.is-error {
    border-left-color: var(--color-danger, #b54a4a);
  }
  .tool-card.is-proposal {
    border-left-color: color-mix(in oklch, var(--color-accent) 65%, #8f6d1c);
  }
  .tool-card__header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
  }
  .tool-card__title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .tool-card__status {
    flex: 0 0 auto;
    padding: 2px 6px;
    border-radius: 999px;
    background: color-mix(in oklch, var(--color-accent) 14%, transparent);
    color: var(--color-accent);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    font-family: var(--font-mono);
  }
  .tool-card.is-error .tool-card__status {
    background: color-mix(in oklch, var(--color-danger, #b54a4a) 12%, transparent);
    color: var(--color-danger, #b54a4a);
  }
  .tool-card__name {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--color-fg);
    word-break: break-word;
  }
  .tool-card__call-id {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 180px;
  }
  .tool-card__summary,
  .tool-card__preview,
  .tool-card__note {
    margin: 0;
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--color-fg-muted);
  }
  .tool-card__preview {
    color: var(--color-fg);
  }
  .tool-card__actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .tool-card__toggle {
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-fg);
    border-radius: 999px;
    padding: 4px 10px;
    font-size: 11px;
    cursor: pointer;
  }
  .tool-card__toggle:hover {
    border-color: var(--color-accent);
  }
  .tool-card__code {
    margin: 0;
    padding: 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
    background: var(--color-surface);
    color: var(--color-fg);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.45;
    overflow: auto;
    max-height: 260px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .tool-card__code--result {
    max-height: 320px;
  }
</style>
