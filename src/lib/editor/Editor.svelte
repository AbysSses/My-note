<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorState } from '@codemirror/state';
  import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
  import { markdown } from '@codemirror/lang-markdown';
  import { syntaxHighlighting, defaultHighlightStyle, indentOnInput } from '@codemirror/language';
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
  }

  let { content = $bindable(''), filePath = null, onChange, onWikiLink, onCursor }: Props = $props();

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
          markdown(),
          wikiCompletionExtension,
          syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
          livePreview,
          frontmatterCollapse,
          livePreviewTheme,
          wikiLinkClickHandler,
          ...(onWikiLink ? [wikiLinkHandler.of(onWikiLink)] : []),
          keymap.of([
            ...closeBracketsKeymap,
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap
          ]),
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
            '&': { height: '100%', fontSize: '15px' },
            '.cm-scroller': {
              fontFamily: 'var(--font-sans)',
              lineHeight: '1.7'
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
  });
</script>

<div class="editor" bind:this={host}></div>

<style>
  .editor {
    width: 100%;
    height: 100%;
    overflow: auto;
  }
</style>
