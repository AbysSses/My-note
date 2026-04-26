/**
 * Wraps d3-force with the bits we actually need: simulation lifecycle,
 * local-mode BFS pruning, and filter-aware subgraph extraction.
 *
 * d3-force mutates node objects in place (adds `x`, `y`, `vx`, `vy`) — we
 * initialise them here with a cheap radial layout so the first frame has
 * *something* to render instead of everything collapsing onto the origin.
 */

import {
  forceCenter,
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  type Simulation
} from 'd3-force';

import type { GraphData, GraphEdge, GraphNode } from '$lib/ipc/graph';
import type { SimEdge, SimNode } from './canvasRenderer';

export interface LayoutOptions {
  /** Natural spring length. Roughly the visible distance between connected
   *  nodes at simulation rest. */
  linkDistance?: number;
  /** Spring rigidity between linked nodes. */
  linkStrength?: number;
  /** Negative = repulsion. More negative = spacier graph. */
  chargeStrength?: number;
  /** Limit how far repulsion reaches. Smaller values stabilise large graphs. */
  chargeDistanceMax?: number;
  /** Radius factor for overlap avoidance. */
  collideRadius?: (n: SimNode) => number;
  /** Number of `forceCollide` relaxation passes per tick. */
  collisionIterations?: number;
  /** Centre pull. 0 disables. */
  centerStrength?: number;
  /** Initial radial spread — nodes start in a circle of this radius. */
  initialSpread?: number;
  /** Alpha decay — larger = faster cooldown. Default 0.03 runs ~100 ticks. */
  alphaDecay?: number;
  /** Velocity damping. Larger values settle faster. */
  velocityDecay?: number;
}

const DEFAULTS: Required<LayoutOptions> = {
  linkDistance: 48,
  linkStrength: 0.5,
  chargeStrength: -180,
  chargeDistanceMax: Number.POSITIVE_INFINITY,
  collideRadius: (n) => 6 + 1.6 * Math.sqrt((n.in_degree ?? 0) + (n.out_degree ?? 0)),
  collisionIterations: 2,
  centerStrength: 0.04,
  initialSpread: 260,
  alphaDecay: 0.03,
  velocityDecay: 0.34
};

export type LayoutPreset = 'small' | 'medium' | 'large';

export function layoutPresetForGraph(data: GraphData): LayoutPreset {
  if (data.nodes.length >= 700 || data.edges.length >= 2200) return 'large';
  if (data.nodes.length >= 260 || data.edges.length >= 900) return 'medium';
  return 'small';
}

function presetDefaultsFor(data: GraphData): Required<LayoutOptions> {
  switch (layoutPresetForGraph(data)) {
    case 'large':
      return {
        linkDistance: 34,
        linkStrength: 0.34,
        chargeStrength: -120,
        chargeDistanceMax: 180,
        collideRadius: (n) => 4.75 + 1.15 * Math.sqrt((n.in_degree ?? 0) + (n.out_degree ?? 0)),
        collisionIterations: 1,
        centerStrength: 0.024,
        initialSpread: 340,
        alphaDecay: 0.055,
        velocityDecay: 0.42
      };
    case 'medium':
      return {
        linkDistance: 42,
        linkStrength: 0.42,
        chargeStrength: -155,
        chargeDistanceMax: 240,
        collideRadius: (n) => 5.5 + 1.4 * Math.sqrt((n.in_degree ?? 0) + (n.out_degree ?? 0)),
        collisionIterations: 1,
        centerStrength: 0.032,
        initialSpread: 300,
        alphaDecay: 0.042,
        velocityDecay: 0.38
      };
    case 'small':
    default:
      return DEFAULTS;
  }
}

/**
 * Convert raw `GraphData` into simulation-ready structures. Nodes get
 * initial `x`, `y` so the first render isn't a single dot. Edges resolve
 * their string endpoints to live node references.
 *
 * Any edge whose endpoint isn't present in `nodes` is dropped — this
 * mirrors the backend's dangling-edge protection but is needed a second
 * time because the local-mode pruner can legitimately drop a node that an
 * edge pointed at.
 */
export function toSimData(
  data: GraphData,
  opts: LayoutOptions = {}
): { nodes: SimNode[]; edges: SimEdge[] } {
  const spread = opts.initialSpread ?? DEFAULTS.initialSpread;
  const nodes: SimNode[] = data.nodes.map((n, i) => {
    const theta = (i / Math.max(1, data.nodes.length)) * Math.PI * 2;
    // Slight golden-ratio jitter on the radius to avoid a perfect circle
    // (which makes d3-force work much harder to separate nodes).
    const r = spread * (0.6 + 0.4 * ((i * 0.618033) % 1));
    return {
      ...n,
      x: Math.cos(theta) * r,
      y: Math.sin(theta) * r,
      vx: 0,
      vy: 0,
      fx: null,
      fy: null
    };
  });
  const byPath = new Map(nodes.map((n) => [n.path, n]));
  const edges: SimEdge[] = [];
  for (const e of data.edges) {
    const source = byPath.get(e.src);
    const target = byPath.get(e.dst);
    if (!source || !target) continue;
    edges.push({ source, target, link_type: e.link_type });
  }
  return { nodes, edges };
}

