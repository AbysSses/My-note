/**
 * Command registry used by the Cmd+P palette.
 *
 * Commands are declared here (not inside the palette component) so other parts
 * of the UI — the command bar, keyboard shortcuts, right-click menus — can
 * reuse the same registry later without duplicating ids/labels.
 *
 * A `PaletteContext` is passed at call-time so commands stay pure with respect
 * to the component tree.
 */

export interface PaletteContext {
  openNote: (path: string) => void;
  openTag: (tag: string) => void;
  runDaily: () => void;
  runWeekly: () => void;
  runCapture: () => void;
  runRecord: () => void;
  runNewNote: () => void;
  runNewMoc: () => void;
  /** Open the "新建项目" modal — creates `4-projects/<slug>/index.md` from the
   *  project template. Slug is derived from the entered title by `slugifyTitle`. */
  runNewProject: () => void;
  runInboxReview: () => void;
  /** Open the "Build MOC from tag" modal — lists all notes carrying `activeTag`
   *  and lets the user pick a subset + title, then materialises a new
   *  `2-moc/<slug>.md` with the selected notes pre-listed as wiki-links.
   *  Only meaningful when a tag is focused in the sidebar tag view. */
  runBuildMocFromTag: () => void;
  /** The tag currently focused in the sidebar tag view (TagView), or null
   *  when the user hasn't clicked a tag. Gates `build-moc-from-tag`. */
  activeTag: string | null;
  /** Open the full-pane force-directed note-graph. In local mode uses
   *  `currentFilePath` as the BFS seed; when no file is open, local mode
   *  is disabled and only global is available. */
  runGraph: () => void;
  /** Extract the currently selected block in the editor into a new
   *  `1-notes/<slug>.md` and replace the selection with a `[[title]]`
   *  wiki-link. Predicate: editor must be visible (i.e. a file is open)
   *  — the actual selection-empty case is handled at run time by
   *  expanding to the enclosing paragraph. */
  runExtractSelection: () => void | Promise<void>;
  promoteCurrent: () => void;
  /** Set the `status:` frontmatter field on the current project's
   *  `4-projects/<slug>/index.md`. Only callable when `currentFilePath`
   *  starts with `4-projects/`; slug is derived from the path. */
  setProjectStatus: (status: string) => void | Promise<void>;
  /** Open the "new note" modal pre-targeted at `4-projects/<slug>/` so the
   *  created file is a project-note (not the project's index.md). Slug is
   *  derived from `currentFilePath`; command is hidden outside projects. */
  runAddNoteToProject: () => void;
  /** Move the current project sub-note to `1-notes/`, rewriting its
   *  frontmatter `type: project-note → note`. Hidden when the open file is
   *  the project index (`.../index.md`) itself or outside `4-projects/`. */
  runExtractFromProject: () => void | Promise<void>;
  /** Overwrite `<vault>/templates/*.md` with the bundled versions. UX layer
   *  is responsible for the confirm prompt — this hook just invokes the
   *  IPC and surfaces the summary. */
  runReseedTemplates: () => void | Promise<void>;
  /** Open the "Unused Attachments" modal — lists `attachments/…` files with
   *  no `![](…)` embed pointing at them and offers batch deletion. */
  runFindUnusedAttachments: () => void | Promise<void>;
  /** Open the "Rename" modal for the currently open file. Rewrites every
   *  `[[wiki]]` / `![](path)` reference so backlinks don't break. */
  runRenameCurrent: () => void | Promise<void>;
  /** Open the "Rename directory" modal, pre-targeted at the *parent directory*
   *  of the currently open file. Refuses when no file is open, when the open
   *  file lives at the vault root, or when the parent is `.mynotes/`. The
   *  underlying IPC rewrites every link pointing into the moved tree. */
  runRenameCurrentDir: () => void | Promise<void>;
  /** Open the Settings modal. Also bound to `⌘,` and the status-bar gear. */
  runOpenSettings: () => void;
  /** Apply a theme choice directly — used by the three `> Set theme → …`
   *  palette commands. Persists to localStorage on the page side. */
  applyThemeChoice: (theme: 'system' | 'light' | 'dark') => void;
  /** Open the save dialog and pack the whole vault as a zip. Excludes
   *  `.mynotes/` (derived). */
  runExportVaultZip: () => void | Promise<void>;
  /** Copy the currently open `.md` file to a user-chosen destination via
   *  the save dialog. Hidden unless a markdown file is open. */
  runExportCurrentNote: () => void | Promise<void>;
  /** Trigger `window.print()` so the browser's "Save as PDF" route
   *  becomes available on the current note. Sidebar/panel/statusbar are
   *  hidden via `@media print`. */
  runPrintCurrentNote: () => void;
  closeVault: () => void;
  currentFilePath: string | null;
  /** Show the AI related-notes panel section (enables it if it was off,
   *  then scrolls the right panel into view). P3-D1. */
  runShowRelatedNotes: () => void;
  /** Embed the currently open note via the configured provider. Guarded on
   *  markdown files outside `.mynotes/`. Surfaces a toast summarising the
   *  outcome (`embedded N chunks` / `up-to-date` / `provider error`). P3-D2a.3a. */
  runEmbedCurrentNote: () => void | Promise<void>;
  /** Whether AI assist is globally enabled. Gates every AI palette command
   *  so a user who has turned AI off in Settings doesn't see entries that
   *  will fail at runtime. P3-D3.3+. */
  aiEnabled: boolean;
  /** Run `> Summarize current note` with the given write-back target.
   *  For `frontmatter` / `top`, opens `DiffPreviewModal` with the proposed
   *  body and only writes on user confirm. For `clipboard`, runs silently
   *  and surfaces a toast on success / failure — no modal. P3-D3.3. */
  runSummarizeCurrentNote: (target: 'frontmatter' | 'top' | 'clipboard') => void | Promise<void>;
  /** Run `> Suggest tags for current note`. Opens `TagSuggestModal` with
   *  AI-proposed candidates + existing tags as checkboxes; the merged list
   *  is written to `frontmatter.tags` on user confirm. No no-modal path —
   *  picking tags *is* the interaction. P3-D3.4. */
  runSuggestTagsForCurrentNote: () => void | Promise<void>;
  /** Run `> Draft MOC from tag (AI)`. Reuses the non-AI mocBuilder modal
   *  as the tag / title / note-picker (no duplication), and adds a
   *  "用 AI 草拟…" fork on its confirm path that feeds the picked notes
   *  into `aiComplete` and previews the grouped entries via
   *  `DiffPreviewModal` against the flat baseline. The command itself
   *  just opens the picker — the AI branch is taken from the modal.
   *  P3-D3.5. */
  runDraftMocFromTag: () => void | Promise<void>;
  /** Toggle the floating "Tweaks" appearance panel (accent / radius / glow /
   *  bg-tint / density). Panel is off by default. */
  toggleTweaks: () => void;
  /** Toggle the floating "Today's tasks" overlay anchored to the workspace. */
  toggleTodayTasks: () => void;
}

