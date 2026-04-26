import { invoke } from '@tauri-apps/api/core';

/**
 * Graph IPC wrappers — see `src-tauri/src/commands/graph.rs`.
 *
 * The backend returns the *entire* resolved note-graph in one shot. Pruning
 * for local mode (BFS from a seed) + type/tag filtering happens client-side —
 * the payload is comfortably small for a personal vault and keeping it in
 * memory avoids a round-trip every time the user toggles a filter.
 */

export interface GraphNode {
  /** Vault-relative, forward-slash-normalized path — the stable id. */
  path: string;
  title: string | null;
  /** `note` / `moc` / `daily` / `weekly` / `project` / `project-note` / `inbox`. */
  note_type: string | null;
  /** Count of resolved links pointing AT this node. */
  in_degree: number;
  /** Count of resolved links leaving this node. */
  out_degree: number;
}

export interface GraphEdge {
  src: string;
  /** Always set — backend filters out `dst_resolved IS NULL`. */
  dst: string;
  /** `wiki` / `markdown` / `embed`, or null for legacy rows. */
  link_type: string | null;
}

export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

/**
 * Fetch the full resolved graph for the active vault.
 *
 * Throws `NoActiveVault` if no vault is open. Expected to be called lazily,
 * only when the user actually opens the graph view (not on every render).
 */
export async function indexGraph(): Promise<GraphData> {
  return await invoke<GraphData>('index_graph');
}
