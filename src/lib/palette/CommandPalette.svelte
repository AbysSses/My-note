<script lang="ts">
  /**
   * Cmd+P palette with four input modes, switched by the first char:
   *
   *   (no prefix) → fuzzy file path picker + command fallthrough
   *   ">"         → command list only
   *   "#"         → tag picker → opens TagView
   *   "/"         → FTS5 full-text search with snippets
   *
   * The parent owns the `open` state; this component just renders the modal
   * when open and calls `onClose` on Esc / backdrop click / after a run.
   */
  import { indexAllNotes, indexSearch, indexTags } from '$lib/ipc/index';
  import type { NoteRef, SearchHit, TagCount } from '$lib/ipc/index';
  import {
    fuzzyScore,
    PALETTE_COMMANDS,
    type PaletteCommand,
    type PaletteContext
  } from './commandRegistry';
  import { fade, fly } from 'svelte/transition';

  interface Props {
    open: boolean;
    onClose: () => void;
    ctx: PaletteContext;
  }

  const { open, onClose, ctx }: Props = $props();

  type Mode = 'file' | 'command' | 'tag' | 'search';
  type Row =
    | { kind: 'file'; path: string; title: string | null; note_type: string | null }
    | { kind: 'command'; cmd: PaletteCommand }
    | { kind: 'tag'; tag: string; count: number }
    | { kind: 'search'; path: string; title: string | null; snippet: string };

  let query = $state('');
  let rows = $state<Row[]>([]);
  let selected = $state(0);
  let loading = $state(false);
  let inputEl = $state<HTMLInputElement | null>(null);
  let allNotesCache: NoteRef[] | null = null;
  let allTagsCache: TagCount[] | null = null;

  let reqSeq = 0;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  /** Derive mode + stripped needle from the raw input. */
  function parseQuery(raw: string): { mode: Mode; needle: string } {
    if (raw.startsWith('>')) return { mode: 'command', needle: raw.slice(1).trim() };
    if (raw.startsWith('#')) return { mode: 'tag', needle: raw.slice(1).trim() };
    if (raw.startsWith('/')) return { mode: 'search', needle: raw.slice(1).trim() };
    return { mode: 'file', needle: raw.trim() };
  }

  const modeInfo = $derived(parseQuery(query));

  // Depend on both `open` and `query` so opening (even when query was already
  // empty) re-fetches. The body short-circuits when closed so no work is done.
  $effect(() => {
    void modeInfo;
    void query;
    void open;
    if (!open) return;
    void refreshRows();
  });

  // Reset on open.
  $effect(() => {
    if (open) {
      query = '';
      selected = 0;
      rows = [];
      // Focus after the DOM updates.
      setTimeout(() => inputEl?.focus(), 0);
    } else {
      if (searchTimer) {
        clearTimeout(searchTimer);
        searchTimer = null;
      }
    }
  });

  async function refreshRows() {
    const myReq = ++reqSeq;
    const { mode, needle } = modeInfo;

    if (mode === 'search') {
      // FTS5 is the only IPC-on-every-keystroke mode; debounce it.
      if (searchTimer) clearTimeout(searchTimer);
      if (!needle) {
        rows = [];
        selected = 0;
        loading = false;
        return;
      }
      loading = true;
      searchTimer = setTimeout(async () => {
        try {
          const hits: SearchHit[] = await indexSearch(needle, 30);
          if (myReq !== reqSeq) return;
          rows = hits.map((h) => ({
            kind: 'search',
            path: h.path,
            title: h.title,
            snippet: h.snippet
          }));
          selected = 0;
        } catch (e) {
          if (myReq !== reqSeq) return;
          console.error('[palette search failed]', e);
          rows = [];
        } finally {
          if (myReq === reqSeq) loading = false;
        }
      }, 150);
      return;
    }

    if (mode === 'command') {
      const avail = PALETTE_COMMANDS.filter((c) => (c.when ? c.when(ctx) : true));
      rows = rankCommands(avail, needle).map((cmd) => ({ kind: 'command', cmd }));
      selected = 0;
      return;
    }

    if (mode === 'tag') {
      loading = true;
      try {
        if (!allTagsCache) allTagsCache = await indexTags();
        if (myReq !== reqSeq) return;
        rows = rankTags(allTagsCache, needle).map((t) => ({
          kind: 'tag',
          tag: t.tag,
          count: t.count
        }));
        selected = 0;
      } catch (e) {
        if (myReq !== reqSeq) return;
        console.error('[palette tags failed]', e);
        rows = [];
      } finally {
        if (myReq === reqSeq) loading = false;
      }
      return;
    }

    // Default: file mode. Fetch all notes once per open; palette-close clears.
    loading = true;
    try {
      if (!allNotesCache) allNotesCache = await indexAllNotes();
      if (myReq !== reqSeq) return;
      const fileRows: Row[] = rankNotes(allNotesCache, needle).map((n) => ({
        kind: 'file',
        path: n.path,
        title: n.title,
        note_type: n.note_type
      }));
      // Let a non-empty query also surface matching commands below files.
      if (needle) {
        const cmdMatches = rankCommands(
          PALETTE_COMMANDS.filter((c) => (c.when ? c.when(ctx) : true)),
          needle
        ).slice(0, 3);
        for (const cmd of cmdMatches) fileRows.push({ kind: 'command', cmd });
      }
      rows = fileRows;
      selected = 0;
    } catch (e) {
      if (myReq !== reqSeq) return;
      console.error('[palette notes failed]', e);
      rows = [];
    } finally {
      if (myReq === reqSeq) loading = false;
    }
  }

  function rankNotes(notes: NoteRef[], needle: string): NoteRef[] {
    if (!needle) return notes.slice(0, 50);
    const scored: { n: NoteRef; s: number }[] = [];
    for (const n of notes) {
      const hay = (n.title ?? '') + ' ' + n.path;
      const s = fuzzyScore(hay, needle);
      if (s >= 0) scored.push({ n, s });
    }
    scored.sort((a, b) => a.s - b.s);
    return scored.slice(0, 50).map((x) => x.n);
  }

  function rankCommands(cmds: PaletteCommand[], needle: string): PaletteCommand[] {
    if (!needle) return cmds;
    const scored: { c: PaletteCommand; s: number }[] = [];
    for (const c of cmds) {
      const s = fuzzyScore(c.label, needle);
      if (s >= 0) scored.push({ c, s });
    }
    scored.sort((a, b) => a.s - b.s);
    return scored.map((x) => x.c);
  }

  function rankTags(tags: TagCount[], needle: string): TagCount[] {
    if (!needle) return tags;
    const scored: { t: TagCount; s: number }[] = [];
    for (const t of tags) {
      const s = fuzzyScore(t.tag, needle);
      if (s >= 0) scored.push({ t, s });
    }
    scored.sort((a, b) => a.s - b.s);
    return scored;
  }

  function runRow(row: Row) {
    switch (row.kind) {
      case 'file':
        ctx.openNote(row.path);
        break;
      case 'command':
        void row.cmd.run(ctx);
        break;
      case 'tag':
        ctx.openTag(row.tag);
        break;
      case 'search':
        ctx.openNote(row.path);
        break;
    }
    onClose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (rows.length) selected = (selected + 1) % rows.length;
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (rows.length) selected = (selected - 1 + rows.length) % rows.length;
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const row = rows[selected];
      if (row) runRow(row);
    }
  }

  // Reset caches when the palette is closed, so re-opening picks up new notes/tags.
  $effect(() => {
    if (!open) {
      allNotesCache = null;
      allTagsCache = null;
    }
  });

  function stemOf(path: string): string {
    const name = path.slice(path.lastIndexOf('/') + 1);
    return name.replace(/\.md$/, '');
  }

  function rowKey(row: Row, i: number): string {
    switch (row.kind) {
      case 'file':
      case 'search':
        return `${row.kind}:${row.path}`;
      case 'tag':
        return `${row.kind}:${row.tag}`;
      case 'command':
        return `${row.kind}:${row.cmd.id}`;
    }
    return `row:${i}`;
  }

  function placeholderFor(mode: Mode): string {
    switch (mode) {
      case 'command':
        return '输入命令…（Esc 退出）';
      case 'tag':
        return '输入标签名…';
      case 'search':
        return '全文搜索（FTS5）…';
      default:
        return '打开笔记 / `>` 命令 · `#` 标签 · `/` 全文搜索';
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="backdrop"
    role="presentation"
    onclick={onClose}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      aria-label="命令面板"
      onclick={(e) => e.stopPropagation()}
      transition:fly={{ y: 8, duration: 200 }}
    >
      <input
        bind:this={inputEl}
        bind:value={query}
        onkeydown={onKey}
        placeholder={placeholderFor(modeInfo.mode)}
        class="palette-input"
        autocomplete="off"
        spellcheck="false"
      />
      <div class="palette-body">
        {#if loading && rows.length === 0}
          <p class="empty">搜索中…</p>
        {:else if rows.length === 0}
          <p class="empty">
            {#if modeInfo.mode === 'search' && !modeInfo.needle}
              输入关键词开始搜索
            {:else}
              没有匹配项
            {/if}
          </p>
        {:else}
          <ul>
            {#each rows as row, i (rowKey(row, i))}
              <li>
                <button
                  class="row"
                  class:active={i === selected}
                  onmouseenter={() => (selected = i)}
                  onclick={() => runRow(row)}
                >
                  {#if row.kind === 'file'}
                    <span class="leader">📄</span>
                    <span class="title">{row.title ?? stemOf(row.path)}</span>
                    <span class="hint-right">{row.path}</span>
                  {:else if row.kind === 'command'}
                    <span class="leader">⚡</span>
                    <span class="title">{row.cmd.label}</span>
                    {#if row.cmd.hint}
                      <span class="hint-right">{row.cmd.hint}</span>
                    {/if}
                  {:else if row.kind === 'tag'}
                    <span class="leader">#</span>
                    <span class="title">{row.tag}</span>
                    <span class="hint-right">{row.count} 篇</span>
                  {:else}
                    <span class="leader">🔍</span>
                    <span class="col">
                      <span class="title">{row.title ?? stemOf(row.path)}</span>
                      <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                      <span class="snippet">{@html row.snippet}</span>
                    </span>
                    <span class="hint-right">{row.path}</span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
      <div class="palette-footer">
        <span><kbd>↑</kbd><kbd>↓</kbd> 选择</span>
        <span><kbd>Enter</kbd> 确认</span>
        <span><kbd>Esc</kbd> 关闭</span>
        <span class="modes">
          <kbd>&gt;</kbd> 命令 · <kbd>#</kbd> 标签 · <kbd>/</kbd> 全文搜索
        </span>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    backdrop-filter: blur(2px);
    -webkit-backdrop-filter: blur(2px);
    z-index: 200;
    display: grid;
    place-items: start center;
    padding-top: 12vh;
  }
  .palette {
    background: var(--glass-bg);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-lg);
    width: 620px;
    max-width: calc(100vw - 40px);
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--glass-shadow);
    overflow: hidden;
  }
  .palette-input {
    border: none;
    border-bottom: 1px solid var(--color-border);
    padding: 14px 18px;
    font-size: 15px;
    background: transparent;
    color: var(--color-fg);
    outline: none;
  }
  .palette-body {
    flex: 1;
    overflow-y: auto;
    min-height: 100px;
  }
  .empty {
    padding: 24px 20px;
    color: var(--color-fg-muted);
    text-align: center;
    font-size: 13px;
    margin: 0;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 16px;
    border: none;
    border-radius: var(--radius-sm);
    background: transparent;
    text-align: left;
    color: var(--color-fg);
    cursor: pointer;
    font-size: 13px;
    transition: background 0.1s ease, transform 0.1s ease;
  }
  .row.active {
    background: var(--color-bg-hover);
    transform: translateX(2px);
  }
  .leader {
    flex-shrink: 0;
    width: 18px;
    color: var(--color-fg-muted);
    text-align: center;
  }
  .title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .col {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .snippet {
    color: var(--color-fg-muted);
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .snippet :global(mark) {
    background: var(--color-accent);
    color: var(--color-bg);
    padding: 0 2px;
    border-radius: 2px;
  }
  .hint-right {
    flex-shrink: 0;
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    padding-left: 12px;
    max-width: 50%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .palette-footer {
    border-top: 1px solid var(--color-border);
    padding: 6px 14px;
    display: flex;
    gap: 12px;
    font-size: 11px;
    color: var(--color-fg-muted);
    flex-wrap: wrap;
  }
  .palette-footer .modes {
    margin-left: auto;
  }
  kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 0 4px;
    margin: 0 2px;
  }
</style>
