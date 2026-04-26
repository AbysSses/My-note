<!--
  Tag-suggestion modal for P3-D3.4 `> Suggest tags for current note`.

  Why a dedicated component instead of reusing `DiffPreviewModal`:
  - The write-back is a checkbox-driven *merge*, not a textual diff. A line
    diff over a flow sequence (`tags: [a, b] → [a, b, c, d]`) reduces to a
    single-line red/green swap and loses the affordance to pick/drop
    individual candidates.
  - Candidate list needs per-row state (checked / "existing" badge /
    "new" badge) that a text-diff viewer can't express.
  - The three-state chrome (loading / error / body) + keyboard shortcuts
    + cancel semantics are identical, so we replicate the shell here and
    keep the component self-contained; DRY-ing the shell into a
    `ModalShell.svelte` would buy ~40 lines and cost flexibility the next
    write-back command might need.

  Rendered rows are the *candidates* the AI proposed. Tags already on the
  note are shown pre-checked (with an `已存在` badge) so the user can drop
  them with the same UI; unknown-to-vault candidates get a `新建` badge so
  the user sees taxonomy drift before committing.
-->
<script lang="ts">
  import type { CompleteFailure } from '$lib/ipc/ai';
  import { mergeTagLists } from './suggestTagsPrompt';

  interface Props {
    /** Controls visibility. Parent owns the flag; every dismissal path
     *  (Esc / backdrop / Discard / Accept) calls back through one of the
     *  hooks below so the parent can reset its own state. */
    open: boolean;
    title: string;
    description?: string;
    /** Tags currently on the note (before any merge). Rendered as
     *  pre-checked rows with an `已存在` badge so the user can untick
     *  them just like AI candidates. */
    existingTags: string[];
    /** Ranked candidates from `parseSuggestedTags(aiReply)`. `null` while
     *  `aiComplete` is in flight; the modal shows a loader in that state. */
    candidates: string[] | null;
    /** Taxonomy the vault already uses (`indexTags()` names). Drives the
     *  `新建` badge for candidates that aren't in the existing-vault set. */
    vaultTags: string[];
    /** `true` while the AI call is pending. */
    loading?: boolean;
    /** Typed failure from `aiComplete`. When set, the body renders a
     *  banner and Accept is disabled/hidden. */
    error?: CompleteFailure | null;
    /** Optional advisory note shown above the body for recoverable
     *  states such as partial results after cancellation. */
    statusNote?: string;
    retryLabel?: string;
    showRetry?: boolean;
    onRetry?: () => void | Promise<void>;
    loadingText?: string;
    cancelLabel?: string;
    cancelBusy?: boolean;
    /** Called with the final merged tag list (existing ∪ newly-picked AI
     *  tags, de-duped, in stable order). The parent owns the file write. */
    onAccept: (finalTags: string[]) => void | Promise<void>;
    /** Esc / backdrop / Discard. */
    onDiscard: () => void;
    /** Optional cancel hook for `loading` state. Called on Esc /
     *  backdrop / the Cancel button while loading. */
    onCancel?: () => void;
  }

  let {
    open,
    title,
    description = '',
    existingTags,
    candidates,
    vaultTags,
    loading = false,
    error = null,
    statusNote = '',
    retryLabel = '重新生成',
    showRetry = false,
    onRetry,
    loadingText = 'AI 正在生成候选标签…',
    cancelLabel = '取消生成',
    cancelBusy = false,
    onAccept,
    onDiscard,
    onCancel
  }: Props = $props();

  /** All rows rendered in the list: existing tags first (pre-checked), then
   *  AI candidates that aren't already on the note, in the model's order.
   *  De-duplication is done here rather than in the parent so the parent
   *  can always pass raw lists. */
  interface Row {
    tag: string;
    /** `true` for tags currently on the note. Affects the badge only. */
    existing: boolean;
    /** `true` for tags neither on the note nor elsewhere in the vault. */
    novel: boolean;
  }

  const rows: Row[] = $derived.by<Row[]>(() => {
    const vaultSet = new Set(vaultTags);
    const existingSet = new Set(existingTags);
    const out: Row[] = existingTags.map((t) => ({
      tag: t,
      existing: true,
      novel: !vaultSet.has(t)
    }));
    if (candidates) {
      for (const c of candidates) {
        if (existingSet.has(c)) continue;
        out.push({ tag: c, existing: false, novel: !vaultSet.has(c) });
      }
    }
    return out;
  });

  /** Selection state keyed by tag slug. Seeded so every existing tag and
   *  every AI candidate starts checked — the user unticks what they want
   *  to drop. Rebuilt whenever `rows` changes (e.g. candidates arrive) to
   *  include newly-added rows. Selections the user already made survive
   *  because the map is only extended, never reset. */
  let selected = $state<Record<string, boolean>>({});

  // Seed selections for any new row that doesn't have an entry yet.
  $effect(() => {
    let changed = false;
    const next = { ...selected };
    for (const row of rows) {
      if (!(row.tag in next)) {
        next[row.tag] = true;
        changed = true;
      }
    }
    if (changed) selected = next;
  });

  /** Final merged list in accept-order: preserves the row order (existing
   *  first, then new candidates the model ranked). */
  const finalTags = $derived.by<string[]>(() => {
    const kept: string[] = [];
    for (const row of rows) {
      if (selected[row.tag]) kept.push(row.tag);
    }
    return mergeTagLists(kept, []);
  });

  /** Count of *newly-added* tags vs the original note (i.e. final ∩ ¬existing). */
  const addedCount = $derived.by<number>(() => {
    const existingSet = new Set(existingTags);
    let n = 0;
    for (const t of finalTags) if (!existingSet.has(t)) n++;
    return n;
  });

  /** Count of *removed* tags (existing tags the user unticked). */
  const removedCount = $derived.by<number>(() => {
    const finalSet = new Set(finalTags);
    let n = 0;
    for (const t of existingTags) if (!finalSet.has(t)) n++;
    return n;
  });

  let accepting = $state(false);
  let dialogEl = $state<HTMLDivElement | null>(null);

  // Mirror DiffPreviewModal: focus the dialog on open so keyboard-only
  // accept/cancel works without a priming click.
  $effect(() => {
    if (!open || !dialogEl) return;
    queueMicrotask(() => dialogEl?.focus());
  });

  function labelForKind(kind: CompleteFailure['kind']): string {
    switch (kind) {
      case 'auth':
        return '认证失败';
      case 'network':
        return '网络错误';
      case 'rate_limit':
        return '触发速率限制';
      case 'invalid_request':
        return '请求无效';
      case 'other':
      default:
        return '生成失败';
    }
  }

  async function handleAccept() {
    if (loading || error || candidates === null || accepting) return;
    if (addedCount === 0 && removedCount === 0) return;
    accepting = true;
    try {
      await onAccept(finalTags);
    } finally {
      accepting = false;
    }
  }

  async function handleRetry() {
    if (loading || accepting || !onRetry) return;
    await onRetry();
  }

  function handleBackdropClick() {
    if (accepting) return;
    if (loading) {
      if (onCancel) onCancel();
      return;
    }
    onDiscard();
  }

  function handleKeydown(ev: KeyboardEvent) {
    if (accepting) return;
    if (ev.key === 'Escape') {
      ev.preventDefault();
      if (loading) {
        if (onCancel) onCancel();
      } else {
        onDiscard();
      }
      return;
    }
    if (ev.key === 'Enter' && (ev.metaKey || ev.ctrlKey) && !loading && !error) {
      ev.preventDefault();
      void handleAccept();
    }
  }

  function toggle(tag: string) {
    selected = { ...selected, [tag]: !selected[tag] };
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="tsm-backdrop" onclick={handleBackdropClick}>
    <div
      class="tsm-card"
      bind:this={dialogEl}
      role="dialog"
      aria-modal="true"
      aria-labelledby="tsm-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeydown}
    >
      <header class="tsm-header">
        <h3 id="tsm-title" class="tsm-title">{title}</h3>
        {#if description}
          <p class="tsm-desc">{description}</p>
        {/if}
      </header>

      <div class="tsm-body">
        {#if loading}
          <div class="tsm-loading" role="status" aria-live="polite">
            <span class="tsm-spinner" aria-hidden="true"></span>
            <span>{cancelBusy ? '正在取消生成…' : loadingText}</span>
          </div>
        {:else if statusNote}
          <div class="tsm-note" role="status">{statusNote}</div>
        {/if}
        {#if !loading && error}
          <div class="tsm-error" role="alert">
            <strong class="tsm-error-kind">{labelForKind(error.kind)}</strong>
            <p class="tsm-error-msg">{error.message}</p>
            {#if error.retry_after_secs != null}
              <p class="tsm-retry">建议 {error.retry_after_secs} 秒后重试。</p>
            {/if}
          </div>
        {:else if candidates !== null}
          <div class="tsm-stats">
            <span class="tsm-stat tsm-stat--add">+{addedCount}</span>
            <span class="tsm-stat tsm-stat--remove">-{removedCount}</span>
            {#if addedCount === 0 && removedCount === 0}
              <span class="tsm-stat-note">未选择任何改动</span>
            {:else}
              <span class="tsm-stat-note">最终标签：{finalTags.length} 个</span>
            {/if}
          </div>

          {#if rows.length === 0}
            <p class="tsm-empty">AI 未生成任何候选，也没有已有标签。</p>
          {:else}
            <ul class="tsm-list">
              {#each rows as row (row.tag)}
                <li class="tsm-row">
                  <label class="tsm-label">
                    <input
                      type="checkbox"
                      checked={!!selected[row.tag]}
                      onchange={() => toggle(row.tag)}
                    />
                    <span class="tsm-tag">{row.tag}</span>
                    {#if row.existing}
                      <span class="tsm-badge tsm-badge--existing" title="笔记当前已有此标签"
                        >已存在</span
                      >
                    {:else if row.novel}
                      <span class="tsm-badge tsm-badge--novel" title="vault 中从未出现过">新建</span
                      >
                    {:else}
                      <span class="tsm-badge tsm-badge--reuse" title="已在 vault 其他笔记中出现"
                        >复用</span
                      >
                    {/if}
                  </label>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </div>

      <footer class="tsm-actions">
        {#if loading}
          {#if onCancel}
            <button
              type="button"
              class="tsm-btn"
              onclick={onCancel}
              disabled={accepting || cancelBusy}>{cancelBusy ? '正在取消…' : cancelLabel}</button
            >
          {/if}
        {:else}
          <button type="button" class="tsm-btn" onclick={onDiscard} disabled={accepting}
            >放弃</button
          >
          {#if showRetry && onRetry}
            <button type="button" class="tsm-btn" onclick={handleRetry} disabled={accepting}
              >{retryLabel}</button
            >
          {/if}
          <button
            type="button"
            class="tsm-btn tsm-btn--primary"
            onclick={handleAccept}
            disabled={accepting ||
              error !== null ||
              candidates === null ||
              (addedCount === 0 && removedCount === 0)}
            title="Cmd/Ctrl + Enter"
          >
            {accepting ? '写入中…' : '写入 frontmatter.tags'}
          </button>
        {/if}
      </footer>
    </div>
  </div>
{/if}

<style>
  .tsm-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.32);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 24px;
  }
  .tsm-card {
    width: min(520px, 100%);
    max-height: calc(100vh - 48px);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .tsm-header {
    padding: 16px 20px 10px;
    border-bottom: 1px solid var(--color-border);
  }
  .tsm-title {
    margin: 0;
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .tsm-desc {
    margin: 6px 0 0;
    font-size: var(--fs-xs);
    color: var(--color-fg-muted);
    line-height: 1.5;
  }

  .tsm-body {
    flex: 1 1 auto;
    overflow: auto;
    padding: 14px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }

  .tsm-loading {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 24px 0;
    justify-content: center;
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
  }
  .tsm-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--color-border);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: tsm-spin 0.8s linear infinite;
  }
  @keyframes tsm-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .tsm-error {
    padding: 10px 12px;
    border: 1px solid var(--color-danger, #c94a4a);
    background: color-mix(in oklch, var(--color-danger, #c94a4a) 10%, transparent);
    border-radius: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tsm-error-kind {
    color: var(--color-danger, #c94a4a);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .tsm-error-msg {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--color-fg);
    word-break: break-word;
  }
  .tsm-retry {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--color-fg-muted);
  }
  .tsm-note {
    padding: 10px 12px;
    border: 1px solid color-mix(in oklch, var(--color-accent) 24%, var(--color-border));
    background: color-mix(in oklch, var(--color-accent) 10%, transparent);
    border-radius: 6px;
    font-size: var(--fs-sm);
    color: var(--color-fg);
    line-height: 1.5;
  }

  .tsm-stats {
    display: flex;
    align-items: center;
    gap: 10px;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }
  .tsm-stat {
    padding: 2px 8px;
    border-radius: 10px;
    font-weight: 600;
  }
  .tsm-stat--add {
    color: #2f7d32;
    background: color-mix(in oklch, #2f7d32 12%, transparent);
  }
  .tsm-stat--remove {
    color: #b3261e;
    background: color-mix(in oklch, #b3261e 12%, transparent);
  }
  .tsm-stat-note {
    color: var(--color-fg-muted);
    font-family: inherit;
  }

  .tsm-empty {
    margin: 0;
    padding: 16px 0;
    text-align: center;
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
  }

  .tsm-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tsm-row {
    padding: 0;
  }
  .tsm-label {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 4px;
    cursor: pointer;
    font-size: var(--fs-sm);
  }
  .tsm-label:hover {
    background: var(--color-bg-hover);
  }
  .tsm-label input[type='checkbox'] {
    flex: 0 0 auto;
    margin: 0;
    cursor: pointer;
  }
  .tsm-tag {
    flex: 1 1 auto;
    font-family: var(--font-mono);
    color: var(--color-fg);
  }
  .tsm-badge {
    flex: 0 0 auto;
    font-size: 10px;
    padding: 2px 6px;
    border-radius: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .tsm-badge--existing {
    color: var(--color-fg-muted);
    background: var(--color-bg);
    border: 1px solid var(--color-border);
  }
  .tsm-badge--novel {
    color: #b36b00;
    background: color-mix(in oklch, #b36b00 12%, transparent);
  }
  .tsm-badge--reuse {
    color: #2f7d32;
    background: color-mix(in oklch, #2f7d32 12%, transparent);
  }

  .tsm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
  }
  .tsm-btn {
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
  .tsm-btn:hover:not(:disabled) {
    background: var(--color-bg-hover);
    border-color: var(--color-accent);
  }
  .tsm-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .tsm-btn--primary {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-bg);
    font-weight: 600;
  }
  .tsm-btn--primary:hover:not(:disabled) {
    background: color-mix(in oklch, var(--color-accent) 85%, white);
    border-color: color-mix(in oklch, var(--color-accent) 85%, white);
  }
</style>
