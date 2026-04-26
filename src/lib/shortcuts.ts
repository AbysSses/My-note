export type ThemePreference = 'system' | 'light' | 'dark';

export type ShortcutActionId =
  | 'palette'
  | 'daily'
  | 'weekly'
  | 'capture'
  | 'record'
  | 'graph'
  | 'extract'
  | 'settings';

export interface ShortcutActionDef {
  label: string;
  defaultAccelerator: string;
  commandId?: string;
}

export const shortcutActionDefs = {
  palette: { label: '打开命令面板', defaultAccelerator: 'Mod+P' },
  daily: { label: 'Today', defaultAccelerator: 'Mod+D', commandId: 'today' },
  weekly: { label: 'This Week', defaultAccelerator: 'Mod+Shift+W', commandId: 'week' },
  capture: { label: 'Quick Capture', defaultAccelerator: 'Mod+Shift+N', commandId: 'capture' },
  record: { label: 'Daily Record', defaultAccelerator: 'Mod+Shift+D', commandId: 'record' },
  graph: { label: 'Open Graph View', defaultAccelerator: 'Mod+Shift+G', commandId: 'open-graph' },
  extract: {
    label: 'Extract Selection',
    defaultAccelerator: 'Mod+Shift+E',
    commandId: 'extract-selection'
  },
  settings: { label: 'Settings', defaultAccelerator: 'Mod+,', commandId: 'open-settings' }
} as const satisfies Record<ShortcutActionId, ShortcutActionDef>;

export const shortcutActionIds = Object.keys(shortcutActionDefs) as ShortcutActionId[];

export const DEFAULT_SHORTCUT_BINDINGS = Object.fromEntries(
  shortcutActionIds.map((id) => [id, shortcutActionDefs[id].defaultAccelerator])
) as Record<ShortcutActionId, string>;

export const PALETTE_SHORTCUT_ACTIONS = Object.fromEntries(
  shortcutActionIds
    .map((id) => {
      const def: ShortcutActionDef = shortcutActionDefs[id];
      const commandId = def.commandId;
      return commandId ? [commandId, id] : null;
    })
    .filter((entry): entry is [string, ShortcutActionId] => entry !== null)
) as Partial<Record<string, ShortcutActionId>>;

export interface ParsedShortcut {
  accelerator: string;
  key: string;
  mod: boolean;
  shift: boolean;
  alt: boolean;
  display: string;
}

function isMacPlatform(): boolean {
  if (typeof navigator === 'undefined') return true;
  const nav = navigator as Navigator & {
    userAgentData?: {
      platform?: string;
    };
  };
  const platform = nav.userAgentData?.platform ?? navigator.platform ?? '';
  return /mac|iphone|ipad|ipod/i.test(platform);
}

