/**
 * Prompt + body-mutation helpers for the P3-D3.4
 * `> Suggest tags for current note` commands.
 *
 * Split from `summarizePrompt.ts` because:
 * - the IO shape is list-of-strings, not a single paragraph;
 * - `frontmatter.tags` is a YAML *list*, which `rewriteFrontmatter` (scalar
 *   only) can't safely touch — this file owns the list-aware merge/emit;
 * - prompt engineering is substantially different (the model is asked to
 *   ground itself in the vault's existing taxonomy instead of free-form).
 *
 * Parsing is intentionally lenient: different provider/model combos love to
 * return different shapes (`["a","b"]`, `a, b, c`, `- a\n- b`, `#a #b`), and
 * we'd rather normalise all of them here than have the user retry.
 */

import { stripFrontmatter } from './summarizePrompt';

/**
 * System prompt: the model picks 3–8 tags, grounded in the vault's existing
 * tag taxonomy first, and may *also* propose 1–2 new ones when none of the
 * existing tags fit. The strict output contract (comma-separated, lowercase,
 * no `#`) keeps {@link parseSuggestedTags} a one-liner.
 */
const SYSTEM_PROMPT = `You are a tag curator for a personal knowledge base. Read the user's markdown note and pick 3 to 8 topical tags that best index it. Ground your picks in the "existing tags" list the user provides — prefer reusing those tags verbatim to keep the taxonomy consistent. You MAY add at most 2 brand-new tags when no existing tag captures a major theme. Rules for every tag you output: lowercase; words joined by '-' (kebab-case, e.g. "graph-db"); no '#' prefix; no spaces; no punctuation other than '-'. Output ONLY the comma-separated tag list. Do not include any explanation, heading, or bullet markers.`;

/** Build the prompt pair for `aiComplete`. `existingTags` = tags already in
 *  the current note's frontmatter; `vaultTags` = the (up to N) most-used tags
 *  in the vault, used as soft few-shot constraint. */
export function buildSuggestTagsPrompt(args: {
  body: string;
  existingTags: string[];
  vaultTags: string[];
}): { systemPrompt: string; userPrompt: string } {
  const stripped = stripFrontmatter(args.body).trim();

  // Cap both lists so the prompt stays small even in vaults with thousands
  // of tags. 40 vault-tags is plenty to give the model a taxonomy "feel"
  // without bloating the context.
  const existing = args.existingTags.slice(0, 50);
  const vault = args.vaultTags.slice(0, 40);

  const existingLine = existing.length ? existing.join(', ') : '(none)';
  const vaultLine = vault.length ? vault.join(', ') : '(none)';

  const userPrompt = [
    `Existing tags on this note: ${existingLine}`,
    `Most-used tags in the vault (for reuse preference): ${vaultLine}`,
    '',
    'Suggest 3–8 tags for the note below. Remember: prefer reusing tags from the two lists above; at most 2 genuinely new tags allowed.',
    '',
    '--- NOTE BEGIN ---',
    stripped,
    '--- NOTE END ---'
  ].join('\n');

  return { systemPrompt: SYSTEM_PROMPT, userPrompt };
}

/**
 * Normalise a single tag candidate. Strips leading `#`, lowercases, collapses
 * whitespace to a single `-`, drops anything that isn't `[a-z0-9-]` after
 * the normalisation pass. Returns `null` when the result is empty or
 * syntactically invalid (e.g. pure digits — those look like issue refs).
 */
