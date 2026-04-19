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
  closeVault: () => void;
  currentFilePath: string | null;
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
