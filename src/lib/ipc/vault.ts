import { invoke } from '@tauri-apps/api/core';

export interface VaultInfo {
  path: string;
  initialized_at: string;
}

export interface DirEntry {
  name: string;
  rel_path: string;
  is_dir: boolean;
}

/** Initialize a directory as a MyNotes vault — creates LYT folders + default templates. */
export async function vaultInit(path: string): Promise<VaultInfo> {
  return await invoke<VaultInfo>('vault_init', { path });
}

/** Open an existing vault (verifies .mynotes/config.json exists). */
export async function vaultOpen(path: string): Promise<VaultInfo> {
  return await invoke<VaultInfo>('vault_open', { path });
}

/** Return a flat list of vault paths recently opened. Empty list if none. */
export async function vaultRecent(): Promise<string[]> {
  return await invoke<string[]>('vault_recent');
}

/** Check whether a path is already a MyNotes vault (has .mynotes/config.json). */
export async function vaultIsInitialized(path: string): Promise<boolean> {
  return await invoke<boolean>('vault_is_initialized', { path });
}

/** Result of `vaultReseedTemplates` — three disjoint buckets over bundled templates. */
export interface ReseedSummary {
  added: string[];
  updated: string[];
  unchanged: string[];
}

/**
 * Overwrite `<vault>/templates/*.md` with the bundled versions baked into
 * the binary. Only touches files that are part of the bundle; user's own
 * custom templates in the same directory are untouched.
 *
 * Use when a bundled template changed in the repo after the vault was
 * first initialized (Week 5 Task 2 hit this: `project_status` → `status`).
 */
export async function vaultReseedTemplates(): Promise<ReseedSummary> {
  return await invoke<ReseedSummary>('vault_reseed_templates');
}
