<script lang="ts">
  /**
   * Collapsible "Projects" panel in the left sidebar.
   *
   * Buckets projects (each is `4-projects/<slug>/index.md`) by frontmatter
   * `status:` into four subsections — active / paused / done / archived.
   * Each subsection has its own expand toggle and count badge.
   *
   * Defaults:
   *   - Outer section: expanded (projects are a primary surface per §2.3).
   *   - Inner groups:  active + paused expanded; done + archived collapsed
   *     so finished work doesn't dominate the sidebar.
   *
   * Re-fetches whenever `refreshToken` changes (parent bumps it after
   * project_set_status / new project / generic panel refresh). Fetches are
   * cheap (four small indexed SQL queries) so we run all four in parallel
   * rather than bothering with lazy per-group loading.
   */
  import { indexProjectsByStatus, type NoteRef } from '$lib/ipc/index';

  type Status = 'active' | 'paused' | 'done' | 'archived';

  interface Props {
    /** If the editor is showing a project index.md, its rel_path — used to highlight. */
    activeProjectPath: string | null;
    /** Called when the user clicks a project row. Hand the full rel_path; parent opens it. */
    onSelect: (relPath: string) => void;
    /** Bumped by the parent when project state may have changed. */
    refreshToken?: number;
  }

  const { activeProjectPath, onSelect, refreshToken = 0 }: Props = $props();

  // Display order matches the status lifecycle, not alphabetical.
  const STATUSES: readonly Status[] = ['active', 'paused', 'done', 'archived'] as const;
  const STATUS_LABEL: Record<Status, string> = {
    active: 'Active',
    paused: 'Paused',
    done: 'Done',
    archived: 'Archived'
  };

  let outerExpanded = $state(true);
  // Per-bucket collapse state. Archived defaults to collapsed because
  // otherwise a user with many wrapped-up projects gets a long sidebar.
  let groupExpanded = $state<Record<Status, boolean>>({
    active: true,
    paused: true,
    done: false,
    archived: false
  });
  let data = $state<Record<Status, NoteRef[]>>({
    active: [],
    paused: [],
    done: [],
    archived: []
  });
  let loading = $state(false);
  let err = $state<string | null>(null);

  let reqSeq = 0;

  async function load() {
    const myReq = ++reqSeq;
    loading = true;
    err = null;
    try {
      const [active, paused, done, archived] = await Promise.all([
        indexProjectsByStatus('active'),
        indexProjectsByStatus('paused'),
        indexProjectsByStatus('done'),
        indexProjectsByStatus('archived')
      ]);
      if (myReq !== reqSeq) return;
      data = { active, paused, done, archived };
    } catch (e) {
      if (myReq !== reqSeq) return;
      err = String(e);
      data = { active: [], paused: [], done: [], archived: [] };
    } finally {
      if (myReq === reqSeq) loading = false;
    }
  }

  // Re-fetch on every bump of refreshToken AND whenever the outer section
  // is expanded from closed → open (deferred load pattern).
  $effect(() => {
    void refreshToken;
    if (outerExpanded) load();
  });

  function toggleOuter() {
    outerExpanded = !outerExpanded;
  }
  function toggleGroup(s: Status) {
    groupExpanded[s] = !groupExpanded[s];
  }

  /** Count across all buckets — shown in the outer header when collapsed. */
  const totalCount = $derived(
    data.active.length + data.paused.length + data.done.length + data.archived.length
  );

  /** `4-projects/Deep-Work/index.md` → `Deep-Work`. */
  function slugOf(relPath: string): string {
    const parts = relPath.split('/');
    // Expected shape: ['4-projects', '<slug>', 'index.md']
    return parts[1] ?? relPath;
  }

  /** Prefer frontmatter title when present; fall back to the slug so rows are never blank. */
  function display(n: NoteRef): string {
    if (n.title && n.title.trim()) return n.title;
    return slugOf(n.path);
  }
</script>

