import { invoke } from '@tauri-apps/api/core';
import type { ShortcutActionId, ThemePreference } from '$lib/shortcuts';

/**
 * Non-secret half of the AI provider config. The API key is **never** here;
 * fetch its presence with `aiProviderHasApiKey()`.
 */
export interface AiProviderConfig {
  /** Provider kind (only `"openai"` for now). */
  kind: string;
  /** Full HTTP base URL, no trailing slash. Empty = not yet set. */
  base_url: string;
  /** Embedding model identifier (e.g. `text-embedding-3-small`). */
  embed_model: string;
  /**
   * Chat model identifier (e.g. `gpt-4o-mini`, `llama3.1`). Optional —
   * empty string means "chat disabled, embeddings-only". Independent from
   * `embed_model` because most deployments mix a small/cheap embedder
   * with a larger chat model.
   */
  chat_model: string;
}

export interface AiToolPermissions {
  allow_readonly: boolean;
  allow_writeback: boolean;
  allow_destructive: boolean;
}

export interface AppConfigSnapshot {
  recent_vaults: string[];
  theme: ThemePreference | null;
  autosave_ms: number | null;
  shortcuts: Partial<Record<ShortcutActionId, string>>;
  /** null means "not set yet" — treat as true (panel visible by default). */
  ai_enabled: boolean | null;
  /** null = no provider configured; show "new provider" form in Settings. */
  ai_provider: AiProviderConfig | null;
  ai_tool_permissions: AiToolPermissions;
}

export async function appConfigGet(): Promise<AppConfigSnapshot> {
  return await invoke<AppConfigSnapshot>('app_config_get');
}

export async function appConfigSetTheme(theme: ThemePreference): Promise<AppConfigSnapshot> {
  return await invoke<AppConfigSnapshot>('app_config_set_theme', { theme });
}

export async function appConfigSetAutosaveMs(autosaveMs: number): Promise<AppConfigSnapshot> {
  return await invoke<AppConfigSnapshot>('app_config_set_autosave_ms', { autosaveMs });
}

export async function appConfigSetShortcuts(
  shortcuts: Partial<Record<ShortcutActionId, string>>
): Promise<AppConfigSnapshot> {
  return await invoke<AppConfigSnapshot>('app_config_set_shortcuts', { shortcuts });
}

export async function appConfigSetAiEnabled(enabled: boolean): Promise<AppConfigSnapshot> {
  return await invoke<AppConfigSnapshot>('app_config_set_ai_enabled', { enabled });
}

export async function appConfigSetAiToolPermissions(
  permissions: AiToolPermissions
): Promise<AppConfigSnapshot> {
  return await invoke<AppConfigSnapshot>('app_config_set_ai_tool_permissions', {
    allowReadonly: permissions.allow_readonly,
    allowWriteback: permissions.allow_writeback,
    allowDestructive: permissions.allow_destructive
  });
}
