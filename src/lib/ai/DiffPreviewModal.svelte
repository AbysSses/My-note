<!--
  Shared diff-preview modal for P3-D3 write-back commands (summarize /
  MOC AI draft). The modal renders the AI reply as a line-level diff
  against the "before" text and collects user confirmation before any
  file write happens — callers never mutate disk until `onAccept` is
  invoked.

  Why a dedicated component rather than reusing the `.ns-*` modal in
  ChatPanel:
  - Different content shape (three-state body: loading / error / diff).
  - The write-back commands fire from the command palette, not from a
    panel, so the host is `+page.svelte`; embedding here keeps the
    modal stateless and consumable from anywhere.
  - Suggest-Tags (D3.4) will reuse the same shell with a different
    body, so the chrome (header / footer / backdrop dismissal) is
    designed to be body-agnostic.

  Tag-specific rendering (checkbox merge) is *not* handled here — D3.4
  will either pass a `body` snippet later, or ship its own component
  that reuses only the LCS helper. This file stays focused on text diff.
-->
<script lang="ts">
  import type { CompleteFailure } from '$lib/ipc/ai';
  import { diffLines, diffStats, type DiffPart } from './diffLines';

  interface Props {
    /** Controls visibility. Parent owns the flag so dismissal (Esc /
     *  backdrop click / accept / discard) feeds back through the
     *  callbacks below and lets the parent decide what to do next. */
    open: boolean;
    title: string;
    /** Short rationale shown under the title, e.g.
     *  `"AI will overwrite frontmatter.summary"`. Empty hides it. */
    description?: string;
    /** "Before" text — always required so the diff renders immediately
     *  once `proposed` arrives; during loading we show the placeholder
     *  body instead. */
    original: string;
    /** "After" text. `null` while the AI call is still in flight so
     *  the modal can show a loader without duplicating state. */
    proposed: string | null;
    /** `true` while `aiComplete` is pending. When set, the body shows
     *  a spinner and the footer swaps in a `Cancel` button (when
     *  `onCancel` is provided) instead of the discard/accept pair. */
    loading?: boolean;
    /** Typed error from `aiComplete`. When present, the body renders
     *  a banner and the accept button is hidden/disabled. */
    error?: CompleteFailure | null;
    /** Optional advisory note shown above the main body. Used for
     *  partial results after cancellation and other recoverable states. */
    statusNote?: string;
    /** Override the primary-button label. Defaults to "应用". */
    acceptLabel?: string;
    /** Override the secondary-button label. Defaults to "放弃". */
    discardLabel?: string;
    /** Optional retry affordance. Hidden unless `showRetry` is true. */
    retryLabel?: string;
    showRetry?: boolean;
    onRetry?: () => void | Promise<void>;
    /** Override the loading / cancel copy when needed. */
    loadingText?: string;
    cancelLabel?: string;
    cancelBusy?: boolean;
    /** Called when the user confirms the write-back. May be async;
     *  while it resolves, the button shows a disabled "应用中…" state
     *  so a slow disk write can't be double-submitted. */
    onAccept: () => void | Promise<void>;
    /** Called on Esc (when not loading) or backdrop/Discard click.
     *  Parent closes the modal and drops the proposed text. */
    onDiscard: () => void;
    /** Optional: when provided and `loading` is true, shown as a
     *  Cancel button that flags the in-flight `aiComplete` call.
     *  Also bound to Esc while loading. */
    onCancel?: () => void;
  }

  let {
    open,
    title,
    description = '',
    original,
    proposed,
    loading = false,
    error = null,
    statusNote = '',
    acceptLabel = '应用',
    discardLabel = '放弃',
    retryLabel = '重新生成',
    showRetry = false,
    onRetry,
    loadingText = 'AI 正在生成…',
    cancelLabel = '取消生成',
    cancelBusy = false,
    onAccept,
    onDiscard,
    onCancel
  }: Props = $props();

  // Recompute only when the inputs settle — an empty array while
  // loading avoids rendering a "-all / +nothing" diff before the
  // reply arrives.
  const parts: DiffPart[] = $derived(proposed === null ? [] : diffLines(original, proposed));
  const stats = $derived(diffStats(parts));

  // Latch so a slow `onAccept` (e.g. file write + re-index) can't be
  // double-triggered by an impatient second click.
  let accepting = $state(false);
  let dialogEl = $state<HTMLDivElement | null>(null);

  // Take keyboard focus when the modal opens so Esc / Cmd+Enter work
  // immediately in palette-driven flows.
  $effect(() => {
    if (!open || !dialogEl) return;
    queueMicrotask(() => dialogEl?.focus());
  });

  // Human-readable Chinese labels for the structured error kinds so
  // the banner header is friendlier than the raw `invalid_request`.
  // Covers every variant of `ProviderErrorKind` emitted by
  // `classify_provider_error` on the backend.
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
    if (loading || error || proposed === null || accepting) return;
    accepting = true;
    try {
      await onAccept();
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
      // Don't let a stray backdrop click silently abandon an
      // in-flight request; route through onCancel if the parent
      // opted in, otherwise ignore.
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
    // Cmd/Ctrl+Enter as the shortcut to accept — matches the chat
    // composer's send binding so the muscle memory transfers.
    if (ev.key === 'Enter' && (ev.metaKey || ev.ctrlKey) && !loading && !error) {
      ev.preventDefault();
      void handleAccept();
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events — backdrop click dismiss is click-only; the dialog body owns keyboard UX. -->
  <!-- svelte-ignore a11y_no_static_element_interactions — passive dismiss surface; focus + keys handled on the dialog element below. -->
  <div class="dpm-backdrop" onclick={handleBackdropClick}>
    <div
      class="dpm-card"
      bind:this={dialogEl}
      role="dialog"
      aria-modal="true"
      aria-labelledby="dpm-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleKeydown}
    >
      <header class="dpm-header">
        <h3 id="dpm-title" class="dpm-title">{title}</h3>
        {#if description}
          <p class="dpm-desc">{description}</p>
        {/if}
      </header>

      <div class="dpm-body">
        {#if loading}
          <div class="dpm-loading" role="status" aria-live="polite">
            <span class="dpm-spinner" aria-hidden="true"></span>
            <span>{cancelBusy ? '正在取消生成…' : loadingText}</span>
          </div>
        {:else if statusNote}
          <div class="dpm-note" role="status">{statusNote}</div>
        {/if}
        {#if !loading && error}
          <div class="dpm-error" role="alert">
            <strong class="dpm-error-kind">{labelForKind(error.kind)}</strong>
            <p class="dpm-error-msg">{error.message}</p>
            {#if error.retry_after_secs != null}
              <p class="dpm-retry">建议 {error.retry_after_secs} 秒后重试。</p>
            {/if}
          </div>
        {:else if proposed !== null}
          <div class="dpm-stats">
            <span class="dpm-stat dpm-stat--add">+{stats.added}</span>
            <span class="dpm-stat dpm-stat--remove">-{stats.removed}</span>
            {#if stats.added === 0 && stats.removed === 0}
              <span class="dpm-stat-note">无变化（AI 建议与原文一致）</span>
            {/if}
          </div>
          <div class="dpm-diff" role="region" aria-label="Diff preview">
            {#each parts as part, i (i)}
              <div class="dpm-line dpm-line--{part.type}">
                <span class="dpm-marker" aria-hidden="true"
                  >{part.type === 'add' ? '+' : part.type === 'remove' ? '−' : ' '}</span
                >
                <span class="dpm-content">{part.value || '\u00a0'}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <footer class="dpm-actions">
        {#if loading}
          {#if onCancel}
            <button
              type="button"
              class="dpm-btn"
              onclick={onCancel}
              disabled={accepting || cancelBusy}>{cancelBusy ? '正在取消…' : cancelLabel}</button
            >
          {/if}
        {:else}
          <button type="button" class="dpm-btn" onclick={onDiscard} disabled={accepting}
            >{discardLabel}</button
          >
          {#if showRetry && onRetry}
            <button type="button" class="dpm-btn" onclick={handleRetry} disabled={accepting}
              >{retryLabel}</button
            >
          {/if}
          <button
            type="button"
            class="dpm-btn dpm-btn--primary"
            onclick={handleAccept}
            disabled={accepting ||
              error !== null ||
              proposed === null ||
              (stats.added === 0 && stats.removed === 0)}
            title="Cmd/Ctrl + Enter"
          >
            {accepting ? '应用中…' : acceptLabel}
          </button>
        {/if}
      </footer>
    </div>
  </div>
{/if}

<style>
  .dpm-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.32);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    padding: 24px;
  }
  .dpm-card {
    width: min(760px, 100%);
    max-height: calc(100vh - 48px);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .dpm-header {
    padding: 16px 20px 10px;
    border-bottom: 1px solid var(--color-border);
  }
  .dpm-title {
    margin: 0;
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .dpm-desc {
    margin: 6px 0 0;
    font-size: var(--fs-xs);
    color: var(--color-fg-muted);
    line-height: 1.5;
  }

  .dpm-body {
    flex: 1 1 auto;
    overflow: auto;
    padding: 14px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }

  /* ── Loading ───────────────────────────────────────────────────── */
  .dpm-loading {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 24px 0;
    justify-content: center;
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
  }
  .dpm-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid var(--color-border);
    border-top-color: var(--color-accent);
    border-radius: 50%;
    animation: dpm-spin 0.8s linear infinite;
  }
  @keyframes dpm-spin {
    to {
      transform: rotate(360deg);
    }
  }

  /* ── Error banner ──────────────────────────────────────────────── */
  .dpm-error {
    padding: 10px 12px;
    border: 1px solid var(--color-danger, #c94a4a);
    background: color-mix(in oklch, var(--color-danger, #c94a4a) 10%, transparent);
    border-radius: 6px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .dpm-error-kind {
    color: var(--color-danger, #c94a4a);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .dpm-error-msg {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--color-fg);
    word-break: break-word;
  }
  .dpm-retry {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--color-fg-muted);
  }
  .dpm-note {
    padding: 10px 12px;
    border: 1px solid color-mix(in oklch, var(--color-accent) 24%, var(--color-border));
    background: color-mix(in oklch, var(--color-accent) 10%, transparent);
    border-radius: 6px;
    font-size: var(--fs-sm);
    color: var(--color-fg);
    line-height: 1.5;
  }

  /* ── Stats row ─────────────────────────────────────────────────── */
  .dpm-stats {
    display: flex;
    align-items: center;
    gap: 10px;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }
  .dpm-stat {
    padding: 2px 8px;
    border-radius: 10px;
    font-weight: 600;
  }
  .dpm-stat--add {
    color: #2f7d32;
    background: color-mix(in oklch, #2f7d32 12%, transparent);
  }
  .dpm-stat--remove {
    color: #b3261e;
    background: color-mix(in oklch, #b3261e 12%, transparent);
  }
  .dpm-stat-note {
    color: var(--color-fg-muted);
    font-family: inherit;
  }

  /* ── Diff view ─────────────────────────────────────────────────── */
  .dpm-diff {
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg);
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    flex: 1 1 auto;
    min-height: 0;
  }
  .dpm-line {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    padding: 1px 8px;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .dpm-line--add {
    background: color-mix(in oklch, #2f7d32 10%, transparent);
  }
  .dpm-line--remove {
    background: color-mix(in oklch, #b3261e 10%, transparent);
  }
  .dpm-marker {
    flex: 0 0 auto;
    width: 1ch;
    color: var(--color-fg-muted);
    user-select: none;
  }
  .dpm-line--add .dpm-marker {
    color: #2f7d32;
  }
  .dpm-line--remove .dpm-marker {
    color: #b3261e;
  }
  .dpm-content {
    flex: 1 1 auto;
    min-width: 0;
  }

  /* ── Footer ────────────────────────────────────────────────────── */
  .dpm-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
  }
  .dpm-btn {
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
  .dpm-btn:hover:not(:disabled) {
    background: var(--color-bg-hover);
    border-color: var(--color-accent);
  }
  .dpm-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .dpm-btn--primary {
    background: var(--color-accent);
    border-color: var(--color-accent);
    color: var(--color-bg);
    font-weight: 600;
  }
  .dpm-btn--primary:hover:not(:disabled) {
    background: color-mix(in oklch, var(--color-accent) 85%, white);
    border-color: color-mix(in oklch, var(--color-accent) 85%, white);
  }
</style>
