import { invoke } from '@tauri-apps/api/core';

export interface NoteRef {
  path: string;
  title: string | null;
  updated: string | null;
  note_type: string | null;
}

export interface BacklinkItem {
  src_path: string;
  src_title: string | null;
  link_text: string;
}

export interface OutgoingLink {
  dst: string;
  dst_resolved: string | null;
  title: string | null;
}

export interface TagCount {
  tag: string;
  count: number;
}

export interface SearchHit {
  path: string;
  title: string | null;
  snippet: string;
}

/** All notes that link into `relPath` (resolved wiki-links only). */
export async function indexBacklinks(relPath: string): Promise<BacklinkItem[]> {
  return await invoke<BacklinkItem[]>('index_backlinks', { relPath });
}

/** All wiki-links contained in `relPath`, including unresolved ones. */
export async function indexOutgoing(relPath: string): Promise<OutgoingLink[]> {
  return await invoke<OutgoingLink[]>('index_outgoing', { relPath });
}

/** Link targets inside `relPath` that don't resolve to any existing note. */
export async function indexUnresolved(relPath: string): Promise<string[]> {
  return await invoke<string[]>('index_unresolved', { relPath });
}

/** Every tag used in the vault, ranked by note count desc. */
export async function indexTags(): Promise<TagCount[]> {
  return await invoke<TagCount[]>('index_tags');
}

/** All notes carrying `tag` (exact match), newest first. */
export async function indexNotesByTag(tag: string): Promise<NoteRef[]> {
  return await invoke<NoteRef[]>('index_notes_by_tag', { tag });
}

/** Notes carrying one or more tags.
 *  `matchAll = true` returns the intersection; `false` returns the union. */
export async function indexNotesByTags(tags: string[], matchAll = true): Promise<NoteRef[]> {
  return await invoke<NoteRef[]>('index_notes_by_tags', { tags, matchAll });
}

/** All notes in the vault — cheap lightweight rows, used for completion/palette. */
export async function indexAllNotes(): Promise<NoteRef[]> {
  return await invoke<NoteRef[]>('index_all_notes');
}

/** Every note in `0-inbox/`, newest first. Feeds the Inbox Review view. */
export async function indexInboxList(): Promise<NoteRef[]> {
  return await invoke<NoteRef[]>('index_inbox_list');
}

/** Count of distinct unresolved wiki-link targets across the vault. */
export async function indexUnresolvedCount(): Promise<number> {
  return await invoke<number>('index_unresolved_count');
}

/** Projects (i.e. `4-projects/{slug}/index.md`) bucketed by status.
 *  Pass `undefined` for all projects; a string for a single status bucket.
 *  Comparison is case- and whitespace-insensitive. */
export async function indexProjectsByStatus(status?: string): Promise<NoteRef[]> {
  return await invoke<NoteRef[]>('index_projects_by_status', { status: status ?? null });
}

/** Notes under `4-projects/<slug>/` excluding the project's own `index.md`.
 *  Feeds the right-hand Panel's "项目笔记" section when an index.md is open. */
export async function indexProjectNotes(slug: string): Promise<NoteRef[]> {
  return await invoke<NoteRef[]>('index_project_notes', { slug });
}

/** FTS5 full-text search; the backend wraps `query` as a literal phrase. */
export async function indexSearch(query: string, limit?: number): Promise<SearchHit[]> {
  return await invoke<SearchHit[]>('index_search', { query, limit });
}

/**
 * Resolve a `[[wiki-link]]` target to a concrete `NoteRef`. Tries an
 * exact frontmatter-title match first, then falls back to a filename
 * stem match — same precedence as the indexer's link-resolution pass.
 * Returns `null` when the link is unresolved; callers render it as
 * plain text in that case.
 *
 * Used by `ChatPanel.svelte` to make `[[…]]` chips in AI replies
 * clickable (D2b.5).
 */
export async function indexResolveWikiLink(target: string): Promise<NoteRef | null> {
  return await invoke<NoteRef | null>('index_resolve_wiki_link', { target });
}

export type TaskPriority = 'urgent' | 'high' | 'med' | 'low';

export interface TaskRow {
  id: number;
  note_path: string;
  note_title: string | null;
  line: number;
  text: string;
  done: boolean;
  due: string | null;
  priority: TaskPriority | null;
}

/** `YYYY-MM-DD` in the user's local timezone. Tasks use local dates because
 *  they are anchored to the user's day, not to UTC. */
export function todayIsoLocal(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

/** Open tasks for `today`: due today OR inside the daily note for today.
 *  Sorted urgent → low, capped at 50. */
export async function indexTasksToday(today?: string): Promise<TaskRow[]> {
  return await invoke<TaskRow[]>('index_tasks_today', { today: today ?? todayIsoLocal() });
}

/** Open tasks with a due date strictly after `today`, earliest first. */
export async function indexTasksUpcoming(today?: string, limit?: number): Promise<TaskRow[]> {
  return await invoke<TaskRow[]>('index_tasks_upcoming', {
    today: today ?? todayIsoLocal(),
    limit: limit ?? null
  });
}

/** Count of all open (`done = 0`) tasks across the vault. */
export async function indexTasksCount(): Promise<number> {
  return await invoke<number>('index_tasks_count');
}

/** Flip `[ ]` ↔ `[x]` on a specific line of a note; re-indexes on completion. */
export async function toggleTaskDone(
  notePath: string,
  line: number,
  done: boolean
): Promise<void> {
  await invoke<void>('toggle_task_done', { notePath, line, done });
}
