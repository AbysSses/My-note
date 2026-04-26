/**
 * Live "tweaks" for the Quire aesthetic. Mirrors the design's Tweaks panel
 * (`app-core.jsx:784-873`): accent + radius + glow + bg-tint + density are
 * editable in-app and persisted to `localStorage`. On change we patch the
 * root element's CSS variables so the whole app re-styles without reload.
 */

export type AccentKey = 'terracotta' | 'violet' | 'cyan' | 'gold' | 'neutral';
export type Density = 'tight' | 'balanced' | 'airy';

export interface Tweaks {
  accent: AccentKey;
  radius: number; // 0..1.5 — multiplier applied to every radius token
  glow: number; // 0..1   — multiplier on accent glow intensity
  bgTint: number; // 0..1   — reserved for future background tint blending
  density: Density;
}

export const DEFAULT_TWEAKS: Tweaks = {
  accent: 'terracotta',
  radius: 1,
  glow: 1,
  bgTint: 0.7,
  density: 'balanced'
};

/** Hue + chroma per accent family (matches `app-core.jsx:32-33`). */
export const ACCENT_PALETTE: Record<AccentKey, { hue: number; chroma: number }> = {
  terracotta: { hue: 38, chroma: 0.13 },
  violet: { hue: 290, chroma: 0.14 },
  cyan: { hue: 220, chroma: 0.11 },
  gold: { hue: 80, chroma: 0.11 },
  neutral: { hue: 70, chroma: 0.005 }
};

/** Hex swatch for the Tweaks panel chip (pure cosmetic). */
export const ACCENT_SWATCH: Record<AccentKey, string> = {
  terracotta: '#c96442',
  violet: '#7c6aef',
  cyan: '#4aa3c7',
  gold: '#d4b86a',
  neutral: '#b8b8ba'
};

const DENSITY_SCALE: Record<Density, number> = { tight: 0.75, balanced: 1, airy: 1.3 };

const STORAGE_KEY = 'quire-tweaks';

function clampNum(v: unknown, lo: number, hi: number, fallback: number): number {
  const n = typeof v === 'number' ? v : Number(v);
  if (!Number.isFinite(n)) return fallback;
  return Math.min(hi, Math.max(lo, n));
}

function parseStored(raw: string | null): Tweaks {
  if (!raw) return { ...DEFAULT_TWEAKS };
  try {
    const data = JSON.parse(raw) as Partial<Tweaks>;
    const accent: AccentKey =
      data.accent && data.accent in ACCENT_PALETTE ? (data.accent as AccentKey) : DEFAULT_TWEAKS.accent;
    const density: Density =
      data.density === 'tight' || data.density === 'balanced' || data.density === 'airy'
        ? data.density
        : DEFAULT_TWEAKS.density;
    return {
      accent,
      density,
      radius: clampNum(data.radius, 0, 1.5, DEFAULT_TWEAKS.radius),
      glow: clampNum(data.glow, 0, 1, DEFAULT_TWEAKS.glow),
      bgTint: clampNum(data.bgTint, 0, 1, DEFAULT_TWEAKS.bgTint)
    };
  } catch {
    return { ...DEFAULT_TWEAKS };
  }
}

/** Patch `:root` inline CSS variables to reflect the given tweaks. */
export function applyTweaks(t: Tweaks): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  const palette = ACCENT_PALETTE[t.accent];
  root.style.setProperty('--accent-hue', String(palette.hue));
  root.style.setProperty('--accent-chroma', String(palette.chroma));
  root.style.setProperty('--radius-scale', String(t.radius));
  root.style.setProperty('--glow-scale', String(t.glow));
  root.style.setProperty('--bg-tint', String(t.bgTint));
  root.style.setProperty('--density-scale', String(DENSITY_SCALE[t.density]));
}

/** Reactive state holder used by `TweaksPanel` and the app shell.
 *  Uses Svelte 5 runes — `.value` is the single source of truth. */
class TweaksStore {
  value = $state<Tweaks>({ ...DEFAULT_TWEAKS });
  visible = $state(false);
  private loaded = false;

  init(): void {
    if (this.loaded) return;
    this.loaded = true;
    if (typeof window !== 'undefined') {
      this.value = parseStored(window.localStorage.getItem(STORAGE_KEY));
    }
    applyTweaks(this.value);
  }

  set(next: Partial<Tweaks>): void {
    this.value = { ...this.value, ...next };
    applyTweaks(this.value);
    if (typeof window !== 'undefined') {
      try {
        window.localStorage.setItem(STORAGE_KEY, JSON.stringify(this.value));
      } catch {
        // quota or disabled storage — ignore; the app still works in-session
      }
    }
  }

  reset(): void {
    this.set({ ...DEFAULT_TWEAKS });
  }

  toggle(): void {
    this.visible = !this.visible;
  }
}

export const tweaksStore = new TweaksStore();
