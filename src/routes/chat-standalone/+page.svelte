<script lang="ts">
  /**
   * Standalone AI chat window (P3-D2b.6).
   *
   * Thin shell that hosts `<ChatPanel variant="standalone" />` in its own
   * Tauri webview. Spun up from `Panel.svelte` when the user clicks the
   * pop-out chevron on the chat tab. The main window replaces its docked
   * chat content with a placeholder while the standalone is open, so we
   * don't end up with two live transcripts listening to the same stream
   * events simultaneously (both windows *would* receive the events, and
   * both would try to persist / reload — the docked placeholder sidesteps
   * that).
   *
   * Communication with the main window (all via `@tauri-apps/api/event`):
   *
   * - `chat-standalone:file-path`: main → standalone. Payload `{ path }`.
   *   The main window pushes the currently-open file path here whenever
   *   it changes so RAG + "link this note" in the new-session modal stay
   *   in sync without re-opening the standalone.
   * - `chat-standalone:open-note`: standalone → main. Payload `{ path }`.
   *   Fired when the user clicks a wiki-link chip or a citation; the
   *   main window picks it up and actually opens the editor tab.
   * - `chat-standalone:close`: main → standalone. No payload. The main
   *   window emits this when "AI 辅助" gets toggled off in Settings, or
   *   when the user clicks "Bring back" from the docked placeholder.
   *   The standalone acknowledges by calling `getCurrentWindow().close()`
   *   instead of the main window force-closing it, so Svelte's
   *   `onDestroy` runs and `aiChatStreamCancel` fires for any in-flight
   *   stream (avoids leaking a running spawn task on the Rust side).
   */
  import { onMount, onDestroy } from 'svelte';
  import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import ChatPanel from '$lib/panel/ChatPanel.svelte';

  let filePath = $state<string | null>(null);
  let unlistenFilePath: UnlistenFn | null = null;
  let unlistenClose: UnlistenFn | null = null;

  onMount(async () => {
    // Initial file path: we don't have a handshake protocol, so the main
    // window is expected to emit `file-path` right after the standalone
    // signals it's ready. Ready-signal is sent synchronously below.
    unlistenFilePath = await listen<{ path: string | null }>('chat-standalone:file-path', (ev) => {
      filePath = ev.payload.path;
    });
    unlistenClose = await listen('chat-standalone:close', () => {
      void getCurrentWindow().close();
    });
    // Tell the main window we're live — it'll respond with the current file path.
    await emit('chat-standalone:ready');
  });

  onDestroy(() => {
    unlistenFilePath?.();
    unlistenClose?.();
    // Let the main window flip the docked UI back. The built-in
    // `tauri://destroyed` event only fires on the destroyed webview's
    // own bus, so we can't rely on the main window listening for it —
    // a plain custom event is cheaper than an `emit_to(main, …)` round
    // trip through the Rust side.
    void emit('chat-standalone:closed');
  });

  function onOpenNote(relPath: string | null, opts?: { forceReload?: boolean }): void {
    // Route through the main window; it owns the editor.
    void emit('chat-standalone:open-note', {
      path: relPath,
      forceReload: opts?.forceReload ?? false
    });
  }
</script>

<svelte:head>
  <title>AI 对话 · MyNotes</title>
</svelte:head>

<main class="standalone-root">
  <ChatPanel {filePath} {onOpenNote} variant="standalone" />
</main>

<style>
  .standalone-root {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--color-bg);
    color: var(--color-fg);
    font-size: var(--fs-sm);
  }
</style>