export interface PaletteCommand {
  id: string;
  label: string;
  /** Short right-aligned hint, e.g. `⌘D` or the category name. */
  hint?: string;
  /** Optional predicate — command is hidden when this returns false. */
  when?: (ctx: PaletteContext) => boolean;
  run: (ctx: PaletteContext) => void | Promise<void>;
}

export const PALETTE_COMMANDS: PaletteCommand[] = [
  {
    id: 'today',
    label: 'Today — open/create daily note',
    hint: '⌘D',
    run: (ctx) => ctx.runDaily()
  },
  {
    id: 'week',
    label: 'This Week — open/create weekly note',
    hint: '⌘⇧W',
    run: (ctx) => ctx.runWeekly()
  },
  {
    id: 'capture',
    label: 'Quick Capture to inbox',
    hint: '⌘⇧N',
    run: (ctx) => ctx.runCapture()
  },
  {
    id: 'record',
    label: 'Daily Record — append to today',
    hint: '⌘⇧D',
    run: (ctx) => ctx.runRecord()
  },
  {
    id: 'new-note',
    label: 'New Note…',
    hint: 'New',
    run: (ctx) => ctx.runNewNote()
  },
  {
    id: 'new-moc',
    label: 'New MOC…',
    hint: '2-moc/',
    run: (ctx) => ctx.runNewMoc()
  },
  {
    id: 'new-project',
    label: 'New Project…',
    hint: '4-projects/',
    run: (ctx) => ctx.runNewProject()
  },
  {
    id: 'inbox-review',
    label: 'Inbox Review — process 0-inbox',
    hint: 'Inbox',
    run: (ctx) => ctx.runInboxReview()
  },
  {
    id: 'build-moc-from-tag',
    label: 'Build MOC from tag…',
    hint: 'MOC',
    // Hidden when no tag is focused — the command is tag-scoped and asking
    // the user to pick a tag inside the palette would duplicate TagView.
    when: (ctx) => !!ctx.activeTag,
    run: (ctx) => ctx.runBuildMocFromTag()
  },
  {
    id: 'open-graph',
    label: 'Open Graph View',
    hint: '⌘⇧G',
    run: (ctx) => ctx.runGraph()
  },
  {
    id: 'extract-selection',
    label: 'Extract selection → new note',
    hint: '⌘⇧E',
    // Only meaningful when a markdown file is being edited. Not restricted
    // to 0-inbox/ — extraction is useful from daily notes and long drafts
    // inside 1-notes/ too.
    when: (ctx) =>
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runExtractSelection()
  },
  {
    id: 'promote-current',
    label: 'Promote current note → 1-notes',
    hint: 'Promote',
    when: (ctx) => !!ctx.currentFilePath && ctx.currentFilePath.startsWith('0-inbox/'),
    run: (ctx) => ctx.promoteCurrent()
  },
  // Project status commands.
  // All four share `when = current file is inside 4-projects/`. The slug is
  // derived from the path on run, so the user doesn't have to disambiguate
  // which project they mean — it's always "the one I'm looking at".
  {
    id: 'set-project-status-active',
    label: 'Set project status → active',
    hint: 'Project',
    when: isInProject,
    run: (ctx) => ctx.setProjectStatus('active')
  },
  {
    id: 'set-project-status-paused',
    label: 'Set project status → paused',
    hint: 'Project',
    when: isInProject,
    run: (ctx) => ctx.setProjectStatus('paused')
  },
  {
    id: 'set-project-status-done',
    label: 'Set project status → done',
    hint: 'Project',
    when: isInProject,
    run: (ctx) => ctx.setProjectStatus('done')
  },
  {
    id: 'set-project-status-archived',
    label: 'Set project status → archived',
    hint: 'Project',
    when: isInProject,
    run: (ctx) => ctx.setProjectStatus('archived')
  },
  // Project membership commands. `Add Note to Project` lives anywhere inside
  // `4-projects/<slug>/` (including on the project index itself) — it opens
  // the new-note modal pre-scoped to the project dir. `Extract from project`
  // only appears on non-index files inside the project (the index itself
  // never "leaves" the project — that's what Archive is for).
  {
    id: 'add-note-to-project',
    label: 'Add Note to Project — new note under current project',
    hint: 'Project',
    when: isInProject,
    run: (ctx) => ctx.runAddNoteToProject()
  },
  {
    id: 'extract-from-project',
    label: 'Extract from project → move to 1-notes',
    hint: 'Project',
    when: (ctx) =>
      !!ctx.currentFilePath &&
      ctx.currentFilePath.startsWith('4-projects/') &&
      !ctx.currentFilePath.endsWith('/index.md'),
    run: (ctx) => ctx.runExtractFromProject()
  },
  {
    id: 'reseed-templates',
    label: 'Reseed templates from bundled',
    hint: 'Vault',
    run: (ctx) => ctx.runReseedTemplates()
  },
  {
    id: 'find-unused-attachments',
    label: 'Find unused attachments',
    hint: 'Vault',
    run: (ctx) => ctx.runFindUnusedAttachments()
  },
  {
    id: 'rename-current',
    label: 'Rename current file…',
    hint: 'Rename',
    when: (ctx) => !!ctx.currentFilePath && !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runRenameCurrent()
  },
  {
    id: 'rename-current-dir',
    label: 'Rename current directory…',
    hint: 'Rename',
    // Only meaningful when the open file lives inside a directory (i.e. has
    // at least one `/` in its rel path) and that parent isn't `.mynotes/`.
    when: (ctx) =>
      !!ctx.currentFilePath &&
      ctx.currentFilePath.includes('/') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runRenameCurrentDir()
  },
  {
    id: 'open-settings',
    label: 'Settings…',
    hint: '⌘,',
    run: (ctx) => ctx.runOpenSettings()
  },
  // Theme switches. These are "set absolute" rather than "toggle" so each
  // palette entry is idempotent — running `> Set theme → dark` from any
  // starting state ends up dark. The status-bar gear still offers the
  // three-way cycle for users who like that shortcut.
  {
    id: 'set-theme-system',
    label: 'Set theme → System',
    hint: 'Theme',
    run: (ctx) => ctx.applyThemeChoice('system')
  },
  {
    id: 'set-theme-light',
    label: 'Set theme → Light',
    hint: 'Theme',
    run: (ctx) => ctx.applyThemeChoice('light')
  },
  {
    id: 'set-theme-dark',
    label: 'Set theme → Dark',
    hint: 'Theme',
    run: (ctx) => ctx.applyThemeChoice('dark')
  },
  {
    id: 'toggle-tweaks',
    label: 'Toggle appearance tweaks panel',
    hint: 'Tweaks',
    run: (ctx) => ctx.toggleTweaks()
  },
  {
    id: 'toggle-today-tasks',
    label: "Toggle today's tasks overlay",
    hint: 'Tasks',
    run: (ctx) => ctx.toggleTodayTasks()
  },
  // Export commands.
  // - Vault zip: always available (save dialog picks destination).
  // - Single-note `.md` copy: requires a markdown file open.
  // - Print: requires any file open (works on Home too, but not useful).
  //   The system print dialog is the "Save as PDF" route — no native PDF
  //   library needed.
  {
    id: 'export-vault-zip',
    label: 'Export vault as zip…',
    hint: 'Export',
    run: (ctx) => ctx.runExportVaultZip()
  },
  {
    id: 'export-current-note',
    label: 'Export current note (.md)…',
    hint: 'Export',
    when: (ctx) =>
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runExportCurrentNote()
  },
  {
    id: 'print-current-note',
    label: 'Print current note (→ Save as PDF)',
    hint: 'Export',
    when: (ctx) =>
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runPrintCurrentNote()
  },
  {
    id: 'show-related-notes',
    label: 'Show Related Notes (AI assist)',
    hint: 'AI',
    when: (ctx) =>
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runShowRelatedNotes()
  },
  {
    id: 'embed-current-note',
    label: 'Embed current note (AI index)',
    hint: 'AI',
    when: (ctx) =>
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runEmbedCurrentNote()
  },
  // Summarize commands — three explicit write-back targets so the
  // command-palette search is the "target picker" (cleaner than opening
  // a modal, picking a radio, then confirming). All three share the
  // same prompt + backend call; only the accept step differs.
  {
    id: 'summarize-to-frontmatter',
    label: 'Summarize → frontmatter.summary (AI)',
    hint: 'AI',
    when: (ctx) =>
      ctx.aiEnabled &&
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runSummarizeCurrentNote('frontmatter')
  },
  {
    id: 'summarize-to-top',
    label: 'Summarize → insert TL;DR at top (AI)',
    hint: 'AI',
    when: (ctx) =>
      ctx.aiEnabled &&
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runSummarizeCurrentNote('top')
  },
  {
    id: 'summarize-to-clipboard',
    label: 'Summarize → copy to clipboard (AI)',
    hint: 'AI',
    when: (ctx) =>
      ctx.aiEnabled &&
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runSummarizeCurrentNote('clipboard')
  },
  // Suggest tags — a single command. Unlike summarize there's no target
  // branching (frontmatter.tags is the only sensible sink), and the
  // interaction *is* the picker (checkbox modal), so a plain palette entry
  // without target suffix is enough.
  {
    id: 'suggest-tags',
    label: 'Suggest tags for current note (AI)',
    hint: 'AI',
    when: (ctx) =>
      ctx.aiEnabled &&
      !!ctx.currentFilePath &&
      ctx.currentFilePath.endsWith('.md') &&
      !ctx.currentFilePath.startsWith('.mynotes/'),
    run: (ctx) => ctx.runSuggestTagsForCurrentNote()
  },
  // Draft MOC (AI) — paired with the non-AI `build-moc-from-tag` above.
  // Both require `activeTag`; the AI entry is additionally gated on
  // `aiEnabled` so it disappears when AI is globally off. Running the
  // command opens the same picker modal — the user picks AI on the
  // modal's fork button, which keeps the one-screen UX.
  {
    id: 'draft-moc-from-tag',
    label: 'Draft MOC from tag (AI)',
    hint: 'AI',
    when: (ctx) => ctx.aiEnabled && !!ctx.activeTag,
    run: (ctx) => ctx.runDraftMocFromTag()
  },
  {
    id: 'close-vault',
    label: 'Close Vault',
    hint: 'Vault',
    run: (ctx) => ctx.closeVault()
  }
];

