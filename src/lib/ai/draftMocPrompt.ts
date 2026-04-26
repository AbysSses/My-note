/**
 * Prompt + output-sanitisation helpers for the P3-D3.5
 * `> Draft MOC from tag (AI)` command.
 *
 * Scope:
 * - `buildDraftMocPrompt` frames the task as "group these notes by theme,
 *   emit markdown sections with `[[title]]` bullets". The system prompt
 *   hard-pins the output shape so `sanitizeDraftMoc` can normalise it with
 *   minimal regex.
 * - `buildFlatEntriesMarkdown` mirrors the non-AI `buildMocFromTag` flat
 *   rendering so the caller can use it as the diff "before" text in
 *   `DiffPreviewModal`.
 * - `sanitizeDraftMoc` fences the AI reply: strips code fences, drops any
 *   `[[link]]` that doesn't point to a note the user actually picked
 *   (AI occasionally invents titles), trims to a reasonable line count.
 *
 * Output contract (enforced by prompt + sanitiser):
 *   ```
 *   ## <theme>
 *
 *   - [[title]]
 *   - [[title]]
 *
 *   ## <theme>
 *
 *   - [[title]]
 *   ```
 * - Only H2 headings and `- [[title]]` bullets; no prose paragraphs, no
 *   blockquotes, no code fences. Keeps the MOC skimmable and keeps the
 *   "entries block" a drop-in for `injectMocEntries`.
 */

import type { NoteRef } from '$lib/ipc/index';

/**
 * System prompt: strict output shape + reuse-only `[[title]]` constraint.
 * We tell the model to "use every title from the list at least once" to
 * prevent it from silently dropping notes (a common failure mode where
 * ~20% of the input disappears into vaguely-themed clusters).
 */
const SYSTEM_PROMPT = `You are a MOC (Map Of Content) curator for a personal knowledge base. Given a set of markdown note titles under a single tag, organise them into 2–6 themed sections and output a markdown block.

Strict output rules:
- Output ONLY the body of the "核心笔记" section — no frontmatter, no top-level heading, no prose paragraphs.
- Each theme is a second-level heading ("## <theme>") followed by a blank line, then "- [[title]]" bullets, one per line, then a blank line.
- Every title MUST come verbatim from the provided list. Do NOT invent titles, do NOT rephrase them.
- Every title from the provided list MUST appear exactly once across all sections (no duplicates, no omissions).
- Section names should be short (2–8 characters when Chinese, 1–4 words when English) and describe what the grouped notes have in common.
- Reply in the same language as the titles: if the majority of titles are Chinese, write theme names in Chinese; otherwise English.
- Output ONLY the markdown block — no explanation, no code fences, no preamble.`;

/** Build the prompt pair for `aiComplete`. `notes` = the NoteRefs the user
 *  picked in the mocBuilder modal (after checkbox filtering). */
export function buildDraftMocPrompt(args: {
  tag: string;
  title: string;
  notes: NoteRef[];
}): { systemPrompt: string; userPrompt: string } {
  const titles = args.notes.map((n) => n.title ?? stemFromPath(n.path)).filter(Boolean);
  const userPrompt = [
    `Tag: #${args.tag}`,
    `MOC title: ${args.title}`,
    '',
    'Titles to group (one per line):',
    ...titles.map((t) => `- ${t}`),
    '',
    'Output the themed section markdown now.'
  ].join('\n');
  return { systemPrompt: SYSTEM_PROMPT, userPrompt };
}

/** Render the flat `- [[title]]` list used by the non-AI `buildMocFromTag`.
 *  Callers pass the same `noteRefs` into both flat and AI flows so the
 *  diff-preview "before" is exactly what the non-AI command would have
 *  produced. */
export function buildFlatEntriesMarkdown(notes: NoteRef[]): string {
  return notes
    .map((ref) => {
      const display = ref.title ?? stemFromPath(ref.path);
      return `- [[${display}]]`;
    })
    .join('\n');
}

/**
 * Clean the AI reply into a drop-in `entriesMarkdown` for
 * `injectMocEntries`. Does:
 *
 * 1. Strips surrounding ```markdown … ``` fences if the model added them
 *    despite the "no code fences" instruction.
 * 2. Trims leading/trailing blank lines.
 * 3. Filters any `[[title]]` link whose target isn't in `allowedTitles` —
 *    hallucinated titles are rewritten to plain text so the MOC doesn't
 *    end up with dead wiki links.
 * 4. Caps total line count to 200 to bound the eventual file size.
 *
 * Returns the cleaned markdown plus structural counts so the caller can
 * warn on egregious drops ("AI dropped 12 of your 15 notes").
 */
export function sanitizeDraftMoc(
  reply: string,
  allowedTitles: string[]
): { markdown: string; sectionCount: number; bulletCount: number; linkedTitles: string[] } {
  const allowedSet = new Set(allowedTitles);

  // Strip fenced code block if the model wrapped its output.
  let text = reply.trim();
  const fence = text.match(/^```[a-zA-Z]*\n([\s\S]*?)\n```$/);
  if (fence) text = fence[1].trim();

  // Drop any leading explanation before the first `##` heading — some
  // models prepend "Here's the grouping:" etc.
  const firstH2 = text.indexOf('\n## ');
  if (firstH2 > 0 && !text.startsWith('## ')) {
    const candidateStart = text.indexOf('## ');
    if (candidateStart >= 0) text = text.slice(candidateStart);
  }

  const lines = text.split(/\r?\n/).slice(0, 200);
  const linkedTitles: string[] = [];
  let sectionCount = 0;
  let bulletCount = 0;

  const cleaned = lines.map((line) => {
    if (/^##\s+/.test(line)) {
      sectionCount++;
      return line;
    }
    // Rewrite `- [[title]]` bullets; validate the title against the
    // allowlist, drop the link wrapper when the title isn't one of the
    // picked notes (treat as a hallucination).
    const bullet = line.match(/^(\s*-\s*)\[\[([^\]|]+)(?:\|[^\]]*)?\]\]\s*$/);
    if (bullet) {
      bulletCount++;
      const title = bullet[2].trim();
      if (allowedSet.has(title)) {
        linkedTitles.push(title);
        return `${bullet[1]}[[${title}]]`;
      }
      // Hallucinated title — keep the bullet shape but as plain text so
      // the user immediately sees something's off without a broken
      // wiki-link poisoning their vault graph.
      return `${bullet[1]}${title}  <!-- AI 生成，非选中笔记 -->`;
    }
    return line;
  });

  // Trim repeated blank lines (> 2 in a row) to keep the diff clean.
  const deduped: string[] = [];
  let blankRun = 0;
  for (const l of cleaned) {
    if (l.trim() === '') {
      blankRun++;
      if (blankRun <= 1) deduped.push('');
    } else {
      blankRun = 0;
      deduped.push(l);
    }
  }

  return {
    markdown: deduped.join('\n').trim(),
    sectionCount,
    bulletCount,
    linkedTitles
  };
}

/** Path stem without extension: `1-notes/foo.md` → `foo`. */
function stemFromPath(path: string): string {
  return path.replace(/\.md$/, '').split('/').pop() ?? path;
}

/** Request-id prefix `moc-` so registry scans distinguish this from
 *  summarize / suggest-tags / chat streams. */
export function makeDraftMocRequestId(): string {
  return `moc-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
