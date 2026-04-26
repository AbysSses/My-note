import { invoke } from '@tauri-apps/api/core';
import type { DirEntry } from './vault';

export async function fileRead(relPath: string): Promise<string> {
  return await invoke<string>('file_read', { relPath });
}

export async function fileWrite(relPath: string, content: string): Promise<void> {
  await invoke('file_write', { relPath, content });
}

export async function fileList(relDir: string): Promise<DirEntry[]> {
  return await invoke<DirEntry[]>('file_list', { relDir });
}

export async function fileExists(relPath: string): Promise<boolean> {
  return await invoke<boolean>('file_exists', { relPath });
}

export async function fileMove(from: string, to: string): Promise<void> {
  await invoke('file_move', { from, to });
}

/** Result of a rename that also rewrites every wiki-link / embed that pointed
 *  at the old location. See `src-tauri/src/commands/rename.rs` for details. */
export interface RenameResult {
  old_path: string;
  new_path: string;
  /** Vault-relative paths of every note whose body was edited. */
  rewritten_files: string[];
  /** Total raw-link-text replacements made across all rewritten files. */
  rewritten_links: number;
  /** Non-fatal issues (e.g. a referrer that couldn't be read). */
  warnings: string[];
}

export interface FileRenamePreview {
  old_path: string;
  new_path: string;
  rewritten_files_total: number;
  rewritten_files_preview: string[];
  rewritten_links: number;
}

/** Move a file AND rewrite every referring note's `[[wiki]]` / `![alt](path)`
 *  so links don't break. Use this instead of {@link fileMove} whenever a file
 *  may be linked from elsewhere. For directories use {@link dirMoveWithRefs}. */
export async function fileMoveWithRefs(from: string, to: string): Promise<RenameResult> {
  return await invoke<RenameResult>('file_move_with_refs', { from, to });
}

/** Dry-run summary for `fileMoveWithRefs`. Performs the same path/index checks
 *  but does not write, move, or reindex anything. */
export async function fileMoveWithRefsPreview(
  from: string,
  to: string
): Promise<FileRenamePreview> {
  return await invoke<FileRenamePreview>('file_move_with_refs_preview', { from, to });
}

/** Result of a directory-level rename that also rewrites every link/embed
 *  pointing anywhere inside the moved tree. See
 *  `src-tauri/src/commands/rename.rs::dir_move_with_refs` for details. */
export interface DirRenameResult {
  old_path: string;
  new_path: string;
  /** Count of files physically moved (md + non-md combined). */
  moved_files: number;
  /** Vault-relative paths of every *external* referrer whose body was edited.
   *  In-tree referrers are NOT listed — they travel with the dir rename and
   *  are re-indexed at their new path. */
  rewritten_files: string[];
  /** Total raw-link-text replacements across all rewritten files. */
  rewritten_links: number;
  /** Non-fatal issues (e.g. a referrer that couldn't be read). */
  warnings: string[];
}

export interface DirRenamePreview {
  old_path: string;
  new_path: string;
  moved_files_total: number;
  moved_markdown_files: number;
  moved_other_files: number;
  moved_files_preview: string[];
  rewritten_files_total: number;
  rewritten_files_preview: string[];
  rewritten_links: number;
}

/** Rename / move an entire directory AND rewrite every `[[wiki]]` / `![alt](…)`
 *  reference that targeted anywhere inside it. Refuses:
 *  - vault-root rename,
 *  - `.mynotes/` either side,
 *  - target inside source (`foo → foo/bar`),
 *  - destination already exists. */
export async function dirMoveWithRefs(from: string, to: string): Promise<DirRenameResult> {
  return await invoke<DirRenameResult>('dir_move_with_refs', { from, to });
}

/** Dry-run summary for `dirMoveWithRefs`. */
export async function dirMoveWithRefsPreview(from: string, to: string): Promise<DirRenamePreview> {
  return await invoke<DirRenamePreview>('dir_move_with_refs_preview', { from, to });
}

export async function fileDelete(relPath: string): Promise<void> {
  await invoke('file_delete', { relPath });
}

/** Reveal a vault-relative path in the OS file manager (Finder on macOS,
 *  Explorer on Windows, default file manager on Linux). The macOS path
 *  pre-selects the file; Linux falls back to opening its parent directory
 *  because xdg-open has no "select" verb. */
export async function pathReveal(relPath: string): Promise<void> {
  await invoke('path_reveal', { relPath });
}

/** Result of a single-file sidebar drop import (see `file_import` in Rust). */
export interface ImportedFile {
  /** Vault-relative path of the written file, forward-slash separated. */
  rel_path: string;
  /** Basename of the source (for user-facing notice copy). */
  original_name: string;
  /** True when a `-N` suffix was appended because the destination basename
   *  collided with an existing file. */
  was_renamed: boolean;
  bytes_copied: number;
}

/** Copy an external file (absolute host path) into a vault-relative directory.
 *  Used by the sidebar Finder drag-drop flow (see design_V2.md §6.13.9).
 *  Rejects directory sources, sources already inside this vault, and
 *  collapses basename collisions by appending `-1`, `-2`, ... suffixes. */
export async function fileImport(srcAbs: string, dstDir: string): Promise<ImportedFile> {
  return await invoke<ImportedFile>('file_import', { srcAbs, dstDir });
}
