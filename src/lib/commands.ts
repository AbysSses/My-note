/**
 * Vault-level commands that are invoked by keyboard shortcuts or buttons.
 *
 * Each command is a pure-ish function: it performs file IO and asks the caller
 * (via `CommandDeps`) to refresh the UI and open files. This keeps routing /
 * state concerns out of the domain logic.
 */

import { fileDelete, fileExists, fileRead, fileWrite } from './ipc/file';
import type { NoteRef } from './ipc/index';
import { formatDate, isoWeek, isoWeekString, render } from './template';

export interface CommandDeps {
  /** Rebuild the sidebar tree (root + any expanded dirs). */
  refreshTree: () => Promise<void>;
  /** Open the .md file at this relative path in the editor. */
  openFile: (relPath: string) => Promise<void>;
  /** Make sure the given directory is expanded in the sidebar. */
  expandDir: (relPath: string) => void;
}

async function nextAvailablePath(relPath: string): Promise<string> {
  if (!(await fileExists(relPath))) return relPath;

  const dotIdx = relPath.lastIndexOf('.');
  const stem = dotIdx === -1 ? relPath : relPath.slice(0, dotIdx);
  const ext = dotIdx === -1 ? '' : relPath.slice(dotIdx);

  for (let i = 1; ; i++) {
    const candidate = `${stem}-${i}${ext}`;
    if (!(await fileExists(candidate))) return candidate;
  }
}

/** Read a template from the active vault's `templates/` directory. */
async function loadTemplate(name: string): Promise<string | null> {
  try {
    return await fileRead(`templates/${name}.md`);
  } catch {
    return null;
  }
}

/** Pick a template name based on the note's path.
 *
 *  Most folders are picked by top-level dir. `4-projects/` is the exception:
 *  its `index.md` is the project scaffold, so it uses `project.md`; everything
 *  else inside a project (`4-projects/<slug>/<note>.md`) is a project-note and
 *  uses `project-note.md`. Pass the full `relPath` so we can distinguish.
 */
function templateForDir(topDir: string, relPath: string): string {
  switch (topDir) {
    case '0-inbox':
      return 'inbox';
    case '1-notes':
      return 'note';
    case '2-moc':
      return 'moc';
    case '3-journal':
      return 'note';
    case '4-projects':
      return relPath.endsWith('/index.md') ? 'project' : 'project-note';
    default:
      return 'note';
  }
}

/** Compute a sensible title from a slug-style filename (no extension). */
function titleFromSlug(slug: string): string {
  return slug.replace(/[-_]+/g, ' ').trim() || slug;
}

/**
 * Create a new note using the appropriate template for its top-level folder.
 * `relPath` must include the `.md` suffix and be a path inside the vault.
 * Returns `false` if the target already existed.
 */
export async function createNoteFromTemplate(
  relPath: string,
  extra: Record<string, string> = {}
): Promise<boolean> {
  if (await fileExists(relPath)) return false;

  const segs = relPath.split('/');
  const topDir = segs[0];
  const filename = segs[segs.length - 1]; // e.g. "my-note.md"
  const slug = filename.replace(/\.md$/, '');
  const title = extra.title ?? titleFromSlug(slug);

  const tplName = templateForDir(topDir, relPath);
  const tplBody = await loadTemplate(tplName);

  let body: string;
  if (tplBody !== null) {
    body = render(tplBody, { title, content: '', ...extra });
  } else {
    // Fallback: minimal frontmatter if templates/ is missing or unreadable.
    const now = formatDate(new Date(), 'YYYY-MM-DD HH:mm');
    body = `---\ntitle: "${title}"\ncreated: "${now}"\nupdated: "${now}"\n---\n\n`;
  }
  await fileWrite(relPath, body);
  return true;
}

