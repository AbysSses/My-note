import { invoke } from '@tauri-apps/api/core';

export interface AttachmentInfo {
  rel_path: string;
  size: number;
  mtime: number;
}

/**
 * Persist binary attachment bytes into `attachments/YYYY/MM/...`. Returns the
 * vault-relative path (forward-slash separated) the editor should write as
 * `![alt](<rel_path>)`.
 */
export async function attachmentSave(
  bytes: Uint8Array,
  originalName: string | null,
  ext: string
): Promise<string> {
  // Tauri 2 deserializes Vec<u8> from an array of numbers over JSON IPC.
  // Array.from(Uint8Array) is O(n) but acceptable for image-sized payloads
  // (<10MB in practice). For larger blobs we'd want the binary protocol.
  const arr = Array.from(bytes);
  return await invoke<string>('attachment_save', {
    bytes: arr,
    originalName,
    ext
  });
}

/**
 * Read raw bytes of an attachment. Only paths under `attachments/` are
 * accepted on the backend. Returns the raw Uint8Array — caller wraps it in a
 * Blob for rendering.
 */
export async function attachmentReadBytes(relPath: string): Promise<Uint8Array> {
  const arr = await invoke<number[]>('attachment_read_bytes', { relPath });
  return new Uint8Array(arr);
}

/**
 * Read an image file from an *absolute* path outside the vault. Used when the
 * user manually types an absolute path into markdown, or pastes a `file://`
 * URI from apps like WeChat whose clipboard doesn't expose `image/*` MIME
 * directly. Backend enforces image-only extension + 50 MB size cap.
 */
export async function attachmentReadExternalBytes(absPath: string): Promise<Uint8Array> {
  const arr = await invoke<number[]>('attachment_read_external_bytes', { absPath });
  return new Uint8Array(arr);
}

/** Full scan of `attachments/**` on disk. Returns entries sorted mtime desc. */
export async function attachmentList(): Promise<AttachmentInfo[]> {
  return await invoke<AttachmentInfo[]>('attachment_list');
}

/** Files that exist on disk but are not linked by any `![...](...)` in any md. */
export async function attachmentUnreferenced(): Promise<AttachmentInfo[]> {
  return await invoke<AttachmentInfo[]>('attachment_unreferenced');
}

/**
 * Batch delete. Returns the subset of `relPaths` that were actually removed
 * (missing / non-attachment / permission errors are logged and skipped).
 */
export async function attachmentDeleteBatch(relPaths: string[]): Promise<string[]> {
  return await invoke<string[]>('attachment_delete_batch', { relPaths });
}
