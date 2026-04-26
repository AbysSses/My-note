<script lang="ts">
  /**
   * Collapsible tag list that lives below the file tree in the left sidebar.
   *
   * Clicking a tag hands it off to the parent, which decides whether to
   * open the TagView. We fetch tags lazily (only when expanded) to avoid
   * blocking initial sidebar render on a sleepy FS.
   */
  import { indexTags, type TagCount } from '$lib/ipc/index';

  interface Props {
    /** Which tag is currently being viewed, if any. */
    activeTag: string | null;
    onSelect: (tag: string) => void;
    /** Bumped by the parent when something might have changed tags. */
    refreshToken?: number;
  }

  const { activeTag, onSelect, refreshToken = 0 }: Props = $props();

  let expanded = $state(true);
  let tags = $state<TagCount[]>([]);
  let loading = $state(false);
  let err = $state<string | null>(null);

  let reqSeq = 0;

  async function load() {
    const myReq = ++reqSeq;
    loading = true;
    err = null;
    try {
      const list = await indexTags();
      if (myReq !== reqSeq) return;
      tags = list;
    } catch (e) {
      if (myReq !== reqSeq) return;
      err = String(e);
      tags = [];
    } finally {
      if (myReq === reqSeq) loading = false;
    }
  }

  // Load whenever refreshToken changes *and* the section is expanded.
  $effect(() => {
    void refreshToken;
    if (expanded) load();
  });

  function toggle() {
    expanded = !expanded;
  }
</script>

<section class="tags-section" class:collapsed={!expanded}>
  <button class="tags-header" onclick={toggle}>
    <span class="chev">{expanded ? '▾' : '▸'}</span>
    <span class="heading">Tags</span>
    <span class="flex"></span>
    {#if expanded && !loading}
      <span class="total">{tags.length}</span>
    {:else if loading}
      <span class="total">…</span>
    {/if}
  </button>
  {#if expanded}
    {#if err}
      <p class="err" title={err}>加载失败</p>
    {:else if tags.length === 0 && !loading}
      <p class="empty">还没有标签。在笔记里写 <code>#tag</code> 试试。</p>
    {:else}
      <ul>
        {#each tags as t (t.tag)}
          <li>
            <button
              class="tag-row"
              class:active={t.tag === activeTag}
              onclick={() => onSelect(t.tag)}
              title={`#${t.tag}`}
            >
              <span class="hash">#</span>
              <span class="name">{t.tag}</span>
              <span class="count">{t.count}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .tags-section {
    box-shadow: inset 0 1px 0 var(--color-border);
    margin-top: var(--space-2);
    padding-top: var(--space-1);
  }
  .tags-header {
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
  .tags-header:hover {
    color: var(--color-fg);
    background: transparent;
    transform: none;
  }
  .chev {
    width: 14px;
    text-align: center;
  }
  .heading {
    flex-shrink: 0;
  }
  .flex {
    flex: 1;
  }
  .total {
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    text-transform: none;
    letter-spacing: 0;
  }
  ul {
    list-style: none;
    padding: 0 0 8px;
    margin: 0;
    max-height: 40vh;
    overflow-y: auto;
  }
  .tag-row {
    display: flex;
    align-items: center;
    gap: 2px;
    width: calc(100% - 12px);
    margin: 1px 6px;
    padding: 4px 10px 4px 22px;
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
  .tag-row:hover {
    background: var(--color-bg-hover);
    transform: translateX(2px);
  }
  .tag-row.active {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    box-shadow: inset 2px 0 0 var(--color-accent);
  }
  .tag-row.active .hash {
    color: var(--color-accent);
  }
  .hash {
    color: var(--color-fg-muted);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    padding-left: 6px;
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
  code {
    font-family: var(--font-mono);
    background: var(--color-bg-subtle);
    padding: 0 4px;
    border-radius: 3px;
  }
</style>