<section class="projects-section" class:collapsed={!outerExpanded}>
  <button class="projects-header" onclick={toggleOuter}>
    <span class="chev">{outerExpanded ? '▾' : '▸'}</span>
    <span class="heading">Projects</span>
    <span class="flex"></span>
    {#if loading && outerExpanded}
      <span class="total">…</span>
    {:else}
      <span class="total">{totalCount}</span>
    {/if}
  </button>
  {#if outerExpanded}
    {#if err}
      <p class="err" title={err}>加载失败</p>
    {:else if totalCount === 0 && !loading}
      <p class="empty">
        还没有项目。试试命令面板 <code>⌘P</code> → <code>&gt; New Project…</code>。
      </p>
    {:else}
      {#each STATUSES as s (s)}
        {@const list = data[s]}
        <div class="group" class:muted={list.length === 0}>
          <button class="group-header" onclick={() => toggleGroup(s)}>
            <span class="chev sub">{groupExpanded[s] ? '▾' : '▸'}</span>
            <span class="group-label">{STATUS_LABEL[s]}</span>
            <span class="flex"></span>
            <span class="count">{list.length}</span>
          </button>
          {#if groupExpanded[s] && list.length > 0}
            <ul>
              {#each list as n (n.path)}
                <li>
                  <button
                    class="proj-row"
                    class:active={n.path === activeProjectPath}
                    onclick={() => onSelect(n.path)}
                    title={n.path}
                  >
                    <span class="dot" aria-hidden="true">●</span>
                    <span class="name">{display(n)}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/each}
    {/if}
  {/if}
</section>

<style>
  .projects-section {
    box-shadow: inset 0 1px 0 var(--color-border);
    margin-top: var(--space-2);
    padding-top: var(--space-1);
  }
  .projects-header {
    display: flex;
    align-items: center;
    width: 100%;
    gap: 6px;
    padding: 8px 12px;
    background: transparent;
    border: none;
    box-shadow: none;
    color: var(--color-fg-dim);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-weight: 500;
    cursor: pointer;
  }
  .projects-header:hover {
    color: var(--color-fg);
    background: transparent;
    transform: none;
  }
  .chev {
    width: 14px;
    text-align: center;
  }
  .chev.sub {
    width: 12px;
    font-size: 10px;
  }
  .heading,
  .group-label {
    flex-shrink: 0;
  }
  .flex {
    flex: 1;
  }
  .total,
  .count {
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    text-transform: none;
    letter-spacing: 0;
  }
  .group {
    padding: 0 0 2px;
  }
  .group.muted .count {
    opacity: 0.5;
  }
  .group-header {
    display: flex;
    align-items: center;
    width: 100%;
    gap: 6px;
    padding: 2px 10px 2px 22px;
    background: transparent;
    border: none;
    box-shadow: none;
    color: var(--color-fg-muted);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
  }
  .group-header:hover {
    color: var(--color-fg);
    background: transparent;
    transform: none;
  }
  ul {
    list-style: none;
    padding: 0 0 4px;
    margin: 0;
  }
  .proj-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: calc(100% - 12px);
    margin: 1px 6px;
    padding: 5px 10px 5px 32px;
    border: none;
    background: transparent;
    box-shadow: none;
    text-align: left;
    color: inherit;
    font-size: 13px;
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: background 0.15s ease, transform 0.15s ease;
  }
  .proj-row:hover {
    background: var(--color-bg-hover);
    transform: translateX(2px);
  }
  .proj-row.active {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    box-shadow: inset 2px 0 0 var(--color-accent);
  }
  .dot {
    color: var(--color-fg-dim);
    font-size: 8px;
    width: 8px;
    text-align: center;
  }
  .proj-row.active .dot {
    color: var(--color-accent);
    text-shadow: var(--accent-glow);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .err,
  .empty {
    padding: 4px 12px 10px 26px;
    margin: 0;
    color: var(--color-fg-muted);
    font-size: 11px;
  }
  .err {
    color: var(--color-danger);
  }
  .empty code {
    font-family: var(--font-mono);
    background: var(--color-bg-subtle);
    padding: 0 4px;
    border-radius: 3px;
  }
</style>