/** Open (or create) today's daily note at `3-journal/YYYY-MM-DD.md`. */
export async function openOrCreateDaily(deps: CommandDeps, date: Date = new Date()) {
  const dateStr = formatDate(date, 'YYYY-MM-DD');
  const relPath = `3-journal/${dateStr}.md`;

  const prev = formatDate(new Date(date.getTime() - 86_400_000), 'YYYY-MM-DD');
  const next = formatDate(new Date(date.getTime() + 86_400_000), 'YYYY-MM-DD');

  let created = false;
  if (!(await fileExists(relPath))) {
    const tpl = (await loadTemplate('daily')) ?? '';
    const body = render(tpl, { prev, next }, date);
    await fileWrite(relPath, body);
    created = true;
  }
  deps.expandDir('3-journal');
  if (created) await deps.refreshTree();
  await deps.openFile(relPath);
}

/** Open (or create) the current ISO week's weekly note at `3-journal/YYYY-Wxx.md`. */
export async function openOrCreateWeekly(deps: CommandDeps, date: Date = new Date()) {
  const key = isoWeekString(date);
  const relPath = `3-journal/${key}.md`;

  const prevD = new Date(date.getTime() - 7 * 86_400_000);
  const nextD = new Date(date.getTime() + 7 * 86_400_000);
  const prev = isoWeekString(prevD);
  const next = isoWeekString(nextD);

  let created = false;
  if (!(await fileExists(relPath))) {
    const tpl = (await loadTemplate('weekly')) ?? '';
    const body = render(tpl, { prev, next }, date);
    await fileWrite(relPath, body);
    created = true;
  }
  deps.expandDir('3-journal');
  if (created) await deps.refreshTree();
  await deps.openFile(relPath);
}

/**
 * Quick-capture a new inbox note at `0-inbox/YYYY-MM-DD-HHmmss.md`.
 * If `content` is non-empty, it's rendered into the template via `{{content}}`.
 */
export async function quickCapture(deps: CommandDeps, content: string = '') {
  const now = new Date();
  const slug = formatDate(now, 'YYYY-MM-DD-HHmmss');
  const relPath = await nextAvailablePath(`0-inbox/${slug}.md`);

  const tpl = (await loadTemplate('inbox')) ?? '';
  const body = render(tpl, { content }, now);
  await fileWrite(relPath, body);
  deps.expandDir('0-inbox');
  await deps.refreshTree();
  await deps.openFile(relPath);
}

/**
 * Append a timestamped bullet under the "## 📝 Daily Record" heading of today's
 * daily note. Creates the daily note first if missing.
 */
export async function appendDailyRecord(deps: CommandDeps, content: string) {
  const trimmed = content.trim();
  if (!trimmed) return;

  const date = new Date();
  const dateStr = formatDate(date, 'YYYY-MM-DD');
  const relPath = `3-journal/${dateStr}.md`;

  if (!(await fileExists(relPath))) {
    await openOrCreateDaily(deps, date);
  }
  const current = await fileRead(relPath);
  const ts = formatDate(date, 'HH:mm');
  const entry = `- **${ts}** — ${trimmed}`;
  const next = insertUnderHeading(current, '## 📝 Daily Record', entry);
  await fileWrite(relPath, next);
  await deps.openFile(relPath);
}

/**
 * Turn a user-facing title into a filename stem.
 *
 * Keeps CJK / latin characters (so "知识管理" stays "知识管理") but strips
 * path-invalid chars and collapses whitespace to single dashes. Case is
 * preserved — we don't lowercase Chinese titles, and latin titles look more
 * natural with case preserved too.
 */
