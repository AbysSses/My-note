/**
 * Live Preview decorations for the Markdown editor.
 *
 * Philosophy (per design_V2): never mutate the user's markdown. Decorations
 * layer on top of the original source so formatting is hidden when the cursor
 * is elsewhere and revealed when the cursor moves onto the relevant range.
 *
 * Week 2B.1 (done):
 *   - Frontmatter `---`...`---` → collapses to a chip
 *   - ATX headings `#`..`######` → hide marks off-line, style H1–H6
 *   - **bold**, *italic*, `inline code`
 *
 * Week 2B.2 (this file):
 *   - Unordered list markers (`-`, `*`, `+`) → bullet widget off-line
 *   - Ordered list markers (`1.`, `2.`)     → preserved, styled
 *   - Task checkboxes (`- [ ]`, `- [x]`)    → interactive <input> widget;
 *                                             click toggles the source
 *   - Wiki links `[[target]]` / `[[target|alias]]`:
 *       - hide brackets off-range
 *       - Cmd/Ctrl+click dispatches via the `wikiLinkHandler` facet
 *
 * Not yet: fenced code block chrome, images, strikethrough.
 */

import {
  Decoration,
  type DecorationSet,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
  WidgetType
} from '@codemirror/view';
import { type EditorState, Facet, type Range, StateField, type Text } from '@codemirror/state';
import { syntaxTree } from '@codemirror/language';

// ---------------------------------------------------------------------------
// Facet: lets the host Svelte component hook into wiki-link clicks.

export type WikiLinkHandler = (target: string) => void;

export const wikiLinkHandler = Facet.define<WikiLinkHandler, WikiLinkHandler | null>({
  combine: (values) => values[0] ?? null
});

// ---------------------------------------------------------------------------
// Widgets

class FrontmatterWidget extends WidgetType {
  constructor(readonly lineCount: number) {
    super();
  }
  toDOM(): HTMLElement {
    // Block-level wrapper: required because the host decoration is a block
    // replace (spans line breaks). CodeMirror lays the widget where the
    // frontmatter lines used to be; the inner span keeps the existing chip
    // styling so we don't need a separate CSS rule for the block case.
    const el = document.createElement('div');
    el.className = 'cm-md-fm-block';
    const chip = document.createElement('span');
    chip.className = 'cm-md-fm-chip';
    chip.textContent = `⟨frontmatter · ${this.lineCount} 行⟩`;
    chip.title = '点击任意 frontmatter 行展开';
    el.appendChild(chip);
    return el;
  }
  eq(o: FrontmatterWidget): boolean {
    return o.lineCount === this.lineCount;
  }
  ignoreEvent(): boolean {
    return false;
  }
}

class BulletWidget extends WidgetType {
  toDOM(): HTMLElement {
    const el = document.createElement('span');
    el.className = 'cm-md-bullet';
    el.textContent = '•';
    return el;
  }
  eq(_: BulletWidget): boolean {
    return true;
  }
  ignoreEvent(): boolean {
    return true;
  }
}

