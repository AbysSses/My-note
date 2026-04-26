/**
 * Canvas renderer for the note-graph view.
 *
 * Responsibilities
 * ----------------
 * 1. devicePixelRatio-aware sizing — the backing store is `dpr * cssSize` so
 *    strokes stay crisp on retina. `resize()` must be called whenever the
 *    host element or dpr changes.
 * 2. d3-zoom integration — the caller hands us a `ZoomTransform` via
 *    `setTransform`. We paint world-space primitives (nodes, edges) using
 *    that transform, then layer labels in screen space so the font size
 *    stays constant at any zoom level (Obsidian-style).
 * 3. Hit-testing via d3-quadtree, rebuilt whenever the layout finishes a
 *    tick. `pick(clientX, clientY)` inverts the transform to world coords
 *    and finds the closest node within its visual radius.
 *
 * Colours are supplied by the host component from CSS custom properties,
 * so the renderer itself is theme-agnostic.
 */

import { quadtree, type Quadtree } from 'd3-quadtree';
import type { ZoomTransform } from 'd3-zoom';
import { zoomIdentity } from 'd3-zoom';

import type { GraphEdge, GraphNode } from '$lib/ipc/graph';

/** One node after d3-force has injected simulation coordinates. */
export interface SimNode extends GraphNode {
  x: number;
  y: number;
  vx?: number;
  vy?: number;
  /** Non-null when the user has pinned the node via drag. */
  fx?: number | null;
  fy?: number | null;
}

/** Edge after d3-force replaces string endpoints with live node references. */
export interface SimEdge extends Omit<GraphEdge, 'src' | 'dst'> {
  source: SimNode;
  target: SimNode;
}

export interface RendererColors {
  /** Background fill — usually the same as the surrounding pane. */
  background: string;
  /** Resolved edge stroke. */
  edge: string;
  /** Highlighted edge stroke (hovered node's neighbourhood). */
  edgeStrong: string;
  /** Node label text. */
  label: string;
  /** Fallback ring/stroke around every node. */
  nodeStroke: string;
  /** Ring around the currently selected / open note. */
  selection: string;
  /** Map from `note_type` → fill colour. `default` is used when a type is
   *  missing from the map. */
  byType: Record<string, string>;
  typeFallback: string;
}

export interface RendererOptions {
  colors: RendererColors;
  /** Screen-space px radius added on top of the degree-based base radius. */
  baseRadius?: number;
  /** Multiplier applied to √degree before adding to base. */
  degreeRadiusScale?: number;
  /** Font for node labels. Caller is responsible for having loaded it. */
  labelFont?: string;
  /** Labels are hidden below this zoom level to avoid clutter. */
  labelMinZoom?: number;
}

const DEFAULTS: Required<Omit<RendererOptions, 'colors'>> = {
  baseRadius: 3.5,
  degreeRadiusScale: 1.6,
  labelFont: "500 11px 'Inter Tight', system-ui, sans-serif",
  labelMinZoom: 1.1
};

/** Visual radius for a node based on its degree. */
export function nodeRadius(n: GraphNode, opts: RendererOptions): number {
  const { baseRadius, degreeRadiusScale } = { ...DEFAULTS, ...opts };
  const deg = (n.in_degree ?? 0) + (n.out_degree ?? 0);
  return baseRadius + degreeRadiusScale * Math.sqrt(deg);
}

/**
 * Stateful canvas renderer. One instance per `<canvas>` — dispose via
 * `destroy()` to release the quadtree.
 */
export class GraphCanvasRenderer {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private opts: RendererOptions & Required<Omit<RendererOptions, 'colors'>>;

  private nodes: SimNode[] = [];
  private edges: SimEdge[] = [];
  /** Lookup path → node for O(1) neighbourhood highlighting. */
  private byPath: Map<string, SimNode> = new Map();
  /** Rebuilt from `nodes` on every `setData` or `markLayoutDirty`. */
  private tree: Quadtree<SimNode> | null = null;
  /** Pre-indexed edges by endpoint path, for highlight rendering. */
  private edgesBySrc: Map<string, SimEdge[]> = new Map();
  private edgesByDst: Map<string, SimEdge[]> = new Map();

  private transform: ZoomTransform = zoomIdentity;
  private hoverPath: string | null = null;
  private selectionPath: string | null = null;

