<script lang="ts">
  /**
   * Tag aggregation view. Shown in the editor pane when a tag is active
   * (instead of the markdown editor or the Home cards).
   *
   * Phase 3-A extends the original "single tag, newest first" view into a
   * lightweight exploration surface: the focused tag remains the anchor,
   * while the user can add extra tag filters, switch between intersection /
   * union semantics, and re-sort the resulting note list.
   */
  import { indexNotesByTags, indexTags, type NoteRef, type TagCount } from '$lib/ipc/index';

  interface Props {
    tag: string;
    onOpenNote: (relPath: string) => void;
    onClose: () => void;
    /** Opens the "Build MOC from tag" modal for the currently focused tag.
     *  Hidden if the parent didn't wire it (e.g. older embeddings). */
    onBuildMoc?: () => void;
  }

  const { tag, onOpenNote, onClose, onBuildMoc }: Props = $props();

  type MatchMode = 'all' | 'any';
  type SortMode = 'updated_desc' | 'updated_asc' | 'title_asc' | 'path_asc';

  let notes = $state<NoteRef[]>([]);
  let allTags = $state<TagCount[]>([]);
  let loading = $state(false);
  let tagsLoading = $state(false);
  let err = $state<string | null>(null);
  let tagsErr = $state<string | null>(null);

  let selectedTags = $state<string[]>([]);
  let pendingTag = $state('');
  let matchMode = $state<MatchMode>('all');
  let sortMode = $state<SortMode>('updated_desc');

  let reqSeq = 0;
  let tagReqSeq = 0;

  $effect(() => {
    void tag;
    selectedTags = [tag];
    pendingTag = '';
    matchMode = 'all';
    sortMode = 'updated_desc';
  });

  $effect(() => {
    const myReq = ++tagReqSeq;
    tagsLoading = true;
    tagsErr = null;
    (async () => {
      try {
        const list = await indexTags();
        if (myReq !== tagReqSeq) return;
        allTags = list;
      } catch (e) {
        if (myReq !== tagReqSeq) return;
        tagsErr = String(e);
        allTags = [];
      } finally {
        if (myReq === tagReqSeq) tagsLoading = false;
      }
    })();
  });

  $effect(() => {
    const tagKey = selectedTags.join('\u0000');
    void tagKey;
    void matchMode;
    const myReq = ++reqSeq;
    loading = true;
    err = null;
    (async () => {
      try {
        const list = await indexNotesByTags(selectedTags, matchMode === 'all');
        if (myReq !== reqSeq) return;
        notes = list;
      } catch (e) {
        if (myReq !== reqSeq) return;
        err = String(e);
        notes = [];
      } finally {
        if (myReq === reqSeq) loading = false;
      }
    })();
  });

  const availableTags = $derived.by(() =>
    allTags.filter((t) => !selectedTags.includes(t.tag) && t.tag !== tag)
  );

  const primaryTagCount = $derived.by(
    () => allTags.find((t) => t.tag === tag)?.count ?? notes.length
  );

  const visibleNotes = $derived.by(() => sortNotes(notes, sortMode));

  function sortNotes(list: NoteRef[], mode: SortMode): NoteRef[] {
    const copy = list.slice();
    copy.sort((a, b) => compareNotes(a, b, mode));
    return copy;
  }

  function compareNotes(a: NoteRef, b: NoteRef, mode: SortMode): number {
    switch (mode) {
      case 'updated_asc':
        return (
          compareText(dateKey(a.updated), dateKey(b.updated), 'asc') || compareText(a.path, b.path)
        );
      case 'title_asc':
        return (
          compareText(noteLabel(a), noteLabel(b), 'asc') ||
          compareText(dateKey(b.updated), dateKey(a.updated), 'asc') ||
          compareText(a.path, b.path)
        );
      case 'path_asc':
        return compareText(a.path, b.path, 'asc');
      case 'updated_desc':
      default:
        return (
          compareText(dateKey(b.updated), dateKey(a.updated), 'asc') || compareText(a.path, b.path)
        );
    }
  }

  function compareText(a: string, b: string, dir: 'asc' | 'desc' = 'asc'): number {
    const out = a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });
    return dir === 'asc' ? out : -out;
  }

  function dateKey(s: string | null): string {
    return s ?? '';
  }

  function noteLabel(n: NoteRef): string {
    return n.title?.trim() || n.path.split('/').pop() || n.path;
  }

  function formatDate(s: string | null): string {
    if (!s) return '';
    const m = s.match(/^(\d{4}-\d{2}-\d{2})/);
    return m ? m[1] : s;
  }

  function addPendingTag() {
    if (!pendingTag || selectedTags.includes(pendingTag)) return;
    selectedTags = [...selectedTags, pendingTag];
    pendingTag = '';
  }

  function removeSelectedTag(t: string) {
    if (t === tag) return;
    selectedTags = selectedTags.filter((x) => x !== t);
  }

  function clearExtraTags() {
    selectedTags = [tag];
    pendingTag = '';
  }

  function emptyMessage(): string {
    if (selectedTags.length === 1) {
      return `还没有笔记带有 #${tag}。`;
    }
    const prefix = matchMode === 'all' ? '同时带有' : '带有以下任一标签';
    return `没有笔记${prefix}：${selectedTags.map((t) => `#${t}`).join(' · ')}。`;
  }

  function modeLabel(): string {
    return matchMode === 'all' ? '交集' : '并集';
  }

  function buildMocTitle(): string {
    if (selectedTags.length === 1) return `从标签 #${tag} 建立 MOC`;
    return `从主标签 #${tag} 建立 MOC（当前附加过滤不会自动带入）`;
  }