function normaliseTag(raw: string): string | null {
  const trimmed = raw.trim().replace(/^#+/, '').toLowerCase();
  if (!trimmed) return null;
  // Collapse any whitespace run into a single dash so "note taking" → "note-taking".
  const dashed = trimmed.replace(/\s+/g, '-');
  // Strip every char that isn't allowed in a tag slug.
  const cleaned = dashed.replace(/[^a-z0-9\u4e00-\u9fff\-_]/g, '');
  if (!cleaned) return null;
  // Reject pure-digit strings — almost always a model hallucination of an
  // issue number rather than a real tag.
  if (/^\d+$/.test(cleaned)) return null;
  // Reject overly long tags — they're almost certainly a sentence that
  // slipped through.
  if (cleaned.length > 40) return null;
  return cleaned;
}

/**
 * Parse an AI reply into a de-duplicated list of tag slugs. Accepts any of:
 *
 * - `"a, b, c"` — the preferred shape enforced by our system prompt;
 * - `"[\"a\", \"b\"]"` — JSON array, sometimes emitted by stricter models;
 * - `"- a\n- b"` — bullet list, emitted despite the "no bullets" instruction;
 * - `"#a #b"` — hashtags, emitted by some smaller models;
 * - any combination of the above, whitespace-separated.
 *
 * Order is preserved from the model's output (the model's own confidence
 * ranking is usually meaningful), with duplicates removed on first sight.
 */
export function parseSuggestedTags(reply: string): string[] {
  // Try JSON array first — if it parses, trust the shape.
  const trimmed = reply.trim();
  let candidates: string[] = [];
  if (trimmed.startsWith('[')) {
    try {
      const parsed = JSON.parse(trimmed);
      if (Array.isArray(parsed)) {
        candidates = parsed.map((x) => String(x));
      }
    } catch {
      // Fall through to string splitting.
    }
  }
  if (candidates.length === 0) {
    // Split primarily on comma / newline / semicolon / bullet prefix. For
    // hashtag-style replies (`#a #b #c`) we additionally split on the `#`
    // boundary — but we deliberately don't split on whitespace in general,
    // because the system prompt constrains each tag to kebab-case and a
    // space inside a chunk is almost always a model-ignored multi-word
    // phrase that `normaliseTag` should collapse to kebab-case (e.g.
    // "bar baz" → "bar-baz") or reject via the 40-char length guard (long
    // accidental sentences). Splitting on whitespace here would shatter
    // both shapes into noise.
    const stripped = trimmed.replace(/^[-*\u2022]\s+/gm, '');
    if (stripped.includes('#')) {
      candidates = stripped.split(/\s*#\s*|[,;\n]+/);
    } else {
      candidates = stripped.split(/[,;\n]+/);
    }
  }

  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of candidates) {
    const t = normaliseTag(raw);
    if (t && !seen.has(t)) {
      seen.add(t);
      out.push(t);
    }
  }
  return out;
}

/** Read the current `frontmatter.tags` value from a note body. Tolerates all
 *  three formats the indexer accepts (YAML list / flow sequence / comma-
 *  separated scalar). Returns an empty array when absent. */
export function parseExistingTags(body: string): string[] {
  const FM_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;
  const match = body.match(FM_RE);
  if (!match) return [];
  const fm = match[1];

  // Case 1: flow sequence — `tags: [a, b, c]` on a single line.
  const flow = fm.match(/^tags:\s*\[([^\]]*)\]\s*$/m);
  if (flow) {
    return flow[1]
      .split(',')
      .map((s) => normaliseTag(s))
      .filter((x): x is string => !!x);
  }

  // Case 2: block sequence — `tags:\n  - a\n  - b\n`.
  const blockHeader = fm.match(/^tags:\s*$/m);
  if (blockHeader) {
    const lines = fm.split(/\r?\n/);
    const idx = lines.findIndex((l) => /^tags:\s*$/.test(l));
    const out: string[] = [];
    for (let i = idx + 1; i < lines.length; i++) {
      const m = lines[i].match(/^\s*-\s*(.*?)\s*$/);
      if (!m) break;
      const t = normaliseTag(m[1].replace(/^["']|["']$/g, ''));
      if (t) out.push(t);
    }
    return out;
  }

  // Case 3: comma-separated scalar — `tags: a, b, c`.
  const scalar = fm.match(/^tags:\s*(.+)$/m);
  if (scalar) {
    return scalar[1]
      .split(/[\s,]+/)
      .map((s) => normaliseTag(s))
      .filter((x): x is string => !!x);
  }

  return [];
}

/**
 * Write the caller-provided *final* tag list back to `frontmatter.tags` as a
 * flow sequence (`tags: [a, b, c]`).
 *
 * `newTags` is treated as the exact intended post-edit state, not an additive
 * delta. This matters for the D3.4 checkbox UI: unchecked existing tags must
 * disappear from the saved note rather than being unioned back in.
 *
 * Writes the flow form unconditionally — even when the file originally used
 * the block form — because round-tripping the original style isn't worth the
 * fragility (and the two forms are YAML-equivalent for the indexer).
 *
 * Emits (or prepends) a full frontmatter block when the note has none.
 */
export function mergeTagsIntoFrontmatter(body: string, newTags: string[]): string {
  const merged = mergeTagLists([], newTags);
  const tagsLine = `tags: [${merged.join(', ')}]`;

  const FM_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/;
  const match = body.match(FM_RE);
  if (!match) {
    // No frontmatter → prepend a minimal block carrying just `tags`.
    const prefix = `---\n${tagsLine}\n---\n\n`;
    return prefix + body.replace(/^\s+/, '');
  }

  const fm = match[1];
  const rest = body.slice(match[0].length);

  // Rewrite / insert `tags:` inside the existing FM block.
  let replaced = false;
  const outLines: string[] = [];
  const srcLines = fm.split(/\r?\n/);
  for (let i = 0; i < srcLines.length; i++) {
    const line = srcLines[i];
    // Flow / scalar form: single-line `tags: ...`.
    if (/^tags:\s*\[.*\]\s*$/.test(line) || /^tags:\s*[^\s].*$/.test(line)) {
      if (!replaced) {
        outLines.push(tagsLine);
        replaced = true;
      }
      continue;
    }
    // Block form: `tags:` on its own line, followed by `  - a` lines.
    if (/^tags:\s*$/.test(line)) {
      if (!replaced) {
        outLines.push(tagsLine);
        replaced = true;
      }
      // Skip over contiguous `  - …` continuation lines.
      while (i + 1 < srcLines.length && /^\s*-\s+/.test(srcLines[i + 1])) {
        i++;
      }
      continue;
    }
    outLines.push(line);
  }
  if (!replaced) {
    outLines.push(tagsLine);
  }

  const trimmedRest = rest.replace(/^\r?\n/, '');
  return `---\n${outLines.join('\n')}\n---\n\n${trimmedRest}`;
}

/** Concatenate two tag lists, de-dup in-order (existing first). Exported for
 *  testability; the modal also uses it directly when previewing the merge. */
export function mergeTagLists(existing: string[], extra: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const t of [...existing, ...extra]) {
    const n = normaliseTag(t);
    if (n && !seen.has(n)) {
      seen.add(n);
      out.push(n);
    }
  }
  return out;
}

/** Short, collision-resistant request id; `tag-` prefix so the registry can
 *  distinguish suggest-tags from summarize/chat at a glance while debugging. */
export function makeSuggestTagsRequestId(): string {
  return `tag-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