/**
 * BFS `maxDepth` hops from `seedPath`, return the induced subgraph. Used
 * by local mode. `edges` are treated as undirected for traversal (you want
 * to see what links at you and what you link out to).
 */
export function localSubgraph(data: GraphData, seedPath: string, maxDepth: number): GraphData {
  const seed = data.nodes.find((n) => n.path === seedPath);
  if (!seed) {
    // Seed isn't in the graph yet (fresh file, index lag). Keep a lone
    // placeholder node so local mode still has an anchor instead of looking
    // broken.
    return {
      nodes: [
        {
          path: seedPath,
          title: null,
          note_type: null,
          in_degree: 0,
          out_degree: 0
        }
      ],
      edges: []
    };
  }
  const adj = new Map<string, Set<string>>();
  for (const e of data.edges) {
    if (!adj.has(e.src)) adj.set(e.src, new Set());
    if (!adj.has(e.dst)) adj.set(e.dst, new Set());
    adj.get(e.src)!.add(e.dst);
    adj.get(e.dst)!.add(e.src);
  }

  const depth = new Map<string, number>([[seedPath, 0]]);
  const queue: string[] = [seedPath];
  while (queue.length) {
    const cur = queue.shift()!;
    const d = depth.get(cur)!;
    if (d >= maxDepth) continue;
    for (const nb of adj.get(cur) ?? []) {
      if (!depth.has(nb)) {
        depth.set(nb, d + 1);
        queue.push(nb);
      }
    }
  }

  const nodes = data.nodes.filter((n) => depth.has(n.path));
  const keep = new Set(nodes.map((n) => n.path));
  const edges = data.edges.filter((e) => keep.has(e.src) && keep.has(e.dst));
  return { nodes, edges };
}

/** Apply a type-whitelist to a graph. Empty set = no filter. */
export function filterByType(data: GraphData, allowed: Set<string>): GraphData {
  if (allowed.size === 0) return data;
  const nodes = data.nodes.filter((n) => allowed.has(n.note_type ?? 'unknown'));
  const keep = new Set(nodes.map((n) => n.path));
  const edges = data.edges.filter((e) => keep.has(e.src) && keep.has(e.dst));
  return { nodes, edges };
}

/**
 * Build and start a force simulation. Caller subscribes via `onTick` to
 * redraw. The returned handle exposes `stop`, `restart(alpha)` (e.g. after
 * a drag), and `simulation` for low-level access.
 */
export interface LayoutHandle {
  simulation: Simulation<SimNode, SimEdge>;
  nodes: SimNode[];
  edges: SimEdge[];
  stop: () => void;
  /** Nudge the sim back to life — use after dragging or when data changes. */
  reheat: (alpha?: number) => void;
}

export function startLayout(
  data: GraphData,
  onTick: () => void,
  opts: LayoutOptions = {}
): LayoutHandle {
  const merged: Required<LayoutOptions> = { ...presetDefaultsFor(data), ...opts };
  const { nodes, edges } = toSimData(data, merged);

  const charge = forceManyBody<SimNode>().strength(merged.chargeStrength);
  if (Number.isFinite(merged.chargeDistanceMax)) {
    charge.distanceMax(merged.chargeDistanceMax);
  }

  const sim = forceSimulation<SimNode>(nodes)
    .force(
      'link',
      forceLink<SimNode, SimEdge>(edges)
        .id((n) => n.path)
        .distance(merged.linkDistance)
        .strength(merged.linkStrength)
    )
    .force('charge', charge)
    .force(
      'collide',
      forceCollide<SimNode>().radius(merged.collideRadius).iterations(merged.collisionIterations)
    )
    .force('center', forceCenter(0, 0).strength(merged.centerStrength))
    .alphaDecay(merged.alphaDecay)
    .velocityDecay(merged.velocityDecay)
    .on('tick', onTick);

  return {
    simulation: sim,
    nodes,
    edges,
    stop: () => sim.stop(),
    reheat: (alpha = 0.5) => sim.alpha(alpha).restart()
  };
}

/** True if the given `GraphNode` has no outgoing or incoming resolved links. */
export function isIsolated(n: GraphNode, edges: GraphEdge[]): boolean {
  for (const e of edges) {
    if (e.src === n.path || e.dst === n.path) return false;
  }
  return true;
}