export function slugifyTitle(title: string): string {
  return title
    .trim()
    .replace(/[\\/:*?"<>|]+/g, '')
    .replace(/\s+/g, '-')
    .replace(/-+/g, '-');
}

/**
 * Rewrite (or insert) frontmatter scalar fields on a markdown body.
 *
 * Keeps the block ordering, only touches the specified keys, and appends any
 * missing required keys right before the closing `---`. If the document has
 * no frontmatter block, one is prepended.
 *
 * Intentionally a small regex-based implementation — full YAML round-tripping
 * isn't worth the payload for personal-notes frontmatter (all scalars).
 */
export function rewriteFrontmatter(
  body: string,
  updates: Record<string, string>
): string {
  const FM_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;
  const match = body.match(FM_RE);
  const keysToSet = new Set(Object.keys(updates));

  if (!match) {
    // No frontmatter: prepend a minimal block.
    const lines = Object.entries(updates).map(([k, v]) => `${k}: ${formatYamlScalar(v)}`);
    return `---\n${lines.join('\n')}\n---\n\n${body.replace(/^\s+/, '')}`;
  }

  const fmBody = match[1];
  const rest = body.slice(match[0].length);

  const outLines: string[] = [];
  const seen = new Set<string>();
  for (const line of fmBody.split(/\r?\n/)) {
    const kv = line.match(/^(\w[\w-]*)\s*:(.*)$/);
    if (kv && keysToSet.has(kv[1])) {
      outLines.push(`${kv[1]}: ${formatYamlScalar(updates[kv[1]])}`);
      seen.add(kv[1]);
    } else {
      outLines.push(line);
    }
  }
  // Append any keys that weren't present.
  for (const k of keysToSet) {
    if (!seen.has(k)) outLines.push(`${k}: ${formatYamlScalar(updates[k])}`);
  }

  // Preserve leading whitespace/newline before the body — but guarantee a
  // blank line between `---` and the first content line.
  const trimmedRest = rest.replace(/^\r?\n/, '');
  return `---\n${outLines.join('\n')}\n---\n\n${trimmedRest}`;
}

/** Quote a scalar if it contains YAML-significant chars; otherwise leave bare. */
function formatYamlScalar(value: string): string {
  if (value === '' || /[:#"\[\]{}]|^[\s-]/.test(value)) {
    return `"${value.replace(/"/g, '\\"')}"`;
  }
  return value;
}

/**
 * Promote an `0-inbox/…` note into `1-notes/{slug}.md`.
 *
 * Writes the new file (with rewritten frontmatter) first, then deletes the
 * source — so a crash mid-flight leaves the user with *both* files, never
 * neither. Returns the final destination path.
 *
 * Throws if the source is not under `0-inbox/` or if we can't find a free
 * target slot within 100 suffix attempts.
 */
export async function promoteInboxNote(
  deps: CommandDeps,
  srcPath: string,
  newTitle: string
): Promise<string> {
  if (!srcPath.startsWith('0-inbox/')) {
    throw new Error(`not an inbox note: ${srcPath}`);
  }
  const title = newTitle.trim();
  if (!title) throw new Error('标题不能为空');

  const slug = slugifyTitle(title);
  if (!slug) throw new Error('标题无法转换为合法文件名');

  // Find the first free 1-notes/{slug}[-N].md slot.
  let dstPath = `1-notes/${slug}.md`;
  for (let i = 1; (await fileExists(dstPath)) && i < 100; i++) {
    dstPath = `1-notes/${slug}-${i}.md`;
  }
  if (await fileExists(dstPath)) {
    throw new Error(`找不到空闲的目标文件名: ${dstPath}`);
  }

  const srcBody = await fileRead(srcPath);
  const now = formatDate(new Date(), 'YYYY-MM-DD HH:mm');
  const newBody = rewriteFrontmatter(srcBody, {
    title,
    type: 'note',
    status: 'draft',
    updated: now
  });

  // Two-step: write new, then delete old. Don't swap the order — a crash after
  // delete but before write would lose the note.
  await fileWrite(dstPath, newBody);
  await fileDelete(srcPath);

  // Surface the new file in the tree + open it.
  deps.expandDir('1-notes');
  await deps.refreshTree();
  await deps.openFile(dstPath);
  return dstPath;
}

/**
 * Build the body of a newly extracted note from a selected block.
 *
 * This is a **pure** string transform. The caller owns the surrounding file
 * orchestration: creating the destination path, writing the new note, replacing
 * the original range with a wiki-link, and refreshing UI state.
 *
 * Design notes
 * ------------
 * * The new note reuses `rewriteFrontmatter`-style minimal frontmatter
 *   (title / type=note / status=draft / created=updated=now). We don't load
 *   `templates/note.md` because template rendering would prepend the `#
 *   title` heading, which we *don't* want here — the extracted block is
 *   kept verbatim so headings inside it stay intact.
 * * The happy-path wiki-link form is `[[title]]`, not `[[1-notes/slug]]`,
 *   because the resolver treats title-lookup as first-class (see §5.4). If
 *   the generated filename has to take a `-N` suffix due to a collision, we
 *   fall back to a path+alias form (`[[1-notes/slug-N|title]]`) so the link
 *   still resolves to the newly created note rather than an older homonym.
 * * Trailing/leading whitespace is stripped from the extracted text before
 *   it becomes the new body, so `## heading\n\ncontent\n\n` in the middle
 *   of a paragraph doesn't leak phantom blank lines into the new file.
 * * If `[[title]]` ends up adjacent to other content on the same line,
 *   we leave it alone — the caller is the one who sliced the range, so
 *   they know whether it was a whole paragraph or an inline chunk.
 */
export function buildExtractedNote(
  extractedText: string,
  title: string,
  now: string
): string {
  const trimmed = extractedText.trim();
  const fmLines = [
    `title: ${formatYamlScalar(title)}`,
    `type: note`,
    `status: draft`,
    `created: ${formatYamlScalar(now)}`,
    `updated: ${formatYamlScalar(now)}`,
    `tags: []`,
    `aliases: []`
  ];
  return `---\n${fmLines.join('\n')}\n---\n\n# ${title}\n\n${trimmed}\n`;
}

/**
 * Full orchestration of block extraction — run from the Svelte layer.
 *
 * 1. Slugify title, find a free `1-notes/<slug>[-N].md` slot.
 * 2. Write the new note (atomic).
 * 3. Return `{ dstPath, linkText }` so the caller can splice
 *    `linkText` into the editor at the extraction range.
 *
 * Failure semantics: any IPC error propagates. On success the source file
 * is **not** touched by this function — the caller owns the editor
 * dispatch that replaces the selection, and the subsequent `fileWrite` of
 * the updated source body. That way a disk-write failure on the source
 * doesn't orphan an already-created destination.
 */
export async function extractBlockToNote(
  title: string,
  extractedText: string
): Promise<{ dstPath: string; linkText: string }> {
  const t = title.trim();
  if (!t) throw new Error('标题不能为空');
  const slug = slugifyTitle(t);
  if (!slug) throw new Error('标题无法转换为合法文件名');
  if (!extractedText.trim()) throw new Error('选中内容为空');

  let dstPath = `1-notes/${slug}.md`;
  for (let i = 1; (await fileExists(dstPath)) && i < 100; i++) {
    dstPath = `1-notes/${slug}-${i}.md`;
  }
  if (await fileExists(dstPath)) {
    throw new Error(`找不到空闲的目标文件名: ${dstPath}`);
  }

  const now = formatDate(new Date(), 'YYYY-MM-DD HH:mm');
  const body = buildExtractedNote(extractedText, t, now);
  await fileWrite(dstPath, body);

  const canonicalPath = `1-notes/${slug}.md`;
  const linkText =
    dstPath === canonicalPath ? `[[${t}]]` : `[[${dstPath.replace(/\.md$/, '')}|${t}]]`;

  return { dstPath, linkText };
}

/** Which strategy `injectMocEntries` used to splice the entries in.
 *  - `sentinel`: the template's `<!-- moc:entries-insertion-point -->` comment
 *    was found and replaced — the new canonical path, independent of the
 *    "## 核心笔记" heading text.
 *  - `legacy`: the old `## 核心笔记\n\n- [[]]` stub was matched (pre-sentinel
 *    templates, still in the wild on vaults that haven't reseeded).
 *  - `none`: neither anchor was present. Caller should surface a toast —
 *    the MOC file has been created but entries were **not** injected, user
 *    needs to paste them manually or reseed the template. */
export type MocInjectStrategy = 'sentinel' | 'legacy' | 'none';

/** Sentinel HTML comment placed in `templates/moc.md` that `buildMocFromTag`
 *  replaces with the rendered entry list. Living as an exported const so
 *  tests + template authoring stay in sync with the injector. */
export const MOC_ENTRIES_SENTINEL = '<!-- moc:entries-insertion-point -->';

/**
 * Pure helper: splice rendered `- [[…]]` entries into a MOC template body,
 * returning the new body + which strategy matched. Exported for unit tests
 * so we don't have to stand up a real vault to verify edge cases.
 *
 * Strategy precedence:
 * 1. **sentinel** (`<!-- moc:entries-insertion-point -->`) — the canonical
 *    anchor in current templates; decouples the injector from heading text.
 *    The sentinel line itself is consumed (replaced with the entries).
 * 2. **legacy** (`## 核心笔记\n\n- [[]]`) — kept so vaults with an
 *    unreseeded old template still work. We only match on the exact stub
 *    (not "any line under heading"), so a hand-edited old MOC that's already
 *    populated won't be stomped.
 * 3. **none** — neither anchor exists. Caller handles the UX fallback.
 */
export function injectMocEntries(
  body: string,
  entriesMarkdown: string
): { next: string; strategy: MocInjectStrategy } {
  if (!entriesMarkdown) return { next: body, strategy: 'none' };

  // Sentinel form: consume the whole sentinel line (optionally surrounded by
  // whitespace-only content) and replace with the entry block. We tolerate
  // Windows CRLF via `\r?\n` and an optional trailing newline after the
  // sentinel so entries slot in without an extra blank line.
  const SENTINEL_RE = new RegExp(
    `[ \\t]*${MOC_ENTRIES_SENTINEL.replace(/[-/\\^$*+?.()|[\]{}]/g, '\\$&')}[ \\t]*\\r?\\n?`
  );
  if (SENTINEL_RE.test(body)) {
    return {
      next: body.replace(SENTINEL_RE, `${entriesMarkdown}\n`),
      strategy: 'sentinel'
    };
  }

  // Legacy form: the original pre-sentinel template's exact stub line under
  // the "## 核心笔记" heading. Deliberately strict to avoid overwriting
  // user-populated MOCs that just happen to have the heading.
  const LEGACY_RE = /## 核心笔记\r?\n\r?\n- \[\[\]\]/;
  if (LEGACY_RE.test(body)) {
    return {
      next: body.replace(LEGACY_RE, `## 核心笔记\n\n${entriesMarkdown}`),
      strategy: 'legacy'
    };
  }

  return { next: body, strategy: 'none' };
}

/**
 * Build a new MOC at `2-moc/<slug>.md` from a set of tagged notes.
 *
 * Workflow:
 * 1. Slugify the user-provided title; find a free `2-moc/<slug>[-N].md` slot.
 * 2. Materialise the file via the normal `moc.md` template (so
 *    frontmatter/structure stays in sync with manual MOC creation).
 * 3. Post-process: splice the rendered `- [[title]]` entries at the
 *    template's sentinel (current) or legacy `- [[]]` stub (pre-reseed).
 *    If neither anchor is present the MOC is still created — we return
 *    `strategy: 'none'` so the caller can surface a "entries not injected"
 *    toast rather than silently succeeding.
 * 4. Expand `2-moc/`, refresh the tree, open the new file.
 *
 * Wiki-link form: we emit `[[title]]` (not `[[1-notes/slug]]`). Per §5.4 the
 * resolver treats title lookup as first-class, and a MOC of bare titles reads
 * far better than a list of paths. On title collision the resolver picks
 * deterministically; the caller is expected to rename duplicates before
 * building the MOC (or accept that the first-lexicographic note wins).
 *
 * The `tag` parameter is stored in frontmatter as `moc_source_tag` so later we
 * can add a "rebuild from tag" affordance — harmless if unused.
 *
 * `params.entriesMarkdown` overrides the default flat `- [[title]]` list when
 * provided — used by the P3-D3.5 `> Draft MOC from tag (AI)` command to inject
 * AI-grouped entries instead. When omitted (default), the flat list is built
 * from `noteRefs` as before. `insertedCount` continues to be the count of
 * `noteRefs` in both cases, so the toast message stays meaningful.
 */
export async function buildMocFromTag(
  deps: CommandDeps,
  params: { tag: string; title: string; noteRefs: NoteRef[]; entriesMarkdown?: string }
): Promise<{ dstPath: string; insertedCount: number; strategy: MocInjectStrategy }> {
  const title = params.title.trim();
  if (!title) throw new Error('标题不能为空');
  const slug = slugifyTitle(title);
  if (!slug) throw new Error('标题无法转换为合法文件名');

  let dstPath = `2-moc/${slug}.md`;
  for (let i = 1; (await fileExists(dstPath)) && i < 100; i++) {
    dstPath = `2-moc/${slug}-${i}.md`;
  }
  if (await fileExists(dstPath)) {
    throw new Error(`找不到空闲的目标文件名: ${dstPath}`);
  }

  const ok = await createNoteFromTemplate(dstPath, { title });
  if (!ok) throw new Error(`文件已存在: ${dstPath}`);

  const body = await fileRead(dstPath);
  const lines = params.noteRefs.map((ref) => {
    const stem = ref.path.replace(/\.md$/, '').split('/').pop() ?? ref.path;
    const display = ref.title ?? stem;
    return `- [[${display}]]`;
  });

  // AI-drafted entries override the flat list when supplied. The override is
  // treated as already-rendered markdown (with `[[…]]` links), not a
  // NoteRef[], because D3.5's caller has already validated and reshaped the
  // AI output. `insertedCount` still reflects the source note count so the
  // toast ("已注入 N 条") stays accurate regardless of draft strategy.
  const entriesMarkdown =
    params.entriesMarkdown?.trim() ? params.entriesMarkdown.trim() : lines.join('\n');

  let insertedCount = 0;
  let strategy: MocInjectStrategy = 'none';
  if (entriesMarkdown.length > 0) {
    const result = injectMocEntries(body, entriesMarkdown);
    strategy = result.strategy;
    if (strategy !== 'none') {
      await fileWrite(dstPath, result.next);
      insertedCount = lines.length;
    }
  }

  // Stamp the source tag into frontmatter (additive; `rewriteFrontmatter`
  // only touches specified keys).
  const afterInject = await fileRead(dstPath);
  const stamped = rewriteFrontmatter(afterInject, { moc_source_tag: params.tag });
  await fileWrite(dstPath, stamped);

  deps.expandDir('2-moc');
  await deps.refreshTree();
  await deps.openFile(dstPath);
  return { dstPath, insertedCount, strategy };
}

/** Insert `entry` at the end of the section under the first line matching `heading`.
 *  If the heading is missing, append both heading and entry at the doc end.
 *  Exported for unit tests. */
export function insertUnderHeading(doc: string, heading: string, entry: string): string {
  const lines = doc.split('\n');
  const headingIdx = lines.findIndex((l) => l.trim() === heading.trim());
  if (headingIdx === -1) {
    const sep = doc.endsWith('\n') ? '\n' : '\n\n';
    return doc.trimEnd() + `${sep}\n${heading}\n\n${entry}\n`;
  }
  // Find end of this section (the line before the next heading of equal or higher level, or EOF).
  let sectionEnd = headingIdx + 1;
  while (sectionEnd < lines.length && !/^#{1,3}\s/.test(lines[sectionEnd])) {
    sectionEnd++;
  }
  // Trim trailing blanks inside this section so insertion is tight.
  let insertAt = sectionEnd;
  while (insertAt > headingIdx + 1 && lines[insertAt - 1].trim() === '') insertAt--;
  // Make sure there's at least one blank line between heading and entries.
  const blockBefore = insertAt === headingIdx + 1 ? [''] : [];
  lines.splice(insertAt, 0, ...blockBefore, entry, '');
  return lines.join('\n');
}

export const commandDefs = {
  daily: { label: 'Today', shortcut: '⌘D' },
  weekly: { label: 'This Week', shortcut: '⌘⇧W' },
  capture: { label: 'Quick Capture', shortcut: '⌘⇧N' },
  record: { label: 'Daily Record', shortcut: '⌘⇧D' }
} as const;

export type CommandName = keyof typeof commandDefs;

/** Internal helper — used by tests. */
export const _internals = { templateForDir, titleFromSlug, isoWeek };
