<script lang="ts">
  /**
   * Inbox Review — lists everything in `0-inbox/` with inline action buttons.
   * Swapped into the editor-pane slot (instead of Editor / Home) when the user
   * triggers "Review Inbox" from Home or the command palette.
   *
   * Per-row actions are delegated to the parent via callbacks; this component
   * doesn't perform file IO directly. It re-fetches whenever `refreshToken`
   * bumps so post-action feedback is immediate.
   */
  import { indexInboxList, type NoteRef } from '$lib/ipc/index';

  interface Props {
    onOpenNote: (path: string) => void;
    onPromote: (path: string) => void;
    onArchive: (path: string) => void;
    onDelete: (path: string) => void;
    onClose: () => void;
    refreshToken?: number;
  }

  const { onOpenNote, onPromote, onArchive, onDelete, onClose, refreshToken = 0 }: Props =
    $props();

  let items = $state<NoteRef[]>([]);
  let loading = $state(true);
  let err = $state<string | null>(null);

  let reqSeq = 0;

  async function load() {
    const myReq = ++reqSeq;
    loading = true;
    err = null;
    try {
      const list = await indexInboxList();
      if (myReq !== reqSeq) return;
      items = list;
    } catch (e) {
      if (myReq !== reqSeq) return;
      err = String(e);
      items = [];
    } finally {
      if (myReq === reqSeq) loading = false;
    }
  }

  $effect(() => {
    void refreshToken;
    void load();
  });

  function stem(path: string): string {
    const name = path.slice(path.lastIndexOf('/') + 1);
    return name.replace(/\.md$/, '');
  }

  function formatUpdated(u: string | null): string {
    if (!u) return '';
    // u is 'YYYY-MM-DD HH:mm' per template.ts; show as-is for density.
    return u;
  }
</script>

<div class="inbox-view">
  <header>
    <h2>
      <span class="hash">📥</span>
      Inbox
      <span class="count">{items.length}</span>
    </h2>
    <button class="close" onclick={onClose} aria-label="关闭 Inbox">✕</button>
  </header>

  {#if loading && items.length === 0}
    <p class="empty">加载中…</p>
  {:else if err}
    <p class="empty err" title={err}>加载失败</p>
  {:else if items.length === 0}
    <p class="empty">
      📥 Inbox 是空的。
      <br />
      下次想到什么按 <kbd>⌘⇧N</kbd> 丢进来。
    </p>
  {:else}
    <ul>
      {#each items as it (it.path)}
        <li class="row">
          <div class="row-body">
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <button
              class="row-title"
              onclick={() => onOpenNote(it.path)}
              title={it.path}
            >
              <span class="title">{it.title ?? stem(it.path)}</span>
              <span class="path">{it.path}</span>
            </button>
            {#if it.updated}
              <span class="updated">{formatUpdated(it.updated)}</span>
            {/if}
          </div>
          <div class="actions">
            <button class="act promote" onclick={() => onPromote(it.path)} title="Promote 到 1-notes/">
              Promote
            </button>
            <button class="act archive" onclick={() => onArchive(it.path)} title="归档到 .mynotes/archive/">
              Archive
            </button>
            <button class="act danger" onclick={() => onDelete(it.path)} title="删除">
              Delete
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .inbox-view {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-bg-subtle);
  }
  header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hash {
    color: var(--color-fg-muted);
  }
  .count {
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 400;
  }
  .close {
    margin-left: auto;
    background: transparent;
    border: none;
    color: var(--color-fg-muted);
    padding: 2px 8px;
    cursor: pointer;
    font-size: 14px;
  }
  .close:hover {
    color: var(--color-fg);
  }
  .empty {
    padding: 40px 20px;
    color: var(--color-fg-muted);
    text-align: center;
    font-size: 13px;
    line-height: 1.8;
  }
  .empty.err {
    color: var(--color-danger);
  }
  kbd {
    font-family: var(--font-mono);
    font-size: 11px;
    background: var(--color-bg-subtle);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 1px 5px;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    overflow-y: auto;
    flex: 1;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--color-border);
  }
  .row:hover {
    background: var(--color-bg-hover);
  }
  .row-body {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .row-title {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border: none;
    background: transparent;
    padding: 0;
    text-align: left;
    cursor: pointer;
    color: inherit;
  }
  .title {
    font-size: 14px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .path {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--color-fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .updated {
    font-size: 11px;
    font-family: var(--font-mono);
    color: var(--color-fg-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .actions {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .act {
    font-size: 11px;
    padding: 4px 8px;
    border-radius: 4px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    cursor: pointer;
  }
  .act:hover {
    background: var(--color-bg-hover);
  }
  .act.promote {
    color: var(--color-accent);
    border-color: var(--color-accent);
  }
  .act.danger:hover {
    background: var(--color-danger);
    color: var(--color-bg);
    border-color: var(--color-danger);
  }
</style>
