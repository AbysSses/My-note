<script lang="ts">
  /**
   * Tag aggregation view. Shown in the editor pane when a tag is active
   * (instead of the markdown editor or the Home cards).
   *
   * Lists every note carrying the tag, newest-first. We intentionally do
   * NOT try to render the note bodies — keeps this page fast and makes it
   * feel more like a map/index than a reader.
   */
  import { indexNotesByTag, type NoteRef } from '$lib/ipc/index';

  interface Props {
    tag: string;
    onOpenNote: (relPath: string) => void;
    onClose: () => void;
  }

  const { tag, onOpenNote, onClose }: Props = $props();

  let notes = $state<NoteRef[]>([]);
  let loading = $state(false);
  let err = $state<string | null>(null);

  let reqSeq = 0;

  $effect(() => {
    const myReq = ++reqSeq;
    loading = true;
    err = null;
    (async () => {
      try {
        const list = await indexNotesByTag(tag);
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

  function formatDate(s: string | null): string {
    if (!s) return '';
    // Accept "YYYY-MM-DD" prefix or full ISO — just keep the date part.
    const m = s.match(/^(\d{4}-\d{2}-\d{2})/);
    return m ? m[1] : s;
  }
</script>

<div class="tag-view">
  <header class="tag-header">
    <span class="tag-label">
      <span class="hash">#</span>
      <span class="name">{tag}</span>
    </span>
    <span class="count">{notes.length} 篇笔记</span>
    <button class="close" onclick={onClose} title="关闭">×</button>
  </header>

  {#if loading}
    <p class="status">加载中…</p>
  {:else if err}
    <p class="status error">加载失败：{err}</p>
  {:else if notes.length === 0}
    <p class="status">还没有笔记带有 <code>#{tag}</code>。</p>
  {:else}
    <ul class="notes">
      {#each notes as n (n.path)}
        <li>
          <button class="row" onclick={() => onOpenNote(n.path)}>
            <span class="title">{n.title ?? n.path.split('/').pop()}</span>
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
    max-width: 840px;
    width: 100%;
    margin: 0 auto;
    box-sizing: border-box;
  }
  .tag-header {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    padding-bottom: var(--space-3);
    border-bottom: 1px solid var(--color-border);
    margin-bottom: var(--space-3);
  }
  .tag-label {
    display: inline-flex;
    align-items: baseline;
    font-size: var(--fs-2xl);
    font-weight: 600;
  }
  .hash {
    color: var(--color-fg-muted);
    margin-right: 2px;
  }
  .count {
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
  }
  .close {
    margin-left: auto;
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
    padding: 1px 6px;
    border-radius: 3px;
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    color: var(--color-fg-muted);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  code {
    font-family: var(--font-mono);
    background: var(--color-bg-subtle);
    padding: 1px 4px;
    border-radius: 3px;
  }
</style>
