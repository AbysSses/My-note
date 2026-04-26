/**
 * Phase 4 Stage 4 — proposal resolution persistence.
 *
 * Without this, accepting / rejecting a proposal lives only in
 * `ChatPanel.svelte`'s local state. Closing the panel, popping out to
 * the standalone window, or reloading the session resets every
 * accepted card back to "pending" — the user sees ghost proposals
 * after writeback. The backend already journals every resolution to
 * `audit.log` / `usage.log` via `ai_record_proposal_resolution`, but
 * there's no IPC to read them back, so we keep a frontend-side mirror
 * keyed by `(session_id, view_model_key)` in `localStorage`.
 *
 * `localStorage` is the right tier because:
 * - The cache is purely a UX hint — losing it just makes a card look
 *   pending again, which is recoverable by the user.
 * - It survives Cmd+R / popout window switches, which is the actual
 *   inconsistency users hit (`ChatPanel.svelte` re-mounts on tab
 *   switch and standalone bring-back).
 * - It does not muddy the `chat_store.jsonl` schema; that file stays
 *   strictly the model ↔ tool transcript.
 *
 * The mirror is bounded: each resolution is a tiny JSON row, and we
 * scope by `session_id` so old session data is reachable but stale
 * sessions can be flushed by deleting their session file. We do not
 * trim aggressively — if it ever grows enough to matter, the user
 * has way more chat history than is realistic.
 */

export type ResolutionKind = 'accepted' | 'rejected' | 'error';

export interface ResolutionRecord {
  kind: ResolutionKind;
  message: string;
}

const PREFIX = 'mynotes:proposal-resolution:v1:';

function storageKey(sessionId: string, viewModelKey: string): string {
  return `${PREFIX}${sessionId}:${viewModelKey}`;
}

function safeStorage(): Storage | null {
  if (typeof window === 'undefined') return null;
  try {
    // Some embedded webviews block localStorage; fall back gracefully.
    return window.localStorage;
  } catch {
    return null;
  }
}

/** Write one resolution. No-op when storage is unavailable. */
export function persistResolution(
  sessionId: string,
  viewModelKey: string,
  record: ResolutionRecord
): void {
  if (!sessionId || !viewModelKey) return;
  const storage = safeStorage();
  if (!storage) return;
  try {
    storage.setItem(storageKey(sessionId, viewModelKey), JSON.stringify(record));
  } catch {
    // Quota exceeded / private mode → silently drop. The backend
    // audit log is the source of truth for compliance; this cache is
    // strictly a UX hint.
  }
}

/** Read every cached resolution for a session. */
export function loadResolutionsForSession(
  sessionId: string
): Record<string, ResolutionRecord> {
  if (!sessionId) return {};
  const storage = safeStorage();
  if (!storage) return {};
  const out: Record<string, ResolutionRecord> = {};
  const sessionPrefix = `${PREFIX}${sessionId}:`;
  for (let i = 0; i < storage.length; i++) {
    const k = storage.key(i);
    if (!k || !k.startsWith(sessionPrefix)) continue;
    const viewModelKey = k.slice(sessionPrefix.length);
    const raw = storage.getItem(k);
    if (!raw) continue;
    try {
      const parsed = JSON.parse(raw) as ResolutionRecord;
      if (parsed && typeof parsed.message === 'string') {
        out[viewModelKey] = parsed;
      }
    } catch {
      // Discard corrupted rows silently — they'll be overwritten on
      // the next user action against this card.
    }
  }
  return out;
}

/** Drop everything for a deleted session so the cache doesn't grow forever. */
export function clearResolutionsForSession(sessionId: string): void {
  if (!sessionId) return;
  const storage = safeStorage();
  if (!storage) return;
  const sessionPrefix = `${PREFIX}${sessionId}:`;
  const toDelete: string[] = [];
  for (let i = 0; i < storage.length; i++) {
    const k = storage.key(i);
    if (k && k.startsWith(sessionPrefix)) toDelete.push(k);
  }
  for (const k of toDelete) {
    try {
      storage.removeItem(k);
    } catch {
      /* ignore */
    }
  }
}
