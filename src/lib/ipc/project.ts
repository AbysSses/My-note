import { invoke } from '@tauri-apps/api/core';

/**
 * Update the `status:` frontmatter field on `4-projects/{slug}/index.md`.
 *
 * `status` is not validated against an enum — the md file is SSOT, so any
 * string is accepted. The command palette constrains the UI-driven values
 * to `active / paused / done / archived`; hand-edits remain the user's
 * responsibility. Comparison in queries is case- and whitespace-insensitive,
 * so minor capitalization differences still bucket correctly.
 *
 * After the write, the backend synchronously reindexes the file so
 * Home / Projects panels reflect the new status without waiting for the
 * file watcher (~200ms).
 */
export async function projectSetStatus(slug: string, status: string): Promise<void> {
  await invoke<void>('project_set_status', { slug, status });
}
