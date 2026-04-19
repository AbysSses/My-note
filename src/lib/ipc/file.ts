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

export async function fileDelete(relPath: string): Promise<void> {
  await invoke('file_delete', { relPath });
}
