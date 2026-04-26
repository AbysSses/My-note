import { invoke } from '@tauri-apps/api/core';
import type { ThemePreference } from '$lib/shortcuts';

/**
 * Summary of a completed vault export, returned from `vault_export_zip`.
 *
 * `bytes_written` is the uncompressed byte total — the zip file itself
 * will be smaller. We don't surface the compressed size because zip
 * entries are computed lazily by the writer; measuring would require an
 * extra stat() that's just status-bar fodder.
 */
export interface ExportSummary {
  dest_path: string;
  file_count: number;
  bytes_written: number;
  /** Count of files not archived (e.g. `.mynotes/` contents, symlinks). */
  skipped_count: number;
}

/**
 * Pack the active vault into a zip at `destAbsPath`. Excludes `.mynotes/`
 * (derived SQLite index + app metadata). Writes through a `.part` file
 * and renames on success, so an interrupted export never leaves a
 * half-written zip at the user's chosen path.
 *
 * `destAbsPath` must be an absolute path — comes straight from the
 * Tauri `save()` dialog. The Rust side refuses to clobber an existing
 * file, so the front-end doesn't have to double-check.
 */
export async function exportVaultZip(destAbsPath: string): Promise<ExportSummary> {
  return await invoke<ExportSummary>('vault_export_zip', { destAbsPath });
}

/**
 * Copy a vault-relative `.md` file to an absolute destination. Used by
 * the "Export current note" command — saves us from pulling in the
 * `@tauri-apps/plugin-fs` dependency just for one write call.
 *
 * Rejects paths that would escape the vault, paths where the source
 * isn't a file, and destinations that already exist.
 */
export async function noteExportCopy(srcRelPath: string, destAbsPath: string): Promise<void> {
  await invoke('note_export_copy', { srcRelPath, destAbsPath });
}

/**
 * Render a vault-relative `.md` file to a standalone HTML print-preview
 * document and open it in the system default browser. Returns the
 * absolute path of the written HTML (e.g. for status-bar confirmation).
 *
 * Why not `window.print()` — on Tauri macOS WKWebView a programmatically
 * triggered `window.print()` is silently dropped. Punting to the default
 * browser gives us a reliable `⌘P` / "Save as PDF" flow, and sidesteps
 * CodeMirror's viewport virtualization entirely (the renderer turns the
 * raw markdown into static HTML, not a screenshot of the editor).
 *
 * `theme` (P3-A7) follows the in-app palette so the preview carries the
 * same light/dark decision the user made in the editor, rather than
 * always rendering on white. Pass `undefined` or `'system'` to let the
 * OS `prefers-color-scheme` choose at preview time.
 */
export async function noteRenderPrintHtml(
  srcRelPath: string,
  theme?: ThemePreference
): Promise<string> {
  return await invoke<string>('note_render_print_html', { srcRelPath, theme });
}