class TaskWidget extends WidgetType {
  constructor(
    readonly checked: boolean,
    readonly innerPos: number // the position of the space/x between brackets
  ) {
    super();
  }
  toDOM(view: EditorView): HTMLElement {
    const box = document.createElement('input');
    box.type = 'checkbox';
    box.className = 'cm-md-task-box';
    box.checked = this.checked;
    // Don't steal focus; let caret stay where the user put it.
    box.addEventListener('mousedown', (e) => e.preventDefault());
    box.addEventListener('click', (e) => {
      e.preventDefault();
      // Toggle by swapping the single character inside the brackets.
      const cur = view.state.doc.sliceString(this.innerPos, this.innerPos + 1);
      const next = cur === ' ' ? 'x' : ' ';
      view.dispatch({
        changes: { from: this.innerPos, to: this.innerPos + 1, insert: next }
      });
    });
    return box;
  }
  eq(o: TaskWidget): boolean {
    return o.checked === this.checked && o.innerPos === this.innerPos;
  }
  // We handle clicks ourselves; other events shouldn't retrigger plugin logic.
  ignoreEvent(): boolean {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Frontmatter detection (markdown-lang doesn't parse YAML FM by default)

interface FrontmatterBlock {
  from: number;
  to: number;
  fromLine: number;
  toLine: number;
}

function detectFrontmatter(doc: Text): FrontmatterBlock | null {
  if (doc.lines < 2) return null;
  const first = doc.line(1);
  if (first.text.trim() !== '---') return null;
  for (let i = 2; i <= doc.lines; i++) {
    const line = doc.line(i);
    if (line.text.trim() === '---') {
      return { from: first.from, to: line.to, fromLine: 1, toLine: i };
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Cursor predicates

function cursorOnLineRange(view: EditorView, loLine: number, hiLine: number): boolean {
  const doc = view.state.doc;
  for (const r of view.state.selection.ranges) {
    const a = doc.lineAt(r.from).number;
    const b = doc.lineAt(r.to).number;
    if (b >= loLine && a <= hiLine) return true;
  }
  return false;
}

function cursorOnLine(view: EditorView, lineNum: number): boolean {
  return cursorOnLineRange(view, lineNum, lineNum);
}

function cursorInRange(view: EditorView, from: number, to: number): boolean {
  for (const r of view.state.selection.ranges) {
    if (r.to >= from && r.from <= to) return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// Decoration builder

function buildDecorations(view: EditorView): DecorationSet {
  const doc = view.state.doc;
  const decs: Range<Decoration>[] = [];

  // --- Frontmatter (expanded-state styling only) ------------------------
  // The collapsed state — a block `Decoration.replace` that spans all
  // frontmatter lines — cannot be emitted from a ViewPlugin (CodeMirror
  // throws `RangeError: Decorations that replace line breaks may not be
  // specified via plugins`). That side lives in the `frontmatterCollapse`
  // StateField below. Here we only paint the expanded look.
  const fm = detectFrontmatter(doc);
  const fmCollapsed = fm ? !cursorOnLineRange(view, fm.fromLine, fm.toLine) : false;
  if (fm && !fmCollapsed) {
    for (let ln = fm.fromLine; ln <= fm.toLine; ln++) {
      const line = doc.line(ln);
      decs.push(Decoration.line({ class: 'cm-md-fm' }).range(line.from));
    }
  }

  // --- Tree walk for block/inline markdown ------------------------------
  const tree = syntaxTree(view.state);

  // Collect code ranges up front so wiki-link regex scan can skip them.
  const codeRanges: { from: number; to: number }[] = [];
  tree.iterate({
    enter: (n) => {
      if (
        n.type.name === 'FencedCode' ||
        n.type.name === 'CodeBlock' ||
        n.type.name === 'InlineCode'
      ) {
        codeRanges.push({ from: n.from, to: n.to });
        return false;
      }
    }
  });
  const inCodeRange = (pos: number) => codeRanges.some((r) => pos >= r.from && pos < r.to);

  for (const { from: vfrom, to: vto } of view.visibleRanges) {
    tree.iterate({
      from: vfrom,
      to: vto,
      enter: (node) => {
        if (fm && fmCollapsed && node.from >= fm.from && node.to <= fm.to) return false;

        const name = node.type.name;

        // --- ATX headings ---------------------------------------------
        const headingMatch = /^ATXHeading([1-6])$/.exec(name);
        if (headingMatch) {
          const lineObj = doc.lineAt(node.from);
          const level = Number(headingMatch[1]);
          decs.push(
            Decoration.line({ class: `cm-md-heading cm-md-h${level}` }).range(lineObj.from)
          );
          if (!cursorOnLine(view, lineObj.number)) {
            const c = node.node.cursor();
            if (c.firstChild()) {
              do {
                if (c.type.name === 'HeaderMark') {
                  let end = c.to;
                  while (end < doc.length && doc.sliceString(end, end + 1) === ' ') end++;
                  decs.push(Decoration.replace({}).range(c.from, end));
                }
              } while (c.nextSibling());
            }
          }
          return;
        }

        // --- Inline emphasis / code -----------------------------------
        if (name === 'StrongEmphasis' || name === 'Emphasis' || name === 'InlineCode') {
          const cls =
            name === 'StrongEmphasis'
              ? 'cm-md-bold'
              : name === 'Emphasis'
                ? 'cm-md-italic'
                : 'cm-md-code';
          decs.push(Decoration.mark({ class: cls }).range(node.from, node.to));
          if (!cursorInRange(view, node.from, node.to)) {
            const c = node.node.cursor();
            if (c.firstChild()) {
              do {
                if (c.type.name === 'EmphasisMark' || c.type.name === 'CodeMark') {
                  decs.push(Decoration.replace({}).range(c.from, c.to));
                }
              } while (c.nextSibling());
            }
          }
          return;
        }

        // --- Blockquote -----------------------------------------------
        if (name === 'Blockquote') {
          const lineObj = doc.lineAt(node.from);
          decs.push(Decoration.line({ class: 'cm-md-quote' }).range(lineObj.from));
          return;
        }

        // --- List items (bullets, numbers, tasks) ---------------------
        if (name === 'ListItem') {
          const first = node.node.firstChild;
          if (!first || first.type.name !== 'ListMark') return;
          const markText = doc.sliceString(first.from, first.to);
          const isOrdered = /^\d/.test(markText);
          const lineObj = doc.lineAt(first.from);
          const onLine = cursorOnLine(view, lineObj.number);

          if (isOrdered) {
            decs.push(Decoration.mark({ class: 'cm-md-list-num' }).range(first.from, first.to));
            return;
          }

          // Unordered: check for task marker `[ ]` / `[x]` after `<mark> `.
          const bracketStart = first.to + 1;
          if (bracketStart + 3 <= doc.length) {
            const tri = doc.sliceString(bracketStart, bracketStart + 3);
            const m = /^\[([ xX])\]$/.exec(tri);
            if (m) {
              // Replace the 3-char `[ ]` with interactive checkbox.
              decs.push(
                Decoration.replace({
                  widget: new TaskWidget(m[1] !== ' ', bracketStart + 1)
                }).range(bracketStart, bracketStart + 3)
              );
              if (m[1] !== ' ') {
                // Fade & strike the rest of the line when task is done.
                decs.push(
                  Decoration.mark({ class: 'cm-md-task-done' }).range(bracketStart + 3, lineObj.to)
                );
              }
              // Fall through to also decorate the bullet itself.
            }
          }

          if (!onLine) {
            decs.push(
              Decoration.replace({ widget: new BulletWidget() }).range(first.from, first.to)
            );
          } else {
            decs.push(Decoration.mark({ class: 'cm-md-list-mark' }).range(first.from, first.to));
          }
          return;
        }
      }
    });
  }

  // --- Wiki links & Tags (regex-scanned over visible text) -------------------
  // [[target]] or [[target|alias]]. Skip hits inside code contexts.
  const wikiRegex = /\[\[([^\]\n|]+?)(?:\|([^\]\n]+?))?\]\]/g;
  const tagRegex = /(?:^|\s)(#[A-Za-z0-9_-]+)/g;

  for (const { from: vfrom, to: vto } of view.visibleRanges) {
    const text = doc.sliceString(vfrom, vto);

    // 1. Wiki Links
    wikiRegex.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = wikiRegex.exec(text)) !== null) {
      const absFrom = vfrom + m.index;
      const absTo = absFrom + m[0].length;
      if (inCodeRange(absFrom)) continue;

      const target = m[1];
      const pipeIdx = m[0].indexOf('|');
      decs.push(
        Decoration.mark({
          class: 'cm-md-wikilink',
          attributes: {
            'data-target': target,
            title: 'Cmd/Ctrl+点击打开'
          }
        }).range(absFrom, absTo)
      );

      if (!cursorInRange(view, absFrom, absTo)) {
        // Hide opening `[[`
        decs.push(Decoration.replace({}).range(absFrom, absFrom + 2));
        // Hide closing `]]`
        decs.push(Decoration.replace({}).range(absTo - 2, absTo));
        if (pipeIdx >= 0) {
          // Hide `target|`, keep only the alias visible.
          const pipeAbs = absFrom + pipeIdx;
          decs.push(Decoration.replace({}).range(absFrom + 2, pipeAbs + 1));
        }
      }
    }

    // 2. Tags
    tagRegex.lastIndex = 0;
    while ((m = tagRegex.exec(text)) !== null) {
      const tagStr = m[1];
      const absFrom = vfrom + m.index + (m[0].length - tagStr.length);
      const absTo = absFrom + tagStr.length;
      if (inCodeRange(absFrom)) continue;
      decs.push(Decoration.mark({ class: 'cm-md-tag' }).range(absFrom, absTo));
    }
  }

  return Decoration.set(decs, true);
}

// ---------------------------------------------------------------------------
// Plugin + DOM event wiring

export const livePreview = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || u.selectionSet) {
        this.decorations = buildDecorations(u.view);
      }
    }
  },
  { decorations: (v) => v.decorations }
);

// ---------------------------------------------------------------------------
// Frontmatter collapse (StateField — must be a state field, not a plugin,
// because the decoration is a block-level `replace` that spans line breaks;
// CodeMirror throws `RangeError: Decorations that replace line breaks may
// not be specified via plugins` otherwise).
//
// The field tracks nothing besides "is the cursor outside frontmatter?" —
// it recomputes on every doc change or selection change, which is fine
// because `detectFrontmatter` is O(frontmatter-length) and bounded.

function computeFrontmatterCollapse(state: EditorState): DecorationSet {
  const fm = detectFrontmatter(state.doc);
  if (!fm) return Decoration.none;
  // Cursor inside frontmatter → don't collapse (lets user edit YAML).
  const doc = state.doc;
  for (const r of state.selection.ranges) {
    const a = doc.lineAt(r.from).number;
    const b = doc.lineAt(r.to).number;
    if (b >= fm.fromLine && a <= fm.toLine) return Decoration.none;
  }
  return Decoration.set([
    Decoration.replace({
      widget: new FrontmatterWidget(fm.toLine - fm.fromLine + 1),
      block: true
    }).range(fm.from, fm.to)
  ]);
}

export const frontmatterCollapse = StateField.define<DecorationSet>({
  create(state) {
    return computeFrontmatterCollapse(state);
  },
  update(value, tr) {
    if (tr.docChanged || tr.selection) {
      return computeFrontmatterCollapse(tr.state);
    }
    return value;
  },
  provide: (f) => EditorView.decorations.from(f)
});

// Cmd/Ctrl+click on a wiki link → call the user-provided handler.
export const wikiLinkClickHandler = EditorView.domEventHandlers({
  mousedown(e, view) {
    if (!(e.metaKey || e.ctrlKey)) return false;
    const target = e.target as HTMLElement | null;
    const el = target?.closest('.cm-md-wikilink') as HTMLElement | null;
    if (!el) return false;
    const slug = el.getAttribute('data-target');
    const handler = view.state.facet(wikiLinkHandler);
    if (slug && handler) {
      e.preventDefault();
      handler(slug);
      return true;
    }
    return false;
  }
});

// ---------------------------------------------------------------------------
// Theme

export const livePreviewTheme = EditorView.theme({
  '&': { backgroundColor: 'transparent' },
  '&.cm-focused': { outline: 'none' },
  '.cm-md-fm-block': {
    padding: '4px 0',
    lineHeight: '1.4'
  },
  '.cm-md-fm-chip': {
    display: 'inline-block',
    padding: '4px 12px',
    borderRadius: '6px',
    border: '1px solid var(--color-border)',
    fontSize: '12px',
    color: 'var(--color-fg-muted)',
    background: 'transparent',
    fontFamily: 'var(--font-mono)',
    userSelect: 'none',
    cursor: 'text'
  },
  '.cm-md-fm': {
    color: 'var(--color-fg-muted)',
    fontFamily: 'var(--font-mono)',
    fontSize: '12px',
    background: 'transparent'
  },
  '.cm-md-heading': { color: 'var(--color-fg)' },
  // Use padding instead of vertical margins on `.cm-line`. Margins create
  // visual space that CodeMirror's click/position mapping doesn't measure
  // reliably, which can make the caret land on the wrong line.
  '.cm-md-h1': {
    fontFamily: 'var(--font-serif)',
    fontSize: '2.4em',
    fontWeight: '400',
    letterSpacing: '-0.03em',
    lineHeight: '1.1',
    paddingTop: '0.55em',
    paddingBottom: '0.18em'
  },
  '.cm-md-h2': {
    fontFamily: 'var(--font-serif)',
    fontSize: '1.8em',
    fontWeight: '400',
    letterSpacing: '-0.02em',
    lineHeight: '1.2',
    paddingTop: '0.45em',
    paddingBottom: '0.14em'
  },
  '.cm-md-h3': {
    fontFamily: 'var(--font-serif)',
    fontSize: '1.4em',
    fontWeight: '500',
    letterSpacing: '-0.01em',
    lineHeight: '1.3',
    paddingTop: '0.32em',
    paddingBottom: '0.08em'
  },
  '.cm-md-h4': { fontFamily: 'var(--font-sans)', fontSize: '1.1em', fontWeight: '600' },
  '.cm-md-h5': { fontFamily: 'var(--font-sans)', fontSize: '1em', fontWeight: '600' },
  '.cm-md-h6': {
    fontFamily: 'var(--font-sans)',
    fontSize: '0.95em',
    fontWeight: '600',
    color: 'var(--color-fg-muted)'
  },
  '.cm-md-bold': { fontWeight: '600' },
  '.cm-md-italic': { fontStyle: 'italic' },
  '.cm-md-code': {
    fontFamily: 'var(--font-mono)',
    color: 'var(--color-fg)',
    padding: '2px 5px',
    background: 'var(--color-bg-subtle)',
    borderRadius: '4px',
    fontSize: '0.9em',
    border: '1px solid var(--color-border)'
  },
  '.cm-md-quote': {
    borderLeft: '3px solid var(--color-border)',
    paddingLeft: '18px',
    color: 'var(--color-fg-muted)',
    background: 'transparent',
    paddingTop: '4px',
    paddingBottom: '4px',
    fontStyle: 'italic'
  },
  '.cm-md-bullet': {
    display: 'inline-block',
    width: '1em',
    textAlign: 'center',
    color: 'var(--color-fg-muted)'
  },
  '.cm-md-list-mark': { color: 'var(--color-fg-muted)' },
  '.cm-md-list-num': { color: 'var(--color-fg-muted)', fontVariantNumeric: 'tabular-nums' },
  '.cm-md-task-box': {
    verticalAlign: 'middle',
    margin: '0 0.35em 0 0',
    cursor: 'pointer'
  },
  '.cm-md-task-done': {
    color: 'var(--color-fg-muted)',
    textDecoration: 'line-through'
  },
  '.cm-md-wikilink': {
    color: 'var(--color-accent)',
    fontWeight: '500',
    textDecoration: 'none',
    cursor: 'var(--wikilink-cursor, pointer)',
    borderBottom: '1px solid transparent',
    transition: 'border-color 0.15s ease'
  },
  '.cm-md-wikilink:hover': {
    borderBottomColor: 'var(--color-accent)'
  },
  '.cm-md-tag': {
    color: 'var(--color-fg-muted)',
    fontWeight: '500',
    cursor: 'pointer',
    transition: 'color 0.15s ease'
  },
  '.cm-md-tag:hover': {
    color: 'var(--color-fg)'
  }
});
