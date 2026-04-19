<script lang="ts">
  import { onMount } from 'svelte';
  import { open as openDialog, ask, message } from '@tauri-apps/plugin-dialog';
  import { vaultInit, vaultOpen, vaultIsInitialized, vaultRecent, vaultReseedTemplates } from '$lib/ipc/vault';
  import type { DirEntry } from '$lib/ipc/vault';
  import { fileExists, fileList, fileRead, fileWrite } from '$lib/ipc/file';
  import { vaultState } from '$lib/state/vault.svelte';
  import Editor from '$lib/editor/Editor.svelte';
  import Panel from '$lib/panel/Panel.svelte';
  import TagsSection from '$lib/tags/TagsSection.svelte';
  import ProjectsSection from '$lib/projects/ProjectsSection.svelte';
  import TagView from '$lib/tags/TagView.svelte';
  import InboxView from '$lib/inbox/InboxView.svelte';
  import CommandPalette from '$lib/palette/CommandPalette.svelte';
  import type { PaletteContext } from '$lib/palette/commandRegistry';
  import { projectSlugFromPath } from '$lib/palette/commandRegistry';
  import { invalidateWikiCompletionCache } from '$lib/editor/wikicomplete';
  import { fileDelete, fileMove } from '$lib/ipc/file';
  import { indexAllNotes, indexUnresolvedCount, type NoteRef } from '$lib/ipc/index';
  import { projectSetStatus } from '$lib/ipc/project';
  import {
    appendDailyRecord,
    commandDefs,
    createNoteFromTemplate,
    openOrCreateDaily,
    openOrCreateWeekly,
    promoteInboxNote,
    quickCapture,
    rewriteFrontmatter,
    slugifyTitle,
    type CommandDeps
  } from '$lib/commands';
  import { formatDate, isoWeekString } from '$lib/template';
  import { fade, fly } from 'svelte/transition';

  type RuntimeMode = 'tauri' | 'browser';
  type PendingSave = {
    path: string;
    content: string;
    vaultPath: string;
  };

  let recent = $state<string[]>([]);
  let tree = $state<DirEntry[]>([]);
  let expanded = $state<Set<string>>(new Set());
  let editorContent = $state<string>('');
  let saveStatus = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
  let saveError = $state<string>('');
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let runtimeMode = $state<RuntimeMode>('tauri');

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

  /** Which full-pane "view" is shown instead of the editor. `null` = editor/home. */
  let activeView = $state<'inbox' | null>(null);

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
      saveStatus = 'error';
      saveError = `archive ${path}: ${String(e)}`;
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
      saveStatus = 'error';
      saveError = `delete ${path}: ${String(e)}`;
    }
  }

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
    promoteCurrent: () => {
      const p = vaultState.openFilePath;
      if (!p || !p.startsWith('0-inbox/')) {
        saveError = 'Promote 仅对 0-inbox/ 下的笔记可用';
        saveStatus = 'error';
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
    closeVault: () => {
      void drainPendingSaves().then(() => {
        resetVaultViewState();
        vaultState.clear();
      });
    },
    currentFilePath: vaultState.openFilePath
  });

  // Theme: 'system' follows OS; 'light'/'dark' override. Persisted in localStorage.
  type Theme = 'system' | 'light' | 'dark';
  const THEME_KEY = 'mynotes:theme';
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
    // Restore persisted theme preference (defaults to 'system').
    try {
      const saved = localStorage.getItem(THEME_KEY) as Theme | null;
      if (saved === 'light' || saved === 'dark' || saved === 'system') {
        theme = saved;
      }
    } catch {
      /* localStorage unavailable — stay on 'system'. */
    }
    applyTheme(theme);
    const off = installShortcuts();

    // Catch async errors (e.g. file_write failing inside a setTimeout callback
    // or a command launched from a button) so they show up in the status bar
    // instead of disappearing as "Unhandled Promise Rejection".
    const onUnhandled = (e: PromiseRejectionEvent) => {
      saveStatus = 'error';
      saveError = String(e.reason);
      console.error('[unhandled rejection]', e.reason);
    };
    window.addEventListener('unhandledrejection', onUnhandled);

    if (isTauriRuntime()) {
      runtimeMode = 'tauri';
      void loadRecentVaults();
    } else {
      runtimeMode = 'browser';
      recent = [];
    }

    return () => {
      clearSaveTimer();
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

  /** Rotate system → light → dark → system. Persist and apply. */
  function cycleTheme() {
    const next: Theme = theme === 'system' ? 'light' : theme === 'light' ? 'dark' : 'system';
    theme = next;
    try {
      localStorage.setItem(THEME_KEY, next);
    } catch {
      /* ignore */
    }
    applyTheme(next);
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
    activeTag = null;
    activeView = null;
    vaultState.closeFile();
  }

  /** Capture-phase keydown listener for app-wide shortcuts. Returns cleanup fn. */
  function installShortcuts(): () => void {
    const handler = (e: KeyboardEvent) => {
      if (!vaultState.current) return;
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      // Ignore keys originating from inside a modal input/textarea — the modal
      // has its own handlers (Enter / Cmd+Enter / Esc).
      const target = e.target as HTMLElement | null;
      if (target?.closest('.modal')) return;

      const key = e.key.toLowerCase();
      const shift = e.shiftKey;
      if (key === 'p' && !shift) {
        e.preventDefault();
        e.stopPropagation();
        paletteOpen = true;
      } else if (key === 'd' && !shift) {
        e.preventDefault();
        e.stopPropagation();
        openOrCreateDaily(cmdDeps);
      } else if (key === 'w' && shift) {
        e.preventDefault();
        e.stopPropagation();
        openOrCreateWeekly(cmdDeps);
      } else if (key === 'n' && shift) {
        e.preventDefault();
        e.stopPropagation();
        quickCapture(cmdDeps);
      } else if (key === 'd' && shift) {
        e.preventDefault();
        e.stopPropagation();
        openRecord();
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
   * Open the Promote modal for the given inbox note. The input is pre-filled
   * with the file's stem so the user usually just tweaks casing / wording
   * before hitting Enter.
   */
  function openPromoteModal(path: string) {
    if (!path.startsWith('0-inbox/')) {
      saveError = 'Promote 仅对 0-inbox/ 下的笔记可用';
      saveStatus = 'error';
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

  /**
   * Palette command: set `status:` frontmatter on the current project's
   * `4-projects/<slug>/index.md`. Slug is derived from the open file path,
   * so the user never has to disambiguate — it's always the project they're
   * currently looking at.
   *
   * Errors surface through the standard save-status banner. After success
   * we kick `refreshHomeData` so the Home page's Active Projects card
   * reflects the new bucket immediately — the backend already reindexed
   * synchronously, but the home aggregate is a separate query.
   */
  async function runSetProjectStatus(status: string) {
    const slug = projectSlugFromPath(vaultState.openFilePath);
    if (!slug) {
      saveError = 'Set project status 仅对 4-projects/ 下的笔记可用';
      saveStatus = 'error';
      return;
    }
    try {
      // Flush in-flight edits to index.md first so they don't race the
      // frontmatter rewrite (backend reads file → edits → atomic_write).
      await drainPendingSaves();
      await projectSetStatus(slug, status);
      saveStatus = 'saved';
      saveError = '';
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
    } catch (err) {
      saveError = `设置项目状态失败：${err instanceof Error ? err.message : String(err)}`;
      saveStatus = 'error';
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
      saveError = 'Add Note to Project 仅对 4-projects/<slug>/ 下的笔记可用';
      saveStatus = 'error';
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
      saveError = 'Extract from project 仅对项目下的非 index.md 笔记可用';
      saveStatus = 'error';
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
      saveError = `找不到空闲的目标文件名: ${dst}`;
      saveStatus = 'error';
      return;
    }
    try {
      // Flush pending edits to the project-note first, otherwise the save
      // timer fires after we delete the source and complains loudly.
      await drainPendingSaves();
      const body = await fileRead(src);
      const now = formatDate(new Date(), 'YYYY-MM-DD HH:mm');
      // Only touch `type` + `updated`. Preserve user tags/title/etc. — the
      // note is "the same note in a new place", not a fresh file.
      const newBody = rewriteFrontmatter(body, { type: 'note', updated: now });

      // Write new → delete old. Don't swap — a crash after delete but before
      // write would lose the note entirely.
      await fileWrite(dst, newBody);
      await fileDelete(src);

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
      saveStatus = 'saved';
      saveError = '';
    } catch (err) {
      saveError = `抽离失败：${err instanceof Error ? err.message : String(err)}`;
      saveStatus = 'error';
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
   * user edits to bundled template files will be lost. On success we surface
   * a concise `saveStatus` banner with the added/updated/unchanged counts.
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
      // Also surface in the status bar for visual continuity.
      saveError = `模板已同步：${parts.join(' · ') || '无变化'}`;
      saveStatus = 'saved';
    } catch (err) {
      // Important: show the FULL error in a modal — the status-bar tooltip is
      // easy to miss. Rust side returns AppError::to_string() (e.g. `io: ...`).
      const msg = err instanceof Error ? err.message : String(err);
      console.error('[reseed] failed:', err);
      await message(`Reseed 失败：\n${msg}`, {
        title: 'Reseed templates',
        kind: 'error'
      });
      saveError = `Reseed 失败：${msg}`;
      saveStatus = 'error';
    }
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

  async function openFile(entry: DirEntry) {
    if (entry.is_dir) return;
    if (!entry.name.endsWith('.md')) return;
    // Leaving any full-pane view (tag / inbox) implicitly; proceed with the file open.
    activeTag = null;
    activeView = null;
    if (entry.rel_path === vaultState.openFilePath) return;
    const vaultPath = vaultState.current?.path;
    if (!vaultPath) return;

    const requestId = ++openRequestSeq;
    try {
      await drainPendingSaves();
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
      const [all, unresolved] = await Promise.all([
        indexAllNotes(),
        indexUnresolvedCount()
      ]);
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
    const relPath = resolveWikiSlug(slug);
    const created = await createNoteFromTemplate(relPath);
    if (created) {
      const parent = relPath.slice(0, relPath.lastIndexOf('/'));
      if (parent && !expanded.has(parent)) {
        expanded = new Set([...expanded, parent]);
      }
      await refreshTree();
    }
    const name = relPath.slice(relPath.lastIndexOf('/') + 1);
    await openFile({ name, rel_path: relPath, is_dir: false });
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
    }, 500);
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
  <div class="app">
    <aside class="sidebar">
      <div class="sidebar-header">
        <span class="vault-name" title={vaultState.current.path}>
          {vaultState.current.path.split('/').pop()}
        </span>
        <span class="header-buttons">
          <button
            class="icon"
            title="回到 Home"
            disabled={!vaultState.openFilePath && !activeTag && !activeView}
            onclick={goHome}>⌂</button
          >
          <button class="icon" title="New note" onclick={() => newNote()}>+</button>
          <button class="icon" title="Change vault" onclick={chooseAndOpen}>⇆</button>
        </span>
      </div>
      <div class="command-bar" role="toolbar" aria-label="Commands">
        <button
          class="cmd"
          title={`${commandDefs.daily.label} (${commandDefs.daily.shortcut})`}
          onclick={() => openOrCreateDaily(cmdDeps)}
        >
          Today
        </button>
        <button
          class="cmd"
          title={`${commandDefs.weekly.label} (${commandDefs.weekly.shortcut})`}
          onclick={() => openOrCreateWeekly(cmdDeps)}
        >
          Week
        </button>
        <button
          class="cmd"
          title={`${commandDefs.capture.label} (${commandDefs.capture.shortcut})`}
          onclick={() => quickCapture(cmdDeps)}
        >
          Capture
        </button>
        <button
          class="cmd"
          title={`${commandDefs.record.label} (${commandDefs.record.shortcut})`}
          onclick={openRecord}
        >
          Record
        </button>
      </div>
      <ul class="tree">
        {#each tree as entry (entry.rel_path)}
          {@render treeNode(entry, 0)}
        {/each}
      </ul>
      <ProjectsSection
        activeProjectPath={vaultState.openFilePath}
        onSelect={(path) =>
          openFile({ name: path.slice(path.lastIndexOf('/') + 1), rel_path: path, is_dir: false })}
        refreshToken={panelRefreshToken}
      />
      <TagsSection
        activeTag={activeTag}
        onSelect={selectTag}
        refreshToken={panelRefreshToken}
      />
    </aside>
    <section class="editor-pane">
      {#if activeView === 'inbox'}
        <InboxView
          onOpenNote={(p) => {
            activeView = null;
            openFile({ name: p.slice(p.lastIndexOf('/') + 1), rel_path: p, is_dir: false });
          }}
          onPromote={(p) => openPromoteModal(p)}
          onArchive={archiveInboxNote}
          onDelete={deleteInboxNote}
          onClose={closeInboxView}
          refreshToken={inboxRefreshToken}
        />
      {:else if activeTag}
        {#key activeTag}
          <TagView
            tag={activeTag}
            onOpenNote={(p) =>
              openFile({ name: p.slice(p.lastIndexOf('/') + 1), rel_path: p, is_dir: false })}
            onClose={closeTagView}
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
          />
        {/key}
      {:else}
        {@render homeView()}
      {/if}
    </section>
    <div class="panel-slot">
      <Panel
        filePath={vaultState.openFilePath}
        onOpenNote={(p) =>
          openFile({ name: p.slice(p.lastIndexOf('/') + 1), rel_path: p, is_dir: false })}
        refreshToken={panelRefreshToken}
      />
    </div>
    <footer class="status-bar">
      <span class="sb-group sb-left">
        <span class="sb-item" title={vaultState.current.path}>
          {vaultState.current.path.split('/').pop()}
        </span>
        {#if vaultState.openFilePath}
          <span class="sb-sep">·</span>
          <span class="sb-item">{vaultState.openFilePath}</span>
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
          class="sb-theme"
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
          <code>templates/project.md</code>；项目名会被 slugify 成目录名（例如 "Deep
          Work" → <code>deep-work/</code>）
        {:else if newNoteTargetDir?.startsWith('4-projects/')}
          在 <code>{newNoteTargetDir}/</code> 下新建，模板套用
          <code>templates/project-note.md</code>；标题会被 slugify 成文件名（例如 "Interview
          Notes" → <code>interview-notes.md</code>）
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

<CommandPalette
  open={paletteOpen}
  onClose={() => (paletteOpen = false)}
  ctx={paletteCtx}
/>

{#snippet treeNode(entry: DirEntry, depth: number)}
  <li>
    <div class="tree-row-wrap">
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
                    <span class="home-list-title">{n.title ?? n.path.slice(n.path.lastIndexOf('/') + 1).replace(/\.md$/, '')}</span>
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
                    <span class="home-list-title">{m.title ?? m.path.slice(m.path.lastIndexOf('/') + 1).replace(/\.md$/, '')}</span>
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
  }
  .welcome-inner {
    text-align: center;
    max-width: 480px;
  }
  .welcome h1 {
    margin: 0 0 8px;
    font-size: 32px;
    font-weight: 600;
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
    font-size: 12px;
    text-transform: uppercase;
    color: var(--color-fg-muted);
    margin: 0 0 8px;
    letter-spacing: 0.05em;
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
    grid-template-columns: var(--sidebar-width) 1fr var(--panel-width, 280px);
    grid-template-rows: 1fr var(--statusbar-height);
    grid-template-areas:
      'side main panel'
      'status status status';
    height: 100vh;
    background: var(--color-bg);
  }
  .sidebar {
    grid-area: side;
    background: transparent;
    overflow-y: auto;
  }
  .editor-pane {
    grid-area: main;
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--color-surface);
    margin: 14px 7px 14px 0;
    border-radius: var(--radius-xl);
    box-shadow: var(--pane-border);
    overflow: hidden;
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
  }
  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-border);
    font-weight: 600;
    background: var(--color-bg);
    position: sticky;
    top: 0;
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
  .command-bar {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    padding: 8px 12px 10px 12px;
    border-bottom: 1px solid var(--color-border);
  }
  .cmd {
    padding: 6px 4px;
    font-size: 12px;
    font-weight: 500;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-fg-muted);
    cursor: pointer;
    box-shadow: none;
    transition: all 0.15s ease;
  }
  .cmd:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    border-color: var(--glass-border);
    transform: translateY(-0.5px);
    box-shadow: 0 1px 3px rgba(0,0,0,0.03);
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
    transition: background 0.15s ease, transform 0.15s ease;
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
    padding: 0 var(--space-3);
    border-top: 1px solid var(--color-border);
    background: transparent;
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    color: var(--color-fg-muted);
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
  .sb-theme {
    padding: 0 var(--space-1);
    border: none;
    background: transparent;
    color: var(--color-fg-muted);
    font-size: var(--fs-md);
    line-height: var(--statusbar-height);
    cursor: pointer;
  }
  .sb-theme:hover {
    color: var(--color-fg);
    background: transparent;
  }

  /* ---- home view ---------------------------------------------------- */
  .home {
    display: grid;
    place-items: center;
    height: 100%;
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
    font-size: var(--fs-3xl);
    font-weight: 600;
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
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-bg);
    text-align: left;
    cursor: pointer;
    transition:
      background 0.1s ease,
      border-color 0.1s ease;
  }
  .home-card:hover {
    background: var(--color-bg-hover);
    border-color: var(--color-accent);
  }
  .home-card-label {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-fg-muted);
  }
  .home-card-value {
    font-size: var(--fs-xl);
    font-weight: 600;
    font-family: var(--font-mono);
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
    font-size: var(--fs-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-fg-muted);
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
    transition: background 0.15s ease, transform 0.15s ease;
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
    background: transparent;
    border: 1px dashed var(--color-border);
    border-radius: var(--radius-md);
    padding: 6px 12px;
    cursor: pointer;
    max-width: 60%;
    min-width: 0;
    text-align: left;
  }
  .home-review:hover {
    border-color: var(--color-accent);
    background: var(--color-bg-hover);
  }
  .home-review-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-fg-muted);
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
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-lg);
    padding: 24px 28px;
    width: 440px;
    max-width: calc(100vw - 40px);
    box-shadow: var(--glass-shadow);
  }
  .modal h3 {
    margin: 0 0 8px;
    font-size: 16px;
    font-weight: 600;
  }
  .modal-hint {
    margin: 0 0 12px;
    font-size: 12px;
    color: var(--color-fg-muted);
    line-height: 1.5;
  }
  .modal-hint code {
    background: var(--color-bg-subtle);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 11px;
  }
  .modal-input {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--color-border);
    border-radius: 4px;
    font-family: var(--font-mono);
    font-size: 13px;
    background: var(--color-bg);
    color: var(--color-fg);
    box-sizing: border-box;
  }
  .modal-input:focus {
    outline: 2px solid var(--color-accent);
    outline-offset: -1px;
  }
  .modal-error {
    margin: 8px 0 0;
    color: #d13438;
    font-size: 12px;
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
</style>
