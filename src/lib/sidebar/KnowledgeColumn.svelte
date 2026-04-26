<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { TaskPriority } from '$lib/ipc';

  /**
   * Middle-left 300px "knowledge base" column. Shell only — the parent supplies
   * the content for each tab via `notesSlot` / `tasksSlot` / `projectsSlot`
   * snippets. Mirrors `app-core.jsx:KnowledgeColumn` from the Second-design
   * handoff.
   *
   * Priority filter is shown only on the Tasks tab, matching the design.
   * Counts are passed in; the column doesn't fetch on its own.
   */
  export type KbTab = 'notes' | 'tasks' | 'projects';
  export type PriorityFilter = 'all' | TaskPriority;

  interface Props {
    tab: KbTab;
    onTabChange: (tab: KbTab) => void;
    onRefresh?: () => void;

    priorityFilter?: PriorityFilter;
    onPriorityChange?: (f: PriorityFilter) => void;
    /** Per-priority counts for the filter pills. `null` hides the counts. */
    counts?: { all: number; urgent: number; high: number; med: number; low: number } | null;

    notesSlot?: Snippet;
    tasksSlot?: Snippet;
    projectsSlot?: Snippet;
  }

  const {
    tab,
    onTabChange,
    onRefresh,
    priorityFilter = 'all',
    onPriorityChange,
    counts = null,
    notesSlot,
    tasksSlot,
    projectsSlot
  }: Props = $props();

  type Tab = { key: KbTab; label: string };
  const tabs: Tab[] = [
    { key: 'notes', label: 'Notes' },
    { key: 'tasks', label: 'Tasks' },
    { key: 'projects', label: 'Projects' }
  ];

  type PriorityPill = { k: PriorityFilter; label: string; dotVar: string | null };
  const priorityPills: PriorityPill[] = [
    { k: 'all', label: 'All', dotVar: null },
    { k: 'urgent', label: 'Urgent', dotVar: '--color-urgent' },
    { k: 'high', label: 'High', dotVar: '--color-high' },
    { k: 'med', label: 'Med', dotVar: '--color-med' },
    { k: 'low', label: 'Low', dotVar: '--color-low' }
  ];
</script>

<section class="kb-col" aria-label="Knowledge base">
  <header class="kb-head">
    <h2 class="title">Knowledge base</h2>
    <button
      class="icon-btn"
      type="button"
      aria-label="Refresh"
      title="Refresh"
      onclick={() => onRefresh?.()}
    >
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
        <path d="M12 7a5 5 0 11-1.5-3.5M12 2v3h-3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
      </svg>
    </button>
  </header>

  <div class="tabs" role="tablist">
    {#each tabs as t (t.key)}
      <button
        class="tab"
        class:is-active={tab === t.key}
        type="button"
        role="tab"
        aria-selected={tab === t.key}
        onclick={() => onTabChange(t.key)}
      >
        {t.label}
      </button>
    {/each}
  </div>

  {#if tab === 'tasks'}
    <div class="priority-wrap">
      <div class="priority-seg" role="group" aria-label="Priority filter">
        {#each priorityPills as p (p.k)}
          {@const isActive = priorityFilter === p.k}
          <button
            class="seg-btn"
            class:is-active={isActive}
            type="button"
            onclick={() => onPriorityChange?.(p.k)}
          >
            {#if p.dotVar}
              <span class="seg-dot" style="background: var({p.dotVar})"></span>
            {/if}
            <span class="seg-label">{p.label}</span>
            {#if counts}
              <span class="seg-count">{counts[p.k]}</span>
            {/if}
          </button>
        {/each}
      </div>
    </div>
  {/if}

  <div class="body">
    {#if tab === 'notes' && notesSlot}{@render notesSlot()}{/if}
    {#if tab === 'tasks' && tasksSlot}{@render tasksSlot()}{/if}
    {#if tab === 'projects' && projectsSlot}{@render projectsSlot()}{/if}
  </div>
</section>

<style>
  .kb-col {
    width: var(--sidebar-width);
    flex-shrink: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-bg);
    border-right: 0.5px solid var(--color-border);
    min-width: 0;
  }
  .kb-head {
    padding: 14px 16px 6px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .title {
    margin: 0;
    flex: 1;
    font-family: var(--font-serif);
    font-size: 15px;
    font-weight: 500;
    color: var(--color-fg);
    letter-spacing: -0.2px;
  }
  .icon-btn {
    width: 24px;
    height: 24px;
    padding: 0;
    border-radius: 7px;
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--color-fg-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .icon-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    transform: none;
  }
  .tabs {
    padding: 4px 16px 10px;
    display: flex;
    gap: 6px;
  }
  .tab {
    padding: 5px 12px;
    border-radius: 9999px;
    border: none;
    background: transparent;
    box-shadow: none;
    font-size: 12px;
    font-weight: 400;
    color: var(--color-fg-muted);
    letter-spacing: -0.1px;
    cursor: pointer;
  }
  .tab:hover {
    background: var(--color-bg-hover);
    transform: none;
  }
  .tab.is-active {
    background: var(--color-bg-hover);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.9);
    color: var(--color-fg);
    font-weight: 500;
  }

  .priority-wrap {
    padding: 2px 16px 14px;
  }
  .priority-seg {
    display: flex;
    align-items: stretch;
    height: 30px;
    border-radius: 9px;
    background: var(--color-bg-hover);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.85);
    padding: 2px;
    gap: 1px;
  }
  .seg-btn {
    flex: 1;
    min-width: 0;
    padding: 0 6px;
    border-radius: 7px;
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--color-fg-muted);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-family: inherit;
    font-size: 11px;
    font-weight: 400;
    letter-spacing: -0.1px;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .seg-btn:hover {
    background: transparent;
    transform: none;
  }
  .seg-btn.is-active {
    background: var(--color-surface-raised);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 1), 0 1px 2px rgba(40, 30, 20, 0.08);
    color: var(--color-fg);
    font-weight: 500;
  }
  .seg-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .seg-label {
    white-space: nowrap;
  }
  .seg-count {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-dim);
    font-weight: 400;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 10px 16px;
  }
</style>
