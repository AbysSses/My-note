<script lang="ts">
  import { onMount } from 'svelte';
  import {
    indexTasksToday,
    toggleTaskDone,
    todayIsoLocal,
    type TaskRow,
    type TaskPriority
  } from '$lib/ipc';

  /**
   * Floating "Today's tasks" glass panel anchored top-right of its containing
   * block. Mirrors `app-core.jsx:TodayTasksPanel` — corner glow blob, task
   * rows with Done pills, a footer "Other tasks" count, and a "View all tasks"
   * button that defers to the parent.
   */
  interface Props {
    visible: boolean;
    onClose?: () => void;
    onOpenNote?: (path: string) => void;
    onViewAll?: () => void;
    /** Bumped by parent to refresh after any vault-side mutation. */
    refreshToken?: number;
  }

  const { visible, onClose, onOpenNote, onViewAll, refreshToken = 0 }: Props = $props();

  let rows = $state<TaskRow[]>([]);
  let loadError = $state<string | null>(null);

  async function load() {
    if (!visible) return;
    try {
      rows = await indexTasksToday(todayIsoLocal());
      loadError = null;
    } catch (err) {
      loadError = err instanceof Error ? err.message : String(err);
    }
  }

  onMount(load);
  $effect(() => {
    visible;
    refreshToken;
    void load();
  });

  // Show the first three in the hero list; the rest roll up under "Other tasks".
  const hero = $derived(rows.slice(0, 3));
  const otherCount = $derived(Math.max(0, rows.length - hero.length));

  async function markDone(row: TaskRow) {
    try {
      await toggleTaskDone(row.note_path, row.line, true);
      await load();
    } catch (err) {
      loadError = err instanceof Error ? err.message : String(err);
    }
  }

  function priorityColor(p: TaskPriority | null): string {
    switch (p) {
      case 'urgent':
        return 'var(--color-urgent)';
      case 'high':
        return 'var(--color-high)';
      case 'med':
        return 'var(--color-med)';
      case 'low':
        return 'var(--color-low)';
      default:
        return 'var(--color-fg-dim)';
    }
  }

  function timeHint(row: TaskRow): string {
    const m = /(\d{2}):(\d{2})/.exec(row.text);
    return m ? `${m[1]}:${m[2]}` : '';
  }
</script>