  private cssWidth = 0;
  private cssHeight = 0;
  private dpr = 1;

  constructor(canvas: HTMLCanvasElement, opts: RendererOptions) {
    this.canvas = canvas;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('GraphCanvasRenderer: 2D context unavailable');
    this.ctx = ctx;
    this.opts = { ...DEFAULTS, ...opts };
    this.resize();
  }

  /**
   * Resize the backing store to match the element's CSS box × devicePixelRatio.
   * Call from a ResizeObserver and on `window.devicePixelRatio` changes.
   */
  resize(): void {
    const rect = this.canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    this.cssWidth = rect.width;
    this.cssHeight = rect.height;
    this.dpr = dpr;
    // Only reallocate when the underlying pixel size actually changes —
    // otherwise every resize wipes the context state (line dash, fonts…).
    const pxW = Math.max(1, Math.round(rect.width * dpr));
    const pxH = Math.max(1, Math.round(rect.height * dpr));
    if (this.canvas.width !== pxW) this.canvas.width = pxW;
    if (this.canvas.height !== pxH) this.canvas.height = pxH;
  }

  setData(nodes: SimNode[], edges: SimEdge[]): void {
    this.nodes = nodes;
    this.edges = edges;
    this.byPath = new Map(nodes.map((n) => [n.path, n]));
    this.edgesBySrc = new Map();
    this.edgesByDst = new Map();
    for (const e of edges) {
      (this.edgesBySrc.get(e.source.path) ?? this.edgesBySrc.set(e.source.path, []).get(e.source.path)!).push(e);
      (this.edgesByDst.get(e.target.path) ?? this.edgesByDst.set(e.target.path, []).get(e.target.path)!).push(e);
    }
    this.markLayoutDirty();
  }

  /**
   * Mark the layout as changed — the quadtree is rebuilt lazily on the next
   * `pick` call. Call from the simulation's `tick` listener.
   */
  markLayoutDirty(): void {
    this.tree = null;
  }

  setTransform(t: ZoomTransform): void {
    this.transform = t;
  }

  setHover(path: string | null): void {
    this.hoverPath = path;
  }

  setSelection(path: string | null): void {
    this.selectionPath = path;
  }

  setColors(colors: RendererColors): void {
    this.opts = { ...this.opts, colors };
  }

  getTransform(): ZoomTransform {
    return this.transform;
  }

  /**
   * Main draw. Cheap enough to call on every rAF tick for a few thousand
   * nodes — the hot path is a single pass over nodes + edges, with the
   * transform applied via `setTransform` so we don't juggle save/restore.
   */
  draw(): void {
    const { ctx, cssWidth, cssHeight, dpr, transform, opts } = this;
    const { colors } = opts;
    const { k, x, y } = transform;

    // 1. Clear in device pixels, ignoring any prior transform.
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    ctx.fillStyle = colors.background;
    ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);

    // 2. World-space drawing: combine dpr with the zoom transform in one matrix.
    ctx.setTransform(dpr * k, 0, 0, dpr * k, dpr * x, dpr * y);

    // Edges. Dimmed globally so the node layer pops. Hovered neighbourhood
    // gets a second pass with a stronger stroke.
    const hoverSet = this.hoverNeighbourEdges();

    ctx.strokeStyle = colors.edge;
    ctx.lineWidth = 1 / k; // keep 1 device-px stroke regardless of zoom
    ctx.globalAlpha = this.hoverPath ? 0.25 : 0.6;
    ctx.beginPath();
    for (const e of this.edges) {
      if (hoverSet.has(e)) continue;
      ctx.moveTo(e.source.x, e.source.y);
      ctx.lineTo(e.target.x, e.target.y);
    }
    ctx.stroke();

    if (hoverSet.size > 0) {
      ctx.globalAlpha = 1;
      ctx.strokeStyle = colors.edgeStrong;
      ctx.lineWidth = 1.6 / k;
      ctx.beginPath();
      for (const e of hoverSet) {
        ctx.moveTo(e.source.x, e.source.y);
        ctx.lineTo(e.target.x, e.target.y);
      }
      ctx.stroke();
    }

