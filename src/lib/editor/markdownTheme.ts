/**
 * Custom syntax highlighting for the Markdown editor.
 *
 * The previous iteration used CodeMirror's `defaultHighlightStyle`, which is
 * tuned for code languages and spits out hard-coded hex colours that fight
 * our oklch / Quire palette. This file replaces it with a `HighlightStyle`
 * tied to Lezer tags and our own `--color-*` CSS vars, so light/dark themes
 * track automatically.
 *
 * Scope
 * -----
 * These styles decorate the *raw* markdown tokens that remain visible —
 * i.e. what the user sees when the cursor is on the line (and the
 * live-preview replacement is suspended). The block-level decorations
 * (headings H1–H6 font sizes, blockquote indent, task checkboxes…) still
 * live in `livepreview.ts` where they can respond to cursor position.
 *
 * Here we only colour:
 *   * meta marks (`#`, `**`, `_`, `` ` ``, `>`, `|`, list bullets when
 *     visible, heading marks when visible) → muted
 *   * inline code + fenced code → mono font + surface background
 *   * links + autolinks → accent
 *   * strong / emphasis / strikethrough → weight/style only
 *   * headings H1–H6 → colour adjustments layered on top of livepreview
 */

import { HighlightStyle } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

/**
 * Map Lezer tags → style objects. The fields accept any CSS property, not
 * just colour — `fontStyle`, `fontWeight`, `textDecoration` all work.
 *
 * Order matters for overlapping tags — last rule wins. We put "mark" rules
 * near the top so specific tags (heading, link, etc.) can override them.
 */
export const markdownHighlightStyle = HighlightStyle.define([
  // --- marks: hash-signs, asterisks, backticks, pipes, list bullets ---
  // These are the raw formatting characters. When live-preview keeps them
  // visible (cursor on line), we want them muted so the content pops.
  { tag: t.meta, color: 'var(--color-fg-dim)' },
  { tag: t.processingInstruction, color: 'var(--color-fg-dim)' },

  // --- headings: layered on top of livepreview's line-level font sizes ---
  // Colour only; font family & sizes are set by .cm-md-h1..h6 rules in
  // livePreviewTheme. Keeping them here too means raw `# heading` lines
  // that happen to be *inside* code blocks or inside an un-decorated tree
  // node still get a consistent colour.
  { tag: t.heading1, color: 'var(--color-fg)' },
  { tag: t.heading2, color: 'var(--color-fg)' },
  { tag: t.heading3, color: 'var(--color-fg)' },
  { tag: t.heading4, color: 'var(--color-fg)' },
  { tag: t.heading5, color: 'var(--color-fg-muted)' },
  { tag: t.heading6, color: 'var(--color-fg-muted)' },

  // --- emphasis families ---
  // weight/style only — colour stays at the ambient `--color-fg`.
  { tag: t.strong, fontWeight: '600' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.strikethrough, textDecoration: 'line-through' },

  // --- links ---
  // Wiki-links are decorated separately (`.cm-md-wikilink` in livepreview).
  // These rules cover standard `[text](url)` markdown links + bare URLs.
  { tag: t.link, color: 'var(--color-accent)' },
  { tag: t.url, color: 'var(--color-accent)', textDecoration: 'underline', textDecorationColor: 'var(--color-accent-weak)' },

  // --- code ---
  // Inline `foo` and fenced ```...``` blocks. Font family is inherited via
  // the .cm-md-code CSS class in livePreviewTheme when possible; this rule
  // catches the backticks themselves + fenced content when livepreview
  // doesn't paint a class onto them.
  { tag: t.monospace, fontFamily: 'var(--font-mono)' },

  // --- quotes / blockquotes ---
  // Line-level styling (left border + italic) lives in livepreview's
  // `.cm-md-quote` rule. We just tint the text colour slightly muted.
  { tag: t.quote, color: 'var(--color-fg-muted)', fontStyle: 'italic' },

  // --- generic syntactic noise ---
  { tag: t.comment, color: 'var(--color-fg-dim)', fontStyle: 'italic' },

  // --- inside fenced code blocks (language=js/ts/rust/etc. highlighted
  // when the user has configured @codemirror/lang-* ; we don't ship those
  // yet so these mostly apply to the lezer generic scopes) ---
  { tag: t.keyword, color: 'oklch(0.55 0.14 280)' },
  { tag: t.string, color: 'oklch(0.55 0.13 140)' },
  { tag: t.number, color: 'oklch(0.60 0.14 40)' },
  { tag: t.bool, color: 'oklch(0.60 0.14 40)' },
  { tag: t.null, color: 'oklch(0.60 0.14 40)' },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: 'oklch(0.50 0.15 230)' },
  { tag: t.typeName, color: 'oklch(0.55 0.13 200)' },
  { tag: t.operator, color: 'var(--color-fg-muted)' },
  { tag: t.punctuation, color: 'var(--color-fg-dim)' }
]);