function normalizeKeyToken(raw: string): string | null {
  const token = raw.trim();
  if (!token) return null;

  const lower = token.toLowerCase();
  switch (lower) {
    case 'cmd':
    case 'command':
    case 'meta':
    case 'ctrl':
    case 'control':
    case 'mod':
      return 'Mod';
    case 'shift':
      return 'Shift';
    case 'alt':
    case 'option':
      return 'Alt';
    case 'comma':
      return ',';
    case 'period':
      return '.';
    case 'slash':
      return '/';
    case 'semicolon':
      return ';';
    case 'quote':
    case 'apostrophe':
      return "'";
    case 'minus':
      return '-';
    case 'equal':
    case 'equals':
      return '=';
    case 'bracketleft':
      return '[';
    case 'bracketright':
      return ']';
    case 'backslash':
      return '\\';
  }

  if (/^[a-z]$/i.test(token)) return token.toUpperCase();
  if (/^[0-9]$/.test(token)) return token;
  if (/^f([1-9]|1[0-2])$/i.test(token)) return token.toUpperCase();
  if (/^[,./;'=\-[\]\\]$/.test(token)) return token;

  return null;
}

function normalizeEventKey(raw: string): string | null {
  if (!raw) return null;
  switch (raw) {
    case 'Meta':
    case 'Control':
    case 'Alt':
    case 'Shift':
      return null;
    default:
      return normalizeKeyToken(raw);
  }
}

function buildDisplay(
  shortcut: Pick<ParsedShortcut, 'key' | 'mod' | 'shift' | 'alt'>,
  mac: boolean
): string {
  if (mac) {
    return [
      shortcut.mod ? '⌘' : '',
      shortcut.alt ? '⌥' : '',
      shortcut.shift ? '⇧' : '',
      shortcut.key
    ].join('');
  }
  const tokens = [
    shortcut.mod ? 'Ctrl' : null,
    shortcut.alt ? 'Alt' : null,
    shortcut.shift ? 'Shift' : null,
    shortcut.key
  ].filter((token): token is string => !!token);
  return tokens.join('+');
}

function buildShortcut(
  shortcut: Pick<ParsedShortcut, 'key' | 'mod' | 'shift' | 'alt'>,
  mac: boolean
): ParsedShortcut {
  const accelerator = [
    shortcut.mod ? 'Mod' : null,
    shortcut.alt ? 'Alt' : null,
    shortcut.shift ? 'Shift' : null,
    shortcut.key
  ]
    .filter((token): token is string => !!token)
    .join('+');

  return {
    accelerator,
    key: shortcut.key,
    mod: shortcut.mod,
    shift: shortcut.shift,
    alt: shortcut.alt,
    display: buildDisplay(shortcut, mac)
  };
}

export function parseShortcut(input: string, mac = isMacPlatform()): ParsedShortcut | null {
  const parts = input
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean);
  if (parts.length === 0) return null;

  let mod = false;
  let shift = false;
  let alt = false;
  let key: string | null = null;

  for (const part of parts) {
    const token = normalizeKeyToken(part);
    if (!token) return null;
    if (token === 'Mod') {
      mod = true;
    } else if (token === 'Shift') {
      shift = true;
    } else if (token === 'Alt') {
      alt = true;
    } else if (key === null) {
      key = token;
    } else {
      return null;
    }
  }

  if (!key || (!mod && !alt)) return null;
  return buildShortcut({ key, mod, shift, alt }, mac);
}

export function shortcutFromKeyboardEvent(
  e: KeyboardEvent,
  mac = isMacPlatform()
): ParsedShortcut | null {
  const key = normalizeEventKey(e.key);
  if (!key) return null;

  const mod = e.metaKey || e.ctrlKey;
  const alt = e.altKey;
  const shift = e.shiftKey;
  if (!mod && !alt) return null;

  return buildShortcut({ key, mod, shift, alt }, mac);
}

export function matchShortcutEvent(e: KeyboardEvent, accelerator: string): boolean {
  const parsed = parseShortcut(accelerator);
  const key = normalizeEventKey(e.key);
  if (!parsed || !key) return false;

  return (
    key === parsed.key &&
    (e.metaKey || e.ctrlKey) === parsed.mod &&
    e.altKey === parsed.alt &&
    e.shiftKey === parsed.shift
  );
}

export function formatShortcutDisplay(accelerator: string): string {
  return parseShortcut(accelerator)?.display ?? accelerator;
}

export function mergeShortcutBindings(
  overrides: Partial<Record<ShortcutActionId, string>> | null | undefined
): Record<ShortcutActionId, string> {
  const merged = { ...DEFAULT_SHORTCUT_BINDINGS };
  if (!overrides) return merged;

  for (const id of shortcutActionIds) {
    const raw = overrides[id];
    if (!raw) continue;
    const parsed = parseShortcut(raw);
    if (parsed) merged[id] = parsed.accelerator;
  }

  return merged;
}

export function findShortcutConflict(
  bindings: Record<ShortcutActionId, string>,
  actionId: ShortcutActionId,
  accelerator: string
): ShortcutActionId | null {
  for (const id of shortcutActionIds) {
    if (id === actionId) continue;
    if (bindings[id] === accelerator) return id;
  }
  return null;
}
