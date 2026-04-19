<script lang="ts">
  /**
   * Right-hand panel: shows relationships of the currently open note —
   * backlinks (who links here), outgoing (where this links to), and
   * unresolved (wiki-link targets that aren't real notes yet).
   *
   * The panel is always mounted in the grid; when no file is open we show
   * a placeholder so the column width doesn't collapse / reflow.
   */
  import { onMount } from 'svelte';
  import {
    indexBacklinks,
    indexOutgoing,
    indexUnresolved,
    indexProjectNotes,
    type BacklinkItem,
    type OutgoingLink,
    type NoteRef
  } from '$lib/ipc/index';

  interface Props {
    /** Vault-relative path of the currently open file, or null. */
    filePath: string | null;
    /** Click handler — the page already knows how to open vault paths. */
    onOpenNote: (relPath: string) => void;
    /**
     * Monotonic counter the parent bumps to force a refetch (e.g. after
     * the user saves, or after a vault-wide rescan). We debounce reloads
     * through this so typing doesn't spam IPC.
     */
    refreshToken?: number;
  }

  const { filePath, onOpenNote, refreshToken = 0 }: Props = $props();

  let backlinks = $state<BacklinkItem[]>([]);
  let outgoing = $state<OutgoingLink[]>([]);
  let unresolved = $state<string[]>([]);
  /** Sibling notes under `4-projects/<slug>/` — only populated when the
   *  current file is a project's index.md. Empty list for all other paths. */
  let projectNotes = $state<NoteRef[]>([]);
  let loading = $state(false);
  let err = $state<string | null>(null);

  let reqSeq = 0;

  /**
   * If `path` looks like `4-projects/<slug>/index.md`, return `<slug>`.
   * Otherwise null — non-project notes don't get the extra section.
   */
  function projectSlugFromIndex(path: string | null): string | null {
    if (!path) return null;
    const m = path.match(/^4-projects\/([^/]+)\/index\.md$/);
    return m ? m[1] : null;
  }

  async function load(path: string | null) {
    if (!path) {
      backlinks = [];
      outgoing = [];
      unresolved = [];
      projectNotes = [];
      err = null;
      return;
    }
    const myReq = ++reqSeq;
    loading = true;
    err = null;
    try {
      const slug = projectSlugFromIndex(path);
      // Fetch project-notes in parallel with the link queries when applicable.
      // For non-project files we skip the extra IPC entirely (small win but
      // keeps the network panel clean when debugging).
      const [bl, og, un, pn] = await Promise.all([
        indexBacklinks(path),
        indexOutgoing(path),
        indexUnresolved(path),
        slug ? indexProjectNotes(slug) : Promise.resolve<NoteRef[]>([])
      ]);
      // If another request started while we were awaiting, drop these results.
      if (myReq !== reqSeq) return;
      backlinks = bl;
      outgoing = og;
      unresolved = un;
      projectNotes = pn;
    } catch (e) {
      if (myReq !== reqSeq) return;
      err = String(e);
    } finally {
      if (myReq === reqSeq) loading = false;
    }
  }

  // React to path changes + manual refresh pokes. Using $effect avoids the
  // need for the parent to call an imperative reload method.
  $effect(() => {
    // Track both dependencies so the effect re-runs when either changes.
    void refreshToken;
    load(filePath);
  });

  function display(link: OutgoingLink): string {
    return link.title ?? link.dst_resolved ?? link.dst;
  }

  function fileName(p: string): string {
    const i = p.lastIndexOf('/');
    return i >= 0 ? p.slice(i + 1) : p;
  }
</script>

