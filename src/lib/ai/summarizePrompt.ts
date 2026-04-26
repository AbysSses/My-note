/**
 * Prompt + body-mutation helpers for the P3-D3.3 `> Summarize current note`
 * commands.
 *
 * Scope:
 * - `buildSummarizePrompt` picks the `system` + `user` halves that get fed
 *   to `aiComplete`. Kept on the frontend so UX tuning (language hint,
 *   length cap, tone) doesn't require a Rust rebuild.
 * - `applySummaryToBody` turns an AI reply into the modified note body for
 *   either write-back target (`frontmatter.summary` / TL;DR blockquote at
 *   the top). Exposed as a pure function so `DiffPreviewModal` can render
 *   the diff reactively when the user flips the target radio.
 * - `stripFrontmatter` / `insertTldrAtTop` are the two primitives — kept
 *   exported so they're trivially unit-testable from a Node one-liner if
 *   we ever add vitest.
 */

import { rewriteFrontmatter } from '$lib/commands';

/** Where the AI-generated summary should land. `clipboard` is handled
 *  without the diff modal, so this type only covers the file-modifying
 *  cases that `applySummaryToBody` knows how to render. */
export type SummarizeTarget = 'frontmatter' | 'top';

/**
 * System prompt: tight boundary around the model's output shape.
 * - "match the note's language" nudges it toward 中文 when the body is
 *   Chinese-dominant and English otherwise, without us having to detect
 *   the language explicitly (which would add another round trip).
 * - "Output ONLY the summary paragraph" forbids headings / bullets /
 *   blockquote markers so the reply slots cleanly into frontmatter or a
 *   single-line TL;DR blockquote we wrap it with.
 * - Length hint is soft ("under 120 characters when possible") rather
 *   than hard — truncating mid-sentence on the client would look worse
 *   than a slightly long summary.
 */
const SYSTEM_PROMPT = `You are a concise-summary writer for a personal knowledge base. Write a faithful, information-dense TL;DR in the user's language — if the note is Chinese, reply in Chinese; otherwise English. Output ONLY the summary paragraph, without any heading, bullet, quote, or markdown decoration. Keep it to 1-3 sentences, under 120 characters when possible.`;

/** Build the prompt pair for `aiComplete`. Frontmatter is stripped so the
 *  model isn't distracted by YAML metadata; body is trimmed so a mostly
 *  empty note (whitespace only) surfaces as an empty user_prompt back in
 *  the caller (which rejects pre-flight). */
export function buildSummarizePrompt(body: string): {
  systemPrompt: string;
  userPrompt: string;
} {
  const stripped = stripFrontmatter(body).trim();
  return {
    systemPrompt: SYSTEM_PROMPT,
    userPrompt: `Summarize the following markdown note into a single TL;DR paragraph:\n\n${stripped}`
  };
}

/** Strip a leading `---\n…\n---` frontmatter block if present. Matches
 *  both `\n` and `\r\n` line endings to keep cross-platform writes happy. */
export function stripFrontmatter(body: string): string {
  const match = body.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/);
  if (!match) return body;
  return body.slice(match[0].length);
}

/**
 * Insert a `> **TL;DR** …` blockquote right after the frontmatter (or at
 * the top when there is none), with a blank line on each side so the
 * preceding frontmatter and following body stay visually separated.
 *
 * The blockquote marker is chosen over a plain paragraph because:
 * - It renders as a visually distinct callout in most markdown renderers
 *   (incl. this app's livepreview).
 * - It's easy for the user to grep / delete later — `^> \*\*TL;DR\*\*`
 *   is unambiguous.
 *
 * Note: this function does **not** detect or replace a pre-existing
 * TL;DR block; it always inserts. The diff in `DiffPreviewModal` will
 * still show the new line being added, and if the user sees an old
 * one in the `same` lines they can cancel, remove the old, and retry.
 * Replacement is deliberately left out because "what counts as the old
 * TL;DR" is hard to pin down without heuristics that might blow away
 * unrelated user text.
 */
export function insertTldrAtTop(body: string, summary: string): string {
  const fmMatch = body.match(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/);
  const block = `> **TL;DR** ${summary}\n\n`;
  if (fmMatch) {
    const fm = fmMatch[0];
    const rest = body.slice(fm.length).replace(/^\r?\n+/, '');
    return `${fm}\n${block}${rest}`;
  }
  return `${block}${body.replace(/^\r?\n+/, '')}`;
}

/**
 * Compose the write-back body for a given summary + target. Pure function
 * so the modal can recompute it reactively (e.g. when the user typoed a
 * word in the reply and we want to re-run — currently not wired, but the
 * purity keeps the door open for an "edit before apply" affordance).
 *
 * `summary` is pre-normalised: newlines collapsed to spaces so frontmatter
 * scalars stay on one line and the TL;DR blockquote stays a single line
 * (blockquotes that span multiple source lines render as one paragraph
 * anyway, but keep the on-disk markdown tidy).
 */
export function applySummaryToBody(
  body: string,
  summary: string,
  target: SummarizeTarget
): string {
  const cleaned = summary.trim().replace(/\s+/g, ' ');
  if (target === 'frontmatter') {
    return rewriteFrontmatter(body, { summary: cleaned });
  }
  return insertTldrAtTop(body, cleaned);
}

/** Generate a short, collision-resistant request id for `aiComplete`.
 *  Stays under the 128-char backend limit; prefixed so registry scans can
 *  distinguish summarize requests from chat streams at a glance when
 *  debugging. */
export function makeSummarizeRequestId(): string {
  return `sum-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