{#if visible}
  <aside class="today-panel" aria-label="Today's tasks">
    <span class="corner-glow" aria-hidden="true"></span>

    <header class="head">
      <div class="head-text">
        <h3 class="h-title">Today's tasks</h3>
        <div class="h-count">{rows.length} to handle</div>
      </div>
      <button
        class="close-btn"
        type="button"
        aria-label="Close"
        title="Close"
        onclick={() => onClose?.()}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
        </svg>
      </button>
    </header>

    <div class="section-label">Today</div>

    <div class="rows">
      {#if loadError}
        <div class="error">{loadError}</div>
      {:else if rows.length === 0}
        <div class="empty">Nothing due today. Breathe.</div>
      {/if}

      {#each hero as row (row.id)}
        <div class="row">
          <span class="icon-tile" aria-hidden="true">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <rect x="2" y="3" width="10" height="9" rx="1.5" stroke="currentColor" stroke-width="1.1"/>
              <path d="M2 6h10M5 1.5v2M9 1.5v2" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
            </svg>
          </span>
          <div class="row-body">
            <div class="row-top">
              <button
                class="row-title"
                type="button"
                title={row.text}
                onclick={() => onOpenNote?.(row.note_path)}
              >
                {row.text.replace(/📅\s*\d{4}-\d{2}-\d{2}|!urgent|!high|!med|!low|@\d{4}-\d{2}-\d{2}/g, '').trim() || row.text}
              </button>
              <button class="done-pill" type="button" onclick={() => void markDone(row)}>
                <svg width="10" height="10" viewBox="0 0 12 12" fill="none">
                  <path d="M2 6l3 3 5-6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
                </svg>
                Done
              </button>
            </div>
            <div class="chips">
              {#if timeHint(row)}
                <span class="chip-time">{timeHint(row)}</span>
              {/if}
              <span class="chip-label">today</span>
              {#if row.priority}
                <span
                  class="chip-tone"
                  style="color: {priorityColor(row.priority)}; background: color-mix(in oklch, {priorityColor(row.priority)} 18%, transparent);"
                >
                  {row.priority}
                </span>
              {/if}
            </div>
            <div class="row-ref" title={row.note_path}>{row.note_path}</div>
          </div>
        </div>
      {/each}
    </div>

    {#if otherCount > 0}
      <div class="other">
        <span>Other tasks</span>
        <span class="other-count">{otherCount}</span>
      </div>
    {/if}

    <button
      class="view-all"
      type="button"
      onclick={() => onViewAll?.()}
    >
      View all tasks
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
        <path d="M3 2l4 3-4 3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </button>
  </aside>
{/if}

<style>
  .today-panel {
    position: absolute;
    top: 18px;
    right: 18px;
    z-index: 50;
    width: 340px;
    max-height: calc(100% - 36px);
    border-radius: var(--radius-xl);
    background: var(--glass-bg);
    backdrop-filter: blur(24px) saturate(160%);
    -webkit-backdrop-filter: blur(24px) saturate(160%);
    box-shadow: var(--float-shadow);
    color: var(--color-fg);
    overflow: hidden;
    font-family: var(--font-sans);
    display: flex;
    flex-direction: column;
  }
  .corner-glow {
    position: absolute;
    top: -60px;
    right: -60px;
    width: 200px;
    height: 200px;
    border-radius: 50%;
    background: var(--color-accent);
    opacity: 0.08;
    filter: blur(70px);
    pointer-events: none;
  }
  .head {
    position: relative;
    padding: 16px 18px 10px;
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .head-text {
    flex: 1;
  }
  .h-title {
    margin: 0;
    font-family: var(--font-serif);
    font-size: 16px;
    font-weight: 500;
    color: var(--color-fg);
    letter-spacing: -0.2px;
  }
  .h-count {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--color-fg-muted);
    margin-top: 2px;
    letter-spacing: 0.3px;
  }
  .close-btn {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--color-fg-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    padding: 0;
  }
  .close-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    transform: none;
  }

  .section-label {
    padding: 0 18px 8px;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-dim);
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .rows {
    padding: 0 10px 8px;
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
  }
  .row {
    padding: 10px 12px;
    border-radius: var(--radius-md);
    display: flex;
    gap: 10px;
    align-items: flex-start;
    transition: background 0.15s ease;
  }
  .row:hover {
    background: var(--color-bg-hover);
  }
  .icon-tile {
    width: 26px;
    height: 26px;
    border-radius: 8px;
    background: var(--color-accent-softer);
    color: var(--color-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    margin-top: 2px;
  }
  .row-body {
    flex: 1;
    min-width: 0;
  }
  .row-top {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }
  .row-title {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    box-shadow: none;
    padding: 0;
    font-size: 12.5px;
    font-weight: 500;
    color: var(--color-fg);
    letter-spacing: -0.1px;
    line-height: 1.35;
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
  }
  .row-title:hover {
    background: transparent;
    color: var(--color-accent);
    transform: none;
  }
  .done-pill {
    padding: 3px 9px;
    border-radius: 9999px;
    border: 1px solid color-mix(in oklch, var(--color-success) 40%, transparent);
    background: transparent;
    box-shadow: none;
    color: var(--color-success);
    font-size: 10px;
    font-weight: 500;
    font-family: inherit;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    white-space: nowrap;
  }
  .done-pill:hover {
    background: color-mix(in oklch, var(--color-success) 10%, transparent);
    transform: none;
  }

  .chips {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 5px;
    flex-wrap: wrap;
  }
  .chip-time {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-muted);
  }
  .chip-label {
    font-size: 10px;
    color: var(--color-fg-dim);
  }
  .chip-tone {
    font-size: 9.5px;
    padding: 1px 6px;
    border-radius: 4px;
    font-weight: 500;
    letter-spacing: 0.2px;
    text-transform: uppercase;
  }
  .row-ref {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-dim);
    margin-top: 5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .other {
    padding: 10px 18px;
    border-top: 0.5px solid var(--color-border);
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 11.5px;
    color: var(--color-fg-muted);
  }
  .other-count {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-fg-dim);
  }

  .view-all {
    width: 100%;
    padding: 12px 18px;
    background: var(--color-bg-hover);
    border: none;
    border-top: 0.5px solid var(--color-border);
    border-radius: 0;
    color: var(--color-fg-muted);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    letter-spacing: -0.1px;
    box-shadow: none;
  }
  .view-all:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    transform: none;
  }

  .empty {
    padding: 12px 12px 4px;
    font-size: 12px;
    color: var(--color-fg-dim);
    font-style: italic;
  }
  .error {
    padding: 12px 12px 4px;
    font-size: 12px;
    color: var(--color-danger);
  }
</style>
