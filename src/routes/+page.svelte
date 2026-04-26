<script lang="ts">
  import { onMount } from 'svelte';
  import { open as openDialog, save as saveDialog, ask, message } from '@tauri-apps/plugin-dialog';
  import {
    vaultInit,
    vaultOpen,
    vaultIsInitialized,
    vaultRecent,
    vaultReseedTemplates
  } from '$lib/ipc/vault';
  import type { DirEntry } from '$lib/ipc/vault';
  import {
    appConfigGet,
    appConfigSetAutosaveMs,
    appConfigSetAiToolPermissions,
    appConfigSetShortcuts,
    appConfigSetTheme,
    type AiToolPermissions
  } from '$lib/ipc/config';
  import {
    appConfigSetAiEnabled,
    aiProviderSetConfig,
    aiProviderClearConfig,
    aiProviderHasApiKey,
    aiProviderTestConnection,
    aiProviderTestChatConnection,
    aiEmbedNote,
    aiEmbedStats,
    aiEmbedClearAll,
    aiEmbedVaultPreview,
    aiEmbedVaultRun,
    aiComplete,
    aiCompleteCancel,
    type ProviderErrorKind,
    type ProviderTestResult,
    type ChatProviderTestResult,
    type EmbedFailure,
    type EmbedNoteResult,
    type EmbedOutcome,
    type EmbeddingStats,
    type VaultEmbedPreview,
    type VaultEmbedRunResult,
    type CompleteFailure
  } from '$lib/ipc/ai';
  import DiffPreviewModal from '$lib/ai/DiffPreviewModal.svelte';
  import TagSuggestModal from '$lib/ai/TagSuggestModal.svelte';
  import {
    buildSummarizePrompt,
    applySummaryToBody,
    makeSummarizeRequestId,
    stripFrontmatter,
    type SummarizeTarget
  } from '$lib/ai/summarizePrompt';
  import {
    buildSuggestTagsPrompt,
    parseSuggestedTags,
    parseExistingTags,
    mergeTagsIntoFrontmatter,
    makeSuggestTagsRequestId
  } from '$lib/ai/suggestTagsPrompt';
  import {
    buildDraftMocPrompt,
    buildFlatEntriesMarkdown,
    sanitizeDraftMoc,
    makeDraftMocRequestId
  } from '$lib/ai/draftMocPrompt';
  import {
    fileExists,
    fileImport,
    fileList,
    fileRead,
    fileWrite,
    type ImportedFile
  } from '$lib/ipc/file';
  import { vaultState } from '$lib/state/vault.svelte';
  import { bootstrapE2eVault, isE2eMockActive } from '$lib/e2e/mockBootstrap';
  import Editor from '$lib/editor/Editor.svelte';
  import Panel from '$lib/panel/Panel.svelte';
  import ChatPanel from '$lib/panel/ChatPanel.svelte';
  import TagsSection from '$lib/tags/TagsSection.svelte';
  import ProjectsSection from '$lib/projects/ProjectsSection.svelte';
  import TagView from '$lib/tags/TagView.svelte';
  import InboxView from '$lib/inbox/InboxView.svelte';
  import CommandPalette from '$lib/palette/CommandPalette.svelte';
  import IconRail from '$lib/sidebar/IconRail.svelte';
  import KnowledgeColumn from '$lib/sidebar/KnowledgeColumn.svelte';
  import TasksList from '$lib/tasks/TasksList.svelte';
  import TodayTasksPanel from '$lib/tasks/TodayTasksPanel.svelte';
  import TweaksPanel from '$lib/tweaks/TweaksPanel.svelte';
  import { tweaksStore } from '$lib/tweaks/tweaksStore.svelte';
  import type { PaletteContext } from '$lib/palette/commandRegistry';
  import { projectSlugFromPath } from '$lib/palette/commandRegistry';
  import { invalidateWikiCompletionCache } from '$lib/editor/wikicomplete';
  import {
    fileDelete,
    fileMove,
    fileMoveWithRefsPreview,
    fileMoveWithRefs,
    dirMoveWithRefsPreview,
    dirMoveWithRefs,
    pathReveal,
    type FileRenamePreview,
    type DirRenamePreview
  } from '$lib/ipc/file';
  import {
    indexAllNotes,
    indexNotesByTag,
    indexResolveWikiLink,
    indexTags,
    indexUnresolvedCount,
    type NoteRef
  } from '$lib/ipc/index';
  import { projectSetStatus } from '$lib/ipc/project';
  import {
    attachmentUnreferenced,
    attachmentDeleteBatch,
    type AttachmentInfo
  } from '$lib/ipc/attachment';
  import {
    exportVaultZip,
    noteExportCopy,
    noteRenderPrintHtml,
    type ExportSummary
  } from '$lib/ipc/export';
  import {
    appendDailyRecord,
    buildMocFromTag,
    createNoteFromTemplate,
    extractBlockToNote,
    openOrCreateDaily,
    openOrCreateWeekly,
    promoteInboxNote,
    quickCapture,
    rewriteFrontmatter,
    slugifyTitle,
    type CommandDeps
  } from '$lib/commands';
  import type { EditorAPI } from '$lib/editor/Editor.svelte';
  import {
    DEFAULT_SHORTCUT_BINDINGS,
    PALETTE_SHORTCUT_ACTIONS,
    findShortcutConflict,
    formatShortcutDisplay,
    matchShortcutEvent,
    mergeShortcutBindings,
    shortcutActionDefs,
    shortcutActionIds,
    shortcutFromKeyboardEvent,
    type ShortcutActionId,
    type ThemePreference
  } from '$lib/shortcuts';
  import { formatDate, isoWeekString } from '$lib/template';
  import { fade, fly } from 'svelte/transition';

  type RuntimeMode = 'tauri' | 'browser';
  type PendingSave = {
    path: string;
    content: string;
    vaultPath: string;
  };
  type NoticeKind = 'info' | 'success' | 'error';
  type Notice = {
    id: number;
    kind: NoticeKind;
    message: string;
  };

  let recent = $state<string[]>([]);
  let tree = $state<DirEntry[]>([]);
  let expanded = $state<Set<string>>(new Set());
  // Sidebar drop-import visual feedback. `dropTargetPath` holds the
  // rel_path of the directory currently hovered with a Finder drag; null
  // means no row-level target. `rootDropActive` is the empty-tree-area
  // fallback that imports into 0-inbox/. Reset aggressively on dragleave /
  // drop / dragend to avoid sticky highlights.
  let dropTargetPath = $state<string | null>(null);
  let rootDropActive = $state<boolean>(false);
  let editorContent = $state<string>('');
  // Autosave-only status bar state. Command feedback uses the notice stack.
  let saveStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
  let saveError = $state<string>('');
  let notices = $state<Notice[]>([]);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let runtimeMode = $state<RuntimeMode>('tauri');
  let noticeSeq = 0;
  const noticeTimers = new Map<number, ReturnType<typeof setTimeout>>();

  // Status bar state.
  let cursorLine = $state(1);
  let cursorCol = $state(0);
  let inboxCount = $state(0);

  // Home-view aggregates, refreshed whenever the tree or index changes.
  let homeRecentNotes = $state<NoteRef[]>([]);
  let homeRecentMocs = $state<NoteRef[]>([]);
  let homeUnresolved = $state(0);
  let homeReview = $state<NoteRef | null>(null);
  /**
   * Sequence counter so async Home refreshes racing against vault switches
   * don't overwrite each other. We bump on every refresh and drop stale
   * results.
   */
  let homeReqSeq = 0;

  /**
   * Bumped whenever the current file is saved or the tree is refreshed.
   * The Panel watches this and re-fetches backlinks/outgoing/unresolved.
   * Saves happen client-side but the index is updated asynchronously by
   * the file watcher, so we wait a beat before poking the panel.
   */
  let panelRefreshToken = $state(0);
  let panelRefreshTimer: ReturnType<typeof setTimeout> | null = null;

  function schedulePanelRefresh(delayMs = 400) {
    if (panelRefreshTimer) clearTimeout(panelRefreshTimer);
    panelRefreshTimer = setTimeout(() => {
      panelRefreshToken = panelRefreshToken + 1;
    }, delayMs);
  }

  function errorMessage(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
  }

  function parseAiFailureText(message: string): {
    kind?: ProviderErrorKind;
    message: string;
    retryAfterSecs?: number;
  } {
    const trimmed = message.trim();
    const rateMatch = trimmed.match(/^rate limit:\s*(.+?)\s*\(retry after\s+(\d+)s\)$/i);
    if (rateMatch) {
      return {
        kind: 'rate_limit',
        message: rateMatch[1],
        retryAfterSecs: Number(rateMatch[2])
      };
    }
    if (/^network:\s*/i.test(trimmed)) {
      return { kind: 'network', message: trimmed.replace(/^network:\s*/i, '') };
    }
    if (/^auth:\s*/i.test(trimmed)) {
      return { kind: 'auth', message: trimmed.replace(/^auth:\s*/i, '') };
    }
    if (/^invalid request:\s*/i.test(trimmed)) {
      return {
        kind: 'invalid_request',
        message: trimmed.replace(/^invalid request:\s*/i, '')
      };
    }
    return { message: trimmed };
  }

  function isQuotaLikeFailure(message: string): boolean {
    const text = message.toLowerCase();
    return (
      text.includes('quota') ||
      text.includes('billing') ||
      text.includes('insufficient') ||
      text.includes('exceeded your current') ||
      text.includes('额度') ||
      text.includes('配额') ||
      text.includes('余额')
    );
  }

  function formatAiFailureText(opts: {
    kind?: ProviderErrorKind;
    message?: string;
    retryAfterSecs?: number;
    storeUnchanged?: boolean;
  }): string {
    const raw = (opts.message ?? '').trim();
    const suffix = opts.storeUnchanged ? '；现有索引未被改坏' : '';

    if (
      raw.includes('no AI provider configured') ||
      raw.includes('AI provider kind is empty') ||
      raw.includes('base_url is empty') ||
      raw.includes('embed_model is empty')
    ) {
      return `AI Provider 尚未配置完整；先在 Settings 补齐 Base URL / 模型 / API key${suffix}`;
    }
    if (raw.includes('embedding store unavailable')) {
      return `AI 索引库当前不可用；重开 vault 后再试${suffix}`;
    }

    if (!opts.kind) {
      if (raw.length > 0) return `${raw}${suffix}`;
      return `AI 请求失败${suffix}`;
    }

    switch (opts.kind) {
      case 'network':
        return `网络或服务连接失败；检查 Base URL / 本地模型服务 / 网络后重试（${raw}）${suffix}`;
      case 'auth':
        return `认证失败；检查 API key、Provider 权限或网关配置（${raw}）${suffix}`;
      case 'rate_limit':
        return isQuotaLikeFailure(raw)
          ? `额度或配额不足；检查账单/余额${opts.retryAfterSecs ? `，约 ${opts.retryAfterSecs}s 后可再试` : ''}（${raw}）${suffix}`
          : `请求过快或 provider 正忙${opts.retryAfterSecs ? `；约 ${opts.retryAfterSecs}s 后重试` : ''}（${raw}）${suffix}`;
      case 'invalid_request':
        return /model|embedding/i.test(raw)
          ? `当前 embedding 模型不可用或不受支持；检查模型名与 provider 协议（${raw}）${suffix}`
          : `请求参数无效；检查 provider 配置（${raw}）${suffix}`;
      default:
        return raw.length > 0 ? `${raw}${suffix}` : `AI 请求失败${suffix}`;
    }
  }

  function joinNotice(parts: Array<string | null | undefined | false>): string {
    return parts.filter((part): part is string => !!part && part.length > 0).join('；');
  }

  function formatUsdEstimate(usd: number): string {
    if (usd === 0) return '$0';
    if (usd >= 1) return `$${usd.toFixed(2)}`;
    if (usd >= 0.01) return `$${usd.toFixed(4)}`;
    return `$${usd.toFixed(6)}`;
  }

  function providerTestFailureText(result: {
    error_kind?: ProviderErrorKind;
    error_message?: string;
    retry_after_secs?: number;
  }): string {
    return formatAiFailureText({
      kind: result.error_kind,
      message: result.error_message,
      retryAfterSecs: result.retry_after_secs
    });
  }

  function normalizeCompleteFailure(
    failure: CompleteFailure | null | undefined,
    fallbackMessage = 'AI 返回空结果'
  ): CompleteFailure {
    const raw = (failure?.message ?? fallbackMessage).trim();
    if (raw === 'cancelled before any content arrived') {
      return {
        kind: 'other',
        message: '已取消生成，尚未产出可用内容'
      };
    }
    return {
      kind: failure?.kind ?? 'other',
      message: formatAiFailureText({
        kind: failure?.kind && failure.kind !== 'other' ? failure.kind : undefined,
        message: raw
      }),
      retry_after_secs: failure?.retry_after_secs
    };
  }

  function partialResultNote(kind: 'summary' | 'tags' | 'moc'): string {
    switch (kind) {
      case 'summary':
        return '已取消生成；以下是取消前产出的部分摘要，可直接采用或重新生成。';
      case 'tags':
        return '已取消生成；以下是取消前产出的部分标签候选，可直接筛选或重新生成。';
      case 'moc':
        return '已取消生成；以下是取消前产出的部分 MOC 分组草稿，可直接采用或重新生成。';
    }
  }

  function clearNoticeTimer(id: number) {
    const timer = noticeTimers.get(id);
    if (!timer) return;
    clearTimeout(timer);
    noticeTimers.delete(id);
  }

  function dismissNotice(id: number) {
    clearNoticeTimer(id);
    notices = notices.filter((notice) => notice.id !== id);
  }

  function clearAllNotices() {
    for (const id of noticeTimers.keys()) {
      clearNoticeTimer(id);
    }
    notices = [];
  }

  function pushNotice(message: string, kind: NoticeKind = 'info', ttlMs?: number) {
    const text = message.trim();
    if (!text) return;

    if (notices.length >= 4) {
      dismissNotice(notices[0].id);
    }

    const id = ++noticeSeq;
    notices = [...notices, { id, kind, message: text }];
    const timeout = ttlMs ?? (kind === 'error' ? 5200 : 3600);
    noticeTimers.set(
      id,
      setTimeout(() => {
        dismissNotice(id);
      }, timeout)
    );
  }

  /**
   * When non-null the editor pane shows the tag aggregation view for this
   * tag instead of the markdown editor or Home. Opening a file clears it.
   */
  let activeTag = $state<string | null>(null);

  function selectTag(tag: string) {
    // Commit any pending writes before replacing the view; otherwise the
    // editor's `{#key}`-driven teardown races the save.
    void drainPendingSaves();
    vaultState.closeFile();
    editorContent = '';
    saveStatus = 'idle';
    saveError = '';
    activeTag = tag;
  }

  function closeTagView() {
    activeTag = null;
  }

  /** Which full-pane "view" is shown instead of the editor. `null` = editor/home.
   *  'chat' surfaces the AI chat in the middle pane (Second-design's default),
   *  distinct from the right-panel AI Chat tab that docks alongside a note. */
  let activeView = $state<'inbox' | 'graph' | 'chat' | null>(null);

  // --- Knowledge-base column state --------------------------------------
  // Second-design splits the left sidebar into an icon rail + a 3-tab
  // knowledge-base column. Active tab + priority filter live here so the rail
  // can switch them; the column itself stays stateless.
  let kbTab = $state<'notes' | 'tasks' | 'projects'>('notes');
  let priorityFilter = $state<'all' | 'urgent' | 'high' | 'med' | 'low'>('all');

  /** Floating "Today's tasks" overlay — hidden by default, toggle via rail/palette. */
  let todayPanelVisible = $state(false);
  let GraphViewComponent = $state<typeof import('$lib/graph/GraphView.svelte').default | null>(
    null
  );
  let graphViewLoading = $state(false);
  let graphViewLoadError = $state<string | null>(null);

  async function ensureGraphViewLoaded() {
    if (GraphViewComponent || graphViewLoading) return;
    graphViewLoading = true;
    graphViewLoadError = null;
    try {
      GraphViewComponent = (await import('$lib/graph/GraphView.svelte')).default;
    } catch (err) {
      graphViewLoadError = errorMessage(err);
      pushNotice(`Graph view 加载失败：${graphViewLoadError}`, 'error');
    } finally {
      graphViewLoading = false;
    }
  }

  function openInboxReview() {
    void drainPendingSaves();
    vaultState.closeFile();
    editorContent = '';
    saveStatus = 'idle';
    saveError = '';
    activeTag = null;
    activeView = 'inbox';
  }

  function closeInboxView() {
    activeView = null;
  }

  /** Open the note-graph view. Unlike Inbox, we DON'T close the current file
   *  or clear the editor buffer — local-mode uses `openFilePath` as the seed,
   *  and clicking a node re-opens a file so the transition feels lateral. */
  function openGraphView() {
    void drainPendingSaves();
    activeTag = null;
    activeView = 'graph';
    void ensureGraphViewLoaded();
  }

  function closeGraphView() {
    activeView = null;
  }

  /** Open the AI-chat-in-the-middle view (Second-design's chat-first surface).
   *  Doesn't close the current file — the user can flip back to the editor via
   *  the Notes / Home button. Unlike the right-panel chat tab, this fills the
   *  whole workspace and doesn't require a note to be open. */
  function openChatView() {
    activeTag = null;
    activeView = 'chat';
  }

  function closeChatView() {
    activeView = null;
  }

  /** Bump to trigger a graph refetch after a vault mutation. */
  let graphRefreshToken = $state(0);

  // --- Block-level Extract ------------------------------------------------
  // Imperative handle into the editor, published via Editor's `onReady`.
  // Stays null while a non-editor view (inbox/graph/tag) occupies the pane.
  let editorApi = $state<EditorAPI | null>(null);

  /** Extract modal state. Flat rather than an object so reactivity is cheap. */
  let extractOpen = $state(false);
  let extractTitle = $state('');
  let extractError = $state('');
  let extractRunning = $state(false);
  let extractInputEl = $state<HTMLInputElement | null>(null);
  /** Captured at modal-open time so the user can freely move the cursor in
   *  the editor (or bump other state) without the range shifting under us. */
  let extractSourceRange = $state<{ from: number; to: number } | null>(null);
  let extractSourceText = $state('');
  let extractSourcePath = $state<string | null>(null);

  function runExtractSelection() {
    if (!editorApi || !vaultState.openFilePath) {
      pushNotice('Extract 仅在编辑器里可用', 'error');
      return;
    }
    // If nothing is selected, expand to the enclosing paragraph first.
    let range = editorApi.getSelectionRange();
    if (range.from === range.to) {
      const expanded = editorApi.expandToParagraph();
      if (!expanded) {
        pushNotice('光标所在行为空，无法提取', 'error');
        return;
      }
      range = expanded;
    }
    const text = editorApi.getSelection();
    if (!text.trim()) {
      pushNotice('选中内容为空', 'error');
      return;
    }
    extractSourceRange = range;
    extractSourceText = text;
    extractSourcePath = vaultState.openFilePath;
    // Seed the title field with the first heading or first 40 chars.
    extractTitle = guessTitleFromBlock(text);
    extractError = '';
    extractRunning = false;
    extractOpen = true;
    queueMicrotask(() => extractInputEl?.focus());
  }

  /** Best-effort title suggestion: first `# ...` / `## ...` line, or first
   *  non-blank line capped at 40 chars. Pure — unit-testable via callers. */
  function guessTitleFromBlock(text: string): string {
    for (const raw of text.split(/\r?\n/)) {
      const line = raw.trim();
      if (!line) continue;
      const h = line.match(/^#{1,6}\s+(.+?)\s*$/);
      if (h) return h[1];
      return line.length > 40 ? line.slice(0, 40).trim() + '…' : line;
    }
    return '';
  }

  function cancelExtract() {
    extractOpen = false;
    extractSourceRange = null;
    extractSourceText = '';
    extractSourcePath = null;
  }

  async function confirmExtract() {
    if (extractRunning) return;
    if (!editorApi || !extractSourceRange || !extractSourcePath) {
      extractError = '编辑器状态已变化，请重试';
      return;
    }
    // Guard against the user navigating to a different file between
    // opening the modal and confirming — the range would no longer map
    // to the same document.
    if (vaultState.openFilePath !== extractSourcePath) {
      extractError = '源文件已切换，请回到原文件再试';
      return;
    }
    const title = extractTitle.trim();
    if (!title) {
      extractError = '请输入新笔记标题';
      return;
    }
    extractRunning = true;
    extractError = '';
    try {
      const { dstPath, linkText } = await extractBlockToNote(title, extractSourceText);
      // Splice the wiki-link into the captured range in a single CM6
      // transaction — undo treats "extract" as one step.
      editorApi.dispatchReplace(extractSourceRange, linkText);

      // The editor `onChange` listener fires on dispatch → editorContent
      // is updated → the autosave scheduler picks it up on the next tick.
      // We don't manually fileWrite the source here.

      // Tree + panels refresh so the new 1-notes/ file shows up.
      // Panel refresh is debounced (same race as confirmBuildMoc — the file
      // just hit disk and the indexer hasn't caught up yet). The `refreshTree`
      // call is a tree-only walk, independent of the SQLite index, so it's
      // safe to await immediately.
      await refreshTree();
      schedulePanelRefresh(200);
      graphRefreshToken += 1;
      invalidateWikiCompletionCache();

      extractOpen = false;
      extractSourceRange = null;
      extractSourceText = '';
      extractSourcePath = null;

      const name = dstPath.slice(dstPath.lastIndexOf('/') + 1);
      pushNotice(`已提取到 1-notes/${name}`, 'success');
    } catch (e) {
      extractError = String(e);
    } finally {
      extractRunning = false;
    }
  }

  /** Increment to force InboxView to re-fetch (e.g. after a promote/archive). */
  let inboxRefreshToken = $state(0);
  function bumpInbox() {
    inboxRefreshToken += 1;
    void refreshInboxCount();
  }

  /** Archive an inbox note by moving it to `.mynotes/archive/inbox/`. */
  async function archiveInboxNote(path: string) {
    const filename = path.slice(path.lastIndexOf('/') + 1);
    const target = `.mynotes/archive/inbox/${filename}`;
    try {
      await fileMove(path, target);
      bumpInbox();
    } catch (e) {
      pushNotice(`archive ${path}: ${errorMessage(e)}`, 'error');
    }
  }

  async function deleteInboxNote(path: string) {
    const ok = confirm(`确定删除 "${path}"？此操作不可恢复。`);
    if (!ok) return;
    try {
      await fileDelete(path);
      // If the file was open, close it.
      if (vaultState.openFilePath === path) {
        vaultState.closeFile();
        editorContent = '';
      }
      bumpInbox();
      await refreshTree();
    } catch (e) {
      pushNotice(`delete ${path}: ${errorMessage(e)}`, 'error');
    }
  }

  /** Whether the AI-assist features (Related notes panel / summarize
   *  commands / chat panel) are globally enabled. Defaults to true so
   *  features are on after first launch. Persisted via
   *  `app_config_set_ai_enabled`. Hoisted above `paletteCtx` because
   *  the palette gates AI commands on this flag. */
  let aiEnabled = $state(true);
  let aiToolPermissions = $state<AiToolPermissions>({
    allow_readonly: true,
    allow_writeback: true,
    allow_destructive: false
  });

  /** Cmd+P command palette open state. */
  let paletteOpen = $state(false);

  /** Context passed to the palette. Rebuilt when the open-file path changes. */
  const paletteCtx: PaletteContext = $derived({
    openNote: (p: string) => {
      void openFile({ name: p.slice(p.lastIndexOf('/') + 1), rel_path: p, is_dir: false });
    },
    openTag: (tag: string) => selectTag(tag),
    runDaily: () => void openOrCreateDaily(cmdDeps),
    runWeekly: () => void openOrCreateWeekly(cmdDeps),
    runCapture: () => void quickCapture(cmdDeps),
    runRecord: () => openRecord(),
    runNewNote: () => newNote(),
    // Stubs — implemented in subsequent tasks.
    runNewMoc: () => {
      newNoteError = '';
      newNote('2-moc');
    },
    runNewProject: () => {
      newNoteError = '';
      newNote('4-projects');
    },
    runInboxReview: () => openInboxReview(),
    runBuildMocFromTag: () => {
      void runBuildMocFromTag();
    },
    activeTag,
    runGraph: () => openGraphView(),
    runExtractSelection: () => runExtractSelection(),
    promoteCurrent: () => {
      const p = vaultState.openFilePath;
      if (!p || !p.startsWith('0-inbox/')) {
        pushNotice('Promote 仅对 0-inbox/ 下的笔记可用', 'error');
        return;
      }
      openPromoteModal(p);
    },
    setProjectStatus: (status: string) => {
      void runSetProjectStatus(status);
    },
    runAddNoteToProject: () => runAddNoteToProject(),
    runExtractFromProject: () => {
      void runExtractFromProject();
    },
    runReseedTemplates: () => {
      void runReseedTemplates();
    },
    runFindUnusedAttachments: () => {
      void openUnusedAttachments();
    },
    runRenameCurrent: () => {
      openRenameModal();
    },
    runRenameCurrentDir: () => {
      openDirRenameModal();
    },
    runOpenSettings: () => openSettings(),
    applyThemeChoice: (t: 'system' | 'light' | 'dark') => setTheme(t),
    runExportVaultZip: () => {
      void runExportVaultZip();
    },
    runExportCurrentNote: () => {
      void runExportCurrentNote();
    },
    runPrintCurrentNote: () => {
      void runPrintCurrentNote();
    },
    closeVault: () => {
      void drainPendingSaves().then(() => {
        resetVaultViewState();
        vaultState.clear();
      });
    },
    runShowRelatedNotes: () => {
      // If AI assist is off, turn it on first so the section appears.
      if (!aiEnabled) {
        aiEnabled = true;
        void appConfigSetAiEnabled(true).catch((err) =>
          console.error('Failed to enable ai_enabled:', err)
        );
      }
      // Scroll the panel related-notes section into view.
      requestAnimationFrame(() => {
        document
          .querySelector('.related-section')
          ?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      });
    },
    runEmbedCurrentNote: () => {
      void embedCurrentNote();
    },
    aiEnabled,
    runSummarizeCurrentNote: (target) => {
      void runSummarizeCurrentNote(target);
    },
    runSuggestTagsForCurrentNote: () => {
      void runSuggestTagsForCurrentNote();
    },
    runDraftMocFromTag: () => {
      void runDraftMocFromTagAi();
    },
    toggleTweaks: () => tweaksStore.toggle(),
    toggleTodayTasks: () => {
      todayPanelVisible = !todayPanelVisible;
    },
    currentFilePath: vaultState.openFilePath
  });

  // Theme: 'system' follows OS; 'light'/'dark' override. Persisted in localStorage.
  type Theme = ThemePreference;
  const THEME_KEY = 'mynotes:theme';
  const SHORTCUTS_KEY = 'mynotes:shortcuts';
  let theme = $state<Theme>('system');
  // CJK chars count individually; latin runs count as one word each.
  const wordCount = $derived.by(() => {
    const m = editorContent.match(/[\u4e00-\u9fa5]|[A-Za-z0-9]+/g);
    return m ? m.length : 0;
  });

  // Inline "new note" dialog state (Tauri webview doesn't support window.prompt).
  let newNoteOpen = $state(false);
  let newNoteTargetDir = $state<string | undefined>(undefined);
  let newNoteInput = $state('');
  let newNoteError = $state('');
  let newNoteInputEl = $state<HTMLInputElement | null>(null);

  // Daily Record modal state.
  let recordOpen = $state(false);
  let recordInput = $state('');
  let recordError = $state('');
  let recordInputEl = $state<HTMLTextAreaElement | null>(null);

  // Promote modal state — used by both palette "Promote current" and Inbox Review rows.
  let promoteOpen = $state(false);
  let promoteSource = $state<string | null>(null); // the inbox path being promoted
  let promoteInput = $state('');
  let promoteError = $state('');
  let promoteInputEl = $state<HTMLInputElement | null>(null);

  // Unused Attachments modal state — palette `> Find unused attachments`.
  // Selection is kept as a Set of rel_paths; default = "all selected" so the
  // common case (delete everything the tool found) is one click.
  let unusedOpen = $state(false);
  let unusedLoading = $state(false);
  let unusedError = $state('');
  let unusedList = $state<AttachmentInfo[]>([]);
  let unusedSelected = $state<Set<string>>(new Set());
  let unusedDeleting = $state(false);

  // Rename modal state — palette `> Rename current file…`.
  // Two-phase flow: edit target → dry-run preview → confirm execute.
  let renameOpen = $state(false);
  let renameSource = $state<string | null>(null); // the file being renamed
  let renameInput = $state('');
  let renameError = $state('');
  let renamePreviewLoading = $state(false);
  let renameRunning = $state(false);
  let renameInputEl = $state<HTMLInputElement | null>(null);
  let renamePreview = $state<FileRenamePreview | null>(null);

  // Directory rename modal state — palette `> Rename current directory…`.
  // Source is the parent dir of the currently open file. We keep state
  // separate from the file-rename modal so they can coexist in the DOM
  // without risk of cross-mutation.
  let dirRenameOpen = $state(false);
  let dirRenameSource = $state<string | null>(null); // the dir being renamed
  let dirRenameInput = $state('');
  let dirRenameError = $state('');
  let dirRenamePreviewLoading = $state(false);
  let dirRenameRunning = $state(false);
  let dirRenameInputEl = $state<HTMLInputElement | null>(null);
  let dirRenamePreview = $state<DirRenamePreview | null>(null);

  // Sidebar right-click context menu. `entry` identifies the row we're
  // acting on; x/y are viewport coordinates from the `contextmenu` event.
  // We render at `position: fixed` and clamp to the viewport so the menu
  // doesn't spill off the right/bottom edge.
  let ctxMenuOpen = $state(false);
  let ctxMenuEntry = $state<DirEntry | null>(null);
  let ctxMenuX = $state(0);
  let ctxMenuY = $state(0);

  // Build-MOC-from-tag modal. Populated by `runBuildMocFromTag()` with all
  // notes carrying the currently focused tag. The user picks a subset + a
  // title, and `confirmBuildMoc` materialises a `2-moc/<slug>.md` with
  // those notes prewritten as wiki-links.
  //
  // Selection is a Set of rel_paths; default = every note selected so the
  // zero-click path ("turn #tag into a MOC of all its notes") is trivial.
  let mocBuilderOpen = $state(false);
  let mocBuilderTag = $state('');
  let mocBuilderTitle = $state('');
  let mocBuilderError = $state('');
  let mocBuilderLoading = $state(false);
  let mocBuilderRunning = $state(false);
  let mocBuilderList = $state<NoteRef[]>([]);
  let mocBuilderSelected = $state<Set<string>>(new Set());
  let mocBuilderTitleEl = $state<HTMLInputElement | null>(null);

  // Settings modal — palette `> Settings…` / ⌘,.
  //
  // The modal edits three things: theme (system/light/dark), autosave
  // debounce (ms), and triggers the "reseed templates" action. Everything
  // is applied immediately on change (radio click / debounced number input),
  // so there's no "Save" button — the modal is purely a view onto persistent
  // state. That matches the app's editor model ("edits auto-persist").
  let settingsOpen = $state(false);
  let settingsReseedRunning = $state(false);
  let settingsReseedMsg = $state('');

  // ── P3-D2a.2 AI provider config state (Settings modal) ──────────────────
  // These four fields are the editable shape; the form persists on blur or
  // explicit "Save". `aiProviderApiKey` holds the plaintext input *only*
  // while the modal is open — never written to any svelte store, never
  // mirrored to localStorage. The backend routes it straight to the OS
  // keystore in `ai_provider_set_config`.
  let aiProviderKind = $state('openai');
  let aiProviderBaseUrl = $state('https://api.openai.com/v1');
  let aiProviderEmbedModel = $state('text-embedding-3-small');
  /** Chat model identifier. Empty string = chat disabled (embeddings-only). */
  let aiProviderChatModel = $state('gpt-4o-mini');
  let aiProviderApiKey = $state('');
  /** True when an API key is already stored in the OS keystore. */
  let aiProviderHasKey = $state(false);
  /** Last embed-test result (null = never tested, or config changed since). */
  let aiProviderTestState = $state<ProviderTestResult | null>(null);
  /** Last chat-test result, kept separately so both banners can coexist. */
  let aiProviderChatTestState = $state<ChatProviderTestResult | null>(null);
  /** In-flight flag so we can disable the test button + show a spinner. */
  let aiProviderTesting = $state(false);
  /** In-flight flag for the chat-test button (independent from embed-test). */
  let aiProviderChatTesting = $state(false);
  /** In-flight flag for save/clear (separate so test + save don't interlock). */
  let aiProviderSaving = $state(false);

  // Embedding index state (D2a.3a).
  // `embedStats` is populated on demand (openSettings + after embed actions);
  // it's null until then so the UI can show "—" instead of misleading zeros.
  let embedStats = $state<EmbeddingStats | null>(null);
  /** In-flight flag for "embed current note" / "clear all" so their buttons
   *  can disable themselves and we don't overlap provider calls. */
  let embedBusy = $state(false);
  /** Last toast message from an embed action. null = nothing to show. */
  let embedNotice = $state<{ kind: 'ok' | 'err' | 'info'; text: string } | null>(null);
  let embedInitOpen = $state(false);
  let embedInitPreviewLoading = $state(false);
  let embedInitRunning = $state(false);
  let embedInitPreview = $state<VaultEmbedPreview | null>(null);
  let embedInitError = $state('');
  const embedActionBusy = $derived(embedBusy || embedInitPreviewLoading || embedInitRunning);

  // Autosave debounce — how long `onContentChange` waits before firing
  // `fileWrite`. 500ms is the historical default; shorter feels snappier
  // on fast machines but risks thrashing on slow IO. Persisted so the
  // user's choice survives relaunches.
  const AUTOSAVE_KEY = 'mynotes:autosave-ms';
  const AUTOSAVE_MIN = 100;
  const AUTOSAVE_MAX = 5000;
  let autosaveDelayMs = $state<number>(500);
  let shortcutBindings = $state<Record<ShortcutActionId, string>>({ ...DEFAULT_SHORTCUT_BINDINGS });
  let recordingShortcutId = $state<ShortcutActionId | null>(null);
  let settingsShortcutMsg = $state('');

  const paletteCommandHints = $derived.by(() => {
    const hints: Record<string, string> = {};
    for (const commandId of Object.keys(PALETTE_SHORTCUT_ACTIONS)) {
      const actionId = PALETTE_SHORTCUT_ACTIONS[commandId];
      if (!actionId) continue;
      hints[commandId] = formatShortcutDisplay(shortcutBindings[actionId]);
    }
    return hints;
  });

  // App version string shown in the Settings footer. Hardcoded from
  // package.json — we'd need vite-plugin-replace or an env inject to
  // DRY this, which is overkill for a single display string.
  const APP_VERSION = '0.1.0';

  let openRequestSeq = 0;
  let pendingSave: PendingSave | null = null;
  let saveInFlight: Promise<void> | null = null;

  /** Dependencies passed to vault commands so they can drive the UI. */
  const cmdDeps: CommandDeps = {
    refreshTree: () => refreshTree(),
    openFile: async (relPath: string) => {
      const name = relPath.slice(relPath.lastIndexOf('/') + 1);
      await openFile({ name, rel_path: relPath, is_dir: false });
    },
    expandDir: (relPath: string) => {
      if (!expanded.has(relPath)) {
        expanded = new Set([...expanded, relPath]);
      }
    }
  };

  function isTauriRuntime(): boolean {
    return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
  }

  onMount(() => {
    theme = readThemeFromBrowserStorage();
    autosaveDelayMs = readAutosaveFromBrowserStorage();
    shortcutBindings = readShortcutsFromBrowserStorage();
    applyTheme(theme);
    tweaksStore.init();
    const off = installShortcuts();

    // Catch async errors (e.g. a command launched from a button) so they show
    // up in the notice stack instead of disappearing as "Unhandled Promise
    // Rejection".
    const onUnhandled = (e: PromiseRejectionEvent) => {
      pushNotice(errorMessage(e.reason), 'error', 6000);
      console.error('[unhandled rejection]', e.reason);
    };
    window.addEventListener('unhandledrejection', onUnhandled);

    if (isTauriRuntime()) {
      runtimeMode = 'tauri';
      void loadRecentVaults();
      void loadAppConfig();
    } else {
      runtimeMode = 'browser';
      recent = [];
      // Phase 4 Stage 1 — when running under the Playwright preview build
      // (`PUBLIC_E2E=1` + `?e2eMock=1`), seed a fake vault so the
      // welcome screen is bypassed and the chat / settings UI is
      // reachable without invoking any Tauri command. Production
      // bundles drop this entire branch via Vite constant folding.
      if (isE2eMockActive()) {
        bootstrapE2eVault();
        aiEnabled = true;
      }
    }

    return () => {
      clearSaveTimer();
      clearAllNotices();
      off();
      window.removeEventListener('unhandledrejection', onUnhandled);
    };
  });

  /** Write the current theme to <html data-theme>. 'system' removes the attribute. */
  function applyTheme(t: Theme) {
    const root = document.documentElement;
    if (t === 'system') {
      root.removeAttribute('data-theme');
    } else {
      root.setAttribute('data-theme', t);
    }
  }

  /**
   * Set the theme to an explicit value. Persists + applies immediately.
   * Called from the Settings radio, the three palette commands, and the
   * deprecated cycle button (via `cycleTheme`).
   */
  function setTheme(next: Theme) {
    theme = next;
    persistTheme(next);
    applyTheme(next);
  }

  /** Rotate system → light → dark → system. Kept for the status-bar gear. */
  function cycleTheme() {
    const next: Theme = theme === 'system' ? 'light' : theme === 'light' ? 'dark' : 'system';
    setTheme(next);
  }

  /** Persist the autosave debounce. The caller is responsible for clamping. */
  function persistAutosaveDelay(ms: number) {
    try {
      localStorage.setItem(AUTOSAVE_KEY, String(ms));
    } catch {
      /* ignore */
    }
    if (runtimeMode === 'tauri') {
      void appConfigSetAutosaveMs(ms).catch((err) => {
        console.error('Failed to persist autosave delay:', err);
        pushNotice(`保存自动保存延迟失败：${errorMessage(err)}`, 'error');
      });
    }
  }

  /**
   * Open the Settings modal. Resets transient state (reseed message) so
   * re-opening after a previous reseed doesn't show stale "已重置 N 个" text.
   */
  function openSettings() {
    settingsReseedMsg = '';
    settingsReseedRunning = false;
    settingsShortcutMsg = '';
    recordingShortcutId = null;
    aiProviderApiKey = '';
    aiProviderTestState = null;
    aiProviderChatTestState = null;
    embedNotice = null;
    settingsOpen = true;
    // Refresh "key stored?" + embed stats in parallel; both are best-effort.
    void refreshAiProviderHasKey();
    void refreshEmbedStats();
  }

  async function refreshAiProviderHasKey() {
    try {
      aiProviderHasKey = await aiProviderHasApiKey();
    } catch (err) {
      console.error('aiProviderHasApiKey failed:', err);
      aiProviderHasKey = false;
    }
  }

  async function saveAiProvider() {
    if (aiProviderSaving) return;
    aiProviderSaving = true;
    try {
      // Empty apiKey here means "don't touch the stored key" — the backend
      // treats it as a config-only update. The user must type the key
      // explicitly to overwrite it.
      await aiProviderSetConfig(
        aiProviderKind.trim() || 'openai',
        aiProviderBaseUrl.trim(),
        aiProviderEmbedModel.trim(),
        aiProviderChatModel.trim() || null,
        aiProviderApiKey
      );
      if (aiProviderApiKey.length > 0) {
        aiProviderApiKey = '';
      }
      await refreshAiProviderHasKey();
      pushNotice('AI 设置已保存', 'info');
    } catch (err) {
      pushNotice(`保存失败: ${String(err)}`, 'error');
    } finally {
      aiProviderSaving = false;
    }
  }

  async function clearAiProvider() {
    const ok = await ask(
      '这会清除已保存的 AI provider 配置和存储在系统 keychain 里的 API key。继续？',
      { title: '清除 AI 配置', kind: 'warning' }
    );
    if (!ok) return;
    aiProviderSaving = true;
    try {
      await aiProviderClearConfig();
      aiProviderApiKey = '';
      aiProviderTestState = null;
      aiProviderChatTestState = null;
      await refreshAiProviderHasKey();
      pushNotice('AI 配置已清除', 'info');
    } catch (err) {
      pushNotice(`清除失败: ${String(err)}`, 'error');
    } finally {
      aiProviderSaving = false;
    }
  }

  async function testAiProvider() {
    if (aiProviderTesting) return;
    aiProviderTesting = true;
    aiProviderTestState = null;
    try {
      // Pass current form values so the test validates **unsaved** edits.
      // apiKeyOverride: if the user typed a key, use it; otherwise fall
      // back to whatever's stored in keyring (backend handles the lookup).
      aiProviderTestState = await aiProviderTestConnection({
        kind: aiProviderKind.trim() || 'openai',
        baseUrl: aiProviderBaseUrl.trim(),
        embedModel: aiProviderEmbedModel.trim(),
        apiKeyOverride: aiProviderApiKey.length > 0 ? aiProviderApiKey : undefined
      });
    } catch (err) {
      aiProviderTestState = {
        ok: false,
        error_kind: 'other',
        error_message: String(err)
      };
    } finally {
      aiProviderTesting = false;
    }
  }

  /**
   * Chat-side counterpart to `testAiProvider`. Validates the chat
   * endpoint by running a short OK-reply conversation with the current
   * form values. Kept separate from the embed test so users can spot
   * which side of the integration is broken.
   */
  async function testAiProviderChat() {
    if (aiProviderChatTesting) return;
    aiProviderChatTesting = true;
    aiProviderChatTestState = null;
    try {
      aiProviderChatTestState = await aiProviderTestChatConnection({
        kind: aiProviderKind.trim() || 'openai',
        baseUrl: aiProviderBaseUrl.trim(),
        chatModel: aiProviderChatModel.trim(),
        apiKeyOverride: aiProviderApiKey.length > 0 ? aiProviderApiKey : undefined
      });
    } catch (err) {
      aiProviderChatTestState = {
        ok: false,
        error_kind: 'other',
        error_message: String(err)
      };
    } finally {
      aiProviderChatTesting = false;
    }
  }

  // ── Embedding-index helpers (D2a.3a) ───────────────────────────────────────

  /** Fetch aggregate counters from the backend; safe to call with no vault. */
  async function refreshEmbedStats() {
    try {
      embedStats = await aiEmbedStats();
    } catch (err) {
      // Non-fatal — treat as "unknown", don't disrupt Settings.
      console.warn('aiEmbedStats failed', err);
      embedStats = null;
    }
  }

  /**
   * Embed the currently open note. Triggered by the command palette
   * (`> Embed current note`) and by a button in Settings. Safely no-ops
   * when no file is open. Updates `embedStats` afterwards so the Settings
   * counters stay live.
   */
  async function embedCurrentNote() {
    if (embedBusy) return;
    const path = vaultState.openFilePath;
    if (!path || !path.endsWith('.md')) {
      embedNotice = { kind: 'info', text: 'No note open' };
      return;
    }
    embedBusy = true;
    embedNotice = { kind: 'info', text: 'Embedding…' };
    try {
      const result: EmbedNoteResult = await aiEmbedNote(path);
      if (!result.ok || !result.outcome) {
        const failure = result.failure;
        embedNotice = {
          kind: 'err',
          text: formatAiFailureText({
            kind: failure?.kind,
            message: failure?.message,
            retryAfterSecs: failure?.retry_after_secs,
            storeUnchanged: failure?.store_unchanged ?? true
          })
        };
        return;
      }
      const out: EmbedOutcome = result.outcome;
      if (out.skipped === 'up_to_date') {
        embedNotice = { kind: 'info', text: 'Up to date — no embed needed' };
      } else if (out.skipped === 'empty') {
        embedNotice = { kind: 'info', text: 'Note is empty — nothing to embed' };
      } else {
        embedNotice = {
          kind: 'ok',
          text: `Embedded ${out.chunks_embedded} chunk${out.chunks_embedded === 1 ? '' : 's'}${out.tokens_used > 0 ? ` · ${out.tokens_used} tokens` : ''}`
        };
      }
      await refreshEmbedStats();
    } catch (err) {
      const failure = parseAiFailureText(errorMessage(err));
      embedNotice = {
        kind: 'err',
        text: formatAiFailureText({
          kind: failure.kind,
          message: failure.message,
          retryAfterSecs: failure.retryAfterSecs,
          storeUnchanged: true
        })
      };
    } finally {
      embedBusy = false;
    }
  }

  // ── Summarize (P3-D3.3) ───────────────────────────────────────────────
  //
  // Three palette commands share this function. For `clipboard` there is no
  // disk write and no file diff, so the flow is toast-driven. For the two
  // file-modifying targets we open `DiffPreviewModal`, show the proposed
  // body once `aiComplete` resolves, and only write on accept.
  //
  // Cancellation is wired both ways — discarding the modal while a request
  // is still in flight fires `aiCompleteCancel` so the backend drops its
  // cancel-flag entry and returns whatever it had accumulated.
  let summarizeOpen = $state(false);
  let summarizeLoading = $state(false);
  let summarizeCanceling = $state(false);
  let summarizeError = $state<CompleteFailure | null>(null);
  let summarizeStatusNote = $state('');
  let summarizeReply = $state<string | null>(null);
  let summarizeOriginal = $state<string>('');
  let summarizePath = $state<string>('');
  let summarizeTarget = $state<SummarizeTarget>('frontmatter');
  // Not `$state` — this is a transient token used only to route
  // cancellation. Making it reactive would trigger spurious `$effect`
  // invalidations on every new request.
  let summarizeRequestId: string | null = null;

  // The proposed body is purely derived from (reply, target, original).
  // `null` while the reply hasn't arrived maps to `DiffPreviewModal`'s
  // loading state via its `proposed: null` branch.
  const summarizeProposed = $derived.by<string | null>(() => {
    if (summarizeReply === null) return null;
    if (summarizeTarget === 'frontmatter' || summarizeTarget === 'top') {
      return applySummaryToBody(summarizeOriginal, summarizeReply, summarizeTarget);
    }
    return null;
  });

  async function runSummarizeCurrentNote(
    target: 'frontmatter' | 'top' | 'clipboard'
  ): Promise<void> {
    const path = vaultState.openFilePath;
    if (!path || !path.endsWith('.md') || path.startsWith('.mynotes/')) {
      pushNotice('Summarize 仅对 vault 内的 markdown 笔记可用', 'error');
      return;
    }
    if (!aiEnabled) {
      pushNotice('请先在设置中启用 AI 辅助', 'error');
      return;
    }
    // Flush pending autosave so the prompt sees what the user actually
    // typed last, and so the `fileWrite` on accept doesn't race an
    // in-flight autosave on the same path.
    await drainPendingSaves();

    let body: string;
    try {
      body = await fileRead(path);
    } catch (e) {
      pushNotice(`读取失败：${errorMessage(e)}`, 'error');
      return;
    }
    const noteBody = stripFrontmatter(body).trim();
    if (noteBody.length === 0) {
      pushNotice('笔记为空，无可摘要内容', 'error');
      return;
    }

    const { systemPrompt, userPrompt } = buildSummarizePrompt(body);

    if (target === 'clipboard') {
      // No modal: fire-and-forget with toast feedback. We still use
      // a request id so a future "cancel summarize clipboard" keybind
      // could route cancellation the same way as the modal path.
      const requestId = makeSummarizeRequestId();
      pushNotice('AI 正在生成摘要…', 'info', 4000);
      try {
        const res = await aiComplete(requestId, {
          systemPrompt,
          userPrompt,
          temperature: 0.3
        });
        if (!res.ok || !res.reply) {
          const failure = normalizeCompleteFailure(res.failure, 'AI 返回空结果');
          pushNotice(`摘要失败：${failure.message}`, 'error', 6000);
          return;
        }
        // Tauri webviews run in a secure context so `navigator.clipboard`
        // works without a plugin. Fallback path is documented as a gap.
        await navigator.clipboard.writeText(res.reply);
        pushNotice('摘要已复制到剪贴板', 'success');
      } catch (e) {
        const failure = normalizeCompleteFailure({ kind: 'other', message: errorMessage(e) });
        pushNotice(`摘要失败：${failure.message}`, 'error', 6000);
      }
      return;
    }

    // File-modifying targets — open the diff modal first so the user
    // sees the loading spinner immediately, then kick off the request.
    summarizeOriginal = body;
    summarizePath = path;
    summarizeTarget = target;
    summarizeError = null;
    summarizeStatusNote = '';
    summarizeReply = null;
    summarizeLoading = true;
    summarizeCanceling = false;
    summarizeOpen = true;

    const requestId = makeSummarizeRequestId();
    summarizeRequestId = requestId;

    try {
      const res = await aiComplete(requestId, {
        systemPrompt,
        userPrompt,
        temperature: 0.3
      });
      // Only apply results to state when this request is still the
      // "current" one — if the user discarded the modal and re-ran
      // while we were awaiting, the stale reply would overwrite the
      // new request's state without this guard.
      if (summarizeRequestId !== requestId) return;
      if (!res.ok || !res.reply) {
        summarizeError = normalizeCompleteFailure(res.failure, 'AI 返回空结果');
        summarizeStatusNote = '';
      } else {
        summarizeReply = res.reply;
        summarizeError = null;
        summarizeStatusNote = res.cancelled ? partialResultNote('summary') : '';
      }
    } catch (e) {
      if (summarizeRequestId !== requestId) return;
      summarizeError = normalizeCompleteFailure({ kind: 'other', message: errorMessage(e) });
      summarizeStatusNote = '';
    } finally {
      if (summarizeRequestId === requestId) {
        summarizeLoading = false;
        summarizeCanceling = false;
        summarizeRequestId = null;
      }
    }
  }

  function closeSummarize() {
    summarizeOpen = false;
    summarizeReply = null;
    summarizeError = null;
    summarizeStatusNote = '';
    summarizeOriginal = '';
    summarizePath = '';
    summarizeLoading = false;
    summarizeCanceling = false;
  }

  async function cancelSummarizeInFlight() {
    const rid = summarizeRequestId;
    if (!rid || summarizeCanceling) return;
    summarizeCanceling = true;
    try {
      await aiCompleteCancel(rid);
    } catch (e) {
      console.warn('[summarize] cancel failed:', e);
      summarizeRequestId = null;
      summarizeLoading = false;
      summarizeCanceling = false;
      summarizeError = normalizeCompleteFailure({
        kind: 'other',
        message: `取消失败：${errorMessage(e)}`
      });
      summarizeStatusNote = '';
    }
  }

  async function retrySummarize(): Promise<void> {
    const target = summarizeTarget;
    closeSummarize();
    await runSummarizeCurrentNote(target);
  }

  async function applySummarize(): Promise<void> {
    const path = summarizePath;
    const newBody = summarizeProposed;
    if (!path || newBody == null) return;
    try {
      await fileWrite(path, newBody);
      // If this is the currently open file, force the editor to reload
      // the on-disk content — same pattern as `runSetProjectStatus`:
      // the watcher reindexes SQLite but does not push body changes
      // back into the editor buffer.
      if (vaultState.openFilePath === path) {
        try {
          const fresh = await fileRead(path);
          editorContent = fresh;
          pendingSave = null;
        } catch (readErr) {
          console.warn('[summarize] editor reload failed:', readErr);
        }
      }
      pushNotice(
        summarizeTarget === 'frontmatter' ? '摘要已写入 frontmatter.summary' : '摘要已插入到文首',
        'success'
      );
      closeSummarize();
    } catch (e) {
      pushNotice(`写入失败：${errorMessage(e)}`, 'error', 6000);
    }
  }

  // ── Suggest tags (P3-D3.4) ────────────────────────────────────────────
  //
  // Shape-wise this mirrors the summarize flow (loading / error / body,
  // cancellable via request-id), but the body is checkbox-driven (see
  // `TagSuggestModal.svelte`) so no `DiffPreviewModal` here. Accept
  // receives the final merged tag list from the modal and writes it back
  // via `mergeTagsIntoFrontmatter`.
  let suggestTagsOpen = $state(false);
  let suggestTagsLoading = $state(false);
  let suggestTagsCanceling = $state(false);
  let suggestTagsError = $state<CompleteFailure | null>(null);
  let suggestTagsStatusNote = $state('');
  /** `null` while the reply is in flight; `string[]` once parsed. An empty
   *  array is a valid "loaded" state (AI returned no usable tags). */
  let suggestTagsCandidates = $state<string[] | null>(null);
  let suggestTagsExisting = $state<string[]>([]);
  let suggestTagsVault = $state<string[]>([]);
  let suggestTagsOriginal = $state<string>('');
  let suggestTagsPath = $state<string>('');
  // Non-reactive — stale-request guard token; making it `$state` would
  // re-run derived consumers on every new request for no gain.
  let suggestTagsRequestId: string | null = null;

  async function runSuggestTagsForCurrentNote(): Promise<void> {
    const path = vaultState.openFilePath;
    if (!path || !path.endsWith('.md') || path.startsWith('.mynotes/')) {
      pushNotice('Suggest tags 仅对 vault 内的 markdown 笔记可用', 'error');
      return;
    }
    if (!aiEnabled) {
      pushNotice('请先在设置中启用 AI 辅助', 'error');
      return;
    }
    await drainPendingSaves();

    let body: string;
    try {
      body = await fileRead(path);
    } catch (e) {
      pushNotice(`读取失败：${errorMessage(e)}`, 'error');
      return;
    }
    const noteBody = stripFrontmatter(body).trim();
    if (noteBody.length === 0) {
      pushNotice('笔记为空，无可分析内容', 'error');
      return;
    }

    // Load the vault taxonomy in parallel with opening the modal — we
    // need the names list both for the prompt and for the modal's badge
    // rendering. `indexTags` is cheap (a single SQLite group-by) so a
    // bounded await here is acceptable before flipping the loader on.
    let vaultTagNames: string[] = [];
    try {
      const vt = await indexTags();
      vaultTagNames = vt.map((x) => x.tag);
    } catch (e) {
      // Non-fatal: without the vault list the prompt just gets "(none)"
      // and the modal renders every candidate with the `新建` badge.
      console.warn('[suggest-tags] indexTags failed:', e);
    }

    const existing = parseExistingTags(body);
    const { systemPrompt, userPrompt } = buildSuggestTagsPrompt({
      body,
      existingTags: existing,
      vaultTags: vaultTagNames
    });

    suggestTagsOriginal = body;
    suggestTagsPath = path;
    suggestTagsExisting = existing;
    suggestTagsVault = vaultTagNames;
    suggestTagsError = null;
    suggestTagsStatusNote = '';
    suggestTagsCandidates = null;
    suggestTagsLoading = true;
    suggestTagsCanceling = false;
    suggestTagsOpen = true;

    const requestId = makeSuggestTagsRequestId();
    suggestTagsRequestId = requestId;

    try {
      const res = await aiComplete(requestId, {
        systemPrompt,
        userPrompt,
        // Low temperature — tag curation is a convergent task, we don't
        // want creative re-interpretations of the vault taxonomy.
        temperature: 0.2
      });
      if (suggestTagsRequestId !== requestId) return;
      if (!res.ok || !res.reply) {
        suggestTagsError = normalizeCompleteFailure(res.failure, 'AI 返回空结果');
        suggestTagsStatusNote = '';
      } else {
        suggestTagsCandidates = parseSuggestedTags(res.reply);
        suggestTagsError = null;
        suggestTagsStatusNote = res.cancelled ? partialResultNote('tags') : '';
      }
    } catch (e) {
      if (suggestTagsRequestId !== requestId) return;
      suggestTagsError = normalizeCompleteFailure({ kind: 'other', message: errorMessage(e) });
      suggestTagsStatusNote = '';
    } finally {
      if (suggestTagsRequestId === requestId) {
        suggestTagsLoading = false;
        suggestTagsCanceling = false;
        suggestTagsRequestId = null;
      }
    }
  }

  function closeSuggestTags() {
    suggestTagsOpen = false;
    suggestTagsCandidates = null;
    suggestTagsError = null;
    suggestTagsStatusNote = '';
    suggestTagsExisting = [];
    suggestTagsOriginal = '';
    suggestTagsPath = '';
    suggestTagsLoading = false;
    suggestTagsCanceling = false;
  }

  async function cancelSuggestTagsInFlight() {
    const rid = suggestTagsRequestId;
    if (!rid || suggestTagsCanceling) return;
    suggestTagsCanceling = true;
    try {
      await aiCompleteCancel(rid);
    } catch (e) {
      console.warn('[suggest-tags] cancel failed:', e);
      suggestTagsRequestId = null;
      suggestTagsLoading = false;
      suggestTagsCanceling = false;
      suggestTagsError = normalizeCompleteFailure({
        kind: 'other',
        message: `取消失败：${errorMessage(e)}`
      });
      suggestTagsStatusNote = '';
    }
  }

  async function retrySuggestTags(): Promise<void> {
    closeSuggestTags();
    await runSuggestTagsForCurrentNote();
  }

  async function applySuggestTags(finalTags: string[]): Promise<void> {
    const path = suggestTagsPath;
    const original = suggestTagsOriginal;
    if (!path || !original) return;
    // `mergeTagsIntoFrontmatter` is idempotent over finalTags vs existing
    // tags: the modal already emits the full intended list (existing ∪
    // newly-picked ∪ …), so the merge just normalises and rewrites the
    // `tags:` line; the rest of the frontmatter + body stays untouched.
    const newBody = mergeTagsIntoFrontmatter(original, finalTags);
    try {
      await fileWrite(path, newBody);
      if (vaultState.openFilePath === path) {
        try {
          const fresh = await fileRead(path);
          editorContent = fresh;
          pendingSave = null;
        } catch (readErr) {
          console.warn('[suggest-tags] editor reload failed:', readErr);
        }
      }
      pushNotice(`已更新 frontmatter.tags（${finalTags.length} 个标签）`, 'success');
      closeSuggestTags();
    } catch (e) {
      pushNotice(`写入失败：${errorMessage(e)}`, 'error', 6000);
    }
  }

  // ── Draft MOC (P3-D3.5) ───────────────────────────────────────────────
  //
  // The non-AI `runBuildMocFromTag` modal stays the sole picker — the AI
  // fork only intercepts at its confirm step. Flow:
  //   1. User opens the mocBuilder modal (palette command `build-moc-from-tag`
  //      OR `draft-moc-from-tag` — both open the same picker).
  //   2. User picks notes + title.
  //   3. On "用 AI 草拟…" button: close picker → open DiffPreviewModal in
  //      loading; aiComplete generates themed markdown; sanitised reply is
  //      shown as the diff-modal `proposed` against the flat baseline as
  //      `original` (i.e. what `buildMocFromTag` would emit without AI).
  //   4. Accept → reuse `buildMocFromTag(..., entriesMarkdown: aiGrouped)`
  //      so everything downstream (template materialisation, frontmatter
  //      stamp, panel refresh, file open) matches the non-AI path exactly.
  //
  // `draftMocPicked` is captured at fork-time (snapshot of the mocBuilder
  // selection) so closing the picker doesn't race the AI call.
  let draftMocOpen = $state(false);
  let draftMocLoading = $state(false);
  let draftMocCanceling = $state(false);
  let draftMocError = $state<CompleteFailure | null>(null);
  let draftMocStatusNote = $state('');
  let draftMocReply = $state<string | null>(null);
  let draftMocTag = $state('');
  let draftMocTitle = $state('');
  let draftMocPicked = $state<NoteRef[]>([]);
  /** Flat `- [[title]]` baseline used as the diff "before" text. */
  let draftMocFlat = $state('');
  // Non-reactive — stale-request guard token.
  let draftMocRequestId: string | null = null;

  const draftMocSanitized = $derived.by(() => {
    if (draftMocReply === null) return null;
    const allowed = draftMocPicked.map((n) => n.title ?? pathStem(n.path));
    return sanitizeDraftMoc(draftMocReply, allowed);
  });

  /** The "after" markdown shown in DiffPreviewModal: the AI-grouped entries
   *  block. Same shape as `original` (both are `entriesMarkdown`, not full
   *  note bodies) so the LCS diff highlights the reorganisation clearly. */
  const draftMocProposed = $derived.by<string | null>(() =>
    draftMocSanitized ? draftMocSanitized.markdown : null
  );

  function pathStem(p: string): string {
    return p.replace(/\.md$/, '').split('/').pop() ?? p;
  }

  /** Palette entry for `> Draft MOC from tag (AI)`. Just routes through
   *  the existing mocBuilder picker — the AI fork is a button in that
   *  modal (see `confirmBuildMocWithAi`). Keeping one picker avoids
   *  duplicating the tag / title / notes inputs. */
  async function runDraftMocFromTagAi(): Promise<void> {
    if (!aiEnabled) {
      pushNotice('请先在设置中启用 AI 辅助', 'error');
      return;
    }
    if (!activeTag) {
      pushNotice('请先在侧栏选中一个标签', 'error');
      return;
    }
    await runBuildMocFromTag();
  }

  /** Fork at the mocBuilder confirm step. Snapshots the picker state,
   *  closes the picker, then opens DiffPreviewModal + kicks off the AI
   *  call. Placed alongside `confirmBuildMoc` so the two fork paths are
   *  visually adjacent in the modal footer. */
  async function confirmBuildMocWithAi(): Promise<void> {
    if (mocBuilderRunning) return;
    if (!aiEnabled) {
      pushNotice('请先在设置中启用 AI 辅助', 'error');
      return;
    }
    const title = mocBuilderTitle.trim();
    if (!title) {
      mocBuilderError = '标题不能为空';
      return;
    }
    const picked = mocBuilderList.filter((n) => mocBuilderSelected.has(n.path));
    if (picked.length === 0) {
      mocBuilderError = 'AI 草拟需要至少一条笔记';
      return;
    }
    mocBuilderOpen = false;
    mocBuilderList = [];
    mocBuilderSelected = new Set();
    await startDraftMocAi(mocBuilderTag, title, picked);
  }

  async function startDraftMocAi(tag: string, title: string, picked: NoteRef[]): Promise<void> {
    draftMocTag = tag;
    draftMocTitle = title;
    draftMocPicked = picked;
    draftMocFlat = buildFlatEntriesMarkdown(picked);
    draftMocError = null;
    draftMocStatusNote = '';
    draftMocReply = null;
    draftMocLoading = true;
    draftMocCanceling = false;
    draftMocOpen = true;

    const { systemPrompt, userPrompt } = buildDraftMocPrompt({
      tag: draftMocTag,
      title: draftMocTitle,
      notes: picked
    });

    const requestId = makeDraftMocRequestId();
    draftMocRequestId = requestId;

    try {
      const res = await aiComplete(requestId, {
        systemPrompt,
        userPrompt,
        // Moderate temperature — grouping benefits from *some* creativity
        // (theme naming) but too much and the model invents titles
        // despite the allowlist.
        temperature: 0.4
      });
      if (draftMocRequestId !== requestId) return;
      if (!res.ok || !res.reply) {
        draftMocError = normalizeCompleteFailure(res.failure, 'AI 返回空结果');
        draftMocStatusNote = '';
      } else {
        draftMocReply = res.reply;
        draftMocError = null;
        draftMocStatusNote = res.cancelled ? partialResultNote('moc') : '';
      }
    } catch (e) {
      if (draftMocRequestId !== requestId) return;
      draftMocError = normalizeCompleteFailure({ kind: 'other', message: errorMessage(e) });
      draftMocStatusNote = '';
    } finally {
      if (draftMocRequestId === requestId) {
        draftMocLoading = false;
        draftMocCanceling = false;
        draftMocRequestId = null;
      }
    }
  }

  function closeDraftMoc() {
    draftMocOpen = false;
    draftMocReply = null;
    draftMocError = null;
    draftMocStatusNote = '';
    draftMocTag = '';
    draftMocTitle = '';
    draftMocPicked = [];
    draftMocFlat = '';
    draftMocLoading = false;
    draftMocCanceling = false;
  }

  async function cancelDraftMocInFlight() {
    const rid = draftMocRequestId;
    if (!rid || draftMocCanceling) return;
    draftMocCanceling = true;
    try {
      await aiCompleteCancel(rid);
    } catch (e) {
      console.warn('[draft-moc] cancel failed:', e);
      draftMocRequestId = null;
      draftMocLoading = false;
      draftMocCanceling = false;
      draftMocError = normalizeCompleteFailure({
        kind: 'other',
        message: `取消失败：${errorMessage(e)}`
      });
      draftMocStatusNote = '';
    }
  }

  async function retryDraftMoc(): Promise<void> {
    const tag = draftMocTag;
    const title = draftMocTitle;
    const picked = draftMocPicked;
    if (!tag || !title || picked.length === 0) return;
    await startDraftMocAi(tag, title, picked);
  }

  async function applyDraftMoc(): Promise<void> {
    const title = draftMocTitle;
    const tag = draftMocTag;
    const picked = draftMocPicked;
    const entriesMarkdown = draftMocProposed;
    if (!title || !tag || picked.length === 0 || !entriesMarkdown) return;
    try {
      const { dstPath, insertedCount, strategy } = await buildMocFromTag(cmdDeps, {
        tag,
        title,
        noteRefs: picked,
        entriesMarkdown
      });
      // Mirror the post-create bookkeeping from `confirmBuildMoc` so AI and
      // non-AI paths behave identically once the file is on disk.
      invalidateWikiCompletionCache();
      schedulePanelRefresh(200);
      graphRefreshToken += 1;
      const sanitized = draftMocSanitized;
      const droppedCount = sanitized
        ? Math.max(0, picked.length - new Set(sanitized.linkedTitles).size)
        : 0;
      if (strategy === 'none') {
        // No dedicated "warning" kind in the notice system — route to
        // error (red) for attention, but with a longer TTL so the user
        // can read the compound message.
        pushNotice(`已创建 ${dstPath}，但模板缺少插入锚点，entries 未注入`, 'error', 7000);
      } else if (droppedCount > 0) {
        pushNotice(
          `已创建 ${dstPath}（${insertedCount} 条；AI 漏掉 ${droppedCount} 条，已标注为注释）`,
          'error',
          7000
        );
      } else {
        pushNotice(`已创建 ${dstPath}（AI 分组 · ${insertedCount} 条）`, 'success');
      }
      closeDraftMoc();
    } catch (e) {
      pushNotice(`创建失败：${errorMessage(e)}`, 'error', 6000);
    }
  }

  /** Wipe every chunk in the store. Settings-only button — palette has no
   *  equivalent to avoid accidental mass-deletion via fuzzy match. */
  async function clearAllEmbeddings() {
    if (embedBusy) return;
    const confirmed = window.confirm('确定清空所有 embedding 向量吗？笔记本身不会被删除。');
    if (!confirmed) return;
    embedBusy = true;
    embedNotice = null;
    try {
      const n = await aiEmbedClearAll();
      embedNotice = { kind: 'ok', text: `已清空 ${n} 个 chunks` };
      await refreshEmbedStats();
    } catch (err) {
      embedNotice = { kind: 'err', text: `清空失败: ${err}` };
    } finally {
      embedBusy = false;
    }
  }

  function embedPreviewCostText(preview: VaultEmbedPreview): string {
    switch (preview.cost_estimate_kind) {
      case 'local':
        return '当前 provider 看起来是本地地址，按 $0 估算';
      case 'open_ai_public_pricing':
        return preview.cost_usd_estimate == null
          ? '未能估算成本'
          : `按 ${preview.model ?? '当前模型'} 的 OpenAI 官方 embedding 单价估算：≈ ${formatUsdEstimate(preview.cost_usd_estimate)}`;
      default:
        return '当前 provider / model 没有内置单价映射，成本未知';
    }
  }

  function closeEmbedInitModal() {
    if (embedInitRunning) return;
    embedInitOpen = false;
    embedInitPreview = null;
    embedInitError = '';
  }

  async function previewEmbedVaultInit() {
    if (embedActionBusy) return;
    embedInitPreviewLoading = true;
    embedInitError = '';
    try {
      embedInitPreview = await aiEmbedVaultPreview();
      embedInitOpen = true;
    } catch (err) {
      embedNotice = { kind: 'err', text: `初始化预估失败: ${errorMessage(err)}` };
    } finally {
      embedInitPreviewLoading = false;
    }
  }

  function summarizeVaultEmbedRun(result: VaultEmbedRunResult): string {
    const base = joinNotice([
      `已写入 ${result.note_count_embedded} 篇`,
      result.chunk_count_embedded > 0 ? `${result.chunk_count_embedded} chunks` : null,
      result.token_count_used > 0 ? `${result.token_count_used} tokens` : null,
      result.note_count_up_to_date > 0 ? `跳过 ${result.note_count_up_to_date} 篇最新` : null,
      result.note_count_empty > 0 ? `${result.note_count_empty} 篇空笔记` : null,
      result.note_count_failed > 0 ? `失败 ${result.note_count_failed} 篇` : null,
      result.note_count_not_attempted > 0 ? `未尝试 ${result.note_count_not_attempted} 篇` : null
    ]);
    if (result.aborted_early) {
      return joinNotice([
        `初始化已中止：${base}`,
        formatAiFailureText({
          kind: result.aborted_error_kind,
          message: result.aborted_error_message,
          retryAfterSecs: result.aborted_retry_after_secs,
          storeUnchanged: true
        })
      ]);
    }
    if (result.failure_preview.length === 0) return `初始化完成：${base}`;
    return `初始化完成：${base}（例如 ${result.failure_preview[0]}）`;
  }

  async function runEmbedVaultInit() {
    if (!embedInitPreview || embedInitRunning) return;
    embedInitRunning = true;
    embedInitError = '';
    try {
      const result = await aiEmbedVaultRun();
      await refreshEmbedStats();
      embedNotice = {
        kind: result.note_count_failed > 0 || result.aborted_early ? 'err' : 'ok',
        text: summarizeVaultEmbedRun(result)
      };
      embedInitOpen = false;
      embedInitPreview = null;
    } catch (err) {
      const failure = parseAiFailureText(errorMessage(err));
      embedInitError = `初始化失败: ${formatAiFailureText({
        kind: failure.kind,
        message: failure.message,
        retryAfterSecs: failure.retryAfterSecs,
        storeUnchanged: true
      })}`;
    } finally {
      embedInitRunning = false;
    }
  }

  function onEmbedInitKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeEmbedInitModal();
      return;
    }
    if (e.key === 'Enter' && embedInitPreview && embedInitPreview.note_count_to_embed > 0) {
      e.preventDefault();
      void runEmbedVaultInit();
    }
  }

  async function closeSettings() {
    // Rescue dangling API key: if the user pasted a key into the AI
    // provider input but didn't click "保存" before closing, auto-save
    // it so the key doesn't silently vanish on next openSettings()
    // (which resets `aiProviderApiKey = ''` for memory-hygiene reasons).
    // Other AI provider fields (base URL / models) survive as in-memory
    // $state across modal re-opens, so only the key needs rescue here.
    // If save fails, `saveAiProvider` already toasts the error and
    // leaves `aiProviderApiKey` populated — we keep the modal open so
    // the user can retry or copy the key out before losing it.
    if (aiProviderApiKey.length > 0 && !aiProviderSaving) {
      await saveAiProvider();
      if (aiProviderApiKey.length > 0) return;
    }
    recordingShortcutId = null;
    closeEmbedInitModal();
    settingsOpen = false;
  }

  /**
   * Reseed templates from within the Settings modal. Prompts for confirm
   * because this overwrites user-edited template files. Result is shown
   * in the modal — not the status bar — so the user sees it in context.
   */
  async function runReseedFromSettings() {
    if (settingsReseedRunning) return;
    const ok = await ask(
      '这会用内置模板覆盖 templates/ 下同名的模板文件。你自己新增的模板不会被动到。继续？',
      { title: '重置模板', kind: 'warning' }
    );
    if (!ok) return;
    settingsReseedRunning = true;
    settingsReseedMsg = '';
    try {
      const { added, updated, unchanged } = await vaultReseedTemplates();
      settingsReseedMsg = `新增 ${added.length} · 覆盖 ${updated.length} · 未改动 ${unchanged.length}`;
    } catch (err) {
      settingsReseedMsg = `失败: ${String(err)}`;
    } finally {
      settingsReseedRunning = false;
    }
  }

  /**
   * Handle the autosave-delay number input. Browser `<input type=number>`
   * fires `input` on every keystroke — we clamp and persist here but
   * don't re-apply to any in-flight timer; the next `onContentChange`
   * will pick up the new value naturally.
   */
  function onAutosaveDelayInput(e: Event) {
    const target = e.target as HTMLInputElement;
    const n = Number(target.value);
    if (!Number.isFinite(n)) return;
    const clamped = Math.max(AUTOSAVE_MIN, Math.min(AUTOSAVE_MAX, Math.round(n)));
    autosaveDelayMs = clamped;
    persistAutosaveDelay(clamped);
  }

  function clampAutosave(ms: number | null | undefined): number {
    if (typeof ms !== 'number' || !Number.isFinite(ms)) return 500;
    return Math.max(AUTOSAVE_MIN, Math.min(AUTOSAVE_MAX, Math.round(ms)));
  }

  function readThemeFromBrowserStorage(): Theme {
    try {
      const saved = localStorage.getItem(THEME_KEY) as Theme | null;
      if (saved === 'light' || saved === 'dark' || saved === 'system') {
        return saved;
      }
    } catch {
      /* ignore */
    }
    return 'system';
  }

  function readAutosaveFromBrowserStorage(): number {
    try {
      const raw = localStorage.getItem(AUTOSAVE_KEY);
      if (raw !== null) return clampAutosave(Number(raw));
    } catch {
      /* ignore */
    }
    return 500;
  }

  function readShortcutsFromBrowserStorage(): Record<ShortcutActionId, string> {
    try {
      const raw = localStorage.getItem(SHORTCUTS_KEY);
      if (!raw) return { ...DEFAULT_SHORTCUT_BINDINGS };
      return mergeShortcutBindings(
        JSON.parse(raw) as Partial<Record<ShortcutActionId, string>> | null
      );
    } catch {
      return { ...DEFAULT_SHORTCUT_BINDINGS };
    }
  }

  function persistTheme(next: Theme) {
    try {
      localStorage.setItem(THEME_KEY, next);
    } catch {
      /* ignore */
    }
    if (runtimeMode === 'tauri') {
      void appConfigSetTheme(next).catch((err) => {
        console.error('Failed to persist theme:', err);
        pushNotice(`保存主题失败：${errorMessage(err)}`, 'error');
      });
    }
  }

  function persistShortcutBindings(next: Record<ShortcutActionId, string>) {
    try {
      localStorage.setItem(SHORTCUTS_KEY, JSON.stringify(next));
    } catch {
      /* ignore */
    }
    if (runtimeMode === 'tauri') {
      void appConfigSetShortcuts(next).catch((err) => {
        console.error('Failed to persist shortcuts:', err);
        pushNotice(`保存快捷键失败：${errorMessage(err)}`, 'error');
      });
    }
  }

  async function loadAppConfig() {
    try {
      const snapshot = await appConfigGet();
      const nextTheme = snapshot.theme ?? readThemeFromBrowserStorage();
      const nextAutosaveMs = snapshot.autosave_ms ?? readAutosaveFromBrowserStorage();
      const nextShortcuts =
        Object.keys(snapshot.shortcuts ?? {}).length > 0
          ? mergeShortcutBindings(snapshot.shortcuts)
          : readShortcutsFromBrowserStorage();

      theme = nextTheme;
      autosaveDelayMs = clampAutosave(nextAutosaveMs);
      shortcutBindings = nextShortcuts;
      // ai_enabled: null means "not set" → default true.
      aiEnabled = snapshot.ai_enabled ?? true;
      aiToolPermissions = snapshot.ai_tool_permissions ?? aiToolPermissions;
      // AI provider config (D2a.2). Non-null means the user has saved at
      // least once — populate the form with the saved values so the
      // Settings modal reflects reality instead of defaults.
      if (snapshot.ai_provider) {
        aiProviderKind = snapshot.ai_provider.kind || 'openai';
        aiProviderBaseUrl = snapshot.ai_provider.base_url || aiProviderBaseUrl;
        aiProviderEmbedModel = snapshot.ai_provider.embed_model || aiProviderEmbedModel;
        // chat_model is optional — an empty persisted value keeps the
        // form's default suggestion visible rather than wiping the input.
        if (snapshot.ai_provider.chat_model) {
          aiProviderChatModel = snapshot.ai_provider.chat_model;
        }
      }
      applyTheme(nextTheme);

      persistTheme(nextTheme);
      persistAutosaveDelay(autosaveDelayMs);
      persistShortcutBindings(nextShortcuts);
    } catch (err) {
      console.error('Failed to load app config:', err);
    }
  }

  async function persistAiToolPermissions(next: AiToolPermissions) {
    aiToolPermissions = next;
    try {
      const snapshot = await appConfigSetAiToolPermissions(next);
      aiToolPermissions = snapshot.ai_tool_permissions ?? next;
    } catch (err) {
      console.error('Failed to persist ai_tool_permissions:', err);
      pushNotice(`保存 AI 工具权限失败：${errorMessage(err)}`, 'error');
      void loadAppConfig();
    }
  }

  function updateAiToolPermissionField(
    key: keyof AiToolPermissions,
    checked: boolean
  ): void {
    const next = { ...aiToolPermissions, [key]: checked };
    void persistAiToolPermissions(next);
  }

  function startShortcutCapture(actionId: ShortcutActionId) {
    recordingShortcutId = actionId;
    settingsShortcutMsg = '按下新的组合键；Esc 取消。';
  }

  function resetShortcutBinding(actionId: ShortcutActionId) {
    const next = { ...shortcutBindings, [actionId]: DEFAULT_SHORTCUT_BINDINGS[actionId] };
    shortcutBindings = next;
    recordingShortcutId = null;
    settingsShortcutMsg = `${shortcutActionDefs[actionId].label} 已恢复默认`;
    persistShortcutBindings(next);
  }

  function shortcutLabel(actionId: ShortcutActionId): string {
    return formatShortcutDisplay(shortcutBindings[actionId]);
  }

  $effect(() => {
    const actionId = recordingShortcutId;
    if (!actionId) return;

    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Meta' || e.key === 'Control' || e.key === 'Alt' || e.key === 'Shift') {
        return;
      }
      e.preventDefault();
      e.stopPropagation();

      if (e.key === 'Escape') {
        recordingShortcutId = null;
        settingsShortcutMsg = '已取消快捷键录入';
        return;
      }

      const parsed = shortcutFromKeyboardEvent(e);
      if (!parsed) {
        settingsShortcutMsg = '请按包含 ⌘/Ctrl 或 Alt 的组合键';
        return;
      }

      const conflict = findShortcutConflict(shortcutBindings, actionId, parsed.accelerator);
      if (conflict) {
        settingsShortcutMsg = `与“${shortcutActionDefs[conflict].label}”冲突`;
        return;
      }

      const next = { ...shortcutBindings, [actionId]: parsed.accelerator };
      shortcutBindings = next;
      recordingShortcutId = null;
      settingsShortcutMsg = `${shortcutActionDefs[actionId].label} → ${parsed.display}`;
      persistShortcutBindings(next);
    };

    window.addEventListener('keydown', handler, true);
    return () => window.removeEventListener('keydown', handler, true);
  });

  // ---------------------------------------------------------------------------
  // Export commands — zip the vault, copy a single note, or print → PDF.
  //
  // The zip path is the "real" export: ships the whole vault minus the
  // derived `.mynotes/` folder so the recipient can reindex on their side.
  // Single-note `.md` copy is a convenience for sharing one note via email /
  // chat without dragging the whole vault. Print renders the note to a
  // standalone HTML file (pulldown-cmark on the Rust side) and opens it in
  // the system default browser — the user then uses the browser's native
  // `⌘P` → "Save as PDF" to get a PDF without shipping a PDF lib, and we
  // sidestep both CodeMirror's viewport virtualization and Tauri WKWebView's
  // programmatic `window.print()` being silently dropped.

  async function runExportVaultZip() {
    if (!vaultState.current) {
      pushNotice('请先打开一个 vault', 'error');
      return;
    }
    const vaultName = vaultState.current.path.split('/').pop() ?? 'vault';
    const stamp = formatDate(new Date(), 'YYYY-MM-DD');
    const suggested = `${vaultName}-${stamp}.zip`;
    let dest: string | null;
    try {
      dest = await saveDialog({
        title: '导出 vault 为 zip',
        defaultPath: suggested,
        filters: [{ name: 'Zip archive', extensions: ['zip'] }]
      });
    } catch (err) {
      pushNotice(`export dialog: ${errorMessage(err)}`, 'error');
      return;
    }
    if (!dest) return; // user cancelled
    try {
      const summary: ExportSummary = await exportVaultZip(dest);
      pushNotice(
        `已导出 ${summary.file_count} 个文件（${formatBytes(summary.bytes_written)}，跳过 ${summary.skipped_count}）`,
        'success'
      );
    } catch (err) {
      pushNotice(`export zip: ${errorMessage(err)}`, 'error');
    }
  }

  async function runExportCurrentNote() {
    const path = vaultState.openFilePath;
    if (!path || !path.endsWith('.md')) {
      pushNotice('请先打开一个 .md 文件', 'error');
      return;
    }
    // Flush any pending edits first — we export the on-disk bytes, not the
    // editor buffer, so the copy would otherwise lag behind by one debounce.
    await drainPendingSaves();
    const suggested = path.slice(path.lastIndexOf('/') + 1);
    let dest: string | null;
    try {
      dest = await saveDialog({
        title: '导出当前笔记',
        defaultPath: suggested,
        filters: [{ name: 'Markdown', extensions: ['md'] }]
      });
    } catch (err) {
      pushNotice(`export dialog: ${errorMessage(err)}`, 'error');
      return;
    }
    if (!dest) return;
    try {
      // Copy on the Rust side so we don't need the `fs` JS plugin. The
      // Rust command reads via the vault's resolved-path logic and writes
      // directly to the absolute path from the save dialog.
      await noteExportCopy(path, dest);
      pushNotice(`已导出 ${dest.split(/[\\/]/).pop() ?? dest}`, 'success');
    } catch (err) {
      pushNotice(`export note: ${errorMessage(err)}`, 'error');
    }
  }

  /**
   * Render the current note to a self-contained HTML file and open it
   * in the system default browser. User then runs the browser's native
   * `⌘P` / `Ctrl+P` → "Save as PDF".
   *
   * Why not `window.print()` directly — on Tauri macOS WKWebView a
   * programmatic `window.print()` is silently dropped (no dialog ever
   * appears), and even when it does fire, CodeMirror's viewport
   * virtualization means print only captures the on-screen slice of
   * the document. Rendering to static HTML sidesteps both problems.
   *
   * The Rust command also drains pending saves implicitly by reading
   * the on-disk bytes — but the editor buffer may have unflushed
   * edits, so we `drainPendingSaves()` first.
   */
  async function runPrintCurrentNote() {
    const path = vaultState.openFilePath;
    if (!path || !path.endsWith('.md')) {
      pushNotice('请先打开一个 .md 文件', 'error');
      return;
    }
    await drainPendingSaves();
    try {
      // P3-A7: thread the current theme through so the preview carries
      // the same light/dark decision as the editor. `theme === 'system'`
      // lets the browser's `prefers-color-scheme` decide at preview time.
      const previewPath = await noteRenderPrintHtml(path, theme);
      const previewName = previewPath.split(/[\\/]/).pop() ?? previewPath;
      pushNotice(
        `已在浏览器打开预览（${previewName}）。在浏览器中按 ⌘P / Ctrl+P 保存为 PDF`,
        'info',
        5200
      );
    } catch (err) {
      pushNotice(`print preview: ${errorMessage(err)}`, 'error');
    }
  }

  /** Format a byte count as KB / MB. Returns "1.2 MB" etc. No i18n. */
  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  /** Close the currently open file, returning to the Home view. */
  function goHome() {
    invalidateOpenRequests();
    vaultState.closeFile();
    editorContent = '';
    saveStatus = 'idle';
    saveError = '';
    activeTag = null;
    activeView = null;
    void refreshHomeData();
  }

  function invalidateOpenRequests() {
    openRequestSeq += 1;
  }

  function clearSaveTimer() {
    if (saveTimer) {
      clearTimeout(saveTimer);
      saveTimer = null;
    }
  }

  async function loadRecentVaults() {
    if (!isTauriRuntime()) {
      recent = [];
      return;
    }
    try {
      recent = await vaultRecent();
    } catch (err) {
      recent = [];
      console.error('Failed to load recent vaults:', err);
    }
  }

  async function runPendingSave() {
    clearSaveTimer();
    if (saveInFlight) {
      await saveInFlight;
    }

    const nextSave = pendingSave;
    if (!nextSave) return;
    pendingSave = null;

    // The Rust side resolves paths against the active vault. If the user has
    // already switched vaults, silently drop the stale save instead of writing
    // to the wrong workspace.
    if (vaultState.current?.path !== nextSave.vaultPath) return;

    const task = (async () => {
      try {
        await fileWrite(nextSave.path, nextSave.content);
        const sameFileStillOpen =
          vaultState.current?.path === nextSave.vaultPath &&
          vaultState.openFilePath === nextSave.path;
        if (!sameFileStillOpen) return;

        const newerSaveQueued = pendingSave !== null;
        if (newerSaveQueued) {
          saveStatus = 'saving';
          return;
        }

        saveStatus = 'saved';
        saveError = '';
        // Give the watcher a moment to ingest the file, then refresh the
        // right-hand panel so new [[links]] and #tags show up.
        schedulePanelRefresh(400);
        setTimeout(() => {
          if (saveStatus === 'saved') saveStatus = 'idle';
        }, 1200);
      } catch (err) {
        if (vaultState.current?.path === nextSave.vaultPath) {
          saveStatus = 'error';
          saveError = `save ${nextSave.path}: ${String(err)}`;
        }
        console.error('[file_write failed]', nextSave.path, err);
      }
    })();

    saveInFlight = task;
    try {
      await task;
    } finally {
      if (saveInFlight === task) {
        saveInFlight = null;
      }
    }
  }

  async function drainPendingSaves() {
    await runPendingSave();
    if (saveInFlight) {
      await saveInFlight;
    }
  }

  function resetVaultViewState() {
    invalidateOpenRequests();
    clearSaveTimer();
    clearAllNotices();
    pendingSave = null;
    tree = [];
    childrenCache = {};
    expanded = new Set();
    editorContent = '';
    saveStatus = 'idle';
    saveError = '';
    cursorLine = 1;
    cursorCol = 0;
    inboxCount = 0;
    homeRecentNotes = [];
    homeRecentMocs = [];
    homeUnresolved = 0;
    homeReview = null;
    newNoteOpen = false;
    newNoteTargetDir = undefined;
    newNoteInput = '';
    newNoteError = '';
    recordOpen = false;
    recordInput = '';
    recordError = '';
    promoteOpen = false;
    promoteSource = null;
    promoteInput = '';
    promoteError = '';
    settingsOpen = false;
    settingsReseedRunning = false;
    settingsReseedMsg = '';
    activeTag = null;
    activeView = null;
    vaultState.closeFile();
  }

  /** Capture-phase keydown listener for app-wide shortcuts. Returns cleanup fn. */
  function installShortcuts(): () => void {
    const handler = (e: KeyboardEvent) => {
      if (!vaultState.current) return;
      // Ignore keys originating from inside a modal input/textarea — the modal
      // has its own handlers (Enter / Cmd+Enter / Esc).
      const target = e.target as HTMLElement | null;
      if (target?.closest('.modal')) return;

      const runners: Record<ShortcutActionId, () => void> = {
        palette: () => {
          paletteOpen = true;
        },
        daily: () => {
          void openOrCreateDaily(cmdDeps);
        },
        weekly: () => {
          void openOrCreateWeekly(cmdDeps);
        },
        capture: () => {
          void quickCapture(cmdDeps);
        },
        record: () => {
          openRecord();
        },
        graph: () => {
          openGraphView();
        },
        extract: () => {
          runExtractSelection();
        },
        settings: () => {
          openSettings();
        }
      };

      for (const actionId of shortcutActionIds) {
        if (!matchShortcutEvent(e, shortcutBindings[actionId])) continue;
        e.preventDefault();
        e.stopPropagation();
        runners[actionId]();
        break;
      }
    };
    window.addEventListener('keydown', handler, true);
    return () => window.removeEventListener('keydown', handler, true);
  }

  function openRecord() {
    recordInput = '';
    recordError = '';
    recordOpen = true;
    setTimeout(() => recordInputEl?.focus(), 0);
  }

  function cancelRecord() {
    recordOpen = false;
    recordError = '';
  }

  async function confirmRecord() {
    const text = recordInput.trim();
    if (!text) {
      recordError = '内容不能为空';
      return;
    }
    try {
      await appendDailyRecord(cmdDeps, text);
      recordOpen = false;
    } catch (err) {
      recordError = String(err);
    }
  }

  function onRecordKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      cancelRecord();
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      confirmRecord();
    }
  }

  /**
   * Open the Build-MOC modal for the currently focused tag. Loads the tag's
   * note list eagerly (it's already in SQLite; IPC round-trip is cheap) and
   * pre-selects every note so the default is "turn this whole tag into a
   * MOC".
   *
   * Requires `activeTag !== null`. The command-palette entry is gated on
   * this; callers outside the palette (TagView button, keyboard shortcut)
   * should double-check before calling.
   */
  async function runBuildMocFromTag() {
    if (!activeTag) {
      pushNotice('请先在侧栏选中一个标签', 'error');
      return;
    }
    mocBuilderTag = activeTag;
    // Seed title with the tag itself — most of the time this is what the
    // user wants (e.g. tag "zettelkasten" → MOC titled "zettelkasten").
    mocBuilderTitle = activeTag;
    mocBuilderError = '';
    mocBuilderLoading = true;
    mocBuilderRunning = false;
    mocBuilderList = [];
    mocBuilderSelected = new Set();
    mocBuilderOpen = true;
    try {
      const notes = await indexNotesByTag(activeTag);
      mocBuilderList = notes;
      // Default: all selected. Keeping the default inclusive means the
      // user's common case ("make a MOC of every note tagged X") is a
      // one-click operation.
      mocBuilderSelected = new Set(notes.map((n) => n.path));
    } catch (err) {
      mocBuilderError = String(err);
    } finally {
      mocBuilderLoading = false;
      setTimeout(() => {
        mocBuilderTitleEl?.focus();
        mocBuilderTitleEl?.select();
      }, 0);
    }
  }

  function cancelBuildMoc() {
    mocBuilderOpen = false;
    mocBuilderTag = '';
    mocBuilderTitle = '';
    mocBuilderError = '';
    mocBuilderList = [];
    mocBuilderSelected = new Set();
  }

  function toggleMocNote(path: string) {
    const next = new Set(mocBuilderSelected);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    mocBuilderSelected = next;
  }

  function toggleAllMocNotes() {
    if (mocBuilderSelected.size === mocBuilderList.length) {
      mocBuilderSelected = new Set();
    } else {
      mocBuilderSelected = new Set(mocBuilderList.map((n) => n.path));
    }
  }

  async function confirmBuildMoc() {
    if (mocBuilderRunning) return;
    const title = mocBuilderTitle.trim();
    if (!title) {
      mocBuilderError = '标题不能为空';
      return;
    }
    const picked = mocBuilderList.filter((n) => mocBuilderSelected.has(n.path));
    // Allow zero — the template keeps its `- [[]]` stub and the user can
    // fill it in manually. This matches the UX of "New MOC…" without a tag.
    mocBuilderRunning = true;
    mocBuilderError = '';
    try {
      const { dstPath, insertedCount, strategy } = await buildMocFromTag(cmdDeps, {
        tag: mocBuilderTag,
        title,
        noteRefs: picked
      });
      invalidateWikiCompletionCache();
      // Debounced panel refresh: the MOC file just hit disk, but the
      // notify-rs → indexer → SQLite pipeline is async. An immediate
      // `panelRefreshToken += 1` races the indexer, so TagsSection refetches
      // from a stale DB (tag count on the selected tag would stay at N
      // instead of N+1 because the new MOC inlines `#<tag>` in frontmatter).
      // 200ms gives the watcher comfortable headroom on a typical SSD.
      schedulePanelRefresh(200);
      graphRefreshToken += 1;
      mocBuilderOpen = false;
      mocBuilderList = [];
      mocBuilderSelected = new Set();
      const name = dstPath.slice(dstPath.lastIndexOf('/') + 1);
      if (strategy === 'none' && picked.length > 0) {
        // MOC was created but the injector couldn't find either the
        // sentinel or the legacy stub — likely a hand-customised template.
        // Tell the user so they can paste the entries manually rather than
        // discovering the empty MOC hours later.
        pushNotice(
          `已创建 2-moc/${name}，但模板中未找到插入点，请手动粘贴 ${picked.length} 条笔记`,
          'info',
          5200
        );
      } else {
        pushNotice(`已创建 2-moc/${name}（${insertedCount} 条笔记）`, 'success');
      }
    } catch (err) {
      mocBuilderError = String(err);
    } finally {
      mocBuilderRunning = false;
    }
  }

  function onMocBuilderKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      cancelBuildMoc();
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void confirmBuildMoc();
    }
  }

  /**
   * Open the Promote modal for the given inbox note. The input is pre-filled
   * with the file's stem so the user usually just tweaks casing / wording
   * before hitting Enter.
   */
  function openPromoteModal(path: string) {
    if (!path.startsWith('0-inbox/')) {
      pushNotice('Promote 仅对 0-inbox/ 下的笔记可用', 'error');
      return;
    }
    const stem = path.slice(path.lastIndexOf('/') + 1).replace(/\.md$/, '');
    promoteSource = path;
    promoteInput = stem;
    promoteError = '';
    promoteOpen = true;
    setTimeout(() => {
      promoteInputEl?.focus();
      promoteInputEl?.select();
    }, 0);
  }

  function cancelPromote() {
    promoteOpen = false;
    promoteSource = null;
    promoteError = '';
  }

  async function confirmPromote() {
    if (!promoteSource) return;
    const title = promoteInput.trim();
    if (!title) {
      promoteError = '标题不能为空';
      return;
    }
    try {
      // Ensure any in-flight edits to the source are flushed before we move it.
      await drainPendingSaves();
      await promoteInboxNote(cmdDeps, promoteSource, title);
      invalidateWikiCompletionCache();
      bumpInbox();
      promoteOpen = false;
      promoteSource = null;
    } catch (err) {
      promoteError = String(err);
    }
  }

  function onPromoteKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      cancelPromote();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      confirmPromote();
    }
  }

  /** Promote target preview shown in the modal hint (shows the eventual slug). */
  const promotePreview = $derived.by(() => {
    const t = promoteInput.trim();
    if (!t) return '';
    const slug = t
      .replace(/[\\/:*?"<>|]+/g, '')
      .replace(/\s+/g, '-')
      .replace(/-+/g, '-')
      .replace(/^-|-$/g, '');
    return slug ? `1-notes/${slug}.md` : '';
  });

  // ---------------------------------------------------------------------------
  // Rename modal — palette `> Rename current file…`.
  //
  // The input is a full vault-relative path (e.g. `1-notes/new-name.md`). The
  // user can rename in place, move to a different folder, OR both at once.
  // We refuse directory renames and extension changes that would leave the
  // file unparseable (`.md` required).
  //
  // Uses `fileMoveWithRefsPreview` for dry-run and `fileMoveWithRefs` for the
  // real execution. After success we re-open the file at its new location and
  // surface a toast summary of the move + ref rewrites.

  function previewOverflow(total: number, shown: number): number {
    return Math.max(0, total - shown);
  }

  function renameMatchesPreview(target: string): boolean {
    return (
      !!renameSource &&
      !!renamePreview &&
      renamePreview.old_path === renameSource &&
      renamePreview.new_path === target
    );
  }

  function dirRenameMatchesPreview(target: string): boolean {
    return (
      !!dirRenameSource &&
      !!dirRenamePreview &&
      dirRenamePreview.old_path === dirRenameSource &&
      dirRenamePreview.new_path === target
    );
  }

  // `path` is optional — when the command palette fires the rename, we fall
  // back to the currently-open file. Sidebar right-click passes an explicit
  // rel_path so the modal can target any file in the tree.
  function openRenameModal(path?: string) {
    const p = path ?? vaultState.openFilePath;
    if (!p) {
      pushNotice('Rename 需要先打开一个文件', 'error');
      return;
    }
    if (p.startsWith('.mynotes/')) {
      pushNotice('Rename 不支持 .mynotes/ 下的文件', 'error');
      return;
    }
    renameSource = p;
    renameInput = p;
    renameError = '';
    renamePreviewLoading = false;
    renameRunning = false;
    renamePreview = null;
    renameOpen = true;
    setTimeout(() => {
      renameInputEl?.focus();
      // Select only the stem part (between last '/' and '.md') so the common
      // case — rename, don't move — is one keystroke of backspace.
      const sep = p.lastIndexOf('/');
      const dot = p.lastIndexOf('.');
      const stemStart = sep + 1;
      const stemEnd = dot > sep ? dot : p.length;
      renameInputEl?.setSelectionRange(stemStart, stemEnd);
    }, 0);
  }

  function onRenameInput() {
    renameError = '';
    renamePreview = null;
  }

  function cancelRename() {
    if (renamePreviewLoading || renameRunning) return; // avoid cancel-during-IPC races
    renameOpen = false;
    renameSource = null;
    renameError = '';
    renamePreview = null;
  }

  async function previewRename() {
    if (!renameSource || renamePreviewLoading || renameRunning) return;
    const target = renameInput.trim();
    if (!target) {
      renameError = '目标路径不能为空';
      return;
    }
    if (target === renameSource) {
      renameError = '目标路径与当前路径相同';
      return;
    }
    if (target.startsWith('.mynotes/')) {
      renameError = '不能移动到 .mynotes/ 下';
      return;
    }
    if (!target.toLowerCase().endsWith('.md')) {
      renameError = '目标路径必须以 .md 结尾';
      return;
    }
    if (await fileExists(target)) {
      renameError = `目标已存在: ${target}`;
      return;
    }

    renamePreviewLoading = true;
    renameError = '';
    renamePreview = null;
    try {
      renamePreview = await fileMoveWithRefsPreview(renameSource, target);
    } catch (err) {
      renameError = err instanceof Error ? err.message : String(err);
    } finally {
      renamePreviewLoading = false;
    }
  }

  async function confirmRename() {
    if (!renameSource || renameRunning) return;
    const target = renameInput.trim();
    if (!renameMatchesPreview(target)) {
      renameError = '目标已变更，请先预览影响';
      return;
    }

    renameRunning = true;
    renameError = '';
    try {
      // Flush in-flight edits to the source before the move, otherwise the
      // pending save fires after the file is gone and we log a write-error.
      await drainPendingSaves();
      const result = await fileMoveWithRefs(renameSource, target);
      invalidateWikiCompletionCache();
      await refreshTree();
      schedulePanelRefresh(200);
      // Follow the file to its new home so the editor doesn't point at a
      // now-nonexistent path.
      const filename = target.slice(target.lastIndexOf('/') + 1);
      await openFile({ name: filename, rel_path: target, is_dir: false });

      if (result.warnings.length > 0) {
        console.warn('[rename] warnings:', result.warnings);
      }
      pushNotice(
        joinNotice([
          `已移动到 ${target}`,
          result.rewritten_links > 0
            ? `重写了 ${result.rewritten_files.length} 个文件中的 ${result.rewritten_links} 处引用`
            : null,
          result.warnings.length > 0 ? `${result.warnings.length} 条警告，详见终端` : null
        ]),
        result.warnings.length > 0 ? 'info' : 'success',
        result.warnings.length > 0 ? 5200 : undefined
      );
      renameOpen = false;
      renameSource = null;
      renamePreview = null;
    } catch (err) {
      renameError = err instanceof Error ? err.message : String(err);
    } finally {
      renameRunning = false;
    }
  }

  async function runRenamePrimaryAction() {
    if (renamePreview) {
      await confirmRename();
      return;
    }
    await previewRename();
  }

  function onRenameKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      cancelRename();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      void runRenamePrimaryAction();
    }
  }

  // ----- Directory rename (palette `> Rename current directory…`) -----------
  //
  // Source = parent dir of the currently open file. The user-facing field
  // pre-fills with that path and we select the last segment so the common case
  // (rename the leaf, keep the parent) is one keystroke. The IPC is the full
  // `dir_move_with_refs` — walks the tree, aggregates a single rewrite plan,
  // moves the folder atomically, then rewrites all external referrers.
  //
  // After success we follow the open file to its new home and surface the
  // rewrite count via the shared save-banner channel (same pattern as
  // `confirmRename`).

  function parentDirOf(relPath: string): string | null {
    const i = relPath.lastIndexOf('/');
    if (i <= 0) return null; // file at vault root — no parent dir to rename
    return relPath.slice(0, i);
  }

  // `dirPath` optional — palette command falls back to "parent dir of open
  // file"; sidebar right-click passes the dir's rel_path directly.
  function openDirRenameModal(dirPath?: string) {
    let parent: string | null;
    if (dirPath) {
      parent = dirPath.replace(/\/+$/, '');
    } else {
      const p = vaultState.openFilePath;
      if (!p) {
        pushNotice('Rename 需要先打开一个文件', 'error');
        return;
      }
      parent = parentDirOf(p);
    }
    if (!parent) {
      pushNotice('当前文件位于 vault 根目录，没有父目录可以重命名', 'error');
      return;
    }
    if (parent.startsWith('.mynotes')) {
      pushNotice('不能重命名 .mynotes/ 下的目录', 'error');
      return;
    }
    dirRenameSource = parent;
    dirRenameInput = parent;
    dirRenameError = '';
    dirRenamePreviewLoading = false;
    dirRenameRunning = false;
    dirRenamePreview = null;
    dirRenameOpen = true;
    setTimeout(() => {
      dirRenameInputEl?.focus();
      // Select the last path segment so "rename the leaf" is a one-keystroke
      // edit. If the path is a single segment (e.g. `1-notes`), select the
      // whole thing.
      const sep = parent.lastIndexOf('/');
      const stemStart = sep + 1;
      dirRenameInputEl?.setSelectionRange(stemStart, parent.length);
    }, 0);
  }

  function onDirRenameInput() {
    dirRenameError = '';
    dirRenamePreview = null;
  }

  function cancelDirRename() {
    if (dirRenamePreviewLoading || dirRenameRunning) return;
    dirRenameOpen = false;
    dirRenameSource = null;
    dirRenameError = '';
    dirRenamePreview = null;
  }

  async function previewDirRename() {
    if (!dirRenameSource || dirRenamePreviewLoading || dirRenameRunning) return;
    const target = dirRenameInput.trim().replace(/\/+$/, '');
    if (!target) {
      dirRenameError = '目标路径不能为空';
      return;
    }
    if (target === dirRenameSource) {
      dirRenameError = '目标路径与当前路径相同';
      return;
    }
    if (target.startsWith('.mynotes')) {
      dirRenameError = '不能移动到 .mynotes/ 下';
      return;
    }
    // Self-nesting guard — duplicated on backend, but gives immediate feedback.
    if (target === dirRenameSource || target.startsWith(`${dirRenameSource}/`)) {
      dirRenameError = `目标 '${target}' 位于源 '${dirRenameSource}' 之内`;
      return;
    }
    if (await fileExists(target)) {
      dirRenameError = `目标已存在: ${target}`;
      return;
    }

    dirRenamePreviewLoading = true;
    dirRenameError = '';
    dirRenamePreview = null;
    try {
      dirRenamePreview = await dirMoveWithRefsPreview(dirRenameSource, target);
    } catch (err) {
      dirRenameError = err instanceof Error ? err.message : String(err);
    } finally {
      dirRenamePreviewLoading = false;
    }
  }

  async function confirmDirRename() {
    if (!dirRenameSource || dirRenameRunning) return;
    const target = dirRenameInput.trim().replace(/\/+$/, ''); // strip trailing /
    if (!dirRenameMatchesPreview(target)) {
      dirRenameError = '目标已变更，请先预览影响';
      return;
    }

    dirRenameRunning = true;
    dirRenameError = '';
    try {
      // Flush in-flight edits before the IPC — same reasoning as confirmRename:
      // a pending save against the soon-to-be-gone path would log a write error.
      await drainPendingSaves();
      const prevOpen = vaultState.openFilePath;
      const result = await dirMoveWithRefs(dirRenameSource, target);
      invalidateWikiCompletionCache();
      await refreshTree();
      schedulePanelRefresh(200);

      // Follow the open file to its new home if it lived inside the renamed dir.
      if (prevOpen && prevOpen.startsWith(`${dirRenameSource}/`)) {
        const suffix = prevOpen.slice(dirRenameSource.length); // includes leading /
        const newOpen = `${target}${suffix}`;
        const name = newOpen.slice(newOpen.lastIndexOf('/') + 1);
        await openFile({ name, rel_path: newOpen, is_dir: false });
      }

      if (result.warnings.length > 0) {
        console.warn('[dir-rename] warnings:', result.warnings);
      }
      pushNotice(
        joinNotice([
          `已移动 ${result.moved_files} 个文件到 ${target}`,
          `重写了 ${result.rewritten_files.length} 个外部文件中的 ${result.rewritten_links} 处引用`,
          result.warnings.length > 0 ? `${result.warnings.length} 条警告，详见终端` : null
        ]),
        result.warnings.length > 0 ? 'info' : 'success',
        result.warnings.length > 0 ? 5200 : undefined
      );
      dirRenameOpen = false;
      dirRenameSource = null;
      dirRenamePreview = null;
    } catch (err) {
      dirRenameError = err instanceof Error ? err.message : String(err);
    } finally {
      dirRenameRunning = false;
    }
  }

  async function runDirRenamePrimaryAction() {
    if (dirRenamePreview) {
      await confirmDirRename();
      return;
    }
    await previewDirRename();
  }

  function onDirRenameKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      cancelDirRename();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      void runDirRenamePrimaryAction();
    }
  }

  // ----- Sidebar context menu --------------------------------------------
  //
  // Triggered by right-click on a tree row (`oncontextmenu`). We stash the
  // entry + viewport coords, let the menu render, and dismiss on:
  //   - click outside (backdrop captures),
  //   - Escape key,
  //   - selection of any menu item.
  //
  // Menu content differs by entry kind:
  //   file → Open · Rename · Reveal · Delete
  //   dir  → Expand/Collapse · Rename · New Note · Reveal
  //
  // Directories don't offer Delete — `file_delete` intentionally refuses
  // dirs (too easy to nuke a whole vault by accident); Phase 3 could add
  // a dedicated dir-delete with stronger confirmation.

  function openContextMenu(e: MouseEvent, entry: DirEntry) {
    e.preventDefault();
    e.stopPropagation();
    // Clamp so the menu stays inside the viewport. Menu is ~200×N px; use a
    // conservative 220×240 and snap the anchor if we're too close to the edge.
    const mx = 220;
    const my = 240;
    const x = Math.min(e.clientX, window.innerWidth - mx - 8);
    const y = Math.min(e.clientY, window.innerHeight - my - 8);
    ctxMenuEntry = entry;
    ctxMenuX = x;
    ctxMenuY = y;
    ctxMenuOpen = true;
  }

  function closeContextMenu() {
    ctxMenuOpen = false;
    ctxMenuEntry = null;
  }

  function onCtxMenuKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeContextMenu();
    }
  }

  async function ctxReveal() {
    const entry = ctxMenuEntry;
    closeContextMenu();
    if (!entry) return;
    try {
      await pathReveal(entry.rel_path);
    } catch (err) {
      console.error('[reveal] failed:', err);
      pushNotice(`Reveal 失败: ${errorMessage(err)}`, 'error');
    }
  }

  function ctxRename() {
    const entry = ctxMenuEntry;
    closeContextMenu();
    if (!entry) return;
    if (entry.is_dir) {
      openDirRenameModal(entry.rel_path);
    } else {
      openRenameModal(entry.rel_path);
    }
  }

  async function ctxDelete() {
    const entry = ctxMenuEntry;
    closeContextMenu();
    if (!entry || entry.is_dir) return; // dirs are handled separately (see above)

    const ok = await ask(`确定要删除 ${entry.rel_path} 吗？\n\n该操作不可撤销（不走回收站）。`, {
      title: '删除文件',
      kind: 'warning'
    });
    if (!ok) return;

    try {
      await drainPendingSaves();
      await fileDelete(entry.rel_path);

      // If the deleted file was open in the editor, drop editor state so we
      // don't write stale content back to a now-gone path.
      if (vaultState.openFilePath === entry.rel_path) {
        vaultState.openFilePath = null;
        editorContent = '';
      }

      invalidateWikiCompletionCache();
      await refreshTree();
      schedulePanelRefresh(200);
      pushNotice(`已删除 ${entry.rel_path}`, 'success');
    } catch (err) {
      pushNotice(`删除失败: ${errorMessage(err)}`, 'error');
    }
  }

  function ctxOpenOrToggle() {
    const entry = ctxMenuEntry;
    closeContextMenu();
    if (!entry) return;
    if (entry.is_dir) {
      void toggleDir(entry);
    } else {
      void openFile(entry);
    }
  }

  function ctxNewNoteInDir() {
    const entry = ctxMenuEntry;
    closeContextMenu();
    if (!entry || !entry.is_dir) return;
    newNote(entry.rel_path);
  }

  /**
   * Palette command: set `status:` frontmatter on the current project's
   * `4-projects/<slug>/index.md`. Slug is derived from the open file path,
   * so the user never has to disambiguate — it's always the project they're
   * currently looking at.
   *
   * Feedback surfaces through the notice stack. After success we kick
   * `refreshHomeData` so the Home page's Active Projects card reflects the
   * new bucket immediately — the backend already reindexed synchronously, but
   * the home aggregate is a separate query.
   */
  async function runSetProjectStatus(status: string) {
    const slug = projectSlugFromPath(vaultState.openFilePath);
    if (!slug) {
      pushNotice('Set project status 仅对 4-projects/ 下的笔记可用', 'error');
      return;
    }
    try {
      // Flush in-flight edits to index.md first so they don't race the
      // frontmatter rewrite (backend reads file → edits → atomic_write).
      await drainPendingSaves();
      await projectSetStatus(slug, status);
      // Reload the editor buffer if the affected index.md is currently open.
      //
      // There's no "external change → reload editor" wire in this app — the
      // watcher only reindexes SQLite, it doesn't push file-contents events
      // back to the Svelte layer. Without this re-read, the user sees stale
      // `status: active` in the editor until they click another file and
      // come back (which is what triggers fileRead via openFile).
      //
      // The re-read is a no-op when the open file is elsewhere (e.g. the
      // user ran the command from a project-note in the same project).
      const indexRel = `4-projects/${slug}/index.md`;
      if (vaultState.openFilePath === indexRel) {
        try {
          const fresh = await fileRead(indexRel);
          // drainPendingSaves() above already flushed, so no pending-save
          // interleaving; editorContent = … flows through Editor.svelte's
          // $effect with suppressChange=true so it doesn't re-fire onChange.
          editorContent = fresh;
          pendingSave = null;
        } catch (readErr) {
          // Non-fatal: status was already written, just the UI didn't refresh.
          console.warn('[set-status] editor reload failed:', readErr);
        }
      }
      // Home aggregates are a separate query — poke them too.
      void refreshHomeData();
      // Project moved between status buckets → sidebar ProjectsSection needs
      // to re-fetch. Short delay so the watcher-driven reindex (if any) has
      // settled; project_set_status itself reindexes synchronously so this is
      // mostly belt-and-suspenders.
      schedulePanelRefresh(200);
      pushNotice(`项目状态已设为 ${status}`, 'success');
    } catch (err) {
      pushNotice(`设置项目状态失败：${errorMessage(err)}`, 'error');
    }
  }

  /**
   * Palette command: open the "new note" modal already scoped to the current
   * project's sub-directory. The modal's 4-projects/<slug> branch slugifies
   * the entered title into the filename and threads the original title into
   * the frontmatter (project-note.md template uses `{{title}}`).
   *
   * Available whenever the open file is under `4-projects/<slug>/` — the
   * command panel's `when` guard enforces that, and we derive slug via
   * `projectSlugFromPath` (works for both the project index and sub-notes).
   */
  function runAddNoteToProject() {
    const slug = projectSlugFromPath(vaultState.openFilePath);
    if (!slug) {
      // Shouldn't happen — command palette's `when` hides the command
      // outside 4-projects/. Surface a message anyway so a misdispatch
      // doesn't silently no-op.
      pushNotice('Add Note to Project 仅对 4-projects/<slug>/ 下的笔记可用', 'error');
      return;
    }
    newNoteError = '';
    // targetDir gets the full project dir path. `confirmNewNote` recognises
    // this pattern and builds `4-projects/<slug>/<note-slug>.md` + carries
    // the raw title into the template.
    newNote(`4-projects/${slug}`);
  }

  /**
   * Palette command: move the currently-open project sub-note to `1-notes/`
   * and rewrite its frontmatter `type: project-note → note`.
   *
   * Per V2 no-md-injection rule we intentionally do NOT edit the project's
   * `index.md` (e.g. we don't prepend a `[[wiki-link]]`). The project→note
   * relationship lived in the filesystem path; when the path moves, the
   * relationship is gone. If the user wants a forward pointer in the project
   * body they can type `[[…]]` themselves.
   *
   * Contract:
   *   - Source must be `4-projects/<slug>/<filename>.md` and NOT the project
   *     index (command palette's `when` enforces this; we re-check anyway).
   *   - Destination is `1-notes/<filename>.md`, or `<stem>-N.md` on collision
   *     (same suffix loop as Promote).
   *   - Two-step write-new-then-delete-old, so a crash mid-flight leaves the
   *     user with both files rather than neither.
   */
  async function runExtractFromProject() {
    const src = vaultState.openFilePath;
    if (!src || !src.startsWith('4-projects/') || src.endsWith('/index.md')) {
      pushNotice('Extract from project 仅对项目下的非 index.md 笔记可用', 'error');
      return;
    }
    const filename = src.slice(src.lastIndexOf('/') + 1);
    const stem = filename.replace(/\.md$/, '');
    // Find the first free 1-notes/<filename> slot; cap at 100 like Promote.
    let dst = `1-notes/${filename}`;
    for (let i = 1; (await fileExists(dst)) && i < 100; i++) {
      dst = `1-notes/${stem}-${i}.md`;
    }
    if (await fileExists(dst)) {
      pushNotice(`找不到空闲的目标文件名: ${dst}`, 'error');
      return;
    }
    try {
      // Flush pending edits to the project-note first, otherwise the save
      // timer fires after we delete the source and complains loudly.
      await drainPendingSaves();

      // Rename with refs: moves the file AND rewrites every `[[wiki]]` /
      // `![](path)` in other notes that pointed at the old location. Without
      // this, extracting a linked project note would leave dangling links.
      const renameOut = await fileMoveWithRefs(src, dst);
      if (renameOut.warnings.length > 0) {
        console.warn('[extract-from-project] warnings:', renameOut.warnings);
      }

      // Now apply the frontmatter delta at the destination. Keep the delta
      // minimal — `type: project-note → note` and `updated`. Don't touch
      // title/tags/status — the note is "same note, new folder".
      const body = await fileRead(dst);
      const now = formatDate(new Date(), 'YYYY-MM-DD HH:mm');
      const newBody = rewriteFrontmatter(body, { type: 'note', updated: now });
      if (newBody !== body) {
        await fileWrite(dst, newBody);
      }

      // Expand 1-notes so the user sees where it landed; refresh the tree so
      // both the project dir (now one file lighter) and 1-notes reflect reality.
      if (!expanded.has('1-notes')) {
        expanded = new Set([...expanded, '1-notes']);
      }
      await refreshTree();
      // ProjectsSection doesn't care (project count hasn't changed) but the
      // Panel's "项目笔记" section does — bump so it re-fetches. Also the
      // wiki-complete cache is now stale (src path vanished, dst appeared).
      invalidateWikiCompletionCache();
      schedulePanelRefresh(200);

      // Follow the file to its new home. openFile drains saves internally
      // and re-reads, so no double-read concern.
      await openFile({ name: filename, rel_path: dst, is_dir: false });
      pushNotice(
        joinNotice([
          `已抽离到 ${dst}`,
          renameOut.rewritten_links > 0
            ? `重写了 ${renameOut.rewritten_files.length} 个文件中的 ${renameOut.rewritten_links} 处引用`
            : null,
          renameOut.warnings.length > 0 ? `${renameOut.warnings.length} 条警告，详见终端` : null
        ]),
        renameOut.warnings.length > 0 ? 'info' : 'success',
        renameOut.warnings.length > 0 ? 5200 : undefined
      );
    } catch (err) {
      pushNotice(`抽离失败：${errorMessage(err)}`, 'error');
    }
  }

  /**
   * Force-refresh `<vault>/templates/*.md` from the bundled copies.
   *
   * `vault_init` has an existence guard that intentionally preserves user
   * edits, which means bundled-template upgrades (e.g. Week 5 Task 2's
   * `project_status` → `status` fix) never reach existing vaults on their
   * own. This is the explicit user-triggered migration knob.
   *
   * V2 principle check: this IS a write to `templates/*.md`, but it's user-
   * initiated via a palette command + confirm prompt — not a silent
   * background migration — so it doesn't violate "no silent md injection".
   * User's own custom templates (files not in the bundle) aren't touched.
   *
   * UX: native `confirm()` is intentionally blunt; we warn specifically that
   * user edits to bundled template files will be lost. Result remains in the
   * native dialog channel rather than borrowing the autosave banner.
   */
  async function runReseedTemplates() {
    if (!isTauriRuntime() || !vaultState.current?.path) {
      await message('Reseed templates 需要先打开 vault', {
        title: 'Reseed templates',
        kind: 'warning'
      });
      return;
    }
    // ask() returns boolean; use it as the confirm gate — keeps us inside the
    // native Tauri dialog system instead of mixing browser `window.confirm`
    // and the native success/error messages (which felt jarring).
    const ok = await ask(
      '将使用内置模板覆盖 vault/templates/*.md（仅限 bundled 的 7 个文件）。\n\n' +
        '你自定义的模板不会被删。但如果你手工改过 bundled 文件（例如 project.md / note.md），改动会丢失。\n\n' +
        '继续吗？',
      { title: 'Reseed templates', kind: 'warning' }
    );
    if (!ok) return;
    try {
      const summary = await vaultReseedTemplates();
      const parts: string[] = [];
      if (summary.updated.length)
        parts.push(`更新 ${summary.updated.length}（${summary.updated.join(', ')}）`);
      if (summary.added.length)
        parts.push(`新增 ${summary.added.length}（${summary.added.join(', ')}）`);
      if (summary.unchanged.length) parts.push(`未变 ${summary.unchanged.length}`);
      await message(`模板已同步：${parts.join(' · ') || '无变化'}`, {
        title: 'Reseed templates',
        kind: 'info'
      });
    } catch (err) {
      // Important: show the FULL error in a modal. Rust side returns
      // AppError::to_string() (e.g. `io: ...`).
      const msg = errorMessage(err);
      console.error('[reseed] failed:', err);
      await message(`Reseed 失败：\n${msg}`, {
        title: 'Reseed templates',
        kind: 'error'
      });
    }
  }

  /**
   * `> Find unused attachments` — load the orphan list and open the review
   * modal. "Unused" means: file exists under `attachments/` but no md file
   * has an `![...](<rel_path>)` embed pointing at it in the SQLite index.
   *
   * Caveat surfaced in the modal itself: a freshly pasted `![](...)` that
   * hasn't been saved yet will look unused. The modal warns; we don't try
   * to auto-flush the editor buffer (would interfere with user intent).
   */
  async function openUnusedAttachments() {
    if (!isTauriRuntime() || !vaultState.current?.path) {
      await message('Find unused attachments 需要先打开 vault', {
        title: 'Find unused attachments',
        kind: 'warning'
      });
      return;
    }
    unusedOpen = true;
    unusedLoading = true;
    unusedError = '';
    unusedList = [];
    unusedSelected = new Set();
    try {
      const items = await attachmentUnreferenced();
      unusedList = items;
      // Default to all-selected so "Review + Delete" is effectively one click.
      unusedSelected = new Set(items.map((i) => i.rel_path));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      unusedError = `加载失败：${msg}`;
      console.error('[unused attachments] load failed:', err);
    } finally {
      unusedLoading = false;
    }
  }

  function cancelUnusedAttachments() {
    if (unusedDeleting) return;
    unusedOpen = false;
    unusedError = '';
    unusedList = [];
    unusedSelected = new Set();
  }

  function toggleUnusedRow(relPath: string) {
    const next = new Set(unusedSelected);
    if (next.has(relPath)) next.delete(relPath);
    else next.add(relPath);
    unusedSelected = next;
  }

  function toggleUnusedAll() {
    if (unusedSelected.size === unusedList.length) {
      unusedSelected = new Set();
    } else {
      unusedSelected = new Set(unusedList.map((i) => i.rel_path));
    }
  }

  async function confirmDeleteUnused() {
    if (unusedSelected.size === 0 || unusedDeleting) return;
    const picked = unusedList.filter((i) => unusedSelected.has(i.rel_path));
    const ok = await ask(
      `将永久删除 ${picked.length} 个附件文件（不走系统回收站）。\n\n` +
        '如果你刚在编辑器里贴了图但还没保存，这里可能会把它当 orphan 误删。建议先 ⌘S 保存。\n\n继续吗？',
      { title: 'Delete unused attachments', kind: 'warning' }
    );
    if (!ok) return;
    unusedDeleting = true;
    unusedError = '';
    try {
      const deleted = await attachmentDeleteBatch(picked.map((i) => i.rel_path));
      // Refresh list: drop deleted entries, clear their selection.
      const deletedSet = new Set(deleted);
      unusedList = unusedList.filter((i) => !deletedSet.has(i.rel_path));
      const nextSel = new Set<string>();
      for (const rel of unusedSelected) {
        if (!deletedSet.has(rel)) nextSel.add(rel);
      }
      unusedSelected = nextSel;
      const skipped = picked.length - deleted.length;
      const note =
        skipped > 0
          ? `已删除 ${deleted.length} · 跳过 ${skipped}（详见终端日志）`
          : `已删除 ${deleted.length} 个附件`;
      pushNotice(note, skipped > 0 ? 'info' : 'success');
      if (unusedList.length === 0) {
        // Auto-close the modal when the list is empty.
        unusedOpen = false;
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      unusedError = `删除失败：${msg}`;
      console.error('[unused attachments] delete failed:', err);
    } finally {
      unusedDeleting = false;
    }
  }

  /** Format a byte size for the list row. */
  function fmtBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  async function chooseAndOpen() {
    if (!isTauriRuntime()) return;
    const picked = await openDialog({ directory: true, multiple: false });
    if (!picked || typeof picked !== 'string') return;
    await tryOpenOrInit(picked);
  }

  async function tryOpenOrInit(path: string) {
    const switchingVault =
      vaultState.current?.path !== undefined && vaultState.current.path !== path;
    if (switchingVault) {
      invalidateOpenRequests();
      await drainPendingSaves();
    }

    const isVault = await vaultIsInitialized(path);
    let info;
    if (isVault) {
      info = await vaultOpen(path);
    } else {
      const ok = await ask(
        `"${path}" 还不是 MyNotes vault。\n要在该目录下初始化 LYT 结构吗？\n将创建：0-inbox/ 1-notes/ 2-moc/ 3-journal/ 4-projects/ attachments/ templates/ .mynotes/`,
        { title: 'Initialize vault?', kind: 'info' }
      );
      if (!ok) return;
      info = await vaultInit(path);
    }

    if (vaultState.current?.path !== info.path) {
      resetVaultViewState();
    }
    vaultState.setCurrent(info);
    await loadRecentVaults();
    await loadTree('');
    await refreshInboxCount();
    void refreshHomeData();
  }

  async function loadTree(relDir: string) {
    if (relDir === '') {
      tree = await fileList('');
    }
  }

  let childrenCache = $state<Record<string, DirEntry[]>>({});

  async function toggleDir(entry: DirEntry) {
    const key = entry.rel_path;
    if (expanded.has(key)) {
      expanded = new Set([...expanded].filter((k) => k !== key));
    } else {
      if (!childrenCache[key]) {
        childrenCache[key] = await fileList(key);
      }
      expanded = new Set([...expanded, key]);
    }
  }

  async function openFile(entry: DirEntry, forceReload = false) {
    if (entry.is_dir) return;
    if (!entry.name.endsWith('.md')) return;
    // Leaving any full-pane view (tag / inbox) implicitly; proceed with the file open.
    activeTag = null;
    activeView = null;
    if (entry.rel_path === vaultState.openFilePath && !forceReload) return;
    const vaultPath = vaultState.current?.path;
    if (!vaultPath) return;

    const requestId = ++openRequestSeq;
    try {
      if (forceReload) {
        pendingSave = null;
        clearSaveTimer();
        saveStatus = 'idle';
        saveError = '';
      } else {
        await drainPendingSaves();
      }
      if (requestId !== openRequestSeq || vaultState.current?.path !== vaultPath) return;

      const content = await fileRead(entry.rel_path);
      if (requestId !== openRequestSeq || vaultState.current?.path !== vaultPath) return;
      editorContent = content;
      pendingSave = null;
      clearSaveTimer();
      saveStatus = 'idle';
      saveError = '';
      vaultState.openFile(entry.rel_path);
    } catch (err) {
      if (requestId !== openRequestSeq || vaultState.current?.path !== vaultPath) return;
      console.error(`Failed to open ${entry.rel_path}:`, err);
    }
  }

  async function handleOpenNote(
    relPath: string | null,
    opts?: { forceReload?: boolean }
  ): Promise<void> {
    if (!relPath) {
      pendingSave = null;
      clearSaveTimer();
      saveStatus = 'idle';
      saveError = '';
      vaultState.openFilePath = null;
      editorContent = '';
      return;
    }
    await openFile(
      { name: relPath.slice(relPath.lastIndexOf('/') + 1), rel_path: relPath, is_dir: false },
      opts?.forceReload ?? false
    );
  }

  /** Rebuild the root tree and re-fetch any expanded directories. */
  async function refreshTree() {
    tree = await fileList('');
    const nextCache: Record<string, DirEntry[]> = {};
    const nextExpanded = new Set<string>();
    for (const key of expanded) {
      try {
        nextCache[key] = await fileList(key);
        nextExpanded.add(key);
      } catch (err) {
        console.warn(`Skipping stale expanded path: ${key}`, err);
      }
    }
    childrenCache = nextCache;
    expanded = nextExpanded;
    await refreshInboxCount();
    void refreshHomeData();
  }

  // ---------------------------------------------------------------------------
  // Sidebar drop-import (P3-A6, design_V2.md §6.13.9).
  //
  // When the user drags files from Finder / another app onto the sidebar
  // tree, we copy them into the vault via the Rust `file_import` command.
  // Absolute paths come out of `text/uri-list` / `text/plain` on the
  // DataTransfer object — the same channel used by the editor image drop
  // path (imageEmbed.ts) after `dragDropEnabled: false`.
  //
  // Drop target resolution:
  //   - directory row  → that directory
  //   - file row       → that file's parent directory
  //   - empty tree area → `0-inbox/`
  //
  // Non-goals (kept deliberately out of scope — see design §6.13.9):
  //   - directory drops (rejected with a notice)
  //   - vault-internal drags (independent rename/move feature)
  //   - bytes-only fallback for sources without a `file://` URI
  // ---------------------------------------------------------------------------

  /** Treat this dataTransfer as "a Finder drag". We match the probe that
   *  imageEmbed.ts already uses so we don't fight the editor when a drag
   *  happens to pass over both zones. */
  function dataTransferHasFiles(dt: DataTransfer | null): boolean {
    if (!dt) return false;
    const types = Array.from(dt.types ?? []);
    return (
      types.includes('Files') ||
      types.includes('text/uri-list') ||
      types.includes('public.file-url')
    );
  }

  /** Decode a `file:///Users/.../%E5%9B%BE.png` URI into an OS-absolute path.
   *  Mirrors `decodeFileUri` from imageEmbed.ts but is duplicated here to
   *  keep the sidebar module self-contained. */
  function decodeFileUri(uri: string): string {
    const noScheme = uri.replace(/^file:\/\//i, '');
    try {
      return decodeURIComponent(noScheme);
    } catch {
      return noScheme;
    }
  }

  /** Parse every absolute-path entry a Finder drag has deposited on the
   *  DataTransfer object. Prefers `text/uri-list` (official RFC 2483 shape),
   *  falls back to a single `text/plain` line (some apps only expose that). */
  function parseDroppedPaths(dt: DataTransfer | null): string[] {
    if (!dt) return [];
    const out: string[] = [];
    const uriList = dt.getData('text/uri-list');
    if (uriList) {
      for (const raw of uriList.split(/\r?\n/)) {
        const line = raw.trim();
        if (!line || line.startsWith('#')) continue;
        if (/^file:\/\//i.test(line)) {
          out.push(decodeFileUri(line));
        } else if (line.startsWith('/') || /^[A-Za-z]:[\\/]/.test(line)) {
          out.push(line);
        }
      }
    }
    if (out.length === 0) {
      const plain = dt.getData('text/plain').trim();
      if (plain) {
        const first = plain.split(/\r?\n/)[0].trim();
        if (/^file:\/\//i.test(first)) {
          out.push(decodeFileUri(first));
        } else if (first.startsWith('/') || /^[A-Za-z]:[\\/]/.test(first)) {
          out.push(first);
        }
      }
    }
    return out;
  }

  /** Normalise a vault-relative drop destination. Strips trailing slashes and
   *  collapses the empty / undefined case back to vault root. Returns the
   *  canonical dstDir used in both the IPC call and user-facing notices. */
  function normalizeDropDstDir(dir: string | null | undefined): string {
    if (!dir) return '';
    return dir.replace(/\\/g, '/').replace(/\/+$/, '');
  }

  function resetSidebarDropState() {
    dropTargetPath = null;
    rootDropActive = false;
  }

  /** Copy a batch of absolute-path sources into `dstDir`. Fans out one
   *  `file_import` IPC per file, aggregates successes / failures into one
   *  notice, refreshes the tree, and (for the "single .md imported" case)
   *  opens the newly-created file to match the "drop to read" intuition. */
  async function handleSidebarDrop(paths: string[], dstDir: string): Promise<void> {
    if (paths.length === 0) return;
    const dstLabel = dstDir === '' ? '<vault root>' : dstDir;

    const imported: ImportedFile[] = [];
    const failures: Array<{ path: string; message: string }> = [];
    for (const abs of paths) {
      try {
        const res = await fileImport(abs, dstDir);
        imported.push(res);
      } catch (err) {
        failures.push({ path: abs, message: errorMessage(err) });
      }
    }

    // Refresh regardless — partial success should still surface on the tree.
    if (imported.length > 0) {
      if (dstDir && !expanded.has(dstDir)) {
        expanded = new Set([...expanded, dstDir]);
      }
      await refreshTree();
      schedulePanelRefresh(200);
    }

    if (imported.length > 0 && failures.length === 0) {
      if (imported.length === 1) {
        const only = imported[0];
        const suffix = only.was_renamed ? `（重命名为 ${only.rel_path.split('/').pop()}）` : '';
        pushNotice(`已导入 ${only.original_name} → ${dstLabel}${suffix}`, 'success');
        if (only.rel_path.toLowerCase().endsWith('.md')) {
          await openFile({
            name: only.rel_path.slice(only.rel_path.lastIndexOf('/') + 1),
            rel_path: only.rel_path,
            is_dir: false
          });
        }
      } else {
        pushNotice(`已导入 ${imported.length} 个文件 → ${dstLabel}`, 'success');
      }
    } else if (imported.length > 0 && failures.length > 0) {
      const firstErr = failures[0];
      pushNotice(
        `已导入 ${imported.length} / ${paths.length} 个文件 → ${dstLabel}；${failures.length} 失败：${firstErr.message}`,
        'info',
        6000
      );
    } else {
      const firstErr = failures[0];
      pushNotice(
        `导入失败（${failures.length}/${paths.length}）：${firstErr?.message ?? 'unknown'}`,
        'error',
        6000
      );
    }
  }

  function onSidebarRowDragOver(entry: DirEntry, e: DragEvent) {
    if (!dataTransferHasFiles(e.dataTransfer)) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    const dst = entry.is_dir ? entry.rel_path : parentDirOf(entry.rel_path);
    dropTargetPath = dst ?? null;
    rootDropActive = false;
  }

  function onSidebarRowDragLeave(entry: DirEntry, e: DragEvent) {
    // Only clear if we're actually leaving the zone — dragleave fires on
    // child elements too; the relatedTarget check avoids flicker.
    const related = e.relatedTarget as Node | null;
    const current = e.currentTarget as HTMLElement | null;
    if (current && related && current.contains(related)) return;
    const dst = entry.is_dir ? entry.rel_path : parentDirOf(entry.rel_path);
    if (dropTargetPath === (dst ?? null)) {
      dropTargetPath = null;
    }
  }

  function onSidebarRowDrop(entry: DirEntry, e: DragEvent) {
    if (!dataTransferHasFiles(e.dataTransfer)) return;
    e.preventDefault();
    e.stopPropagation();
    const dst = entry.is_dir ? entry.rel_path : parentDirOf(entry.rel_path);
    const paths = parseDroppedPaths(e.dataTransfer);
    resetSidebarDropState();
    if (paths.length === 0) {
      pushNotice('无法识别拖入的文件路径，请从 Finder 重试', 'error');
      return;
    }
    void handleSidebarDrop(paths, normalizeDropDstDir(dst));
  }

  function onSidebarRootDragOver(e: DragEvent) {
    if (!dataTransferHasFiles(e.dataTransfer)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    if (dropTargetPath === null) rootDropActive = true;
  }

  function onSidebarRootDragLeave(e: DragEvent) {
    const related = e.relatedTarget as Node | null;
    const current = e.currentTarget as HTMLElement | null;
    if (current && related && current.contains(related)) return;
    rootDropActive = false;
  }

  function onSidebarRootDrop(e: DragEvent) {
    if (!dataTransferHasFiles(e.dataTransfer)) return;
    e.preventDefault();
    const paths = parseDroppedPaths(e.dataTransfer);
    const hadRowTarget = dropTargetPath;
    resetSidebarDropState();
    if (paths.length === 0) {
      pushNotice('无法识别拖入的文件路径，请从 Finder 重试', 'error');
      return;
    }
    // If a row-level handler already claimed this drop, let it win —
    // dragover bubbles up to the <ul>, so we'll still see this drop fire
    // on the root, but the row handler has set dropTargetPath and called
    // preventDefault earlier. Guard with hadRowTarget just in case.
    if (hadRowTarget) return;
    void handleSidebarDrop(paths, '0-inbox');
  }

  /** Count un-processed inbox items (used on the Home view and status bar). */
  async function refreshInboxCount() {
    try {
      const items = await fileList('0-inbox');
      inboxCount = items.filter((e) => !e.is_dir && e.name.endsWith('.md')).length;
    } catch {
      inboxCount = 0;
    }
  }

  /**
   * Reload aggregate data shown on the Home view. Safe to call on every
   * tree refresh — the underlying IPCs are cheap (single indexed queries).
   * Skips the work entirely when the Home view isn't visible to avoid
   * stalling the watcher-triggered refresh loop.
   */
  async function refreshHomeData() {
    // Only the editor-less / tag-less / inbox-less state shows Home.
    if (vaultState.openFilePath || activeTag || activeView) return;
    const req = ++homeReqSeq;
    try {
      const [all, unresolved] = await Promise.all([indexAllNotes(), indexUnresolvedCount()]);
      if (req !== homeReqSeq) return;

      // "Recent" == anything under 1-notes / 2-moc / 4-projects. We exclude
      // 0-inbox (captures are processed via Inbox Review) and 3-journal
      // (dailies would dominate the list and drown out substantive work).
      const recent = all.filter(
        (n) =>
          n.path.startsWith('1-notes/') ||
          n.path.startsWith('2-moc/') ||
          n.path.startsWith('4-projects/')
      );
      homeRecentNotes = recent.slice(0, 5);
      homeRecentMocs = all.filter((n) => n.path.startsWith('2-moc/')).slice(0, 5);
      homeUnresolved = unresolved;

      // "Old-note review" — sample one 1-notes/ entry from the tail of the
      // updated-desc list (i.e. the oldest half). Tiny vaults fall back to
      // any 1-notes/ note so this isn't blank on day one.
      const canon = all.filter((n) => n.path.startsWith('1-notes/'));
      if (canon.length === 0) {
        homeReview = null;
      } else if (canon.length < 4) {
        homeReview = canon[Math.floor(Math.random() * canon.length)];
      } else {
        const tail = canon.slice(Math.floor(canon.length / 2));
        homeReview = tail[Math.floor(Math.random() * tail.length)];
      }
    } catch (e) {
      if (req !== homeReqSeq) return;
      // Non-fatal — Home shows empty sections if the index isn't ready yet.
      console.warn('[home data refresh]', e);
      homeRecentNotes = [];
      homeRecentMocs = [];
      homeUnresolved = 0;
      homeReview = null;
    }
  }

  function onCursorMove(line: number, col: number) {
    cursorLine = line;
    cursorCol = col;
  }

  /** Expand 0-inbox in the sidebar (used by the Home view "Inbox" card). */
  function expandInbox() {
    if (!expanded.has('0-inbox')) {
      expanded = new Set([...expanded, '0-inbox']);
      refreshTree();
    }
  }

  function timestampSlug() {
    const d = new Date();
    const pad = (n: number) => String(n).padStart(2, '0');
    return (
      d.getFullYear() +
      '-' +
      pad(d.getMonth() + 1) +
      '-' +
      pad(d.getDate()) +
      '-' +
      pad(d.getHours()) +
      pad(d.getMinutes()) +
      pad(d.getSeconds())
    );
  }

  /** Open the "new note" modal. If `targetDir` is given, place it there. */
  function newNote(targetDir?: string) {
    newNoteTargetDir = targetDir;
    newNoteInput = '';
    newNoteError = '';
    newNoteOpen = true;
    // Focus the input after render.
    setTimeout(() => newNoteInputEl?.focus(), 0);
  }

  function cancelNewNote() {
    newNoteOpen = false;
    newNoteError = '';
  }

  /** Actually create the file from the modal's current state. */
  async function confirmNewNote() {
    const input = newNoteInput.trim();
    let relPath: string;
    let extra: Record<string, string> = {};
    if (newNoteTargetDir === '4-projects') {
      // Projects are folders. Input is a human title like "Deep Work" that
      // becomes `4-projects/<slug>/index.md` with `title: <original>` carried
      // into the template so the project title survives the slug transform.
      if (!input) {
        newNoteError = '项目名不能为空';
        return;
      }
      const slug = slugifyTitle(input);
      if (!slug) {
        newNoteError = '项目名无法转换为合法目录名';
        return;
      }
      relPath = `4-projects/${slug}/index.md`;
      extra = { title: input };
    } else if (newNoteTargetDir?.startsWith('4-projects/')) {
      // Project sub-note: input is a human title like "Interview Notes".
      // Target is `<targetDir>/<note-slug>.md` where targetDir already
      // contains `4-projects/<slug>`. We slugify for the filename and thread
      // the raw title through `extra` so the `project-note.md` template's
      // `{{title}}` keeps the user's capitalization/spacing.
      if (!input) {
        newNoteError = '笔记标题不能为空';
        return;
      }
      const noteSlug = slugifyTitle(input);
      if (!noteSlug) {
        newNoteError = '标题无法转换为合法文件名';
        return;
      }
      relPath = `${newNoteTargetDir}/${noteSlug}.md`;
      extra = { title: input };
    } else if (newNoteTargetDir !== undefined) {
      const name = input || timestampSlug();
      relPath = `${newNoteTargetDir}/${name}`;
    } else {
      relPath = input;
      if (!relPath) {
        relPath = `0-inbox/${timestampSlug()}`;
      } else if (!relPath.includes('/')) {
        relPath = `0-inbox/${relPath}`;
      }
    }
    if (!relPath.endsWith('.md')) relPath += '.md';

    const created = await createNoteFromTemplate(relPath, extra);
    if (!created) {
      newNoteError =
        newNoteTargetDir === '4-projects'
          ? `项目 ${relPath.split('/')[1]} 已存在`
          : `${relPath} 已存在`;
      return;
    }
    // New note means the cached completion list is stale.
    invalidateWikiCompletionCache();

    // Ensure every ancestor dir is expanded so the new file is visible.
    // For flat cases like `1-notes/foo.md` the only ancestor is `1-notes`; for
    // nested cases like `4-projects/deep-work/index.md` we need both
    // `4-projects` and `4-projects/deep-work` expanded — otherwise the
    // intermediate folder stays collapsed and the new file is hidden.
    const segs = relPath.split('/');
    if (segs.length > 1) {
      const toExpand: string[] = [];
      for (let i = 1; i < segs.length; i++) {
        toExpand.push(segs.slice(0, i).join('/'));
      }
      const next = new Set(expanded);
      for (const p of toExpand) next.add(p);
      expanded = next;
    }
    await refreshTree();
    // Bump so the sidebar's ProjectsSection re-fetches when we just created
    // `4-projects/<slug>/index.md` — otherwise the new row doesn't appear
    // until another save happens. Cheap (4 small queries) when we're not in
    // projects territory, so unconditional.
    schedulePanelRefresh(200);

    // Open the new file and close the modal.
    const name = relPath.slice(relPath.lastIndexOf('/') + 1);
    newNoteOpen = false;
    await openFile({ name, rel_path: relPath, is_dir: false });
  }

  function onNewNoteKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      confirmNewNote();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      cancelNewNote();
    }
  }

  /**
   * Resolve a [[wiki-link]] slug to a vault-relative path. Heuristic:
   *   - path with `/` or `.md` suffix → treated literally
   *   - `YYYY-MM-DD` → `3-journal/<slug>.md`
   *   - `YYYY-Wxx`   → `3-journal/<slug>.md`
   *   - anything else → `1-notes/<slug>.md`
   */
  function resolveWikiSlug(slug: string): string {
    const s = slug.trim();
    if (s.includes('/') || s.endsWith('.md')) {
      return s.endsWith('.md') ? s : `${s}.md`;
    }
    if (/^\d{4}-\d{2}-\d{2}$/.test(s)) return `3-journal/${s}.md`;
    if (/^\d{4}-W\d{2}$/.test(s)) return `3-journal/${s}.md`;
    return `1-notes/${s}.md`;
  }

  /** Cmd/Ctrl+click on [[…]] → open or create the target note. */
  async function followWikiLink(slug: string) {
    const resolved = await indexResolveWikiLink(slug);
    if (resolved?.path) {
      const name = resolved.path.slice(resolved.path.lastIndexOf('/') + 1);
      await openFile({ name, rel_path: resolved.path, is_dir: false });
      return;
    }

    const fallbackPath = resolveWikiSlug(slug);
    const created = await createNoteFromTemplate(fallbackPath);
    if (created) {
      const parent = fallbackPath.slice(0, fallbackPath.lastIndexOf('/'));
      if (parent && !expanded.has(parent)) {
        expanded = new Set([...expanded, parent]);
      }
      await refreshTree();
    }
    const name = fallbackPath.slice(fallbackPath.lastIndexOf('/') + 1);
    await openFile({ name, rel_path: fallbackPath, is_dir: false });
  }

  function onContentChange(next: string) {
    editorContent = next;
    const path = vaultState.openFilePath;
    const vaultPath = vaultState.current?.path;
    if (!path || !vaultPath) return;
    saveStatus = 'saving';
    saveError = '';
    pendingSave = { path, content: next, vaultPath };
    clearSaveTimer();
    saveTimer = setTimeout(() => {
      void runPendingSave();
    }, autosaveDelayMs);
  }
</script>

{#if !vaultState.current}
  <main class="welcome">
    <div class="welcome-inner">
      <h1>MyNotes</h1>
      <p class="tagline">一个基于 LYT 工作流的纯 Markdown 个人知识库。</p>
      {#if runtimeMode === 'browser'}
        <p class="runtime-note">
          当前是浏览器预览模式；vault、文件系统与系统对话框需要通过
          <code>pnpm tauri:dev</code> 运行。
        </p>
      {/if}
      <button class="primary" onclick={chooseAndOpen} disabled={runtimeMode === 'browser'}>
        Open Vault...
      </button>
      {#if recent.length > 0}
        <div class="recent">
          <h3>Recent</h3>
          <ul>
            {#each recent as path}
              <li>
                <button class="link" onclick={() => tryOpenOrInit(path)}>{path}</button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
    </div>
  </main>
{:else}
  <div
    class="app"
    class:app-no-panel={!vaultState.openFilePath || activeView === 'graph'}
    class:app-graph-focus={activeView === 'graph'}
  >
    <IconRail
      active={activeView === 'chat'
        ? 'chat'
        : activeView === 'inbox'
        ? 'inbox'
        : activeView === 'graph'
        ? 'graph'
        : !vaultState.openFilePath && !activeTag
        ? kbTab === 'tasks'
          ? 'tasks'
          : 'notes'
        : null}
      onGoHome={goHome}
      onSelectChat={openChatView}
      onSelectNotes={() => {
        kbTab = 'notes';
        goHome();
      }}
      onSelectTasks={() => {
        kbTab = 'tasks';
      }}
      onSelectInbox={openInboxReview}
      onSelectGraph={openGraphView}
      onOpenDaily={() => void openOrCreateDaily(cmdDeps)}
      onQuickCapture={() => void quickCapture(cmdDeps)}
      onOpenPalette={() => (paletteOpen = true)}
      onToggleTweaks={() => tweaksStore.toggle()}
    />
    {#if activeView !== 'graph'}
      <KnowledgeColumn
        tab={kbTab}
        onTabChange={(t) => (kbTab = t)}
        onRefresh={() => (panelRefreshToken += 1)}
        {priorityFilter}
        onPriorityChange={(p) => (priorityFilter = p)}
      >
        {#snippet notesSlot()}
          <div class="kb-section-head">
            <span class="vault-name" title={vaultState.current?.path ?? ''}>
              {vaultState.current?.path.split('/').pop() ?? ''}
            </span>
            <span class="header-buttons">
              <button
                class="icon"
                title="New note"
                aria-label="新建笔记"
                onclick={() => newNote()}
              >+</button>
              <button
                class="icon"
                title="Change vault"
                aria-label="切换 vault"
                onclick={chooseAndOpen}
              >⇆</button>
            </span>
          </div>
          <ul
            class="tree"
            class:drop-root-active={rootDropActive}
            ondragover={onSidebarRootDragOver}
            ondragleave={onSidebarRootDragLeave}
            ondrop={onSidebarRootDrop}
          >
            {#each tree as entry (entry.rel_path)}
              {@render treeNode(entry, 0)}
            {/each}
          </ul>
          <TagsSection {activeTag} onSelect={selectTag} refreshToken={panelRefreshToken} />
        {/snippet}
        {#snippet tasksSlot()}
          <TasksList
            refreshToken={panelRefreshToken}
            filter={priorityFilter}
            onOpenNote={(p) => handleOpenNote(p)}
          />
        {/snippet}
        {#snippet projectsSlot()}
          <ProjectsSection
            activeProjectPath={vaultState.openFilePath}
            onSelect={(path) =>
              openFile({ name: path.slice(path.lastIndexOf('/') + 1), rel_path: path, is_dir: false })}
            refreshToken={panelRefreshToken}
          />
        {/snippet}
      </KnowledgeColumn>
    {/if}
    <section class="editor-pane">
      {#if activeView === 'inbox'}
        <InboxView
          onOpenNote={(p) => {
            activeView = null;
            handleOpenNote(p);
          }}
          onPromote={(p) => openPromoteModal(p)}
          onArchive={archiveInboxNote}
          onDelete={deleteInboxNote}
          onClose={closeInboxView}
          refreshToken={inboxRefreshToken}
        />
      {:else if activeView === 'chat'}
        <!-- Middle-pane chat = global "agent" surface. Intentionally NOT tied
             to the currently-open file: the agent can still read any note via
             its tools, but it shouldn't look like a chat about one specific
             file. For that, use the right-panel docked chat while editing. -->
        <ChatPanel
          filePath={null}
          onOpenNote={(p) => {
            if (p) {
              activeView = null;
              void handleOpenNote(p);
            }
          }}
          variant="standalone"
        />
      {:else if activeView === 'graph'}
        {#if GraphViewComponent}
          <GraphViewComponent
            currentFilePath={vaultState.openFilePath}
            refreshToken={graphRefreshToken}
            onOpenNote={(p: string) => {
              activeView = null;
              handleOpenNote(p);
            }}
            onClose={closeGraphView}
          />
        {:else}
          <div class="empty-state">
            {#if graphViewLoading}
              <p>加载图谱视图…</p>
            {:else if graphViewLoadError}
              <p>图谱视图加载失败：{graphViewLoadError}</p>
              <button class="cmd" onclick={() => void ensureGraphViewLoaded()}>重试</button>
            {:else}
              <p>正在准备图谱视图…</p>
            {/if}
          </div>
        {/if}
      {:else if activeTag}
        {#key activeTag}
          <TagView
            tag={activeTag}
            onOpenNote={(p) => handleOpenNote(p)}
            onClose={closeTagView}
            onBuildMoc={() => {
              void runBuildMocFromTag();
            }}
          />
        {/key}
      {:else if vaultState.openFilePath}
        {#key vaultState.openFilePath}
          <Editor
            bind:content={editorContent}
            filePath={vaultState.openFilePath}
            onChange={onContentChange}
            onWikiLink={followWikiLink}
            onCursor={onCursorMove}
            onReady={(api) => (editorApi = api)}
          />
        {/key}
      {:else}
        {@render homeView()}
      {/if}
      <TodayTasksPanel
        visible={todayPanelVisible && activeView !== 'graph'}
        refreshToken={panelRefreshToken}
        onClose={() => (todayPanelVisible = false)}
        onOpenNote={(p) => {
          todayPanelVisible = false;
          handleOpenNote(p);
        }}
        onViewAll={() => {
          todayPanelVisible = false;
          kbTab = 'tasks';
        }}
      />
    </section>
    {#if vaultState.openFilePath && activeView !== 'graph'}
      <div class="panel-slot">
        <Panel
          filePath={vaultState.openFilePath}
          onOpenNote={handleOpenNote}
          refreshToken={panelRefreshToken}
          {aiEnabled}
        />
      </div>
    {/if}
    <footer class="status-bar">
      <span class="sb-group sb-left">
        <span class="sb-item" title={vaultState.current.path}>
          {vaultState.current.path.split('/').pop()}
        </span>
        {#if vaultState.openFilePath}
          <span class="sb-sep">·</span>
          <span class="sb-item" data-testid="active-file-path">{vaultState.openFilePath}</span>
        {/if}
      </span>
      <span class="sb-group sb-right">
        {#if vaultState.openFilePath}
          <span class="sb-item">{wordCount} 字</span>
          <span class="sb-sep">·</span>
          <span class="sb-item">{cursorLine}:{cursorCol + 1}</span>
          <span class="sb-sep">·</span>
        {/if}
        <span class="sb-item sb-save" data-state={saveStatus} title={saveError}>
          {#if saveStatus === 'saving'}saving…
          {:else if saveStatus === 'saved'}saved
          {:else if saveStatus === 'error'}⚠ save failed
          {:else}·{/if}
        </span>
        <span class="sb-sep">·</span>
        <button
          class="sb-icon-btn"
          onclick={openSettings}
          title="Settings (⌘,)"
          aria-label="Open settings"
          data-testid="open-settings"
        >
          ⚙
        </button>
        <button
          class="sb-icon-btn"
          onclick={cycleTheme}
          title={`主题: ${theme}（点击切换）`}
          aria-label="Toggle theme"
        >
          {#if theme === 'light'}☀{:else if theme === 'dark'}☾{:else}◐{/if}
        </button>
      </span>
    </footer>
  </div>
{/if}

{#if notices.length > 0}
  <div class="notice-stack" aria-live="polite" aria-atomic="false">
    {#each notices as notice (notice.id)}
      <div
        class="notice"
        data-kind={notice.kind}
        role={notice.kind === 'error' ? 'alert' : 'status'}
        transition:fly={{ x: 20, duration: 180 }}
      >
        <div class="notice-copy">
          <div class="notice-label">
            {#if notice.kind === 'error'}
              错误
            {:else if notice.kind === 'success'}
              完成
            {:else}
              提示
            {/if}
          </div>
          <div class="notice-message">{notice.message}</div>
        </div>
        <button
          type="button"
          class="notice-close"
          aria-label="Dismiss notice"
          onclick={() => dismissNotice(notice.id)}
        >
          ×
        </button>
      </div>
    {/each}
  </div>
{/if}

{#if newNoteOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={cancelNewNote}
    onkeydown={(e) => e.key === 'Escape' && cancelNewNote()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="新建笔记"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>
        {#if newNoteTargetDir === '2-moc'}
          新建 MOC
        {:else if newNoteTargetDir === '4-projects'}
          新建项目
        {:else if newNoteTargetDir?.startsWith('4-projects/')}
          新建项目笔记
        {:else}
          新建笔记
        {/if}
      </h3>
      <p class="modal-hint">
        {#if newNoteTargetDir === '2-moc'}
          在 <code>2-moc/</code> 下新建，模板套用 <code>templates/moc.md</code>（留空 = 时间戳）
        {:else if newNoteTargetDir === '4-projects'}
          在 <code>4-projects/&lt;slug&gt;/index.md</code> 下新建，模板套用
          <code>templates/project.md</code>；项目名会被 slugify 成目录名（例如 "Deep Work" →
          <code>deep-work/</code>）
        {:else if newNoteTargetDir?.startsWith('4-projects/')}
          在 <code>{newNoteTargetDir}/</code> 下新建，模板套用
          <code>templates/project-note.md</code>；标题会被 slugify 成文件名（例如 "Interview Notes"
          → <code>interview-notes.md</code>）
        {:else if newNoteTargetDir !== undefined}
          在 <code>{newNoteTargetDir}/</code> 下新建（留空 = 时间戳）
        {:else}
          输入相对路径（如 <code>1-notes/my-note</code>）；只写文件名时放入
          <code>0-inbox/</code>；留空 = 时间戳
        {/if}
      </p>
      <input
        bind:this={newNoteInputEl}
        bind:value={newNoteInput}
        onkeydown={onNewNoteKey}
        placeholder={newNoteTargetDir === '2-moc'
          ? 'Python · Deep Work …'
          : newNoteTargetDir === '4-projects'
            ? 'Deep Work'
            : newNoteTargetDir?.startsWith('4-projects/')
              ? 'Interview Notes'
              : newNoteTargetDir !== undefined
                ? 'my-note'
                : '1-notes/my-note'}
        class="modal-input"
      />
      {#if newNoteError}
        <p class="modal-error">{newNoteError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={cancelNewNote}>取消</button>
        <button class="primary" onclick={confirmNewNote}>创建</button>
      </div>
    </div>
  </div>
{/if}

{#if recordOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={cancelRecord}
    onkeydown={(e) => e.key === 'Escape' && cancelRecord()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="Daily Record"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>Daily Record</h3>
      <p class="modal-hint">
        将追加一条时间戳记录到今天的 daily note（<code>3-journal/</code> 下）。<br />
        <kbd>⌘↵</kbd> 提交 · <kbd>Esc</kbd> 取消
      </p>
      <textarea
        bind:this={recordInputEl}
        bind:value={recordInput}
        onkeydown={onRecordKey}
        placeholder="刚刚发生了什么…"
        class="modal-input"
        rows="4"
      ></textarea>
      {#if recordError}
        <p class="modal-error">{recordError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={cancelRecord}>取消</button>
        <button class="primary" onclick={confirmRecord}>记录</button>
      </div>
    </div>
  </div>
{/if}

{#if promoteOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={cancelPromote}
    onkeydown={(e) => e.key === 'Escape' && cancelPromote()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="Promote to 1-notes"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>Promote 到 1-notes/</h3>
      <p class="modal-hint">
        {#if promoteSource}
          从 <code>{promoteSource}</code>
          <br />
        {/if}
        将写入标准 frontmatter（<code>type: note</code> · <code>status: draft</code>）并移动到
        {#if promotePreview}
          <code>{promotePreview}</code>
        {:else}
          <code>1-notes/</code>
        {/if}。<br />
        <kbd>↵</kbd> 确认 · <kbd>Esc</kbd> 取消
      </p>
      <input
        bind:this={promoteInputEl}
        bind:value={promoteInput}
        onkeydown={onPromoteKey}
        placeholder="Deep Work · Python 协程 …"
        class="modal-input"
      />
      {#if promoteError}
        <p class="modal-error">{promoteError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={cancelPromote}>取消</button>
        <button class="primary" onclick={confirmPromote}>Promote</button>
      </div>
    </div>
  </div>
{/if}

{#if unusedOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={cancelUnusedAttachments}
    onkeydown={(e) => e.key === 'Escape' && cancelUnusedAttachments()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal modal-wide"
      role="dialog"
      aria-modal="true"
      aria-label="Find unused attachments"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>未被引用的附件</h3>
      <p class="modal-hint">
        这些文件位于 <code>attachments/</code>，但没有任何笔记通过
        <code>![](…)</code> 引用它们。<br />
        ⚠ 如果你刚粘贴了图片还没 <kbd>⌘S</kbd>，它会出现在这里 —— 先保存再清理更稳妥。
      </p>

      {#if unusedLoading}
        <p class="modal-hint">扫描中…</p>
      {:else if unusedList.length === 0 && !unusedError}
        <p class="modal-hint">🎉 没有孤儿附件，vault 很整齐。</p>
      {:else}
        <div class="unused-toolbar">
          <label class="unused-row unused-row-header">
            <input
              type="checkbox"
              checked={unusedSelected.size === unusedList.length && unusedList.length > 0}
              onchange={toggleUnusedAll}
            />
            <span class="unused-path">全选</span>
            <span class="unused-size">{unusedSelected.size} / {unusedList.length}</span>
          </label>
        </div>
        <div class="unused-list">
          {#each unusedList as item (item.rel_path)}
            <label class="unused-row">
              <input
                type="checkbox"
                checked={unusedSelected.has(item.rel_path)}
                onchange={() => toggleUnusedRow(item.rel_path)}
              />
              <span class="unused-path" title={item.rel_path}>{item.rel_path}</span>
              <span class="unused-size">{fmtBytes(item.size)}</span>
            </label>
          {/each}
        </div>
      {/if}

      {#if unusedError}
        <p class="modal-error">{unusedError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={cancelUnusedAttachments} disabled={unusedDeleting}>取消</button>
        <button
          class="primary"
          onclick={confirmDeleteUnused}
          disabled={unusedSelected.size === 0 || unusedDeleting}
        >
          {unusedDeleting ? '删除中…' : `删除 ${unusedSelected.size}`}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if renameOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={cancelRename}
    onkeydown={(e) => e.key === 'Escape' && cancelRename()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="Rename file"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>重命名文件</h3>
      <p class="modal-hint">
        {#if renameSource}
          从 <code>{renameSource}</code>
          <br />
        {/if}
        所有指向它的 <code>[[wiki]]</code> 和 <code>![](path)</code> 引用会被自动重写。<br />
        <kbd>↵</kbd>
        {renamePreview ? '确认重命名' : '预览影响'} · <kbd>Esc</kbd> 取消
      </p>
      <input
        bind:this={renameInputEl}
        bind:value={renameInput}
        oninput={onRenameInput}
        onkeydown={onRenameKey}
        placeholder="1-notes/new-name.md"
        class="modal-input"
        disabled={renamePreviewLoading || renameRunning}
      />
      {#if renamePreview}
        <div class="modal-preview">
          <div class="modal-preview-label">预览</div>
          <div class="modal-preview-body">
            将移动 1 个文件；将重写 {renamePreview.rewritten_files_total} 个文件中的
            {renamePreview.rewritten_links} 处引用
          </div>
          {#if renamePreview.rewritten_files_total > 0}
            <div class="modal-preview-group">
              <div class="modal-preview-label">将被改写的文件</div>
              <div class="modal-preview-body modal-preview-list">
                {#each renamePreview.rewritten_files_preview as path}
                  <div>{path}</div>
                {/each}
                {#if previewOverflow(renamePreview.rewritten_files_total, renamePreview.rewritten_files_preview.length) > 0}
                  <div class="modal-preview-more">
                    … 另有 {previewOverflow(
                      renamePreview.rewritten_files_total,
                      renamePreview.rewritten_files_preview.length
                    )} 项
                  </div>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {/if}
      {#if renameError}
        <p class="modal-error">{renameError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={cancelRename} disabled={renamePreviewLoading || renameRunning}>取消</button
        >
        <button
          class="primary"
          onclick={runRenamePrimaryAction}
          disabled={renamePreviewLoading || renameRunning}
        >
          {#if renamePreview}
            {renameRunning ? '重命名中…' : '确认重命名'}
          {:else}
            {renamePreviewLoading ? '预览中…' : '预览影响'}
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if dirRenameOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={cancelDirRename}
    onkeydown={(e) => e.key === 'Escape' && cancelDirRename()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="Rename directory"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>重命名目录</h3>
      <p class="modal-hint">
        {#if dirRenameSource}
          从 <code>{dirRenameSource}/</code>
          <br />
        {/if}
        目录内所有文件会一并移动；指向它们的 <code>[[wiki]]</code> 和
        <code>![](path)</code> 引用会被自动重写。<br />
        <kbd>↵</kbd>
        {dirRenamePreview ? '确认重命名' : '预览影响'} · <kbd>Esc</kbd> 取消
      </p>
      <input
        bind:this={dirRenameInputEl}
        bind:value={dirRenameInput}
        oninput={onDirRenameInput}
        onkeydown={onDirRenameKey}
        placeholder="1-notes/archive"
        class="modal-input"
        disabled={dirRenamePreviewLoading || dirRenameRunning}
      />
      {#if dirRenamePreview}
        <div class="modal-preview">
          <div class="modal-preview-label">预览</div>
          <div class="modal-preview-body">
            将移动 {dirRenamePreview.moved_files_total} 个文件（{dirRenamePreview.moved_markdown_files}
            篇笔记 + {dirRenamePreview.moved_other_files} 个附件/其他）；将重写
            {dirRenamePreview.rewritten_files_total} 个外部文件中的
            {dirRenamePreview.rewritten_links} 处引用
          </div>
          {#if dirRenamePreview.moved_files_total > 0}
            <div class="modal-preview-group">
              <div class="modal-preview-label">将移动的文件</div>
              <div class="modal-preview-body modal-preview-list">
                {#each dirRenamePreview.moved_files_preview as path}
                  <div>{path}</div>
                {/each}
                {#if previewOverflow(dirRenamePreview.moved_files_total, dirRenamePreview.moved_files_preview.length) > 0}
                  <div class="modal-preview-more">
                    … 另有 {previewOverflow(
                      dirRenamePreview.moved_files_total,
                      dirRenamePreview.moved_files_preview.length
                    )} 项
                  </div>
                {/if}
              </div>
            </div>
          {/if}
          <div class="modal-preview-group">
            <div class="modal-preview-label">将被改写的外部文件</div>
            <div class="modal-preview-body modal-preview-list">
              {#if dirRenamePreview.rewritten_files_total === 0}
                <div>无外部文件需要改写。</div>
              {:else}
                {#each dirRenamePreview.rewritten_files_preview as path}
                  <div>{path}</div>
                {/each}
                {#if previewOverflow(dirRenamePreview.rewritten_files_total, dirRenamePreview.rewritten_files_preview.length) > 0}
                  <div class="modal-preview-more">
                    … 另有 {previewOverflow(
                      dirRenamePreview.rewritten_files_total,
                      dirRenamePreview.rewritten_files_preview.length
                    )} 项
                  </div>
                {/if}
              {/if}
            </div>
          </div>
        </div>
      {/if}
      {#if dirRenameError}
        <p class="modal-error">{dirRenameError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={cancelDirRename} disabled={dirRenamePreviewLoading || dirRenameRunning}>
          取消
        </button>
        <button
          class="primary"
          onclick={runDirRenamePrimaryAction}
          disabled={dirRenamePreviewLoading || dirRenameRunning}
        >
          {#if dirRenamePreview}
            {dirRenameRunning ? '重命名中…' : '确认重命名'}
          {:else}
            {dirRenamePreviewLoading ? '预览中…' : '预览影响'}
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if ctxMenuOpen && ctxMenuEntry}
  <!-- transparent backdrop captures outside-click for dismissal -->
  <div
    class="ctx-menu-backdrop"
    role="button"
    tabindex="-1"
    onclick={closeContextMenu}
    oncontextmenu={(e) => {
      // right-clicking outside the menu should dismiss (not open a second one)
      e.preventDefault();
      closeContextMenu();
    }}
    onkeydown={onCtxMenuKey}
  >
    <div
      class="ctx-menu"
      role="menu"
      tabindex="-1"
      style="left: {ctxMenuX}px; top: {ctxMenuY}px"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="ctx-menu-header">{ctxMenuEntry.rel_path}</div>
      {#if ctxMenuEntry.is_dir}
        <button class="ctx-menu-item" role="menuitem" onclick={ctxOpenOrToggle}>
          {expanded.has(ctxMenuEntry.rel_path) ? '折叠' : '展开'}
        </button>
        <button class="ctx-menu-item" role="menuitem" onclick={ctxRename}> 重命名… </button>
        <button class="ctx-menu-item" role="menuitem" onclick={ctxNewNoteInDir}>
          在此文件夹新建笔记…
        </button>
        <div class="ctx-menu-sep"></div>
        <button class="ctx-menu-item" role="menuitem" onclick={ctxReveal}>
          在 Finder 中显示
        </button>
      {:else}
        <button class="ctx-menu-item" role="menuitem" onclick={ctxOpenOrToggle}> 打开 </button>
        <button class="ctx-menu-item" role="menuitem" onclick={ctxRename}> 重命名… </button>
        <div class="ctx-menu-sep"></div>
        <button class="ctx-menu-item" role="menuitem" onclick={ctxReveal}>
          在 Finder 中显示
        </button>
        <button class="ctx-menu-item danger" role="menuitem" onclick={ctxDelete}> 删除 </button>
      {/if}
    </div>
  </div>
{/if}

{#if extractOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={cancelExtract}
    onkeydown={(e) => e.key === 'Escape' && cancelExtract()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="Extract selection to new note"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>提取选中块为新笔记</h3>
      <p class="modal-hint">
        选中区域会被搬到 <code>1-notes/&lt;slug&gt;.md</code>，原位置替换为 <code>[[标题]]</code>
        双链。<br />
        <kbd>↵</kbd> 确认 · <kbd>Esc</kbd> 取消
      </p>
      <input
        bind:this={extractInputEl}
        bind:value={extractTitle}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            void confirmExtract();
          }
        }}
        placeholder="新笔记标题"
        class="modal-input"
        disabled={extractRunning}
      />
      <div class="modal-preview">
        {#if extractSourceText}
          <div class="modal-preview-label">将提取 {extractSourceText.length} 字符：</div>
          <pre class="modal-preview-body">{extractSourceText.length > 240
              ? extractSourceText.slice(0, 240).trimEnd() + '…'
              : extractSourceText}</pre>
        {/if}
      </div>
      {#if extractError}
        <p class="modal-error">{extractError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={cancelExtract} disabled={extractRunning}>取消</button>
        <button class="primary" onclick={confirmExtract} disabled={extractRunning}>
          {extractRunning ? '提取中…' : '提取'}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if settingsOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    data-testid="settings-backdrop"
    onclick={closeSettings}
    onkeydown={(e) => e.key === 'Escape' && closeSettings()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal settings-modal"
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      data-testid="settings-modal"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>设置</h3>

      <div class="settings-section">
        <div class="settings-label">主题</div>
        <div class="settings-radio-group">
          <label class="settings-radio">
            <input
              type="radio"
              name="theme"
              value="system"
              checked={theme === 'system'}
              onchange={() => setTheme('system')}
            />
            <span>跟随系统</span>
          </label>
          <label class="settings-radio">
            <input
              type="radio"
              name="theme"
              value="light"
              checked={theme === 'light'}
              onchange={() => setTheme('light')}
            />
            <span>浅色</span>
          </label>
          <label class="settings-radio">
            <input
              type="radio"
              name="theme"
              value="dark"
              checked={theme === 'dark'}
              onchange={() => setTheme('dark')}
            />
            <span>深色</span>
          </label>
        </div>
      </div>

      <div class="settings-section">
        <label class="settings-label" for="autosave-input">自动保存延迟</label>
        <div class="settings-row">
          <input
            id="autosave-input"
            class="settings-number"
            type="number"
            min={AUTOSAVE_MIN}
            max={AUTOSAVE_MAX}
            step="50"
            value={autosaveDelayMs}
            oninput={onAutosaveDelayInput}
          />
          <span class="settings-unit">毫秒</span>
        </div>
        <p class="settings-hint">
          编辑器在停止输入 {autosaveDelayMs}ms 后写入磁盘。范围 {AUTOSAVE_MIN}–{AUTOSAVE_MAX}ms。
        </p>
      </div>

      <div class="settings-section">
        <div class="settings-label">快捷键</div>
        <div class="settings-shortcuts">
          {#each shortcutActionIds as actionId (actionId)}
            <div class="shortcut-row">
              <div class="shortcut-meta">
                <div class="shortcut-name">{shortcutActionDefs[actionId].label}</div>
                <p class="settings-hint">
                  默认 {formatShortcutDisplay(DEFAULT_SHORTCUT_BINDINGS[actionId])}
                </p>
              </div>
              <div class="shortcut-actions">
                <button
                  class="shortcut-capture"
                  class:shortcut-recording={recordingShortcutId === actionId}
                  onclick={() => startShortcutCapture(actionId)}
                >
                  {recordingShortcutId === actionId ? '按下组合键…' : shortcutLabel(actionId)}
                </button>
                <button
                  class="shortcut-reset"
                  onclick={() => resetShortcutBinding(actionId)}
                  disabled={shortcutBindings[actionId] === DEFAULT_SHORTCUT_BINDINGS[actionId]}
                >
                  默认
                </button>
              </div>
            </div>
          {/each}
        </div>
        <p class="settings-hint">点击右侧按钮后直接按组合键；需要包含 ⌘/Ctrl 或 Alt。Esc 取消。</p>
        {#if settingsShortcutMsg}
          <p class="settings-msg">{settingsShortcutMsg}</p>
        {/if}
      </div>

      <div class="settings-section">
        <div class="settings-label">模板</div>
        <div class="settings-row">
          <button onclick={runReseedFromSettings} disabled={settingsReseedRunning}>
            {settingsReseedRunning ? '重置中…' : '从内置重置 templates/'}
          </button>
          {#if settingsReseedMsg}
            <span class="settings-msg">{settingsReseedMsg}</span>
          {/if}
        </div>
        <p class="settings-hint">
          覆盖 <code>templates/</code> 下同名的内置模板文件（daily / weekly / moc / project / project-note
          / capture）。用户自创的同目录模板不会被动到。
        </p>
      </div>

      <div class="settings-section">
        <div class="settings-label">中文分词</div>
        <p class="settings-hint">
          索引库使用 SQLite FTS5 的 <code>unicode61</code> tokenizer —— 不是运行时开关，改动需要重建索引（下次发版处理）。
        </p>
      </div>

      <div class="settings-section">
        <div class="settings-row">
          <label class="settings-label" for="ai-enabled-toggle"> AI 辅助面板 </label>
          <input
            id="ai-enabled-toggle"
            type="checkbox"
            checked={aiEnabled}
            data-testid="ai-enabled-toggle"
            onchange={(e) => {
              const next = (e.currentTarget as HTMLInputElement).checked;
              aiEnabled = next;
              void appConfigSetAiEnabled(next).catch((err) =>
                console.error('Failed to persist ai_enabled:', err)
              );
            }}
          />
        </div>
        <p class="settings-hint">
          在右侧面板末尾显示「相关笔记」列表，基于 tag 重叠 / 链接关系 / 共同被引；完成 AI
          索引初始化后再叠加语义向量相似度，完全离线运行。
        </p>
      </div>

      <div class="settings-section">
        <div class="settings-label">AI 工具权限</div>
        <p class="settings-hint">
          控制对话里的 agent 能看到哪些工具。关闭某一档后，该类工具会从模型视角直接消失。
        </p>
        <div class:settings-disabled={!aiEnabled} class="ai-tool-permissions">
          <label class="ai-tool-permission">
            <input
              type="checkbox"
              checked={aiToolPermissions.allow_readonly}
              disabled={!aiEnabled}
              data-testid="ai-tool-readonly"
              onchange={(e) =>
                updateAiToolPermissionField(
                  'allow_readonly',
                  (e.currentTarget as HTMLInputElement).checked
                )}
            />
            <span>
              <strong>🟢 读取类</strong>
              <small>搜索、读笔记、列标签、找相关笔记</small>
            </span>
          </label>
          <label class="ai-tool-permission">
            <input
              type="checkbox"
              checked={aiToolPermissions.allow_writeback}
              disabled={!aiEnabled}
              data-testid="ai-tool-writeback"
              onchange={(e) =>
                updateAiToolPermissionField(
                  'allow_writeback',
                  (e.currentTarget as HTMLInputElement).checked
                )}
            />
            <span>
              <strong>🟡 提案类</strong>
              <small>起草摘要、标签更新、MOC、笔记修改；仍需手动接受才会落盘</small>
            </span>
          </label>
          <label class="ai-tool-permission">
            <input
              type="checkbox"
              checked={aiToolPermissions.allow_destructive}
              disabled={!aiEnabled}
              data-testid="ai-tool-destructive"
              onchange={(e) =>
                updateAiToolPermissionField(
                  'allow_destructive',
                  (e.currentTarget as HTMLInputElement).checked
                )}
            />
            <span>
              <strong>🔴 破坏类</strong>
              <small>删除、重命名；会要求二次确认并写入审计日志</small>
            </span>
          </label>
        </div>
      </div>

      <div class="settings-section">
        <div class="settings-label">AI Provider（Embedding）</div>
        <p class="settings-hint">
          配置 OpenAI 兼容的 embedding 服务。同一协议下可接 OpenAI · Ollama（本地）· OpenRouter · LM
          Studio · Together.ai 等。API key 走系统 keychain 存储，不写入配置文件。
        </p>
        <div class="ai-provider-grid">
          <label class="ai-provider-field">
            <span>Base URL</span>
            <input
              type="text"
              spellcheck="false"
              placeholder="https://api.openai.com/v1"
              bind:value={aiProviderBaseUrl}
            />
          </label>
          <label class="ai-provider-field">
            <span>Embed model</span>
            <input
              type="text"
              spellcheck="false"
              placeholder="text-embedding-3-small"
              bind:value={aiProviderEmbedModel}
            />
          </label>
          <label class="ai-provider-field">
            <span>Chat model · 留空停用</span>
            <input
              type="text"
              spellcheck="false"
              placeholder="gpt-4o-mini"
              bind:value={aiProviderChatModel}
            />
          </label>
          <label class="ai-provider-field">
            <span>API key {aiProviderHasKey ? '· 已存储' : '· 未存储'}</span>
            <input
              type="password"
              spellcheck="false"
              autocomplete="off"
              placeholder={aiProviderHasKey
                ? '已存储在 keychain，留空以保留'
                : '粘贴 sk-... 或留空（Ollama）'}
              bind:value={aiProviderApiKey}
            />
          </label>
        </div>
        <div class="ai-provider-actions">
          <button onclick={testAiProvider} disabled={aiProviderTesting || aiProviderSaving}>
            {aiProviderTesting ? '测试中…' : '测试 Embedding'}
          </button>
          <button
            onclick={testAiProviderChat}
            disabled={aiProviderChatTesting || aiProviderSaving || !aiProviderChatModel.trim()}
            title={aiProviderChatModel.trim() ? '' : '请先填写 Chat model'}
          >
            {aiProviderChatTesting ? '测试中…' : '测试聊天'}
          </button>
          <button
            class="primary"
            onclick={saveAiProvider}
            disabled={aiProviderSaving || aiProviderTesting || aiProviderChatTesting}
          >
            {aiProviderSaving ? '保存中…' : '保存'}
          </button>
          <button
            onclick={clearAiProvider}
            disabled={aiProviderSaving || aiProviderTesting || aiProviderChatTesting}
          >
            清除
          </button>
        </div>
        {#if aiProviderTestState}
          <div
            class="ai-provider-test-result"
            class:ok={aiProviderTestState.ok}
            class:err={!aiProviderTestState.ok}
          >
            {#if aiProviderTestState.ok}
              ✓ Embedding 连接成功 · 维度 {aiProviderTestState.dim} · {aiProviderTestState.total_tokens ??
                0} tokens
            {:else}
              ✗ Embedding: {providerTestFailureText(aiProviderTestState)}
            {/if}
          </div>
        {/if}
        {#if aiProviderChatTestState}
          <div
            class="ai-provider-test-result"
            class:ok={aiProviderChatTestState.ok}
            class:err={!aiProviderChatTestState.ok}
          >
            {#if aiProviderChatTestState.ok}
              ✓ Chat 连接成功 · 回复 "{aiProviderChatTestState.reply ?? ''}"
              {#if aiProviderChatTestState.output_tokens != null}
                · {aiProviderChatTestState.output_tokens} out tokens
              {/if}
            {:else}
              ✗ Chat: {providerTestFailureText(aiProviderChatTestState)}
            {/if}
          </div>
        {/if}
      </div>

      <!-- Embedding index (D2a.3a). Stats are loaded lazily on openSettings. -->
      <div class="settings-section">
        <h3>AI 索引 · Embedding</h3>
        <p class="settings-hint">
          每篇笔记被分块后向量化存于
          <code>.mynotes/ai/embeddings.sqlite</code>，后续相关笔记 / 问答会用到。 当前已支持 watcher
          30 s 自动增量；首次建索引建议先跑一次 dry-run 预估，再确认初始化整库。
        </p>
        <div class="ai-embed-stats">
          {#if embedStats}
            已索引 <strong>{embedStats.chunk_count}</strong> chunks ·
            <strong>{embedStats.note_count}</strong> notes ·
            <strong>{embedStats.model_count}</strong> 模型
          {:else}
            索引规模读取中…
          {/if}
        </div>
        <div class="ai-provider-actions">
          <button
            onclick={embedCurrentNote}
            disabled={embedActionBusy || !vaultState.openFilePath}
            title={vaultState.openFilePath ? `Embed ${vaultState.openFilePath}` : '无打开的笔记'}
          >
            {embedBusy ? '处理中…' : 'Embed 当前笔记'}
          </button>
          <button onclick={previewEmbedVaultInit} disabled={embedActionBusy}>
            {embedInitPreviewLoading ? '预估中…' : '初始化索引'}
          </button>
          <button onclick={clearAllEmbeddings} disabled={embedActionBusy}> 清空 AI 索引 </button>
        </div>
        {#if embedNotice}
          <div
            class="ai-provider-test-result"
            class:ok={embedNotice.kind === 'ok'}
            class:err={embedNotice.kind === 'err'}
          >
            {embedNotice.text}
          </div>
        {/if}
      </div>

      <div class="settings-footer">
        <span class="settings-version">MyNotes v{APP_VERSION}</span>
        <button class="primary" onclick={closeSettings}>完成</button>
      </div>
    </div>
  </div>
{/if}

{#if summarizeOpen}
  <DiffPreviewModal
    open={summarizeOpen}
    title={summarizeTarget === 'frontmatter'
      ? 'AI 摘要 · 写入 frontmatter.summary'
      : 'AI 摘要 · 插入 TL;DR 到文首'}
    description={`将修改 ${summarizePath}`}
    original={summarizeOriginal}
    proposed={summarizeProposed}
    loading={summarizeLoading}
    cancelBusy={summarizeCanceling}
    error={summarizeError}
    statusNote={summarizeStatusNote}
    showRetry={summarizeError !== null || summarizeStatusNote.length > 0}
    onRetry={retrySummarize}
    acceptLabel={summarizeTarget === 'frontmatter' ? '写入 frontmatter' : '插入到文首'}
    onAccept={applySummarize}
    onDiscard={closeSummarize}
    onCancel={cancelSummarizeInFlight}
  />
{/if}

{#if suggestTagsOpen}
  <TagSuggestModal
    open={suggestTagsOpen}
    title="AI 建议标签 · 写入 frontmatter.tags"
    description={`将修改 ${suggestTagsPath}`}
    existingTags={suggestTagsExisting}
    candidates={suggestTagsCandidates}
    vaultTags={suggestTagsVault}
    loading={suggestTagsLoading}
    cancelBusy={suggestTagsCanceling}
    error={suggestTagsError}
    statusNote={suggestTagsStatusNote}
    showRetry={suggestTagsError !== null || suggestTagsStatusNote.length > 0}
    onRetry={retrySuggestTags}
    onAccept={applySuggestTags}
    onDiscard={closeSuggestTags}
    onCancel={cancelSuggestTagsInFlight}
  />
{/if}

{#if draftMocOpen}
  <DiffPreviewModal
    open={draftMocOpen}
    title={`AI 草拟 MOC · #${draftMocTag}`}
    description={`将创建 2-moc/${draftMocTitle}.md；下图是 AI 分组版本 vs 默认扁平列表的 diff`}
    original={draftMocFlat}
    proposed={draftMocProposed}
    loading={draftMocLoading}
    cancelBusy={draftMocCanceling}
    error={draftMocError}
    statusNote={draftMocStatusNote}
    showRetry={draftMocError !== null || draftMocStatusNote.length > 0}
    onRetry={retryDraftMoc}
    acceptLabel="创建 MOC（AI 分组）"
    onAccept={applyDraftMoc}
    onDiscard={closeDraftMoc}
    onCancel={cancelDraftMocInFlight}
  />
{/if}

{#if embedInitOpen && embedInitPreview}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={closeEmbedInitModal}
    onkeydown={onEmbedInitKey}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal modal-wide"
      role="dialog"
      aria-modal="true"
      aria-label="Initialize AI index"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>初始化 AI 索引</h3>
      <p class="modal-hint">
        先 dry-run 预估整库规模与成本，再确认执行。<br />
        <kbd>↵</kbd> 开始初始化 · <kbd>Esc</kbd> 取消
      </p>
      <div class="modal-preview">
        <div class="modal-preview-label">预览</div>
        <div class="modal-preview-body">
          将初始化 {embedInitPreview.note_count_to_embed} 篇笔记（vault 内共
          {embedInitPreview.note_count_total} 篇 Markdown；{embedInitPreview.note_count_up_to_date}
          篇已是最新；{embedInitPreview.note_count_empty} 篇为空）<br />
          预计写入 {embedInitPreview.chunk_count_to_embed} chunks · 约
          {embedInitPreview.token_count_estimated} tokens<br />
          {embedPreviewCostText(embedInitPreview)}
        </div>
        <div class="modal-preview-group">
          <div class="modal-preview-label">将初始化的笔记</div>
          <div class="modal-preview-body modal-preview-list">
            {#if embedInitPreview.note_count_to_embed === 0}
              <div>当前模型下没有待初始化的笔记。</div>
            {:else}
              {#each embedInitPreview.notes_preview as path}
                <div>{path}</div>
              {/each}
              {#if previewOverflow(embedInitPreview.note_count_to_embed, embedInitPreview.notes_preview.length) > 0}
                <div class="modal-preview-more">
                  … 另有 {previewOverflow(
                    embedInitPreview.note_count_to_embed,
                    embedInitPreview.notes_preview.length
                  )} 项
                </div>
              {/if}
            {/if}
          </div>
        </div>
      </div>
      {#if embedInitError}
        <p class="modal-error">{embedInitError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={closeEmbedInitModal} disabled={embedInitRunning}>取消</button>
        <button
          class="primary"
          onclick={runEmbedVaultInit}
          disabled={embedInitRunning || embedInitPreview.note_count_to_embed === 0}
        >
          {embedInitRunning ? '初始化中…' : '开始初始化'}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if mocBuilderOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    onclick={cancelBuildMoc}
    onkeydown={(e) => e.key === 'Escape' && cancelBuildMoc()}
    transition:fade={{ duration: 150 }}
  >
    <div
      class="modal moc-builder"
      role="dialog"
      aria-modal="true"
      aria-label="Build MOC from tag"
      onclick={(e) => e.stopPropagation()}
      onkeydown={onMocBuilderKey}
      tabindex="-1"
      transition:fly={{ y: 8, duration: 200 }}
    >
      <h3>从标签建立 MOC</h3>
      <p class="modal-hint">
        将 <code>#{mocBuilderTag}</code> 下的笔记汇总为新的
        <code>2-moc/&lt;slug&gt;.md</code>。勾选要收入的笔记。<br />
        <kbd>⌘↵</kbd> 确认 · <kbd>Esc</kbd> 取消
      </p>
      <input
        bind:this={mocBuilderTitleEl}
        bind:value={mocBuilderTitle}
        placeholder="MOC 标题"
        class="modal-input"
        disabled={mocBuilderRunning}
      />
      {#if mocBuilderLoading}
        <p class="modal-hint">加载中…</p>
      {:else if mocBuilderList.length === 0}
        <p class="modal-hint">该标签下没有任何笔记。创建后仅会生成模板骨架。</p>
      {:else}
        <div class="moc-builder-header">
          <label class="moc-builder-all">
            <input
              type="checkbox"
              checked={mocBuilderSelected.size === mocBuilderList.length}
              indeterminate={mocBuilderSelected.size > 0 &&
                mocBuilderSelected.size < mocBuilderList.length}
              onchange={toggleAllMocNotes}
              disabled={mocBuilderRunning}
            />
            全选 · 已选 {mocBuilderSelected.size} / {mocBuilderList.length}
          </label>
        </div>
        <ul class="moc-builder-list">
          {#each mocBuilderList as note (note.path)}
            <li>
              <label>
                <input
                  type="checkbox"
                  checked={mocBuilderSelected.has(note.path)}
                  onchange={() => toggleMocNote(note.path)}
                  disabled={mocBuilderRunning}
                />
                <span class="moc-builder-title">{note.title ?? note.path}</span>
                <span class="moc-builder-path">{note.path}</span>
              </label>
            </li>
          {/each}
        </ul>
      {/if}
      {#if mocBuilderError}
        <p class="modal-error">{mocBuilderError}</p>
      {/if}
      <div class="modal-actions">
        <button onclick={cancelBuildMoc} disabled={mocBuilderRunning}>取消</button>
        {#if aiEnabled}
          <button
            onclick={() => void confirmBuildMocWithAi()}
            disabled={mocBuilderRunning || mocBuilderLoading || mocBuilderSelected.size === 0}
            title="用 AI 对选中的笔记按主题分组"
          >
            用 AI 草拟…
          </button>
        {/if}
        <button
          class="primary"
          onclick={confirmBuildMoc}
          disabled={mocBuilderRunning || mocBuilderLoading}
        >
          {mocBuilderRunning ? '创建中…' : '创建 MOC'}
        </button>
      </div>
    </div>
  </div>
{/if}

<CommandPalette
  open={paletteOpen}
  onClose={() => (paletteOpen = false)}
  ctx={paletteCtx}
  commandHints={paletteCommandHints}
/>

<TweaksPanel
  theme={theme === 'dark'
    ? 'dark'
    : theme === 'light'
    ? 'light'
    : typeof window !== 'undefined' &&
      window.matchMedia('(prefers-color-scheme: dark)').matches
    ? 'dark'
    : 'light'}
  onSetTheme={(t) => setTheme(t)}
/>

{#snippet treeNode(entry: DirEntry, depth: number)}
  <li>
    <div
      class="tree-row-wrap"
      class:drop-target={dropTargetPath ===
        (entry.is_dir ? entry.rel_path : parentDirOf(entry.rel_path))}
      role="group"
      aria-label={`${entry.is_dir ? '文件夹' : '文件'} ${entry.name}`}
      oncontextmenu={(e) => openContextMenu(e, entry)}
      ondragover={(e) => onSidebarRowDragOver(entry, e)}
      ondragleave={(e) => onSidebarRowDragLeave(entry, e)}
      ondrop={(e) => onSidebarRowDrop(entry, e)}
    >
      <button
        class="tree-row"
        style="padding-left: {8 + depth * 16}px"
        onclick={() => (entry.is_dir ? toggleDir(entry) : openFile(entry))}
      >
        <span class="icon-slot">
          {entry.is_dir ? (expanded.has(entry.rel_path) ? '▾' : '▸') : '·'}
        </span>
        <span class="name">{entry.name}</span>
      </button>
      {#if entry.is_dir}
        <button
          class="row-action"
          title="在此文件夹新建笔记"
          aria-label={`在 ${entry.name} 中新建笔记`}
          onclick={(e) => {
            e.stopPropagation();
            newNote(entry.rel_path);
          }}
        >
          +
        </button>
      {/if}
    </div>
    {#if entry.is_dir && expanded.has(entry.rel_path) && childrenCache[entry.rel_path]}
      <ul>
        {#each childrenCache[entry.rel_path] as child (child.rel_path)}
          {@render treeNode(child, depth + 1)}
        {/each}
      </ul>
    {/if}
  </li>
{/snippet}

{#snippet homeView()}
  <div class="home">
    <div class="home-inner">
      <h1 class="home-title">MyNotes</h1>
      <p class="home-tagline">Link your thinking.</p>

      <div class="home-grid">
        <button class="home-card" onclick={() => openOrCreateDaily(cmdDeps)}>
          <div class="home-card-label">今日</div>
          <div class="home-card-value">{formatDate(new Date(), 'YYYY-MM-DD')}</div>
          <div class="home-card-hint">{formatDate(new Date(), 'ddd')} · ⌘D</div>
        </button>
        <button class="home-card" onclick={() => openOrCreateWeekly(cmdDeps)}>
          <div class="home-card-label">本周</div>
          <div class="home-card-value">{isoWeekString(new Date())}</div>
          <div class="home-card-hint">周记 · ⌘⇧W</div>
        </button>
        <button class="home-card" onclick={() => quickCapture(cmdDeps)}>
          <div class="home-card-label">快速记录</div>
          <div class="home-card-value">Capture</div>
          <div class="home-card-hint">投入 inbox · ⌘⇧N</div>
        </button>
        <button class="home-card" onclick={openInboxReview}>
          <div class="home-card-label">Inbox</div>
          <div class="home-card-value">{inboxCount}</div>
          <div class="home-card-hint">Review · 未处理</div>
        </button>
      </div>

      <div class="home-lists">
        <section class="home-list">
          <header class="home-list-head">
            <h3>最近编辑</h3>
            <span class="home-list-sub">1-notes · 2-moc · 4-projects</span>
          </header>
          {#if homeRecentNotes.length === 0}
            <p class="home-list-empty">还没有笔记。<kbd>⌘⇧N</kbd> 捕获第一条。</p>
          {:else}
            <ul>
              {#each homeRecentNotes as n (n.path)}
                <li>
                  <button
                    class="home-list-row"
                    onclick={() =>
                      openFile({
                        name: n.path.slice(n.path.lastIndexOf('/') + 1),
                        rel_path: n.path,
                        is_dir: false
                      })}
                    title={n.path}
                  >
                    <span class="home-list-title"
                      >{n.title ??
                        n.path.slice(n.path.lastIndexOf('/') + 1).replace(/\.md$/, '')}</span
                    >
                    <span class="home-list-meta">{n.updated ?? ''}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        <section class="home-list">
          <header class="home-list-head">
            <h3>MOCs</h3>
            <button class="home-list-action" onclick={() => paletteCtx.runNewMoc()}>+ 新建</button>
          </header>
          {#if homeRecentMocs.length === 0}
            <p class="home-list-empty">还没有 MOC。MOC 是把若干笔记串成主题的「枢纽」。</p>
          {:else}
            <ul>
              {#each homeRecentMocs as m (m.path)}
                <li>
                  <button
                    class="home-list-row"
                    onclick={() =>
                      openFile({
                        name: m.path.slice(m.path.lastIndexOf('/') + 1),
                        rel_path: m.path,
                        is_dir: false
                      })}
                    title={m.path}
                  >
                    <span class="home-list-title"
                      >{m.title ??
                        m.path.slice(m.path.lastIndexOf('/') + 1).replace(/\.md$/, '')}</span
                    >
                    <span class="home-list-meta">{m.updated ?? ''}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      </div>

      <div class="home-footer">
        <div class="home-stat" title="全库 unresolved wiki-link 目标数量">
          <span class="home-stat-label">Unresolved links</span>
          <span class="home-stat-value" class:warn={homeUnresolved > 0}>{homeUnresolved}</span>
        </div>
        {#if homeReview}
          {@const r = homeReview}
          <button
            class="home-review"
            onclick={() =>
              openFile({
                name: r.path.slice(r.path.lastIndexOf('/') + 1),
                rel_path: r.path,
                is_dir: false
              })}
            title={r.path}
          >
            <span class="home-review-label">📜 旧笔记回顾</span>
            <span class="home-review-title"
              >{r.title ?? r.path.slice(r.path.lastIndexOf('/') + 1).replace(/\.md$/, '')}</span
            >
          </button>
        {/if}
      </div>
    </div>
  </div>
{/snippet}

<style>
  .welcome {
    display: grid;
    place-items: center;
    height: 100vh;
    height: 100dvh;
  }
  .welcome-inner {
    text-align: center;
    max-width: 480px;
  }
  .welcome h1 {
    margin: 0 0 8px;
    font-family: var(--font-serif);
    font-size: 36px;
    font-weight: 400;
    letter-spacing: -0.02em;
  }
  .tagline {
    margin: 0 0 24px;
    color: var(--color-fg-muted);
  }
  .runtime-note {
    margin: 0 0 16px;
    color: var(--color-fg-muted);
    line-height: 1.5;
  }
  .runtime-note code {
    font-family: var(--font-mono);
    font-size: 12px;
  }
  .recent {
    margin-top: 36px;
    text-align: left;
  }
  .recent h3 {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    color: var(--color-fg-dim);
    margin: 0 0 8px;
    letter-spacing: 0.08em;
  }
  .recent ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .recent li {
    margin: 2px 0;
  }
  .link {
    border: none;
    padding: 4px 8px;
    text-align: left;
    width: 100%;
    color: var(--color-accent);
    font-family: var(--font-mono);
    font-size: 12px;
  }

  .app {
    display: grid;
    grid-template-columns:
      var(--rail-width, 56px)
      var(--sidebar-width)
      1fr
      var(--panel-width, 300px);
    grid-template-rows: minmax(0, 1fr) var(--statusbar-height);
    grid-template-areas:
      'rail side main panel'
      'status status status status';
    height: 100vh;
    height: 100dvh;
    background: var(--color-bg);
    min-height: 0;
    overflow: hidden;
  }
  /* With no note open the right panel is hidden; grid collapses to 3 cols. */
  .app.app-no-panel {
    grid-template-columns:
      var(--rail-width, 56px)
      var(--sidebar-width)
      1fr;
    grid-template-areas:
      'rail side main'
      'status status status';
  }
  .app.app-no-panel .editor-pane {
    margin-right: 14px;
  }
  .app.app-graph-focus {
    grid-template-columns:
      var(--rail-width, 56px)
      1fr
      var(--panel-width, 300px);
    grid-template-areas:
      'rail main panel'
      'status status status';
  }
  .app.app-graph-focus.app-no-panel {
    grid-template-columns:
      var(--rail-width, 56px)
      1fr;
    grid-template-areas:
      'rail main'
      'status status';
  }
  .app.app-graph-focus .editor-pane {
    margin-left: 7px;
  }
  /* IconRail + KnowledgeColumn are separate components; place them by class. */
  .app :global(.rail) {
    grid-area: rail;
    min-height: 0;
  }
  .app :global(.kb-col) {
    grid-area: side;
    min-height: 0;
  }
  .editor-pane {
    grid-area: main;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--color-surface);
    margin: 14px 7px 14px 0;
    border-radius: var(--radius-xl);
    box-shadow: var(--pane-border);
    overflow: hidden;
    position: relative; /* anchor the floating TodayTasksPanel */
  }
  /* Middle-pane chat (standalone variant mounted directly in .editor-pane):
     make it a proper flex-column child so the composer stays pinned at the
     bottom and doesn't overflow the pane. Scoped so Panel.svelte's docked
     tab and the standalone webview mount are unaffected. */
  .editor-pane > :global(.chat-panel) {
    flex: 1 1 0;
    min-height: 0;
    height: auto;
  }
  .kb-section-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 6px 10px;
    font-family: var(--font-serif);
    font-size: 13px;
    color: var(--color-fg-muted);
    letter-spacing: -0.01em;
  }
  .kb-section-head .vault-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .panel-slot {
    grid-area: panel;
    display: flex;
    min-width: 0;
    min-height: 0;
    background: var(--color-surface);
    margin: 14px 14px 14px 7px;
    border-radius: var(--radius-xl);
    box-shadow: var(--pane-border);
    overflow: hidden;
  }
  .panel-slot :global(.panel) {
    flex: 1;
    border-left: none !important;
    background: transparent !important;
    padding: 0 !important;
  }
  .status-bar {
    grid-area: status;
    min-height: 0;
  }
  .vault-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  button.icon {
    padding: 6px;
    font-size: 13px;
    border: none;
    box-shadow: none;
    background: transparent;
    color: var(--color-fg-muted);
    border-radius: var(--radius-sm);
  }
  button.icon:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    transform: none;
  }
  button.icon:disabled {
    opacity: 0.35;
    cursor: default;
  }
  button.icon:disabled:hover {
    background: transparent;
  }
  .header-buttons {
    display: flex;
    gap: 4px;
  }
  .cmd {
    padding: 6px 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.04em;
    font-weight: 500;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-fg-muted);
    cursor: pointer;
    box-shadow: none;
    transition:
      background 0.15s ease,
      color 0.15s ease,
      box-shadow 0.15s ease,
      transform 0.15s ease;
  }
  .cmd:hover {
    background: var(--color-surface-raised);
    color: var(--color-fg);
    border-color: transparent;
    box-shadow: var(--pane-border);
    transform: translateY(-0.5px);
  }
  kbd {
    font-family: var(--font-mono);
    font-size: 10px;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: 3px;
    padding: 0 4px;
  }
  .tree-row-wrap {
    display: flex;
    align-items: center;
    position: relative;
  }
  .tree-row-wrap:hover .row-action {
    opacity: 1;
  }
  /* P3-A6: drop-import visual target. The `drop-target` class tracks
     whichever directory would receive the current Finder drag. We highlight
     the whole row-wrap rather than the inner button because the drop zone
     includes the padding area on the right. */
  .tree-row-wrap.drop-target {
    background: var(--color-accent-tint, rgba(180, 120, 60, 0.08));
    outline: 1px solid var(--color-accent, #b4783c);
    outline-offset: -1px;
    border-radius: var(--radius-sm);
  }
  /* The empty-area fallback target — drops land in `0-inbox/`. */
  .tree.drop-root-active {
    outline: 2px dashed var(--color-accent, #b4783c);
    outline-offset: -4px;
    border-radius: var(--radius-sm);
  }
  .row-action {
    opacity: 0;
    position: absolute;
    right: 8px;
    padding: 0 6px;
    font-size: 12px;
    border: none;
    background: transparent;
    color: var(--color-fg-muted);
    border-radius: 3px;
    cursor: pointer;
  }
  .row-action:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
  }
  .tree,
  .tree ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .tree-row {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    border: none;
    text-align: left;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    background: transparent;
    color: inherit;
    font-size: 13px;
    margin: 2px 8px;
    width: calc(100% - 16px);
    transition:
      background 0.15s ease,
      transform 0.15s ease;
  }
  .tree-row:hover {
    background: var(--color-bg-hover);
    transform: translateX(2px);
  }
  .icon-slot {
    width: 14px;
    text-align: center;
    color: var(--color-fg-muted);
    flex-shrink: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* ---- status bar --------------------------------------------------- */
  .status-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 var(--space-4);
    box-shadow: inset 0 1px 0 var(--color-border);
    background: transparent;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    letter-spacing: 0.04em;
    color: var(--color-fg-dim);
    user-select: none;
  }
  .sb-group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .sb-left .sb-item {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sb-sep {
    color: var(--color-border);
  }
  .sb-save[data-state='saving'] {
    color: var(--color-warning);
  }
  .sb-save[data-state='saved'] {
    color: var(--color-success);
  }
  .sb-save[data-state='error'] {
    color: var(--color-danger);
  }
  .sb-icon-btn {
    padding: 0 var(--space-1);
    border: none;
    background: transparent;
    color: var(--color-fg-muted);
    font-size: var(--fs-md);
    line-height: var(--statusbar-height);
    cursor: pointer;
  }
  .sb-icon-btn:hover {
    color: var(--color-fg);
    background: transparent;
  }

  /* ---- notices ------------------------------------------------------ */
  .notice-stack {
    position: fixed;
    top: 16px;
    right: 16px;
    width: min(420px, calc(100vw - 24px));
    display: grid;
    gap: 10px;
    z-index: 200;
    pointer-events: none;
  }
  .notice {
    pointer-events: auto;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    align-items: start;
    padding: 12px 14px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    background: color-mix(in oklab, var(--color-surface-raised) 94%, white 6%);
    box-shadow:
      var(--pane-border),
      0 10px 28px rgba(0, 0, 0, 0.14);
  }
  .notice[data-kind='success'] {
    border-color: color-mix(in oklab, var(--color-success) 28%, var(--color-border));
  }
  .notice[data-kind='error'] {
    border-color: color-mix(in oklab, var(--color-danger) 32%, var(--color-border));
  }
  .notice[data-kind='info'] {
    border-color: color-mix(in oklab, var(--color-accent) 24%, var(--color-border));
  }
  .notice-copy {
    min-width: 0;
  }
  .notice-label {
    margin-bottom: 4px;
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-dim);
  }
  .notice[data-kind='success'] .notice-label {
    color: var(--color-success);
  }
  .notice[data-kind='error'] .notice-label {
    color: var(--color-danger);
  }
  .notice[data-kind='info'] .notice-label {
    color: var(--color-accent);
  }
  .notice-message {
    font-size: 13px;
    line-height: 1.55;
    color: var(--color-fg);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .notice-close {
    border: none;
    background: transparent;
    color: var(--color-fg-muted);
    font-size: 18px;
    line-height: 1;
    padding: 0;
    cursor: pointer;
  }
  .notice-close:hover,
  .notice-close:focus-visible {
    color: var(--color-fg);
    outline: none;
  }

  /* ---- home view ---------------------------------------------------- */
  .home {
    display: flex;
    justify-content: center;
    align-items: flex-start;
    height: 100%;
    min-height: 0;
    padding: var(--space-8);
    overflow: auto;
  }
  .home-inner {
    width: 100%;
    max-width: 640px;
    text-align: center;
  }
  .home-title {
    margin: 0;
    font-family: var(--font-serif);
    font-size: var(--fs-3xl);
    font-weight: 400;
    letter-spacing: -0.02em;
  }
  .home-tagline {
    margin: var(--space-1) 0 var(--space-6);
    color: var(--color-fg-muted);
    font-size: var(--fs-md);
  }
  .home-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }
  .home-card {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    padding: var(--space-4) var(--space-5);
    border: 1px solid transparent;
    border-radius: var(--radius-lg);
    background: var(--color-surface-raised);
    box-shadow: var(--pane-border);
    text-align: left;
    cursor: pointer;
    transition:
      transform 0.15s ease,
      box-shadow 0.15s ease,
      background 0.15s ease;
  }
  .home-card:hover {
    transform: translateY(-1px);
    background: var(--color-surface-raised);
    box-shadow:
      var(--pane-border),
      0 0 0 1px var(--color-accent-weak),
      var(--accent-glow);
  }
  .home-card-label {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-dim);
  }
  .home-card-value {
    font-family: var(--font-serif);
    font-size: var(--fs-2xl);
    font-weight: 500;
    letter-spacing: -0.01em;
  }
  .home-card-hint {
    font-size: var(--fs-sm);
    color: var(--color-fg-muted);
  }

  .home-lists {
    margin-top: var(--space-6);
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-5);
    text-align: left;
  }
  .home-list {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .home-list-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: var(--space-2);
    padding-bottom: var(--space-1);
    border-bottom: 1px solid var(--color-border);
  }
  .home-list-head h3 {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    font-weight: 500;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-dim);
  }
  .home-list-sub {
    font-size: 11px;
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
  }
  .home-list-action {
    border: none;
    background: transparent;
    color: var(--color-accent);
    font-size: 11px;
    cursor: pointer;
    padding: 0;
  }
  .home-list-action:hover {
    text-decoration: underline;
  }
  .home-list-empty {
    margin: 0;
    padding: var(--space-2) 0;
    font-size: 12px;
    color: var(--color-fg-muted);
    line-height: 1.7;
  }
  .home-list ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .home-list li {
    margin: 0;
  }
  .home-list-row {
    width: 100%;
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
    padding: 6px 8px;
    border: none;
    background: transparent;
    color: inherit;
    text-align: left;
    border-radius: 4px;
    cursor: pointer;
    font: inherit;
    transition:
      background 0.15s ease,
      transform 0.15s ease;
  }
  .home-list-row:hover {
    background: var(--color-bg-subtle);
    transform: translateX(2px);
  }
  .home-list-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
    font-size: 13px;
  }
  .home-list-meta {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-fg-muted);
    flex-shrink: 0;
  }

  .home-footer {
    margin-top: var(--space-6);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px dashed var(--color-border);
    flex-wrap: wrap;
    text-align: left;
  }
  .home-stat {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    font-size: 12px;
    color: var(--color-fg-muted);
  }
  .home-stat-label {
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .home-stat-value {
    font-family: var(--font-mono);
    font-size: 14px;
    color: var(--color-fg);
  }
  .home-stat-value.warn {
    color: var(--color-warning);
  }
  .home-review {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    background: var(--color-surface-raised);
    border: 1px solid transparent;
    box-shadow: var(--pane-border);
    border-radius: var(--radius-md);
    padding: 8px 14px;
    cursor: pointer;
    max-width: 60%;
    min-width: 0;
    text-align: left;
    transition:
      box-shadow 0.15s ease,
      transform 0.15s ease;
  }
  .home-review:hover {
    transform: translateY(-1px);
    box-shadow:
      var(--pane-border),
      0 0 0 1px var(--color-accent-weak),
      var(--accent-glow);
  }
  .home-review-label {
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-dim);
  }
  .home-review-title {
    font-size: 13px;
    color: var(--color-fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 320px;
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    backdrop-filter: blur(2px);
    -webkit-backdrop-filter: blur(2px);
    display: grid;
    place-items: center;
    z-index: 100;
    border: none;
    padding: 0;
  }
  .modal {
    background: var(--glass-bg);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
    border: 1px solid transparent;
    border-radius: var(--radius-lg);
    padding: 24px 28px;
    width: 440px;
    max-width: calc(100vw - 40px);
    /* Cap height to the viewport (minus symmetric gutter) and let the
     * body scroll when content overflows. Without this, the grid-centered
     * backdrop clips oversized modals (e.g. Settings) at the viewport top
     * + bottom with no way to reach cut-off content. Harmless for short
     * modals — they only grow to their natural height. */
    max-height: calc(100vh - 40px);
    overflow-y: auto;
    overscroll-behavior: contain;
    box-shadow: var(--pane-border), var(--glass-shadow);
  }
  .modal h3 {
    margin: 0 0 8px;
    font-family: var(--font-serif);
    font-size: 20px;
    font-weight: 500;
    letter-spacing: -0.01em;
  }
  .modal-hint {
    margin: 0 0 12px;
    font-size: 12px;
    color: var(--color-fg-muted);
    line-height: 1.5;
  }
  .modal-hint code {
    background: var(--color-surface-raised);
    box-shadow: var(--pane-border);
    padding: 1px 6px;
    border-radius: 4px;
    font-size: 11px;
  }
  .modal-input {
    width: 100%;
    padding: 9px 12px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 13px;
    background: var(--color-surface-raised);
    box-shadow: var(--pane-border);
    color: var(--color-fg);
    box-sizing: border-box;
    transition: box-shadow 0.15s ease;
  }
  .modal-input:focus {
    outline: none;
    box-shadow:
      var(--pane-border),
      0 0 0 2px var(--color-accent-weak),
      var(--accent-glow);
  }
  .modal-error {
    margin: 8px 0 0;
    color: var(--color-danger);
    font-size: 12px;
  }

  /* Sidebar context menu — transparent full-screen backdrop anchors a small
     floating menu positioned by fixed coords. */
  .ctx-menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 120; /* above modals so chained right-click still dismisses */
    background: transparent;
    border: none;
    padding: 0;
  }
  .ctx-menu {
    position: fixed;
    min-width: 200px;
    padding: 6px 0;
    background: var(--glass-bg);
    backdrop-filter: blur(24px);
    -webkit-backdrop-filter: blur(24px);
    border-radius: var(--radius-md, 10px);
    box-shadow: var(--pane-border), var(--glass-shadow);
    font-size: 13px;
    color: var(--color-fg);
  }
  .ctx-menu-header {
    padding: 4px 12px 6px;
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.06em;
    color: var(--color-fg-dim);
    text-transform: uppercase;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 260px;
    border-bottom: 1px solid var(--color-border);
    margin-bottom: 4px;
  }
  .ctx-menu-item {
    display: block;
    width: 100%;
    padding: 6px 14px;
    background: transparent;
    border: none;
    text-align: left;
    font-size: 13px;
    color: inherit;
    cursor: pointer;
    border-radius: 0;
  }
  .ctx-menu-item:hover,
  .ctx-menu-item:focus-visible {
    background: var(--color-surface-raised, rgba(0, 0, 0, 0.04));
    outline: none;
  }
  .ctx-menu-item.danger {
    color: var(--color-danger);
  }
  .ctx-menu-item.danger:hover {
    background: color-mix(in oklab, var(--color-danger) 10%, transparent);
  }
  .ctx-menu-sep {
    height: 1px;
    background: var(--color-border);
    margin: 4px 0;
  }
  .modal-preview {
    margin-top: 10px;
  }
  .modal-preview-group {
    margin-top: 10px;
  }
  .modal-preview-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-dim);
    margin-bottom: 4px;
  }
  .modal-preview-body {
    margin: 0;
    padding: 10px 12px;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
    max-height: 180px;
    overflow: auto;
    white-space: pre-wrap;
    color: var(--color-fg-muted);
  }
  .modal-preview-list {
    display: grid;
    gap: 4px;
  }
  .modal-preview-more {
    color: var(--color-fg-dim);
  }
  .modal-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
  .modal-actions button {
    padding: 6px 14px;
    font-size: 13px;
  }
  .modal-wide {
    width: 560px;
  }
  .moc-builder {
    width: 560px;
  }

  @media (max-width: 720px) {
    .notice-stack {
      top: 12px;
      right: 12px;
      left: 12px;
      width: auto;
    }
  }

  @media (max-height: 760px) {
    .editor-pane {
      margin-top: 8px;
      margin-bottom: 8px;
    }
    .app.app-no-panel .editor-pane {
      margin-right: 8px;
    }
    .panel-slot {
      margin-top: 8px;
      margin-right: 8px;
      margin-bottom: 8px;
    }
  }
  .moc-builder-header {
    margin-top: 10px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--color-border);
  }
  .moc-builder-all {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--color-fg-muted);
    cursor: pointer;
    user-select: none;
  }
  .moc-builder-list {
    list-style: none;
    margin: 0;
    padding: 4px 0 0 0;
    max-height: 320px;
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
  }
  .moc-builder-list li {
    padding: 0;
  }
  .moc-builder-list label {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    cursor: pointer;
    border-bottom: 1px solid var(--color-border-subtle, var(--color-border));
  }
  .moc-builder-list label:last-child {
    border-bottom: none;
  }
  .moc-builder-list label:hover {
    background: var(--color-bg-hover, var(--color-bg-muted));
  }
  .moc-builder-title {
    font-size: 13px;
    color: var(--color-fg);
    flex: 0 0 auto;
    max-width: 55%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .moc-builder-path {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-fg-dim);
    flex: 1 1 auto;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
  }
  .unused-toolbar {
    margin-top: 8px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--color-border);
  }
  .unused-list {
    max-height: 320px;
    overflow-y: auto;
    margin-top: 2px;
    border-radius: var(--radius-sm);
  }
  .unused-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    font-size: 12px;
    cursor: pointer;
  }
  .unused-row:hover {
    background: var(--color-bg-hover);
  }
  .unused-row-header {
    font-weight: 500;
    color: var(--color-fg-muted);
    cursor: pointer;
  }
  .unused-row input[type='checkbox'] {
    margin: 0;
    flex: 0 0 auto;
    cursor: pointer;
  }
  .unused-path {
    flex: 1 1 auto;
    min-width: 0;
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .unused-size {
    flex: 0 0 auto;
    color: var(--color-fg-muted);
    font-variant-numeric: tabular-nums;
  }

  /* -------------------------------------------------------------------------
   * Settings modal. Wider than the default modal so the radio row and
   * autosave hint don't wrap awkwardly. Footer pairs version string +
   * close button on a single line.
   */
  .settings-modal {
    width: 520px;
  }
  .settings-section {
    margin-top: 14px;
    padding-top: 14px;
    border-top: 1px solid var(--color-border);
  }
  .settings-section:first-of-type {
    margin-top: 6px;
    padding-top: 0;
    border-top: none;
  }
  .settings-label {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-fg-dim);
    margin-bottom: 8px;
  }
  .settings-radio-group {
    display: flex;
    gap: 18px;
    align-items: center;
  }
  .settings-radio {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    font-size: 13px;
    color: var(--color-fg);
  }
  .settings-radio input[type='radio'] {
    margin: 0;
    cursor: pointer;
  }
  .settings-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .settings-shortcuts {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .shortcut-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .shortcut-meta {
    min-width: 0;
  }
  .shortcut-name {
    color: var(--color-fg);
    font-size: 13px;
  }
  .shortcut-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 0 0 auto;
  }
  .shortcut-capture {
    min-width: 116px;
    font-family: var(--font-mono);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .shortcut-recording {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--color-accent) 28%, transparent);
  }
  .shortcut-reset {
    font-size: 12px;
  }
  .settings-number {
    width: 96px;
    padding: 6px 8px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
    color: var(--color-fg);
    font-family: var(--font-mono);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .settings-unit {
    color: var(--color-fg-muted);
    font-size: 12px;
  }
  .settings-hint {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--color-fg-muted);
    line-height: 1.55;
  }
  .settings-hint code {
    font-family: var(--font-mono);
    font-size: 11px;
    background: var(--color-bg);
    padding: 1px 4px;
    border-radius: 3px;
  }
  .ai-tool-permissions {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .ai-tool-permission {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-bg);
  }
  .ai-tool-permission input[type='checkbox'] {
    margin-top: 2px;
  }
  .ai-tool-permission span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ai-tool-permission strong {
    font-size: 13px;
    color: var(--color-fg);
  }
  .ai-tool-permission small {
    font-size: 12px;
    color: var(--color-fg-muted);
    line-height: 1.45;
  }
  .settings-disabled {
    opacity: 0.55;
    pointer-events: none;
  }
  .settings-msg {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
  }
  .settings-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 20px;
    padding-top: 14px;
    border-top: 1px solid var(--color-border);
  }
  .settings-version {
    font-family: var(--font-mono);
    font-size: 11px;
    letter-spacing: 0.04em;
    color: var(--color-fg-dim);
  }
  /* AI Provider subsection (Settings · D2a.2) */
  .ai-provider-grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 10px;
    margin-top: 10px;
  }
  .ai-provider-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }
  .ai-provider-field > span {
    font-weight: 500;
    color: var(--color-fg-muted);
  }
  .ai-provider-field input {
    font-family: var(--font-mono);
    font-size: 12px;
    padding: 6px 8px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    background: var(--color-bg);
    color: var(--color-fg);
  }
  .ai-provider-field input:focus {
    outline: none;
    border-color: var(--color-accent);
  }
  .ai-provider-actions {
    display: flex;
    gap: 8px;
    margin-top: 12px;
  }
  .ai-provider-test-result {
    margin-top: 10px;
    padding: 8px 10px;
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
  }
  .ai-provider-test-result.ok {
    background: color-mix(in srgb, var(--color-accent) 12%, transparent);
    color: var(--color-accent);
    border: 1px solid color-mix(in srgb, var(--color-accent) 28%, transparent);
  }
  .ai-provider-test-result.err {
    background: color-mix(in srgb, #e04a4a 12%, transparent);
    color: #c94545;
    border: 1px solid color-mix(in srgb, #e04a4a 35%, transparent);
  }
  .ai-embed-stats {
    margin-top: 10px;
    padding: 8px 10px;
    border-radius: 4px;
    background: var(--color-bg-muted, #f5f5f4);
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.5;
  }
  .ai-embed-stats strong {
    color: var(--color-fg);
    font-weight: 600;
  }

  /* -------------------------------------------------------------------------
   * Print layout — legacy backup path.
   *
   * **Not the primary print flow anymore.** `> Print current note` now
   * renders the note to a standalone HTML file via `note_render_print_html`
   * and hands off to the system default browser (the Tauri WKWebView
   * silently drops programmatic `window.print()` calls on macOS, and
   * CodeMirror's viewport virtualization only keeps on-screen lines in
   * the DOM — `@media print` here could only ever capture the visible
   * slice). This block stays in case a user manually hits `⌘P` while
   * the app is focused, so that attempt still produces something
   * passable rather than the three-column grid.
   */
  @media print {
    :global(html, body) {
      overflow: visible !important;
      background: #fff !important;
      color: #000 !important;
    }
    .app {
      display: block !important;
      height: auto !important;
      background: #fff !important;
    }
    .app :global(.rail),
    .app :global(.kb-col),
    .panel-slot,
    .status-bar,
    .modal-backdrop {
      display: none !important;
    }
    .editor-pane {
      margin: 0 !important;
      box-shadow: none !important;
      border-radius: 0 !important;
      overflow: visible !important;
      background: #fff !important;
    }
    /* Let the editor content flow naturally across page breaks. */
    :global(.cm-editor) {
      height: auto !important;
    }
    :global(.cm-scroller) {
      overflow: visible !important;
      height: auto !important;
    }
  }
</style>
