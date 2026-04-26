<script lang="ts">
  /**
   * Right-hand panel with a two-tab layout (P3-D2b.3 / .6):
   *
   * 1. **Links** — backlinks / outgoing / unresolved / related-notes
   *    (what the panel has shown since P3-B, plus D1's "相关笔记" section
   *    when AI assist is enabled).
   * 2. **AI Chat** — streaming chat against the configured provider
   *    (D2b.4 + D2b.5). The tab is hidden when AI assist is disabled so
   *    users who turned AI off don't see a dormant tab header.
   *    - D2b.6 adds a **pop-out** button on the chat tab that spawns a
   *      dedicated `chat-standalone` Tauri webview. While the standalone
   *      is open, this panel shows a "Brought back" placeholder instead
   *      of mounting a second live transcript — Tauri's `emit()` is a
   *      global broadcast, two concurrent `ChatPanel` listeners would
   *      both try to persist the same stream.
   *
   * The panel is always mounted in the grid; when no file is open we show
   * a placeholder so the column width doesn't collapse / reflow.
   *
   * Tab state is panel-local on purpose: the right panel is small enough
   * that "which tab is active" doesn't belong in global app state, and
   * resetting to Links whenever the panel re-mounts (e.g. vault switch)
   * matches what users expect from scratch-space UI.
   */
  import { onMount, onDestroy } from 'svelte';
  import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
  import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import {
    indexBacklinks,
    indexOutgoing,
    indexUnresolved,
    indexProjectNotes,
    type BacklinkItem,
    type OutgoingLink,
    type NoteRef
  } from '$lib/ipc/index';
  import { aiRelatedNotes, type RelatedNote } from '$lib/ipc/ai';
  import ChatPanel from './ChatPanel.svelte';

  const STANDALONE_LABEL = 'chat-standalone';
  const EV_FILE_PATH = 'chat-standalone:file-path';
  const EV_OPEN_NOTE = 'chat-standalone:open-note';
  const EV_CLOSE = 'chat-standalone:close';
  const EV_READY = 'chat-standalone:ready';
  const EV_CLOSED = 'chat-standalone:closed';

  interface Props {
    /** Vault-relative path of the currently open file, or null. */
    filePath: string | null;
    /** Click handler — the page already knows how to open vault paths. */
    onOpenNote: (relPath: string | null, opts?: { forceReload?: boolean }) => void;
    /**
     * Monotonic counter the parent bumps to force a refetch (e.g. after
     * the user saves, or after a vault-wide rescan). We debounce reloads
     * through this so typing doesn't spam IPC.
     */
    refreshToken?: number;
    /**
     * Whether the AI-assist sections (related-notes + AI Chat tab) are
     * shown. Controlled by Settings → AI 辅助. Defaults to true.
     */
    aiEnabled?: boolean;
  }

  const { filePath, onOpenNote, refreshToken = 0, aiEnabled = true }: Props = $props();

  type Tab = 'links' | 'chat';
  let activeTab = $state<Tab>('links');

  // ── Standalone chat window bookkeeping (D2b.6) ────────────────────
  //
  // `standaloneOpen` is the single source of truth in the main window:
  // flipped true when `openStandalone()` succeeds, flipped false on
  // `tauri://close-requested` from the popup, on `aiEnabled → false`,
  // and on `bringBack()`. The docked chat content watches it to decide
  // whether to render `<ChatPanel>` or a placeholder.
  let standaloneOpen = $state(false);
  /** Remote window handle so we can close/emit to it without looking it up every time. */
  let standaloneWindow: WebviewWindow | null = null;
  let unlistenOpenNote: UnlistenFn | null = null;
  let unlistenReady: UnlistenFn | null = null;
  let unlistenClosed: UnlistenFn | null = null;

  async function openStandalone(): Promise<void> {
    if (standaloneOpen) {
      // Second click = focus the existing window.
      const existing = await WebviewWindow.getByLabel(STANDALONE_LABEL);
      await existing?.setFocus();
      return;
    }
    // Re-use a lingering window from a previous session (Tauri keeps
    // webview labels alive until the process exits). If the label is
    // free, build a fresh one.
    const existing = await WebviewWindow.getByLabel(STANDALONE_LABEL);
    const win = existing ?? new WebviewWindow(STANDALONE_LABEL, {
      url: '/chat-standalone',
      title: 'AI 对话 · MyNotes',
      width: 720,
      height: 860,
      minWidth: 480,
      minHeight: 520,
      resizable: true,
      focus: true
    });
    standaloneWindow = win;
    await ensureStandaloneListeners();
    standaloneOpen = true;
    // Wait for the standalone to say it's mounted; it will re-emit
    // `EV_READY` which triggers the file-path push below. If the window
    // was already open (existing), poke it directly so we don't race.
    if (existing) {
      await emit(EV_FILE_PATH, { path: filePath });
    }
  }

  async function ensureStandaloneListeners(): Promise<void> {
    if (unlistenOpenNote && unlistenReady && unlistenClosed) return;
    unlistenOpenNote = await listen<{ path: string | null; forceReload?: boolean }>(
      EV_OPEN_NOTE,
      (ev) => {
        onOpenNote(ev.payload?.path ?? null, {
          forceReload: ev.payload?.forceReload ?? false
        });
      }
    );
    unlistenReady = await listen(EV_READY, () => {
      void emit(EV_FILE_PATH, { path: filePath });
    });
    // The standalone window emits `chat-standalone:closed` from its
    // `onDestroy` whether it's closed via the OS close button or by our
    // `bringBack()` path. This is more reliable than `tauri://destroyed`
    // which only fires on the destroyed webview's own event bus.
    unlistenClosed = await listen(EV_CLOSED, () => {
      standaloneOpen = false;
      standaloneWindow = null;
    });
  }

  async function bringBack(): Promise<void> {
    // Ask the standalone to close itself (see route docstring for why
    // we don't call `.close()` directly — the graceful path lets the
    // child run `onDestroy` and cancel any in-flight stream).
    await emit(EV_CLOSE);
    // Belt + suspenders: if the child doesn't respond within 600ms
    // (e.g. its webview is frozen), force-close from our side.
    setTimeout(() => {
      if (standaloneOpen && standaloneWindow) {
        void standaloneWindow.close();
      }
    }, 600);
  }

  // Push the current file path to the standalone whenever it changes —
  // the chat window uses it for RAG context + "link this note" checkbox.
  $effect(() => {
    if (!standaloneOpen) return;
    void emit(EV_FILE_PATH, { path: filePath });
  });

  // Close the standalone automatically when AI assist gets disabled.
  // Symmetrical with the "hide the chat tab" behavior below — we don't
  // want a dangling popup pointing at a disabled feature.
  $effect(() => {
    if (!aiEnabled && standaloneOpen) {
      void bringBack();
    }
  });

  // Restore `standaloneOpen` if a popup survived a Panel re-mount
  // (e.g. vault switch). Webview labels outlive Svelte component mounts
  // on the Tauri side; without this the docked view would show the live
  // ChatPanel while a zombie standalone is still listening to the same
  // events (= double-write races).
  onMount(async () => {
    const existing = await WebviewWindow.getByLabel(STANDALONE_LABEL);
    if (existing) {
      standaloneWindow = existing;
      standaloneOpen = true;
      await ensureStandaloneListeners();
      await emit(EV_FILE_PATH, { path: filePath });
    }
  });

  onDestroy(() => {
    unlistenOpenNote?.();
    unlistenReady?.();
    unlistenClosed?.();
  });

  // Automatically switch back to "Links" when AI gets disabled while the
  // chat tab was active — otherwise the tab header would be hidden but
  // the content stuck on chat.
  $effect(() => {
    if (!aiEnabled && activeTab === 'chat') {
      activeTab = 'links';
    }
  });

  let backlinks = $state<BacklinkItem[]>([]);
  let outgoing = $state<OutgoingLink[]>([]);
  let unresolved = $state<string[]>([]);
  /** Sibling notes under `4-projects/<slug>/` — only populated when the
   *  current file is a project's index.md. Empty list for all other paths. */
  let projectNotes = $state<NoteRef[]>([]);
  /** Related notes from the AI-assist heuristic scorer (P3-D1). */
  let relatedNotes = $state<RelatedNote[]>([]);
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
      relatedNotes = [];
      err = null;
      return;
    }
    const myReq = ++reqSeq;
    loading = true;
    err = null;
    try {
      const slug = projectSlugFromIndex(path);
      // Fetch all panel data in parallel. Related-notes is only fetched
      // when AI assist is enabled to avoid unnecessary IPC calls.
      const [bl, og, un, pn, rn] = await Promise.all([
        indexBacklinks(path),
        indexOutgoing(path),
        indexUnresolved(path),
        slug ? indexProjectNotes(slug) : Promise.resolve<NoteRef[]>([]),
        aiEnabled ? aiRelatedNotes(path, 8) : Promise.resolve<RelatedNote[]>([])
      ]);
      // If another request started while we were awaiting, drop these results.
      if (myReq !== reqSeq) return;
      backlinks = bl;
      outgoing = og;
      unresolved = un;
      projectNotes = pn;
      relatedNotes = rn;
    } catch (e) {
      if (myReq !== reqSeq) return;
      err = String(e);
    } finally {
      if (myReq === reqSeq) loading = false;
    }
  }

  // React to path changes, refresh pokes, and aiEnabled toggle — all in one
  // effect so a single incremented reqSeq correctly cancels earlier fetches.
  $effect(() => {
    void refreshToken;
    void aiEnabled;
    load(filePath);
  });

  function display(link: OutgoingLink): string {
    return link.title ?? link.dst_resolved ?? link.dst;
  }

  function fileName(p: string): string {
    const i = p.lastIndexOf('/');
    return i >= 0 ? p.slice(i + 1) : p;
  }

  /** Build a short tooltip string summarising which signals fired. */
  function signalTooltip(n: RelatedNote): string {
    const s = n.signals;
    const parts: string[] = [];
    if (s.tag_overlap > 0) parts.push(`tag重叠 ${(s.tag_overlap * 100).toFixed(0)}%`);
    if (s.direct_link) parts.push('直接链接');
    if (s.co_cited) parts.push('共同被引');
    if (s.embedding_cosine > 0.1) parts.push(`语义相近 ${(s.embedding_cosine * 100).toFixed(0)}%`);
    return parts.length
      ? `相关度 ${n.score.toFixed(2)} · ${parts.join(' · ')}`
      : `相关度 ${n.score.toFixed(2)}`;
  }
