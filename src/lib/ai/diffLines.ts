/**
 * Minimal line-level diff used by {@link DiffPreviewModal} (P3-D3.2).
 *
 * Why a custom implementation instead of `diff` / `jsdiff`:
 * - Write-back targets here (summary / MOC / tag block) are bounded —
 *   a single note, typically under a few hundred lines. The LCS DP
 *   table is trivially small (O(m·n)), so we don't need Myers; the
 *   code ships in ~30 lines with zero dependencies.
 * - Keeps the bundle lean and the algorithm auditable next to the
 *   component that consumes it.
 *
 * Line semantics:
 * - Input strings are split on `\n`. A trailing newline produces a
 *   final empty line, which we keep so the diff is lossless (callers
 *   that don't care can strip it themselves).
 * - Reconstruction: concatenating the `value` of every `same` /
 *   `remove` part with `\n` yields the original; every `same` /
 *   `add` part yields the proposed text.
 */

/** A single line-diff entry. */
export type DiffPart = {
  type: 'add' | 'remove' | 'same';
  value: string;
};

/**
 * Compute a line-level diff between `a` (original) and `b` (proposed).
 *
 * Backtracks a standard LCS DP table; ties prefer keeping the left
 * side (`remove` before `add`) so consecutive deletions group up in
 * the rendered output instead of interleaving with additions.
 */
export function diffLines(a: string, b: string): DiffPart[] {
  const la = a.split('\n');
  const lb = b.split('\n');
  const m = la.length;
  const n = lb.length;

  // dp[i][j] = LCS length of la[i..] vs lb[j..].
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = m - 1; i >= 0; i--) {
    for (let j = n - 1; j >= 0; j--) {
      dp[i][j] = la[i] === lb[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const parts: DiffPart[] = [];
  let i = 0;
  let j = 0;
  while (i < m && j < n) {
    if (la[i] === lb[j]) {
      parts.push({ type: 'same', value: la[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      parts.push({ type: 'remove', value: la[i] });
      i++;
    } else {
      parts.push({ type: 'add', value: lb[j] });
      j++;
    }
  }
  while (i < m) parts.push({ type: 'remove', value: la[i++] });
  while (j < n) parts.push({ type: 'add', value: lb[j++] });
  return parts;
}

/** Summary counts for the badge row in {@link DiffPreviewModal}. */
export function diffStats(parts: DiffPart[]): { added: number; removed: number; same: number } {
  let added = 0;
  let removed = 0;
  let same = 0;
  for (const p of parts) {
    if (p.type === 'add') added++;
    else if (p.type === 'remove') removed++;
    else same++;
  }
  return { added, removed, same };
}
