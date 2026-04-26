<script lang="ts">
  import { onMount } from 'svelte';
  import {
    indexTasksToday,
    indexTasksUpcoming,
    toggleTaskDone,
    todayIsoLocal,
    type TaskRow,
    type TaskPriority
  } from '$lib/ipc';

  /**
   * Scrolling "Today · N" + "Upcoming · N" list of tasks for the Knowledge
   * column's Tasks tab. Rows render with a checkbox, the task text, a
   * monospace due hint, and a priority triangle per design.
   */
  type PriorityFilter = 'all' | TaskPriority;

  interface Props {
    /** Bumped by parent to force re-fetch after external mutations. */
    refreshToken?: number;
    filter?: PriorityFilter;
    onOpenNote?: (path: string) => void;
  }

  const { refreshToken = 0, filter = 'all', onOpenNote }: Props = $props();

  let today = $state<TaskRow[]>([]);
  let upcoming = $state<TaskRow[]>([]);
  let loadError = $state<string | null>(null);
  let loading = $state(false);

  async function load() {
    loading = true;
    loadError = null;
    try {
      const d = todayIsoLocal();
      const [t, u] = await Promise.all([indexTasksToday(d), indexTasksUpcoming(d, 30)]);
      today = t;
      upcoming = u;
    } catch (err) {
      loadError = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  onMount(load);
  $effect(() => {
    refreshToken;
    void load();
  });

  function matchesFilter(row: TaskRow): boolean {
    if (filter === 'all') return true;
    return row.priority === filter;
  }

  const todayFiltered = $derived(today.filter(matchesFilter));
  const upcomingFiltered = $derived(upcoming.filter(matchesFilter));

  async function onToggle(row: TaskRow) {
    try {
      await toggleTaskDone(row.note_path, row.line, !row.done);
      await load();
    } catch (err) {
      loadError = err instanceof Error ? err.message : String(err);
    }
  }

  function fmtDue(row: TaskRow): string {
    // Design shows `MM-DD`. Fall back to source note's daily-note suffix when no explicit due.
    if (row.due && /^\d{4}-\d{2}-\d{2}$/.test(row.due)) return row.due.slice(5);
    const m = /3-journal\/(\d{4}-\d{2}-\d{2})\.md$/.exec(row.note_path);
    return m ? m[1].slice(5) : '';
  }

  function priorityVar(p: TaskPriority | null): string {
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
        return 'transparent';
    }
  }
</script>

<div class="tasks-list">
  {#if loadError}
    <div class="error">{loadError}</div>
  {/if}

  <div class="section-head">
    <span class="accent-dot" aria-hidden="true"></span>
    <span>Today · {todayFiltered.length}</span>
  </div>
  {#if todayFiltered.length === 0 && !loading}
    <div class="empty-mini">No tasks for today.</div>
  {/if}
  {#each todayFiltered as row (row.id)}
    <button
      class="task-row"
      type="button"
      onclick={() => onOpenNote?.(row.note_path)}
    >
      <span
        class="checkbox"
        role="checkbox"
        tabindex="0"
        aria-checked={row.done}
        onclick={(e) => {
          e.stopPropagation();
          void onToggle(row);
        }}
        onkeydown={(e) => {
          if (e.key === ' ' || e.key === 'Enter') {
            e.preventDefault();
            e.stopPropagation();
            void onToggle(row);
          }
        }}
      ></span>
      <span class="task-text" title={row.text}>{row.text}</span>
      <span class="task-date">{fmtDue(row)}</span>
      {#if row.priority}
        <svg class="priority-tri" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M5 1 L9 8 L1 8 Z" fill={priorityVar(row.priority)} />
        </svg>
      {/if}
    </button>
  {/each}

  <div class="section-head upcoming">
    <span>Upcoming · {upcomingFiltered.length}</span>
  </div>
  {#if upcomingFiltered.length === 0 && !loading}
    <div class="empty-mini">No upcoming tasks.</div>
  {/if}
  {#each upcomingFiltered as row (row.id)}
    <button
      class="task-row"
      type="button"
      onclick={() => onOpenNote?.(row.note_path)}
    >
      <span
        class="checkbox"
        role="checkbox"
        tabindex="0"
        aria-checked={row.done}
        onclick={(e) => {
          e.stopPropagation();
          void onToggle(row);
        }}
        onkeydown={(e) => {
          if (e.key === ' ' || e.key === 'Enter') {
            e.preventDefault();
            e.stopPropagation();
            void onToggle(row);
          }
        }}
      ></span>
      <span class="task-text" title={row.text}>{row.text}</span>
      <span class="task-date">{fmtDue(row)}</span>
      {#if row.priority}
        <svg class="priority-tri" width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M5 1 L9 8 L1 8 Z" fill={priorityVar(row.priority)} />
        </svg>
      {/if}
    </button>
  {/each}
</div>

<style>
  .tasks-list {
    padding-top: 2px;
  }
  .section-head {
    padding: 10px 6px 6px;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-fg-muted);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .section-head.upcoming {
    padding-top: 14px;
  }
  .accent-dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--color-accent);
    box-shadow: var(--accent-glow);
  }
  .task-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--color-fg);
    font-size: 12.5px;
    letter-spacing: -0.1px;
    text-align: left;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .task-row:hover {
    background: var(--color-bg-hover);
    transform: none;
  }
  .checkbox {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    border: 1.2px solid var(--color-border-strong);
    flex-shrink: 0;
    display: inline-block;
    cursor: pointer;
  }
  .checkbox:hover {
    background: var(--color-bg-hover);
  }
  .checkbox[aria-checked='true'] {
    background: var(--color-accent);
    border-color: transparent;
  }
  .task-text {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .task-date {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-dim);
    flex-shrink: 0;
  }
  .priority-tri {
    flex-shrink: 0;
  }
  .empty-mini {
    padding: 8px 12px;
    font-size: 11px;
    color: var(--color-fg-dim);
  }
  .error {
    padding: 8px 12px;
    font-size: 11px;
    color: var(--color-danger);
  }
</style>