</script>

<aside class="panel">
  <!-- ── Tab header ───────────────────────────────────────────────── -->
  <header class="panel-header">
    <div class="tab-bar" role="tablist" aria-label="面板切换">
      <button
        class="tab"
        class:active={activeTab === 'links'}
        role="tab"
        aria-selected={activeTab === 'links'}
        data-testid="panel-tab-links"
        onclick={() => (activeTab = 'links')}
      >
        笔记关系
      </button>
      {#if aiEnabled}
        <button
          class="tab"
          class:active={activeTab === 'chat'}
          role="tab"
          aria-selected={activeTab === 'chat'}
          data-testid="panel-tab-chat"
          onclick={() => (activeTab = 'chat')}
        >
          AI 对话
        </button>
      {/if}
    </div>
    {#if activeTab === 'links' && loading}
      <span class="spinner" aria-label="loading">…</span>
    {/if}
    {#if aiEnabled && activeTab === 'chat' && !standaloneOpen}
      <button
        type="button"
        class="popout-btn"
        title="在独立窗口中打开 AI 对话"
        aria-label="弹出独立窗口"
        onclick={() => void openStandalone()}
      >
        ⧉
      </button>
    {/if}
  </header>

  {#if activeTab === 'links'}
    <!-- ── Links tab ──────────────────────────────────────────────── -->
    <div class="links-tab">
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

        {#if aiEnabled && relatedNotes.length > 0}
          <section class="related-section">
            <h4 class="related-heading">
              <span class="ai-badge" title="本地索引打分，无网络请求">AI</span>
              相关笔记
              <span class="count">{relatedNotes.length}</span>
            </h4>
            <ul>
              {#each relatedNotes as n (n.path)}
                <li>
                  <button class="link" onclick={() => onOpenNote(n.path)} title={signalTooltip(n)}>
                    <span class="link-title">{n.title ?? fileName(n.path)}</span>
                    <span class="link-path">{n.path}</span>
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      {/if}
    </div>
  {:else if activeTab === 'chat' && aiEnabled}
    {#if standaloneOpen}
      <!-- ── Docked placeholder while the popout is open (D2b.6) ── -->
      <div class="standalone-placeholder" role="status">
        <p class="sp-title">AI 对话已在独立窗口</p>
        <p class="sp-hint">
          独立窗口便于长对话 / 并排参考。关闭它即可回到此处。
        </p>
        <div class="sp-actions">
          <button
            type="button"
            class="sp-primary"
            onclick={() => {
              void WebviewWindow.getByLabel(STANDALONE_LABEL).then((w) => w?.setFocus());
            }}
          >
            聚焦独立窗口
          </button>
          <button type="button" onclick={() => void bringBack()}>
            取回到此处
          </button>
        </div>
      </div>
    {:else}
      <!-- ── AI Chat tab (D2b.4 / .5) ───────────────────────────── -->
      <ChatPanel {filePath} {onOpenNote} variant="docked" />
    {/if}
  {/if}
</aside>

<style>
  .panel {
    background: transparent;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 0;
    font-size: var(--fs-sm);
    min-width: 0;
    height: 100%;
  }
  .panel-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 12px;
    position: sticky;
    top: 0;
    background: var(--color-surface);
    box-shadow: 0 1px 0 var(--color-border);
    z-index: 1;
  }
  .links-tab {
    flex: 1 1 0;
    min-height: 0;
    overflow-y: auto;
  }
  .tab-bar {
    display: flex;
    gap: 0;
    flex: 1;
  }
  .tab {
    padding: 12px 14px;
    border: none;
    background: transparent;
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    font-weight: 500;
    cursor: pointer;
    border-bottom: 2px solid transparent;
    transition:
      color 0.15s ease,
      border-color 0.15s ease;
  }
  .tab:hover {
    color: var(--color-fg);
  }
  .tab.active {
    color: var(--color-fg);
    border-bottom-color: var(--color-accent);
  }
  .spinner {
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
    padding: 0 8px;
  }
  .popout-btn {
    padding: 2px 8px;
    margin-right: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-fg-muted);
    border-radius: 4px;
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
    transition:
      color 0.12s ease,
      border-color 0.12s ease,
      background 0.12s ease;
  }
  .popout-btn:hover,
  .popout-btn:focus-visible {
    color: var(--color-accent);
    border-color: var(--color-border);
    background: var(--color-bg-hover);
    outline: none;
  }

  /* ── Docked placeholder while the popout is open (D2b.6) ─────────── */
  .standalone-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 32px 20px;
    height: 100%;
    text-align: center;
    color: var(--color-fg-muted);
  }
  .sp-title {
    margin: 0;
    font-size: var(--fs-md);
    color: var(--color-fg);
    font-weight: 600;
  }
  .sp-hint {
    margin: 0;
    font-size: var(--fs-xs);
    line-height: 1.5;
    max-width: 260px;
  }
  .sp-actions {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  .sp-actions button {
    padding: 6px 12px;
    border: 1px solid var(--color-border);
    background: var(--color-bg);
    color: var(--color-fg);
    border-radius: 4px;
    cursor: pointer;
    font-size: var(--fs-xs);
    transition:
      background 0.12s ease,
      border-color 0.12s ease;
  }
  .sp-actions button:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-accent);
  }
  .sp-actions .sp-primary {
    background: var(--color-accent);
    color: var(--color-bg);
    border-color: var(--color-accent);
    font-weight: 600;
  }
  .sp-actions .sp-primary:hover {
    background: color-mix(in oklch, var(--color-accent) 85%, white);
    border-color: color-mix(in oklch, var(--color-accent) 85%, white);
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
    background: var(--color-surface-raised);
    border: 1px solid transparent;
    border-radius: var(--radius-lg);
    padding: var(--space-2) 0 var(--space-3);
    margin: var(--space-3);
    box-shadow: var(--pane-border);
  }
  section + section {
    margin-top: 0;
  }
  h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: var(--space-3) var(--space-4) var(--space-2);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-dim);
    font-weight: 500;
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
    transition:
      background 0.15s ease,
      transform 0.15s ease;
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

  /* Docked chat must flex below the tab header; otherwise its internal
     transcript consumes the full panel height and pushes the composer out
     of view. */
  .panel :global(.chat-panel),
  .standalone-placeholder {
    flex: 1 1 0;
    min-height: 0;
    overflow: hidden;
  }

  /* ── AI related-notes section (P3-D1) ─────────────────────────────────── */
  .related-section {
    /* Slightly different border style to visually distinguish AI content. */
    border-top: 1px dashed var(--color-border);
  }
  .related-heading {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .ai-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 1px 5px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: var(--color-accent);
    background: var(--color-accent-weak);
    border-radius: 4px;
    line-height: 1.4;
    cursor: default;
  }
</style>