</script>

<div class="tag-view">
  <header class="tag-header">
    <div class="tag-heading">
      <span class="tag-label">
        <span class="hash">#</span>
        <span class="name">{tag}</span>
      </span>
      <span class="count"
        >{visibleNotes.length} 篇笔记 · {modeLabel()} · {selectedTags.length} 个标签</span
      >
    </div>
    <div class="tag-header-actions">
      {#if onBuildMoc}
        <button
          class="build-moc"
          onclick={() => onBuildMoc()}
          title={buildMocTitle()}
          disabled={primaryTagCount === 0}
        >
          建 MOC
        </button>
      {/if}
      <button class="close" onclick={onClose} title="关闭">×</button>
    </div>
  </header>

  <section class="tag-controls" aria-label="Tag filters and sorting">
    <div class="control-block">
      <div class="control-label">筛选标签</div>
      <div class="selected-tags">
        {#each selectedTags as selected (selected)}
          <span class="tag-chip" class:tag-chip-locked={selected === tag}>
            <span class="chip-text">#{selected}</span>
            {#if selected === tag}
              <span class="chip-kind">主标签</span>
            {:else}
              <button
                class="chip-remove"
                onclick={() => removeSelectedTag(selected)}
                aria-label={`移除标签 #${selected}`}
                title={`移除 #${selected}`}
              >
                ×
              </button>
            {/if}
          </span>
        {/each}
      </div>
      <div class="tag-adder">
        <select bind:value={pendingTag} aria-label="添加附加标签" disabled={tagsLoading}>
          <option value="">添加附加标签…</option>
          {#each availableTags as item (item.tag)}
            <option value={item.tag}>#{item.tag} ({item.count})</option>
          {/each}
        </select>
        <button onclick={addPendingTag} disabled={!pendingTag}>添加</button>
        <button onclick={clearExtraTags} disabled={selectedTags.length === 1}>清空附加标签</button>
      </div>
      {#if tagsErr}
        <p class="control-hint error">标签列表加载失败：{tagsErr}</p>
      {:else if tagsLoading}
        <p class="control-hint">正在读取可用标签…</p>
      {:else if availableTags.length === 0}
        <p class="control-hint">没有更多可追加的标签了。</p>
      {/if}
    </div>

    <div class="control-grid">
      <div class="control-block compact">
        <div class="control-label">匹配方式</div>
        <div class="segmented" role="tablist" aria-label="Tag match mode">
          <button class:active={matchMode === 'all'} onclick={() => (matchMode = 'all')}
            >交集</button
          >
          <button class:active={matchMode === 'any'} onclick={() => (matchMode = 'any')}
            >并集</button
          >
        </div>
      </div>

      <label class="control-block compact control-select">
        <span class="control-label">排序</span>
        <select bind:value={sortMode} aria-label="Tag result sort">
          <option value="updated_desc">最近更新优先</option>
          <option value="updated_asc">最早更新优先</option>
          <option value="title_asc">标题 A → Z</option>
          <option value="path_asc">按路径</option>
        </select>
      </label>
    </div>
  </section>

  {#if loading}
    <p class="status">加载中…</p>
  {:else if err}
    <p class="status error">加载失败：{err}</p>
  {:else if visibleNotes.length === 0}
    <p class="status">{emptyMessage()}</p>
  {:else}
    <ul class="notes">
      {#each visibleNotes as n (n.path)}
        <li>
          <button class="row" onclick={() => onOpenNote(n.path)}>
            <span class="title">{noteLabel(n)}</span>
            <span class="meta">
              {#if n.note_type}<span class="badge">{n.note_type}</span>{/if}
              <span class="path">{n.path}</span>
              {#if n.updated}<span class="date">· {formatDate(n.updated)}</span>{/if}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .tag-view {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
    padding: var(--space-6) var(--space-8);
    max-width: 920px;
    width: 100%;
    margin: 0 auto;
    box-sizing: border-box;
  }
  .tag-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: var(--space-4);
    padding-bottom: var(--space-3);
    box-shadow: inset 0 -1px 0 var(--color-border);
    margin-bottom: var(--space-4);
  }
  .tag-heading {
    min-width: 0;
  }
  .tag-label {
    display: inline-flex;
    align-items: baseline;
    font-family: var(--font-serif);
    font-size: var(--fs-2xl);
    font-weight: 500;
    letter-spacing: -0.01em;
  }
  .hash {
    color: var(--color-fg-muted);
    margin-right: 2px;
  }
  .count {
    display: block;
    margin-top: 4px;
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
  }
  .tag-header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 auto;
  }
  .build-moc {
    padding: 3px 10px;
    font-size: var(--fs-xs);
    line-height: 1.3;
    border: 1px solid var(--color-border);
    background: var(--color-surface-raised);
    color: var(--color-fg-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .build-moc:hover:not(:disabled) {
    background: var(--color-bg-hover);
    color: var(--color-fg);
  }
  .build-moc:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .close {
    padding: 0 8px;
    font-size: 16px;
    line-height: 1;
    border: none;
    background: transparent;
    color: var(--color-fg-muted);
    cursor: pointer;
    border-radius: 3px;
  }
  .close:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
  }
  .tag-controls {
    display: grid;
    gap: 14px;
    margin-bottom: var(--space-4);
    padding: 14px 16px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: color-mix(in oklab, var(--color-surface-raised) 72%, transparent);
  }
  .control-grid {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(220px, 260px);
    gap: 14px;
  }
  .control-block {
    min-width: 0;
  }
  .control-block.compact {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .control-label {
    display: block;
    margin-bottom: 8px;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-fg-dim);
  }
  .selected-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .tag-chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 0 10px;
    border-radius: 999px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-fg);
  }
  .tag-chip-locked {
    border-color: color-mix(in oklab, var(--color-accent) 30%, var(--color-border));
    background: color-mix(in oklab, var(--color-accent) 10%, var(--color-bg));
  }
  .chip-text {
    font-size: 13px;
  }
  .chip-kind {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-fg-muted);
  }
  .chip-remove {
    padding: 0;
    width: 18px;
    height: 18px;
    border-radius: 999px;
    border: none;
    background: transparent;
    color: var(--color-fg-muted);
    cursor: pointer;
    line-height: 1;
  }
  .chip-remove:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
  }
  .tag-adder {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 10px;
  }
  .tag-adder select,
  .control-select select {
    min-height: 32px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-fg);
    font-size: 13px;
  }
  .control-hint {
    margin: 8px 0 0;
    color: var(--color-fg-muted);
    font-size: 12px;
  }
  .control-hint.error {
    color: var(--color-danger);
  }
  .segmented {
    display: inline-flex;
    gap: 8px;
  }
  .segmented button {
    min-width: 72px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-fg-muted);
  }
  .segmented button.active {
    background: color-mix(in oklab, var(--color-accent) 12%, var(--color-bg));
    color: var(--color-fg);
    border-color: color-mix(in oklab, var(--color-accent) 35%, var(--color-border));
  }
  .status {
    padding: var(--space-4) 0;
    color: var(--color-fg-muted);
  }
  .status.error {
    color: var(--color-danger);
  }
  .notes {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .row {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    gap: 4px;
    padding: 8px 12px;
    border: none;
    background: transparent;
    text-align: left;
    color: inherit;
    cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .row:hover {
    background: var(--color-bg-hover);
  }
  .title {
    font-size: var(--fs-md);
    color: var(--color-fg);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--color-fg-muted);
    font-size: var(--fs-xs);
    font-family: var(--font-mono);
    flex-wrap: wrap;
  }
  .badge {
    padding: 1px 7px;
    border-radius: 4px;
    background: var(--color-surface-raised);
    border: 1px solid transparent;
    box-shadow: var(--pane-border);
    color: var(--color-fg-muted);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  @media (max-width: 720px) {
    .tag-view {
      padding: var(--space-5) var(--space-4);
    }
    .tag-header {
      flex-direction: column;
    }
    .tag-header-actions {
      width: 100%;
      justify-content: space-between;
    }
    .control-grid {
      grid-template-columns: 1fr;
    }
    .tag-adder {
      flex-direction: column;
      align-items: stretch;
    }
  }
</style>
