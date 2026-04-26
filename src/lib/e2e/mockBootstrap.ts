/**
 * Phase 4 Stage 1 — E2E browser-mode bootstrap.
 *
 * The Playwright suite cannot spin up the Tauri Rust backend, so the
 * SvelteKit shell needs a way to fake "vault open + AI enabled + tool
 * permissions on" without ever calling `invoke()`. This module:
 *
 * - Is **only loaded when** `import.meta.env.PUBLIC_E2E === '1'` (Vite
 *   constant-folds the gate at build time, so production bundles drop
 *   the entire dynamic import).
 * - Activates **only when** `?e2eMock=1` is present in the URL — gives
 *   developers a clean way to opt into the mock from the same dev
 *   build (e.g. `vite preview` with `PUBLIC_E2E=1`).
 *
 * The mock seeds:
 * - A pretend `VaultInfo` so `{#if !vaultState.current}` short-circuits
 *   to the main shell (Settings, Panel, ChatPanel become reachable).
 * - A blank list of recent vaults / files so the sidebars render.
 *
 * It does NOT touch ChatPanel — that file owns its own URL-flag check
 * + mock provider stub (the same one the existing tests already lean
 * on). The two layers are gated by the same env var so they always
 * ship or skip together.
 */

import type { VaultInfo } from '$lib/ipc/vault';
import { vaultState } from '$lib/state/vault.svelte';

const E2E_BUILD = import.meta.env.PUBLIC_E2E === '1';

/** Returns true when the URL flag is on **and** this is an E2E build. */
export function isE2eMockActive(): boolean {
  if (!E2E_BUILD) return false;
  if (typeof window === 'undefined') return false;
  return new URLSearchParams(window.location.search).get('e2eMock') === '1';
}

/**
 * Seed a fake vault so the welcome screen step is bypassed. Idempotent —
 * calling it twice is a no-op once `vaultState.current` is set.
 *
 * Returns the VaultInfo that was set, or `null` when the gate is off
 * (caller should fall back to its normal bootstrap path).
 */
export function bootstrapE2eVault(): VaultInfo | null {
  if (!isE2eMockActive()) return null;
  if (vaultState.current) return vaultState.current;
  const fake: VaultInfo = {
    path: '/mock-vault',
    initialized_at: new Date().toISOString()
  };
  vaultState.setCurrent(fake);
  installInvokeStub();
  return fake;
}

/**
 * Install a `window.__TAURI_INTERNALS__` stub so the existing Tauri IPC
 * wrappers (`invoke('file_write', …)`, `invoke('file_delete', …)`, …)
 * resolve to plausible mock responses instead of throwing
 * `Cannot read properties of undefined (reading 'invoke')`. Only the
 * commands that the agent-chat E2E suite actually exercises need real
 * shapes — everything else returns `null` so callers see "ok, no data".
 *
 * Idempotent: re-installs are no-ops once a stub is in place. Safe to
 * call multiple times during HMR / Strict-Mode-style remounts.
 */
function installInvokeStub(): void {
  if (typeof window === 'undefined') return;
  if ((window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) return;

  type InvokeArgs = Record<string, unknown> | undefined;

  function fakeInvoke(cmd: string, args: InvokeArgs): unknown {
    switch (cmd) {
      case 'file_write':
      case 'file_delete':
      case 'file_move':
      case 'attachment_save':
      case 'ai_record_proposal_resolution':
        return null;
      case 'file_move_with_refs': {
        const from = (args?.from as string) ?? '';
        const to = (args?.to as string) ?? from;
        return {
          old_path: from,
          new_path: to,
          rewritten_files: [],
          rewritten_links: 0,
          warnings: []
        };
      }
      case 'file_read':
        return '---\ntitle: Mock Note\n---\n\n这是 e2e mock 注入的笔记内容。';
      case 'file_exists':
        return true;
      case 'file_list':
        return [];
      case 'app_config_get':
        return {
          theme: 'system',
          autosave_ms: 500,
          shortcuts: {},
          ai_enabled: true,
          ai_tool_permissions: {
            allow_readonly: true,
            allow_writeback: true,
            allow_destructive: true
          }
        };
      case 'index_backlinks':
      case 'index_outgoing':
      case 'index_unresolved':
      case 'index_tags':
      case 'index_inbox_list':
      case 'index_projects_by_status':
      case 'index_tasks_today':
      case 'index_tasks_upcoming':
      case 'attachment_unreferenced':
      case 'vault_recent':
        return [];
      case 'index_unresolved_count':
      case 'index_tasks_count':
        return 0;
      default:
        return null;
    }
  }

  (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
    invoke: (cmd: string, args: InvokeArgs) => Promise.resolve(fakeInvoke(cmd, args)),
    transformCallback: <T>(cb?: (payload: T) => void) => {
      // Tauri normally registers a callback ID; in e2e mode we just hand
      // back a no-op number so the wrapper code is happy.
      void cb;
      return Math.floor(Math.random() * 1_000_000);
    },
    plugins: {},
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } }
  };
}