/** True when the currently open file is inside `4-projects/<slug>/`. */
function isInProject(ctx: PaletteContext): boolean {
  return !!ctx.currentFilePath && ctx.currentFilePath.startsWith('4-projects/');
}

/** Pull the `<slug>` out of a `4-projects/<slug>/...` path.
 *  Returns null if the path doesn't look like a project file. */
export function projectSlugFromPath(path: string | null): string | null {
  if (!path) return null;
  const segs = path.split('/');
  if (segs.length < 2 || segs[0] !== '4-projects') return null;
  const slug = segs[1];
  if (!slug) return null;
  return slug;
}

/**
 * Case-insensitive subsequence match — every char of `needle` appears in
 * `haystack` in order. Returns a score (lower = tighter) or -1 on miss.
 *
 * The score prefers matches that are contiguous and closer to the start. It's
 * purposely cheap — palette lists top out around a few hundred entries.
 */
export function fuzzyScore(haystack: string, needle: string): number {
  if (!needle) return 0;
  const h = haystack.toLowerCase();
  const n = needle.toLowerCase();
  let hi = 0;
  let ni = 0;
  let score = 0;
  let lastHit = -1;
  while (hi < h.length && ni < n.length) {
    if (h[hi] === n[ni]) {
      // Gap from the previous hit (contiguous matches are cheapest).
      if (lastHit >= 0) score += hi - lastHit - 1;
      else score += hi; // penalty for starting later
      lastHit = hi;
      ni++;
    }
    hi++;
  }
  if (ni < n.length) return -1;
  return score;
}