<aside class="panel">
  <header class="panel-header">
    <span>笔记关系</span>
    {#if loading}
      <span class="spinner" aria-label="loading">…</span>
    {/if}
  </header>

  {#if !filePath}
    <p class="panel-empty">打开一个笔记以查看反向链接。</p>
  {:else}
    {#if err}
      <p class="panel-error" title={err}>加载失败</p>
    {/if}

    {#if projectSlugFromIndex(filePath)}
      <section>
        <h4>项目笔记 <span class="count">{projectNotes.length}</span></h4>
        {#if projectNotes.length === 0}
          <p class="section-empty">
            还没有同项目笔记。试试命令面板 <code>⌘P</code> → <code>&gt; Add Note to Project</code>。
          </p>
        {:else}
          <ul>
            {#each projectNotes as n (n.path)}
              <li>
                <button class="link" onclick={() => onOpenNote(n.path)} title={n.path}>
                  <span class="link-title">{n.title ?? fileName(n.path)}</span>
                  <span class="link-path">{fileName(n.path)}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <section>
      <h4>反向链接 <span class="count">{backlinks.length}</span></h4>
      {#if backlinks.length === 0}
        <p class="section-empty">暂无笔记链接到这里。</p>
      {:else}
        <ul>
          {#each backlinks as b (b.src_path)}
            <li>
              <button class="link" onclick={() => onOpenNote(b.src_path)} title={b.src_path}>
                <span class="link-title">{b.src_title ?? fileName(b.src_path)}</span>
                <span class="link-path">{b.src_path}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h4>链出 <span class="count">{outgoing.length}</span></h4>
      {#if outgoing.length === 0}
        <p class="section-empty">这篇笔记没有 <code>[[wiki-link]]</code>。</p>
      {:else}
        <ul>
          {#each outgoing as l, i (i)}
            <li>
              {#if l.dst_resolved}
                <button
                  class="link"
                  onclick={() => onOpenNote(l.dst_resolved!)}
                  title={l.dst_resolved}
                >
                  <span class="link-title">{display(l)}</span>
                  <span class="link-path">{l.dst_resolved}</span>
                </button>
              {:else}
                <span class="link unresolved" title="未解析：{l.dst}">
                  <span class="link-title">{l.dst}</span>
                  <span class="link-path">未创建</span>
                </span>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if unresolved.length > 0}
      <section>
        <h4>未解析 <span class="count">{unresolved.length}</span></h4>
        <ul>
          {#each unresolved as target, i (i)}
            <li>
              <span class="link unresolved" title="未解析：{target}">
                <span class="link-title">{target}</span>
                <span class="link-path">⌘点击正文中的链接以创建</span>
              </span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</aside>

<style>
  .panel {
    border-left: 1px solid var(--color-border);
    background: var(--color-bg-subtle);
    overflow-y: auto;
    padding: 0 0 var(--space-4);
    font-size: var(--fs-sm);
    min-width: 0;
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-border);
    position: sticky;
    top: 0;
    background: var(--color-bg-subtle);
    font-weight: 600;
  }
  .spinner {
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
  }
  .panel-empty {
    padding: var(--space-4) var(--space-3);
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
  }
  .panel-error {
    padding: var(--space-2) var(--space-3);
    color: var(--color-danger);
    font-size: var(--fs-xs);
  }
  section {
    background: var(--color-card);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-md);
    padding: var(--space-2) 0 var(--space-3);
    margin: var(--space-3);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.02);
  }
  section + section {
    margin-top: 0;
  }
  h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: var(--space-2) var(--space-3);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-fg-muted);
    font-weight: 600;
  }
  .count {
    font-weight: 400;
    color: var(--color-fg-muted);
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  li {
    margin: 0;
  }
  .link {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    width: calc(100% - 16px);
    margin: 2px 8px;
    padding: 6px 10px;
    border: none;
    background: transparent;
    text-align: left;
    color: inherit;
    cursor: pointer;
    border-radius: var(--radius-sm);
    font-size: 13px;
    transition: background 0.15s ease, transform 0.15s ease;
  }
  button.link:hover {
    background: var(--color-bg-hover);
    transform: translateX(2px);
  }
  .link-title {
    display: block;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--color-fg);
  }
  .link-path {
    display: block;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-fg-muted);
  }
  .unresolved {
    cursor: default;
  }
  .unresolved .link-title {
    color: var(--color-fg-muted);
    font-style: italic;
  }
  .section-empty {
    padding: 2px 12px 4px;
    margin: 0;
    color: var(--color-fg-muted);
    font-size: var(--fs-xs);
  }
  code {
    font-family: var(--font-mono);
    font-size: 11px;
  }
</style>
