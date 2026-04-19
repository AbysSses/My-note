/**
 * Wiki-link autocompletion for the markdown editor.
 *
 * Triggers when the cursor is inside a `[[…` sequence. Completions are the
 * notes already in the index. Apply inserts the note's path stem (or full
 * path if we need to disambiguate) and closes the `]]` if it isn't already
 * there.
 */
import type { Completion, CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import { markdownLanguage } from '@codemirror/lang-markdown';

import { indexAllNotes, type NoteRef } from '$lib/ipc/index';

// Cache the all-notes list. 5s TTL is plenty — new notes show up within a
// keystroke of the watcher re-indexing them, and the index is cheap to query.
let cache: NoteRef[] = [];
let fetchedAt = 0;
let inflight: Promise<NoteRef[]> | null = null;

const TTL_MS = 5_000;

async function getNotes(): Promise<NoteRef[]> {
  const now = Date.now();
  if (now - fetchedAt < TTL_MS && cache.length > 0) return cache;

  if (inflight) return await inflight;

  inflight = (async () => {
    try {
      const list = await indexAllNotes();
      cache = list;
      fetchedAt = Date.now();
      return list;
    } catch (err) {
      // Keep whatever cache we have so autocomplete doesn't disappear on a
      // transient failure (e.g. vault switching mid-fetch).
      console.warn('[wikicomplete] indexAllNotes failed', err);
      return cache;
    } finally {
      inflight = null;
    }
  })();

  // If we have nothing yet, we must await; otherwise serve stale.
  if (cache.length === 0) return await inflight;
  return cache;
}

/** Clear the cache — call after actions that mutate the note set in bulk. */
export function invalidateWikiCompletionCache() {
  cache = [];
  fetchedAt = 0;
}

function stemOf(path: string): string {
  const idx = path.lastIndexOf('/');
  const file = idx >= 0 ? path.slice(idx + 1) : path;
  return file.endsWith('.md') ? file.slice(0, -3) : file;
}

/** The path without a trailing `.md` — unambiguous for the Rust resolver. */
function fullPathSlug(path: string): string {
  return path.endsWith('.md') ? path.slice(0, -3) : path;
}

/**
 * Decide the text to insert for a note. Use the bare stem when it's unique
 * across the vault; otherwise fall back to the full path so there's no
 * ambiguity when the resolver runs.
 */
function applyTextFor(note: NoteRef, stemCounts: Map<string, number>): string {
  const stem = stemOf(note.path);
  if ((stemCounts.get(stem) ?? 0) > 1) return fullPathSlug(note.path);
  return stem;
}

/** Build a stem → count map so we can tell uniques from collisions. */
function countStems(notes: NoteRef[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const n of notes) {
    const s = stemOf(n.path);
    m.set(s, (m.get(s) ?? 0) + 1);
  }
  return m;
}

/**
 * CompletionSource — returns notes whenever the user is inside a
 * `[[…` run. Lets CM filter by the typed query against labels.
 */
async function wikiCompletion(ctx: CompletionContext): Promise<CompletionResult | null> {
  // "[[" followed by anything that isn't "]", "[", or newline. Note: the
  // regex must NOT consume the trailing "]]" — otherwise we'd think the
  // user already finished and skip completion.
  const token = ctx.matchBefore(/\[\[[^\]\[\n]*/);
  if (!token) return null;

  // `ctx.explicit` is true when the user force-triggered (Ctrl+Space).
  // For implicit triggering, only fire once the user has typed at least `[[`.
  // matchBefore gives at least `[[` (2 chars) at the minimum — always OK.
  const query = token.text.slice(2); // text after "[["

  const notes = await getNotes();
  const stemCounts = countStems(notes);

  const options: Completion[] = notes.map((n) => {
    const label = n.title ?? stemOf(n.path);
    const apply = applyTextFor(n, stemCounts);
    const typeHint = n.note_type ?? '';
    return {
      label,
      detail: `${typeHint ? typeHint + ' · ' : ''}${n.path}`,
      apply: (view, _completion, from, to) => {
        // If the next two chars are already "]]", don't re-add them.
        const after = view.state.sliceDoc(to, to + 2);
        const closesItself = after === ']]';
        const insert = closesItself ? apply : apply + ']]';
        view.dispatch({
          changes: { from, to, insert },
          // Leave the caret after the closing "]]" so the user can keep typing.
          selection: { anchor: from + insert.length + (closesItself ? 2 : 0) }
        });
      }
    };
  });

  return {
    // Start the filter window after "[[" so typing filters on the label only.
    from: token.from + 2,
    options,
    // Keep the popup open as long as we're still inside a wiki-link run.
    validFor: /^[^\]\[\n]*$/,
    filter: true
  };

  // `query` is unused directly — CM does the filtering based on `from` + `validFor`.
  // We keep the variable to document intent.
  void query;
}

/**
 * Language-data extension that plugs the completion source into the
 * existing `autocompletion()` extension already present in the editor.
 * No changes needed at the `autocompletion({})` call site.
 */
export const wikiCompletionExtension = markdownLanguage.data.of({
  autocomplete: wikiCompletion
});
