import type { VaultInfo } from '$lib/ipc/vault';

/**
 * Global vault state. Uses Svelte 5 runes.
 * One vault open at a time; switching is rare and involves a full reload of sidebars/editor.
 */
class VaultState {
  current = $state<VaultInfo | null>(null);
  openFilePath = $state<string | null>(null);

  setCurrent(info: VaultInfo) {
    this.current = info;
    this.openFilePath = null;
  }

  clear() {
    this.current = null;
    this.openFilePath = null;
  }

  openFile(relPath: string) {
    this.openFilePath = relPath;
  }

  /** Return to the Home view by deselecting the current file. */
  closeFile() {
    this.openFilePath = null;
  }
}

export const vaultState = new VaultState();
