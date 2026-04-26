<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView, keymap } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { markdown } from '@codemirror/lang-markdown';
  import { GFM, Strikethrough } from '@lezer/markdown';
  import { syntaxHighlighting, indentOnInput } from '@codemirror/language';
  import { searchKeymap } from '@codemirror/search';
  import { closeBrackets, closeBracketsKeymap, autocompletion } from '@codemirror/autocomplete';
  import {
    livePreview,
    livePreviewTheme,
    wikiLinkHandler,
    wikiLinkClickHandler,
    frontmatterCollapse
  } from './livepreview';
  import { wikiCompletionExtension } from './wikicomplete';
  import {
    imageEmbedField,
    attachmentPasteDrop,
    imageEmbedTheme,
    revokeAllAttachmentBlobs
  } from './imageEmbed';
  import { markdownHighlightStyle } from './markdownTheme';

  /**
   * Imperative handle exposed to the parent for commands that need direct
   * access to the editor state — currently block-level Extract to Note.
   * Keep the surface minimal; the reactive `content` prop + `onChange` are
   * still the canonical sync path for everything else.
   */
  export interface EditorAPI {
    /** Main selection text, or empty string if no range is selected. */
    getSelection(): string;
    /** Main selection range (`from === to` if the cursor is collapsed). */
    getSelectionRange(): { from: number; to: number };
    /** Replace the main selection with `text`; leaves the cursor at the
     *  end of the inserted text. Safe to call when selection is empty —
     *  acts as an insert. */
    replaceSelection(text: string): void;
    /** Replace an arbitrary doc range with `text` in a single CM6
     *  transaction — undo treats it as one step. Used by block-level
     *  Extract so the caller can capture a range at modal-open time and
     *  splice a `[[wiki-link]]` in atomically on confirm, regardless of
     *  what the user is selecting at the moment. */
    dispatchReplace(range: { from: number; to: number }, text: string): void;
    /** Expand the main selection to the enclosing paragraph (blank-line
     *  delimited). Use when the user invoked a block-level command with an
     *  empty selection — "extract the paragraph I'm in". Returns the new
     *  range; or null if the cursor is on an empty line. */
    expandToParagraph(): { from: number; to: number } | null;
  }

  interface Props {
    content: string;
    /** Identity of the currently open file — used to force editor sync even
     *  when two files happen to have identical content. */
    filePath?: string | null;
    onChange?: (value: string) => void;
    /** Called when the user Cmd/Ctrl+clicks a [[wiki-link]]. */
    onWikiLink?: (target: string) => void;
    /** Fires whenever the main selection head moves. 1-based line, 0-based col. */
    onCursor?: (line: number, col: number) => void;
    /** Invoked once after the view is created. Use this to stash the API
     *  in the parent for later imperative use (e.g. block extraction). */
    onReady?: (api: EditorAPI) => void;
  }

  let {
    content = $bindable(''),
    filePath = null,
    onChange,
    onWikiLink,
    onCursor,
    onReady
  }: Props = $props();

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  /** Tracks which file the editor is currently showing. */
  let lastFilePath: string | null = null;
  /**
   * When true, updateListener skips firing `onChange` / writing back to the
   * `content` prop. We flip this on during an external sync (switching files
   * etc.) to prevent the dispatched replace-all from being mistaken for a user
   * edit — otherwise every file open would trigger a redundant auto-save.
   */
  let suppressChange = false;

  function createView(initial: string) {
    return new EditorView({
      parent: host,
      state: EditorState.create({
        doc: initial,
        extensions: [
          history(),
          indentOnInput(),
          closeBrackets(),
          autocompletion(),
          // GFM extensions enable strikethrough (~~foo~~), task lists,
          // tables, and autolinks at the parser level so our custom
          // highlight style (below) actually has tags to paint against.
          markdown({ extensions: [GFM, Strikethrough] }),
          wikiCompletionExtension,
          // Custom highlight style tied to our CSS-var palette. `fallback:
          // true` means: if a tag isn't in the style, do nothing (rather
          // than crashing) — important because we don't exhaustively cover
          // every lezer tag.
          syntaxHighlighting(markdownHighlightStyle, { fallback: true }),
          livePreview,
          frontmatterCollapse,
          livePreviewTheme,
          imageEmbedField,
          imageEmbedTheme,
          attachmentPasteDrop,
          wikiLinkClickHandler,
          ...(onWikiLink ? [wikiLinkHandler.of(onWikiLink)] : []),
          keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
          EditorView.lineWrapping,
          EditorView.updateListener.of((update) => {
            if (update.docChanged && !suppressChange) {
              const next = update.state.doc.toString();
              content = next;
              onChange?.(next);
            }
            if (onCursor && (update.selectionSet || update.docChanged)) {
              const head = update.state.selection.main.head;
              const line = update.state.doc.lineAt(head);
              onCursor(line.number, head - line.from);
            }
          }),
          EditorView.theme({
            '&': { height: '100%', fontSize: '15px', overflow: 'hidden' },
            '.cm-scroller': {
              fontFamily: 'var(--font-sans)',
              lineHeight: '1.7',
              height: '100%',
              overflow: 'auto'
            },
            '.cm-content': {
              padding: '64px 32px 128px',
              maxWidth: '780px',
              margin: '0 auto'
            },
            '.cm-gutters': {
              display: 'none'
            }
          })
        ]
      })
    });
  }

  onMount(() => {
    view = createView(content);
    lastFilePath = filePath;
    // Build and hand out the imperative API. Captured `view` never changes
    // for the lifetime of this component (recreated only on full remount).
    if (onReady) {
      const v = view;
      onReady({
        getSelection() {
          const { from, to } = v.state.selection.main;
          return v.state.doc.sliceString(from, to);
        },
        getSelectionRange() {
          const { from, to } = v.state.selection.main;
          return { from, to };
        },
        replaceSelection(text: string) {
          const { from, to } = v.state.selection.main;
          v.dispatch({
            changes: { from, to, insert: text },
            selection: { anchor: from + text.length },
            scrollIntoView: true
          });
          // Re-focus so the user can keep typing after the replacement.
          v.focus();
        },
        dispatchReplace(range, text) {
          // Clamp in case the doc shrank between capture and dispatch
          // (defensive — callers generally hold a fresh range).
          const max = v.state.doc.length;
          const from = Math.max(0, Math.min(range.from, max));
          const to = Math.max(from, Math.min(range.to, max));
          v.dispatch({
            changes: { from, to, insert: text },
            selection: { anchor: from + text.length },
            scrollIntoView: true
          });
          v.focus();
        },
        expandToParagraph() {
          const doc = v.state.doc;
          const { from } = v.state.selection.main;
          const startLine = doc.lineAt(from);
          if (startLine.text.trim() === '') return null;
          // Walk backward to the first blank line (or doc start).
          let top = startLine.number;
          while (top > 1 && doc.line(top - 1).text.trim() !== '') top--;
          // Walk forward to the last non-blank line (or doc end).
          let bot = startLine.number;
          while (bot < doc.lines && doc.line(bot + 1).text.trim() !== '') bot++;
          const range = { from: doc.line(top).from, to: doc.line(bot).to };
          v.dispatch({ selection: { anchor: range.from, head: range.to } });
          return range;
        }
      });
    }
  });

  // Sync external content changes (e.g. switching files) into the editor without causing loops.
  //
  // Single source of truth for the editor's current state is `view.state.doc.toString()`.
  // We compare the incoming props to that directly — no shadow copy of "last content
  // we were told about", which previously could drift out of sync with user edits and
  // cause a file switch to be skipped (showing the previous file's content).
  $effect(() => {
    // Read the reactive props up front so Svelte registers them as deps even
    // on an early return — otherwise if `view` is still null on the first run
    // the effect never re-subscribes and subsequent file switches never sync.
    const nextContent = content;
    const nextFilePath = filePath;
    if (!view) return;

    const fileUnchanged = nextFilePath === lastFilePath;
    const docUnchanged = view.state.doc.toString() === nextContent;
    // Already showing the right file + right content → nothing to do.
    if (fileUnchanged && docUnchanged) return;

    // Either the file changed, or the content for the current file changed
    // from an external source (e.g. `editorContent = …` in the parent).
    // In both cases, replace the doc.
    suppressChange = true;
    try {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: nextContent
        }
      });
    } finally {
      suppressChange = false;
    }
    lastFilePath = nextFilePath;
  });

  onDestroy(() => {
    view?.destroy();
    view = null;
    revokeAllAttachmentBlobs();
  });
</script>

<div class="editor" data-testid="editor-host" bind:this={host}></div>

<style>
  .editor {
    width: 100%;
    height: 100%;
    min-height: 0;
    overflow: hidden;
  }
</style>
