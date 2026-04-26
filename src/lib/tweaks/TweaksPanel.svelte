<script lang="ts">
  import {
    tweaksStore,
    ACCENT_PALETTE,
    ACCENT_SWATCH,
    type AccentKey,
    type Density,
    type Tweaks
  } from './tweaksStore.svelte';

  /**
   * Floating Tweaks overlay — bottom-right 280px glass panel. Mirrors
   * `app-core.jsx:TweaksPanel`. Purely controls the app's CSS design vars
   * through `tweaksStore`, so every change applies instantly across the UI.
   */

  interface Props {
    /** Current effective theme (`'light' | 'dark'`), for highlighting the Mode toggle.
     *  The parent owns theme persistence via the existing config IPC — we just
     *  surface the control here. Optional: when omitted, Mode toggle is hidden. */
    theme?: 'light' | 'dark';
    onSetTheme?: (next: 'light' | 'dark') => void;
  }

  const { theme, onSetTheme }: Props = $props();

  const accents: AccentKey[] = ['terracotta', 'violet', 'cyan', 'gold', 'neutral'];
  const densities: Density[] = ['tight', 'balanced', 'airy'];

  const t = $derived<Tweaks>(tweaksStore.value);

  function setField<K extends keyof Tweaks>(key: K, value: Tweaks[K]) {
    tweaksStore.set({ [key]: value } as Partial<Tweaks>);
  }
</script>

{#if tweaksStore.visible}
  <aside class="tweaks" aria-label="Appearance tweaks">
    <header class="t-head">
      <span class="t-swatch" style="background: {ACCENT_SWATCH[t.accent]};"></span>
      <span class="t-title">Tweaks</span>
      <span class="t-live">live</span>
      <button
        class="t-close"
        type="button"
        aria-label="Close tweaks"
        onclick={() => tweaksStore.toggle()}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <path d="M2 2l8 8M10 2l-8 8" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
        </svg>
      </button>
    </header>

    {#if theme && onSetTheme}
      <div class="group">
        <div class="label"><span>Mode</span><span class="muted">{theme}</span></div>
        <div class="row">
          <button
            class="seg"
            class:is-active={theme === 'light'}
            type="button"
            onclick={() => onSetTheme('light')}
          >Light</button>
          <button
            class="seg"
            class:is-active={theme === 'dark'}
            type="button"
            onclick={() => onSetTheme('dark')}
          >Dark</button>
        </div>
      </div>
    {/if}

    <div class="group">
      <div class="label"><span>Accent</span><span class="muted">{t.accent}</span></div>
      <div class="row">
        {#each accents as a (a)}
          <button
            class="swatch"
            class:is-active={t.accent === a}
            type="button"
            aria-label={a}
            title={a}
            onclick={() => setField('accent', a)}
          >
            <span class="swatch-dot" style="background: {ACCENT_SWATCH[a]};"></span>
          </button>
        {/each}
      </div>
    </div>

    <div class="group">
      <div class="label"><span>Radius</span><span class="muted">{t.radius.toFixed(2)}</span></div>
      <input
        type="range"
        min="0"
        max="1.5"
        step="0.05"
        value={t.radius}
        oninput={(e) => setField('radius', parseFloat((e.target as HTMLInputElement).value))}
      />
    </div>

    <div class="group">
      <div class="label"><span>Glow</span><span class="muted">{t.glow.toFixed(2)}</span></div>
      <input
        type="range"
        min="0"
        max="1"
        step="0.05"
        value={t.glow}
        oninput={(e) => setField('glow', parseFloat((e.target as HTMLInputElement).value))}
      />
    </div>

    <div class="group">
      <div class="label"><span>Bg tint</span><span class="muted">{t.bgTint.toFixed(2)}</span></div>
      <input
        type="range"
        min="0"
        max="1"
        step="0.05"
        value={t.bgTint}
        oninput={(e) => setField('bgTint', parseFloat((e.target as HTMLInputElement).value))}
      />
    </div>

    <div class="group">
      <div class="label"><span>Density</span></div>
      <div class="row">
        {#each densities as d (d)}
          <button
            class="seg"
            class:is-active={t.density === d}
            type="button"
            onclick={() => setField('density', d)}
          >{d}</button>
        {/each}
      </div>
    </div>

    <div class="group last">
      <button class="reset" type="button" onclick={() => tweaksStore.reset()}>Reset defaults</button>
    </div>
  </aside>
{/if}

<style>
  .tweaks {
    position: fixed;
    bottom: 24px;
    right: 24px;
    z-index: 9999;
    width: 280px;
    border-radius: var(--radius-xl);
    background: var(--glass-bg);
    backdrop-filter: blur(20px) saturate(140%);
    -webkit-backdrop-filter: blur(20px) saturate(140%);
    box-shadow: var(--float-shadow);
    color: var(--color-fg);
    padding: 20px;
    font-family: var(--font-sans);
  }
  .t-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 20px;
  }
  .t-swatch {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .t-title {
    font-size: 13px;
    font-weight: 500;
    letter-spacing: -0.1px;
    color: var(--color-fg);
  }
  .t-live {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-dim);
  }
  .t-close {
    padding: 0;
    width: 22px;
    height: 22px;
    border: none;
    background: transparent;
    box-shadow: none;
    color: var(--color-fg-muted);
    cursor: pointer;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .t-close:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    transform: none;
  }

  .group {
    margin-bottom: 18px;
  }
  .group.last {
    margin-bottom: 0;
  }
  .label {
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-fg-muted);
    margin-bottom: 8px;
    display: flex;
    justify-content: space-between;
  }
  .muted {
    color: var(--color-fg-dim);
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .seg {
    flex: 1;
    padding: 8px 10px;
    border-radius: 10px;
    background: transparent;
    color: var(--color-fg-muted);
    border: 1px solid var(--color-border);
    box-shadow: none;
    font-size: 11px;
    letter-spacing: 0.2px;
    text-transform: capitalize;
    cursor: pointer;
  }
  .seg.is-active {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    border-color: var(--color-border-strong);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.9);
  }
  .seg:hover {
    background: var(--color-bg-hover);
    transform: none;
  }

  .swatch {
    width: 32px;
    height: 32px;
    border-radius: 10px;
    padding: 0;
    background: transparent;
    border: 1px solid var(--color-border);
    box-shadow: none;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .swatch:hover {
    background: var(--color-bg-hover);
    transform: none;
  }
  .swatch.is-active {
    border-color: var(--color-border-strong);
  }
  .swatch-dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
  }

  input[type='range'] {
    width: 100%;
    accent-color: var(--color-accent);
    height: 4px;
  }

  .reset {
    width: 100%;
    padding: 8px 10px;
    border-radius: 10px;
    background: transparent;
    border: 1px solid var(--color-border);
    box-shadow: none;
    color: var(--color-fg-muted);
    font-size: 11px;
    cursor: pointer;
  }
  .reset:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
    transform: none;
  }
</style>
