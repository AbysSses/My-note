<script lang="ts">
  /**
   * Far-left 56px navigation rail. Fixed-width column in the app grid.
   * Mirrors `app-core.jsx:IconRail` from the Second-design handoff — brand mark
   * + vertical nav + settings. Active state is driven by the parent via
   * `active`, so the rail itself stays stateless.
   */
  type ActiveKey = 'chat' | 'notes' | 'tasks' | 'inbox' | 'graph' | null;

  interface Props {
    active: ActiveKey;
    onGoHome?: () => void;
    onSelectChat?: () => void;
    onSelectNotes?: () => void;
    onSelectTasks?: () => void;
    onSelectInbox?: () => void;
    onOpenDaily?: () => void;
    onQuickCapture?: () => void;
    onSelectGraph?: () => void;
    onOpenPalette?: () => void;
    onToggleTweaks?: () => void;
  }

  const {
    active,
    onGoHome,
    onSelectChat,
    onSelectNotes,
    onSelectTasks,
    onSelectInbox,
    onOpenDaily,
    onQuickCapture,
    onSelectGraph,
    onOpenPalette,
    onToggleTweaks
  }: Props = $props();

  type Item = {
    key: Exclude<ActiveKey, null> | 'daily' | 'capture' | 'search';
    label: string;
    onClick?: () => void;
    icon: 'chat' | 'book' | 'tasks' | 'calendar' | 'scissors' | 'pin' | 'search' | 'graph';
  };

  // Order + icon mapping derived from `app-core.jsx:119-127`. We keep the
  // design's nav icons but point them at app-native actions. Chat sits at the
  // top — clicking it surfaces the AI chat in the middle pane (the design's
  // default), distinct from the right-panel "AI Chat" tab that only appears
  // when a note is open.
  const items: Item[] = $derived([
    { key: 'chat', label: 'Chat', icon: 'chat', onClick: onSelectChat },
    { key: 'notes', label: 'Notes', icon: 'book', onClick: onSelectNotes },
    { key: 'tasks', label: 'Tasks', icon: 'tasks', onClick: onSelectTasks },
    { key: 'daily', label: 'Today', icon: 'calendar', onClick: onOpenDaily },
    { key: 'capture', label: 'Capture', icon: 'scissors', onClick: onQuickCapture },
    { key: 'inbox', label: 'Inbox', icon: 'pin', onClick: onSelectInbox },
    { key: 'graph', label: 'Graph', icon: 'graph', onClick: onSelectGraph },
    { key: 'search', label: 'Search', icon: 'search', onClick: onOpenPalette }
  ]);
</script>

<aside class="rail" aria-label="Workspace navigation">
  <!-- brand mark = home -->
  <button
    class="brand"
    type="button"
    aria-label="Home"
    title="Home"
    onclick={() => onGoHome?.()}
  >
    Q
  </button>

  {#each items as item (item.key)}
    {@const isActive = active !== null && item.key === active}
    <button
      class="rail-btn"
      class:is-active={isActive}
      type="button"
      aria-label={item.label}
      title={item.label}
      onclick={() => item.onClick?.()}
    >
      {#if isActive}
        <span class="active-bar" aria-hidden="true"></span>
      {/if}
      {#if item.icon === 'chat'}
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <path d="M3 5a2 2 0 012-2h8a2 2 0 012 2v5a2 2 0 01-2 2H8l-4 3v-3H5a2 2 0 01-2-2V5z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/>
        </svg>
      {:else if item.icon === 'book'}
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <path d="M3 3h5a2 2 0 012 2v10a2 2 0 00-2-2H3V3zM15 3h-5a2 2 0 00-2 2v10a2 2 0 012-2h5V3z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
        </svg>
      {:else if item.icon === 'tasks'}
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <path d="M4 5l2 2 4-4M4 11l2 2 4-4M13 5h2M13 11h2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      {:else if item.icon === 'calendar'}
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <rect x="3" y="4" width="12" height="11" rx="2" stroke="currentColor" stroke-width="1.2"/>
          <path d="M3 7h12M6 2v3M12 2v3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        </svg>
      {:else if item.icon === 'scissors'}
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <circle cx="5" cy="5" r="2" stroke="currentColor" stroke-width="1.2"/>
          <circle cx="5" cy="13" r="2" stroke="currentColor" stroke-width="1.2"/>
          <path d="M7 6l8 7M7 12l8-7" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        </svg>
      {:else if item.icon === 'pin'}
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <path d="M9 2v5l3 3H6l3-3V2M9 10v5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      {:else if item.icon === 'graph'}
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <circle cx="4" cy="5" r="1.8" stroke="currentColor" stroke-width="1.2"/>
          <circle cx="14" cy="5" r="1.8" stroke="currentColor" stroke-width="1.2"/>
          <circle cx="9" cy="13" r="1.8" stroke="currentColor" stroke-width="1.2"/>
          <path d="M5.5 6L7.8 11.5M12.5 6L10.2 11.5M5.5 5h7" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
        </svg>
      {:else if item.icon === 'search'}
        <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
          <circle cx="8" cy="8" r="4.5" stroke="currentColor" stroke-width="1.3"/>
          <path d="M11.5 11.5l3 3" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
        </svg>
      {/if}
    </button>
  {/each}

  <div class="flex-spacer"></div>

  <button
    class="rail-btn"
    type="button"
    aria-label="Tweaks"
    title="Tweaks (appearance)"
    onclick={() => onToggleTweaks?.()}
  >
    <svg width="18" height="18" viewBox="0 0 18 18" fill="none">
      <circle cx="9" cy="9" r="2" stroke="currentColor" stroke-width="1.2"/>
      <path d="M9 2v1.5M9 14.5V16M2 9h1.5M14.5 9H16M3.5 3.5l1 1M13.5 13.5l1 1M3.5 14.5l1-1M13.5 4.5l1-1" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
    </svg>
  </button>
</aside>

<style>
  .rail {
    width: var(--rail-width, 56px);
    flex-shrink: 0;
    height: 100%;
    min-height: 0;
    padding: 10px 0 14px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    border-right: 0.5px solid var(--color-border);
    background: var(--color-surface-sunken);
    overflow: hidden;
  }

  .brand {
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: linear-gradient(
      135deg,
      var(--color-accent),
      oklch(0.5 0.14 290)
    );
    box-shadow: var(--accent-glow);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-family: var(--font-serif);
    font-weight: 500;
    font-size: 14px;
    margin-bottom: 12px;
    padding: 0;
    border: none;
    cursor: pointer;
  }
  .brand:hover {
    filter: brightness(1.05);
    transform: none;
  }

  .rail-btn {
    position: relative;
    width: 36px;
    height: 36px;
    padding: 0;
    border-radius: 10px;
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--color-fg-muted);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .rail-btn:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    transform: none;
  }
  .rail-btn.is-active {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.9);
  }

  .active-bar {
    position: absolute;
    left: -11px;
    top: 8px;
    bottom: 8px;
    width: 2px;
    border-radius: 2px;
    background: var(--color-accent);
    box-shadow: var(--accent-glow);
  }

  .flex-spacer {
    flex: 1;
  }
</style>