    // Nodes.
    ctx.globalAlpha = 1;
    ctx.strokeStyle = colors.nodeStroke;
    ctx.lineWidth = 1 / k;
    for (const n of this.nodes) {
      const r = nodeRadius(n, opts);
      const fill = colors.byType[n.note_type ?? ''] ?? colors.typeFallback;
      ctx.fillStyle = fill;
      ctx.beginPath();
      ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
    }

    // Selection / hover rings — drawn LAST in world-space so they sit above
    // the node fills.
    if (this.selectionPath) {
      const sel = this.byPath.get(this.selectionPath);
      if (sel) this.drawRing(sel, colors.selection, 2.5 / k);
    }
    if (this.hoverPath && this.hoverPath !== this.selectionPath) {
      const hv = this.byPath.get(this.hoverPath);
      if (hv) this.drawRing(hv, colors.edgeStrong, 1.8 / k);
    }

    // 3. Labels in screen space (constant font size). Switch back to the
    //    device-pixel identity matrix and manually project each node.
    if (k >= opts.labelMinZoom) {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.fillStyle = colors.label;
      ctx.font = opts.labelFont;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'top';
      for (const n of this.nodes) {
        const px = transform.applyX(n.x);
        const py = transform.applyY(n.y);
        if (px < -80 || px > cssWidth + 80) continue;
        if (py < -20 || py > cssHeight + 40) continue;
        const r = nodeRadius(n, opts) * k;
        ctx.fillText(labelFor(n), px, py + r + 3);
      }
    }
  }

  /**
   * Pick the topmost node under client-space (`clientX`, `clientY`). Returns
   * null if nothing's within its visual radius. Uses d3-quadtree for the
   * initial candidate so cost stays O(log n).
   */
  pick(clientX: number, clientY: number): SimNode | null {
    const rect = this.canvas.getBoundingClientRect();
    const cssX = clientX - rect.left;
    const cssY = clientY - rect.top;
    const [wx, wy] = this.transform.invert([cssX, cssY]);
    if (!this.tree) this.rebuildTree();
    // Visual radius in world coords = visual radius / zoom. Start with the
    // max node radius plus a few px of slop for fat-finger tolerance.
    const maxR = 30 / this.transform.k;
    const hit = this.tree!.find(wx, wy, maxR);
    if (!hit) return null;
    const dx = hit.x - wx;
    const dy = hit.y - wy;
    const rCss = nodeRadius(hit, this.opts);
    const rWorld = (rCss + 2) / this.transform.k; // +2 px slop
    if (dx * dx + dy * dy > rWorld * rWorld) return null;
    return hit;
  }

  /** Convert a world point to device-independent (CSS px) screen coords. */
  project(wx: number, wy: number): { x: number; y: number } {
    return { x: this.transform.applyX(wx), y: this.transform.applyY(wy) };
  }

  destroy(): void {
    this.tree = null;
    this.nodes = [];
    this.edges = [];
    this.byPath.clear();
    this.edgesBySrc.clear();
    this.edgesByDst.clear();
  }

  // --- internals ---

  private hoverNeighbourEdges(): Set<SimEdge> {
    const out = new Set<SimEdge>();
    if (!this.hoverPath) return out;
    for (const e of this.edgesBySrc.get(this.hoverPath) ?? []) out.add(e);
    for (const e of this.edgesByDst.get(this.hoverPath) ?? []) out.add(e);
    return out;
  }

  private drawRing(n: SimNode, color: string, width: number): void {
    const { ctx, opts } = this;
    ctx.strokeStyle = color;
    ctx.lineWidth = width;
    ctx.beginPath();
    ctx.arc(n.x, n.y, nodeRadius(n, opts) + 3 * width, 0, Math.PI * 2);
    ctx.stroke();
  }

  private rebuildTree(): void {
    this.tree = quadtree<SimNode>()
      .x((n) => n.x)
      .y((n) => n.y)
      .addAll(this.nodes);
  }
}

/** Short label — prefers the `title` frontmatter field, falls back to the
 *  file basename without extension. */
export function labelFor(n: GraphNode): string {
  if (n.title && n.title.trim()) return n.title;
  const slash = n.path.lastIndexOf('/');
  const base = slash >= 0 ? n.path.slice(slash + 1) : n.path;
  return base.replace(/\.md$/i, '');
}
