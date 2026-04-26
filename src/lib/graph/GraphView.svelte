<script lang="ts" module>
  // Tiny helper shared between template and script — gets the colour a
  // given note_type uses, by reading the same CSS vars the renderer does.
  // Kept in a module block so Svelte doesn't re-create it per instance.
  export function colorForType(t: string): string {
    // Cheap static map; keep in sync with GraphView.svelte's colorsFromCss.
    switch (t) {
      case 'moc':
        return 'oklch(0.55 0.16 300)';
      case 'project':
      case 'project-note':
        return 'var(--color-warning)';
      case 'evergreen':
        return 'var(--color-success)';
      case 'daily':
      case 'weekly':
      case 'inbox':
        return 'var(--color-fg-dim)';
      case 'note':
        return 'var(--color-accent)';
      default:
        return 'var(--color-fg-muted)';
    }
  }
</script>

<script lang="ts">
  /**
   * Note-graph view, swapped into the editor-pane slot when
   * `activeView === 'graph'`. Mirrors InboxView's lifecycle (fetch on mount,
   * re-fetch on refreshToken bump).
   *
   * Rendering is delegated to:
   *   * `forceLayout.ts` — d3-force simulation + BFS subgraph
   *   * `canvasRenderer.ts` — device-pixel-aware canvas + quadtree hit-test
   *
   * This component owns: data fetching, mode toggles (global / local),
   * filter state, zoom, node drag, and dispatching "open note" events back
   * to the parent so clicking a node jumps into it.
   */
  import { onDestroy, onMount, untrack } from 'svelte';
  import { select } from 'd3-selection';
  import { drag, type D3DragEvent } from 'd3-drag';
  import { zoom, zoomIdentity, type D3ZoomEvent } from 'd3-zoom';

  import { indexGraph, type GraphData } from '$lib/ipc/graph';
  import {
    GraphCanvasRenderer,
    labelFor,
    nodeRadius,
    type RendererColors,
    type SimEdge,
    type SimNode
  } from './canvasRenderer';
  import {
    filterByType,
    layoutPresetForGraph,
    localSubgraph,
    startLayout,
    type LayoutHandle
  } from './forceLayout';

  interface Props {
    /** Path of the currently open note. When in local mode we BFS from this. */
    currentFilePath: string | null;
    /** Bumped by parent to trigger a refetch after a file mutation. */
    refreshToken?: number;
    onOpenNote: (path: string) => void;
    onClose: () => void;
  }

  const { currentFilePath, refreshToken = 0, onOpenNote, onClose }: Props = $props();

  // ----- fetched data -----
  let data = $state<GraphData | null>(null);
  let loadErr = $state<string | null>(null);
  let loading = $state(true);
  let reqSeq = 0;

  async function load() {
    const myReq = ++reqSeq;
    loading = true;
    loadErr = null;
    try {
      const g = await indexGraph();
      if (myReq !== reqSeq) return;
      data = g;
    } catch (e) {
      if (myReq !== reqSeq) return;
      loadErr = String(e);
    } finally {
      if (myReq === reqSeq) loading = false;
    }
  }

  $effect(() => {
    void refreshToken;
    void load();
  });

  // ----- mode / filter state -----
  let mode = $state<'global' | 'local'>('global');
  let localDepth = $state(2);
  let query = $state('');
  /** Empty set = no filter (all types visible). */
  let excludedTypes = $state<Set<string>>(new Set());
  /** Toggle for hiding isolated nodes — they pack the periphery and rarely
   *  add information. Default ON, matching Obsidian. */
  let hideOrphans = $state(true);

  /** Distinct note types present in `data`, sorted for stable filter UI. */
  let allTypes = $derived(
    data ? [...new Set(data.nodes.map((n) => n.note_type ?? 'unknown'))].sort() : []
  );

  function toggleTypeExclusion(t: string) {
    const next = new Set(excludedTypes);
    if (next.has(t)) next.delete(t);
    else next.add(t);
    excludedTypes = next;
  }

  // ----- effective graph after mode + filter -----
  let scopedGraph = $derived.by(() => {
    if (!data) return null;
    let g: GraphData = data;
    if (mode === 'local') {
      if (!currentFilePath) return { nodes: [], edges: [] };
      g = localSubgraph(g, currentFilePath, localDepth);
    }
    return g;
  });

  let typeCounts = $derived.by(() => {
    const counts = new Map<string, number>();
    for (const node of scopedGraph?.nodes ?? []) {
      const key = node.note_type ?? 'unknown';
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return counts;
  });

  let viewGraph = $derived.by(() => {
    if (!scopedGraph) return null;
    let g: GraphData = scopedGraph;
    if (excludedTypes.size > 0) {
      const allowed = new Set(allTypes.filter((t) => !excludedTypes.has(t)));
      g = filterByType(g, allowed);
    }
    if (hideOrphans) {
      const connected = new Set<string>();
      for (const e of g.edges) {
        connected.add(e.src);
        connected.add(e.dst);
      }
      // In local mode we always keep the seed so the user sees "where am I"
      // even when the note has no outgoing links yet.
      if (mode === 'local' && currentFilePath) connected.add(currentFilePath);
      g = {
        nodes: g.nodes.filter((n) => connected.has(n.path)),
        edges: g.edges
      };
    }
    return g;
  });

  // ----- canvas + renderer lifecycle -----
  let canvasEl: HTMLCanvasElement | null = $state(null);
  let hostEl: HTMLDivElement | null = $state(null);
  let renderer: GraphCanvasRenderer | null = null;
  let layout: LayoutHandle | null = null;
  let rafPending = false;

  let hoverNode = $state<SimNode | null>(null);
  let focusPath = $state<string | null>(null);
  let selectionPath = $derived(currentFilePath);
  const labelCollator = new Intl.Collator(undefined, {
    numeric: true,
    sensitivity: 'base'
  });

  let visibleNodes = $derived.by(() => {
    if (!viewGraph) return [];
    return [...viewGraph.nodes].sort((a, b) => {
      const labelCmp = labelCollator.compare(labelFor(a), labelFor(b));
      if (labelCmp !== 0) return labelCmp;
      return labelCollator.compare(a.path, b.path);
    });
  });

  let focusNode = $derived.by<SimNode | null>(() => {
    if (!focusPath) return null;
    return (
      layout?.nodes.find((n) => n.path === focusPath) ??
      (viewGraph?.nodes.find((n) => n.path === focusPath) as SimNode | undefined) ??
      null
    );
  });

  let inspectedNode = $derived(hoverNode ?? focusNode);
  let focusIndex = $derived(focusPath ? visibleNodes.findIndex((n) => n.path === focusPath) : -1);
  let layoutPreset = $derived(viewGraph ? layoutPresetForGraph(viewGraph) : 'small');
  let keyboardStatus = $derived.by(() => {
    if (!focusNode || focusIndex < 0) return 'No graph node focused.';
    const typeLabel = focusNode.note_type ?? 'unknown';
    return `${labelFor(focusNode)}. ${typeLabel}. ${focusIndex + 1} of ${visibleNodes.length}. In ${focusNode.in_degree}, out ${focusNode.out_degree}.`;
  });
  let emptyState = $derived.by<{ title: string; body: string } | null>(() => {
    if (loading || loadErr) return null;
    if (viewGraph && viewGraph.nodes.length > 0) return null;
    if (mode === 'local') {
      if (!currentFilePath) {
        return {
          title: 'Open a note to seed local mode.',
          body: 'Local mode builds a neighbourhood around the active note. Open any note, then come back to the graph.'
        };
      }
      return {
        title: `Nothing visible around ${labelFor({
          path: currentFilePath,
          title: null,
          note_type: null,
          in_degree: 0,
          out_degree: 0
        })}.`,
        body: hideOrphans
          ? 'Try increasing depth, switching to Global, or turning off Hide orphans. Type filters can also hide the current note.'
          : 'The current note has not been indexed into the graph yet, or your type filters removed every visible node.'
      };
    }
    return {
      title: 'No notes in the vault yet.',
      body: 'Create a note, or wait for the indexer to finish, then reopen the graph.'
    };
  });

  function colorsFromCss(): RendererColors {
    if (!hostEl) {
      // fallback during very first render
      return {
        background: '#fafaf8',
        edge: 'rgba(40,30,20,0.25)',
        edgeStrong: '#b45309',
        label: '#2a1f14',
        nodeStroke: 'rgba(0,0,0,0.25)',
        selection: '#b45309',
        byType: {},
        typeFallback: '#8a8a8a'
      };
    }
    const s = getComputedStyle(hostEl);
    const accent = s.getPropertyValue('--color-accent').trim();
    const fg = s.getPropertyValue('--color-fg').trim();
    const fgMuted = s.getPropertyValue('--color-fg-muted').trim();
    const fgDim = s.getPropertyValue('--color-fg-dim').trim();
    const surface = s.getPropertyValue('--color-bg').trim();
    const warning = s.getPropertyValue('--color-warning').trim();
    const success = s.getPropertyValue('--color-success').trim();
    return {
      background: surface,
      edge: fgDim,
      edgeStrong: accent,
      label: fgMuted,
      nodeStroke: 'rgba(0,0,0,0.15)',
      selection: accent,
      byType: {
        note: accent,
        moc: 'oklch(0.55 0.16 300)', // purple-ish — MOCs are the "hubs"
        daily: fgDim,
        weekly: fgDim,
        project: warning,
        'project-note': warning,
        inbox: fgDim,
        evergreen: success
      },
      typeFallback: fgMuted
    };
  }

  function scheduleDraw() {
    if (rafPending || !renderer) return;
    rafPending = true;
    requestAnimationFrame(() => {
      rafPending = false;
      renderer?.draw();
    });
  }

  function refreshRendererColors() {
    if (!renderer) return;
    renderer.setColors(colorsFromCss());
    scheduleDraw();
  }

  function rebuildLayout() {
    if (!renderer || !viewGraph) return;
    // Stop previous simulation before starting a new one.
    layout?.stop();
    layout = startLayout(viewGraph, () => {
      renderer?.markLayoutDirty();
      scheduleDraw();
    });
    renderer.setData(layout.nodes, layout.edges);
    attachDrag();
    // Re-centre & reset zoom whenever the dataset changes meaningfully.
    resetView();
  }

  function fitTransformFor(nodes: SimNode[], padding = 56) {
    if (!hostEl || nodes.length === 0) return null;
    const rect = hostEl.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;

    let minX = Number.POSITIVE_INFINITY;
    let minY = Number.POSITIVE_INFINITY;
    let maxX = Number.NEGATIVE_INFINITY;
    let maxY = Number.NEGATIVE_INFINITY;
    const radiusOpts = { colors: colorsFromCss() };

    for (const node of nodes) {
      const r = nodeRadius(node, radiusOpts);
      minX = Math.min(minX, node.x - r);
      minY = Math.min(minY, node.y - r);
      maxX = Math.max(maxX, node.x + r);
      maxY = Math.max(maxY, node.y + r);
    }

    const graphWidth = Math.max(1, maxX - minX);
    const graphHeight = Math.max(1, maxY - minY);
    const innerWidth = Math.max(1, rect.width - padding * 2);
    const innerHeight = Math.max(1, rect.height - padding * 2);
    let scale = Math.min(innerWidth / graphWidth, innerHeight / graphHeight);
    if (!Number.isFinite(scale)) scale = 1;
    scale = nodes.length === 1 ? Math.max(scale, 1.4) : scale;
    scale = Math.max(0.45, Math.min(scale, 2.6));

    const cx = (minX + maxX) / 2;
    const cy = (minY + maxY) / 2;
    return zoomIdentity.translate(rect.width / 2 - cx * scale, rect.height / 2 - cy * scale).scale(scale);
  }

  function resetView() {
    if (!canvasEl || !renderer || !hostEl) return;
    const next =
      fitTransformFor(layout?.nodes ?? [], 64) ??
      zoomIdentity.translate(hostEl.getBoundingClientRect().width / 2, hostEl.getBoundingClientRect().height / 2);
    select(canvasEl).call(zoomBehaviour.transform, next);
    renderer.setTransform(next);
    scheduleDraw();
  }

  function nodeForPath(path: string | null): SimNode | null {
    if (!path) return null;
    return layout?.nodes.find((n) => n.path === path) ?? null;
  }

  function zoomToPath(path: string | null, minScale = 1.4) {
    if (!path || !renderer || !hostEl || !canvasEl) return;
    const node = nodeForPath(path);
    if (!node) return;
    const rect = hostEl.getBoundingClientRect();
    const k = Math.max(minScale, renderer.getTransform().k);
    const tx = rect.width / 2 - node.x * k;
    const ty = rect.height / 2 - node.y * k;
    const t = zoomIdentity.translate(tx, ty).scale(k);
    select(canvasEl).call(zoomBehaviour.transform, t);
  }

  function setKeyboardFocus(
    path: string | null,
    options: { reveal?: boolean; focusCanvas?: boolean } = {}
  ) {
    const nextPath =
      path && visibleNodes.some((n) => n.path === path) ? path : (visibleNodes[0]?.path ?? null);
    focusPath = nextPath;
    hoverNode = null;
    if (options.focusCanvas) canvasEl?.focus();
    if (options.reveal) zoomToPath(nextPath);
    else scheduleDraw();
  }

  function focusRelative(delta: number) {
    if (visibleNodes.length === 0) return;
    const currentIndex =
      focusPath !== null ? visibleNodes.findIndex((n) => n.path === focusPath) : -1;
    const fallbackIndex =
      selectionPath !== null ? visibleNodes.findIndex((n) => n.path === selectionPath) : -1;
    const startIndex = currentIndex >= 0 ? currentIndex : fallbackIndex >= 0 ? fallbackIndex : 0;
    const nextIndex =
      currentIndex >= 0
        ? (startIndex + delta + visibleNodes.length) % visibleNodes.length
        : startIndex;
    setKeyboardFocus(visibleNodes[nextIndex]?.path ?? null, {
      reveal: true,
      focusCanvas: true
    });
  }

  function focusBoundary(edge: 'first' | 'last') {
    if (visibleNodes.length === 0) return;
    const target = edge === 'first' ? visibleNodes[0] : visibleNodes[visibleNodes.length - 1];
    setKeyboardFocus(target?.path ?? null, { reveal: true, focusCanvas: true });
  }

  function openFocusedNode() {
    if (!focusPath) return;
    onOpenNote(focusPath);
  }

  // d3-zoom on the canvas. Panning + wheel zoom. Click-through on nodes is
  // handled manually (zoom filter rejects primary button on a node hit).
  const zoomBehaviour = zoom<HTMLCanvasElement, unknown>()
    .scaleExtent([0.15, 8])
    .filter((event: Event) => {
      // Let d3-drag take over when the primary button lands on a node.
      if (event.type === 'mousedown') {
        const me = event as MouseEvent;
        if (me.button !== 0) return false;
        if (!renderer) return true;
        return renderer.pick(me.clientX, me.clientY) === null;
      }
      // Wheel and keyboard always pass through.
      return !('button' in (event as MouseEvent)) || (event as MouseEvent).button === 0;
    })
    .on('zoom', (e: D3ZoomEvent<HTMLCanvasElement, unknown>) => {
      renderer?.setTransform(e.transform);
      scheduleDraw();
    });

  // d3-drag re-applied whenever the node set changes (d3 needs the current
  // data selection bound to the target element).
  function attachDrag() {
    if (!canvasEl || !renderer || !layout) return;
    const dragBehaviour = drag<HTMLCanvasElement, unknown>()
      .filter((event: Event) => {
        if (!(event instanceof MouseEvent)) return false;
        if (event.button !== 0) return false;
        return renderer!.pick(event.clientX, event.clientY) !== null;
      })
      .on('start', function (event: D3DragEvent<HTMLCanvasElement, unknown, unknown>) {
        if (!layout) return;
        const n = renderer!.pick(event.sourceEvent.clientX, event.sourceEvent.clientY);
        if (!n) return;
        // Convert client → world so we compute drag offsets in world coords.
        const t = renderer!.getTransform();
        const rect = canvasEl!.getBoundingClientRect();
        const [wx, wy] = t.invert([
          event.sourceEvent.clientX - rect.left,
          event.sourceEvent.clientY - rect.top
        ]);
        (this as unknown as Record<string, unknown>).__drag = {
          node: n,
          offsetX: n.x - wx,
          offsetY: n.y - wy
        };
        n.fx = n.x;
        n.fy = n.y;
        layout.reheat(0.4);
      })
      .on('drag', function (event: D3DragEvent<HTMLCanvasElement, unknown, unknown>) {
        if (!layout) return;
        const state = (this as unknown as Record<string, unknown>).__drag as
          | { node: SimNode; offsetX: number; offsetY: number }
          | undefined;
        if (!state) return;
        const t = renderer!.getTransform();
        const rect = canvasEl!.getBoundingClientRect();
        const [wx, wy] = t.invert([
          event.sourceEvent.clientX - rect.left,
          event.sourceEvent.clientY - rect.top
        ]);
        state.node.fx = wx + state.offsetX;
        state.node.fy = wy + state.offsetY;
      })
      .on('end', function () {
        const state = (this as unknown as Record<string, unknown>).__drag as
          | { node: SimNode }
          | undefined;
        if (!state) return;
        // Release pin so nodes settle naturally. Shift-drag-to-pin could be
        // added later if users want explicit pinning.
        state.node.fx = null;
        state.node.fy = null;
        (this as unknown as Record<string, unknown>).__drag = undefined;
      });
    select(canvasEl).call(dragBehaviour);
  }

  function onPointerMove(e: PointerEvent) {
    if (!renderer) return;
    const n = renderer.pick(e.clientX, e.clientY);
    hoverNode = n;
    if (canvasEl) canvasEl.style.cursor = n ? 'pointer' : 'grab';
  }

  function onPointerLeave() {
    hoverNode = null;
    if (canvasEl) canvasEl.style.cursor = 'grab';
  }

  function onClick(e: MouseEvent) {
    if (!renderer) return;
    const n = renderer.pick(e.clientX, e.clientY);
    if (n) {
      setKeyboardFocus(n.path, { focusCanvas: true });
      onOpenNote(n.path);
      return;
    }
    canvasEl?.focus();
  }

  // ----- search highlight -----
  // Typing in the search box highlights the first matching node (and zooms
  // there on Enter). Doesn't filter — matches Obsidian's behaviour.
  let searchHit = $derived.by<SimNode | null>(() => {
    const q = query.trim().toLowerCase();
    if (!q || !layout) return null;
    return (
      layout.nodes.find(
        (n) => n.path.toLowerCase().includes(q) || (n.title ?? '').toLowerCase().includes(q)
      ) ?? null
    );
  });

  let highlightPath = $derived(searchHit?.path ?? hoverNode?.path ?? focusPath ?? null);
  let graphStats = $derived(
    viewGraph ? { nodes: viewGraph.nodes.length, edges: viewGraph.edges.length } : { nodes: 0, edges: 0 }
  );
  let modeLabel = $derived(mode === 'local' ? 'Local neighborhood' : 'Global graph');
  let focusTone = $derived(hoverNode ? 'hover' : 'focus');

  function typeLabel(t: string): string {
    switch (t) {
      case 'project-note':
        return 'Project note';
      default:
        return t.charAt(0).toUpperCase() + t.slice(1);
    }
  }

  $effect(() => {
    renderer?.setHover(highlightPath);
    scheduleDraw();
  });

  function onSearchKey(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    if (!searchHit) return;
    e.preventDefault();
    setKeyboardFocus(searchHit.path, { reveal: true, focusCanvas: true });
  }

  function onCanvasKey(e: KeyboardEvent) {
    switch (e.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        e.preventDefault();
        focusRelative(1);
        return;
      case 'ArrowLeft':
      case 'ArrowUp':
        e.preventDefault();
        focusRelative(-1);
        return;
      case 'Home':
        e.preventDefault();
        focusBoundary('first');
        return;
      case 'End':
        e.preventDefault();
        focusBoundary('last');
        return;
      case 'Enter':
      case ' ':
        if (!focusPath) return;
        e.preventDefault();
        openFocusedNode();
        return;
      case 'Escape':
        if (!query) return;
        e.preventDefault();
        query = '';
        setKeyboardFocus(selectionPath ?? focusPath, { focusCanvas: true });
        return;
      default:
        return;
    }
  }

  // ----- wire everything up -----
  onMount(() => {
    if (!canvasEl || !hostEl) return;
    renderer = new GraphCanvasRenderer(canvasEl, { colors: colorsFromCss() });
    renderer.setSelection(selectionPath);

    const ro = new ResizeObserver(() => {
      renderer?.resize();
      scheduleDraw();
    });
    ro.observe(hostEl);

    const root = document.documentElement;
    const themeObserver = new MutationObserver((records) => {
      if (records.some((r) => r.attributeName === 'data-theme')) {
        refreshRendererColors();
      }
    });
    themeObserver.observe(root, { attributes: true, attributeFilter: ['data-theme'] });

    const darkModeQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const onSystemThemeChange = () => {
      // Explicit light/dark themes are driven by `data-theme`; only react to
      // OS theme changes while we're in "system" mode (`data-theme` absent).
      if (!root.hasAttribute('data-theme')) refreshRendererColors();
    };
    darkModeQuery.addEventListener('change', onSystemThemeChange);

    // Bind zoom behaviour to the canvas. d3-zoom installs wheel/drag handlers
    // on the selection.
    select(canvasEl).call(zoomBehaviour);

    return () => {
      ro.disconnect();
      themeObserver.disconnect();
      darkModeQuery.removeEventListener('change', onSystemThemeChange);
    };
  });

  $effect(() => {
    // Rebuild the simulation whenever the effective view-graph changes.
    const g = viewGraph;
    untrack(() => {
      if (!renderer || !g) return;
      rebuildLayout();
    });
  });

  $effect(() => {
    if (visibleNodes.length === 0) {
      if (focusPath !== null) focusPath = null;
      return;
    }
    if (focusPath && visibleNodes.some((n) => n.path === focusPath)) return;
    const fallback =
      visibleNodes.find((n) => n.path === selectionPath)?.path ?? visibleNodes[0]?.path ?? null;
    if (focusPath !== fallback) focusPath = fallback;
  });

  $effect(() => {
    renderer?.setSelection(selectionPath);
    scheduleDraw();
  });

  $effect(() => {
    if (!selectionPath) return;
    if (!visibleNodes.some((n) => n.path === selectionPath)) return;
    if (focusPath !== selectionPath) focusPath = selectionPath;
  });

  onDestroy(() => {
    layout?.stop();
    renderer?.destroy();
    renderer = null;
    layout = null;
  });

  let modeHint = $derived(
    mode === 'local'
      ? currentFilePath
        ? `Local — ${localDepth} hop${localDepth === 1 ? '' : 's'} from ${labelFor({
            path: currentFilePath,
            title: null,
            note_type: null,
            in_degree: 0,
            out_degree: 0
          })}`
        : 'Local — no file open'
      : 'Global'
  );
</script>

<div class="graph-root" bind:this={hostEl}>
  <header class="graph-toolbar">
    <div class="toolbar-label">
      <span class="eyebrow">Graph View</span>
      <span class="hint">{modeHint}</span>
    </div>

    <div class="toolbar-main">
      <div class="segmented" role="tablist" aria-label="Graph mode">
        <button
          class:active={mode === 'global'}
          onclick={() => (mode = 'global')}
          role="tab"
          aria-selected={mode === 'global'}>Global</button
        >
        <button
          class:active={mode === 'local'}
          onclick={() => (mode = 'local')}
          role="tab"
          aria-selected={mode === 'local'}
          disabled={!currentFilePath}>Local</button
        >
      </div>

      {#if mode === 'local'}
        <label class="depth">
          Depth
          <input type="range" min="1" max="4" bind:value={localDepth} aria-label="BFS depth" />
          <span>{localDepth}</span>
        </label>
      {/if}

      <input
        class="search"
        type="search"
        placeholder="Search title or path…"
        bind:value={query}
        onkeydown={onSearchKey}
      />
    </div>

    <div class="toolbar-actions">
      <button class="ghost" onclick={resetView} title="Fit graph to view">Fit</button>
      <button class="ghost" onclick={onClose} title="Close graph">Close</button>
    </div>
  </header>

  <div class="graph-layout">
    <aside class="sidebar">
      <section class="overview-card">
        <div class="eyebrow">Overview</div>
        <div class="overview-head">
          <strong>{modeLabel}</strong>
          <span class="overview-meta">{graphStats.nodes} nodes · {graphStats.edges} edges</span>
        </div>
        {#if layoutPreset !== 'small'}
          <div class="perf-hint">
            {layoutPreset === 'large'
              ? 'Large-graph tuning active.'
              : 'Medium-graph tuning active.'}
          </div>
        {/if}
      </section>

      <section>
        <div class="section-head">
          <h3>Filter</h3>
          <label class="check compact">
            <input type="checkbox" bind:checked={hideOrphans} />
            Hide orphans
          </label>
        </div>
        <div class="type-pills">
          {#each allTypes as t}
            <button
              type="button"
              class="type-pill"
              class:is-off={excludedTypes.has(t)}
              onclick={() => toggleTypeExclusion(t)}
              title={excludedTypes.has(t) ? `Show ${typeLabel(t)}` : `Hide ${typeLabel(t)}`}
            >
              <span class="swatch" style:background={colorForType(t)}></span>
              <span class="pill-copy">
                <span class="label">{typeLabel(t)}</span>
                <span class="pill-count">{typeCounts.get(t) ?? 0}</span>
              </span>
            </button>
          {/each}
        </div>
      </section>

      <section>
        <div class="section-head">
          <h3>Focus</h3>
          {#if focusNode}
            <span class="section-meta">{focusIndex + 1} / {visibleNodes.length}</span>
          {/if}
        </div>
        {#if focusNode}
          <div class="focus-card">
            <div class="focus-label">Keyboard focus</div>
            <strong>{labelFor(focusNode)}</strong>
            <div class="focus-meta">
              {focusIndex + 1} / {visibleNodes.length} · {focusNode.note_type ?? 'unknown'}
            </div>
            <div class="path">{focusNode.path}</div>
            <div class="deg">
              in {focusNode.in_degree} · out {focusNode.out_degree}
            </div>
          </div>
          <div class="kbd-actions">
            <button class="ghost small" onclick={() => focusRelative(-1)}>Prev</button>
            <button class="ghost small" onclick={() => focusRelative(1)}>Next</button>
            <button class="ghost small" onclick={() => zoomToPath(focusPath)}>Center</button>
            <button class="ghost small" onclick={openFocusedNode}>Open</button>
          </div>
        {/if}
        {#if inspectedNode}
          <div class="hover">
            <div class="focus-label">{focusTone === 'hover' ? 'Hover' : 'Selected'}</div>
            <strong>{labelFor(inspectedNode)}</strong>
            <div class="path">{inspectedNode.path}</div>
            <div class="deg">
              in {inspectedNode.in_degree} · out {inspectedNode.out_degree}
            </div>
          </div>
        {/if}
      </section>

      <p class="kbd-help">
        Focus the canvas, then use arrow keys to step nodes. Enter opens. Escape clears search.
      </p>
    </aside>

    <div class="canvas-wrap">
      <p id="graph-canvas-help" class="sr-only">
        Use arrow keys to move between visible graph nodes. Press Enter to open the focused note,
        Home or End to jump across the list, and Escape to clear the current search highlight.
      </p>
      <p id="graph-canvas-status" class="sr-only" aria-live="polite">
        {keyboardStatus}
      </p>
      <ul class="sr-only" aria-label="Visible graph nodes">
        {#each visibleNodes as node}
          <li aria-current={node.path === focusPath ? 'true' : undefined}>
            {labelFor(node)}. {node.note_type ?? 'unknown'}. In {node.in_degree}, out {node.out_degree}.
            {node.path}
          </li>
        {/each}
      </ul>
      {#if loading}
        <div class="empty">Loading graph…</div>
      {:else if loadErr}
        <div class="empty error">Failed to load: {loadErr}</div>
      {:else if emptyState}
        <div class="empty">
          <div class="empty-card">
            <strong>{emptyState.title}</strong>
            <p>{emptyState.body}</p>
          </div>
        </div>
      {/if}
      <canvas
        bind:this={canvasEl}
        tabindex="0"
        aria-label="Note graph canvas"
        aria-describedby="graph-canvas-help graph-canvas-status"
        onkeydown={onCanvasKey}
        onpointermove={onPointerMove}
        onpointerleave={onPointerLeave}
        onclick={onClick}
      ></canvas>
    </div>
  </div>
</div>

<style>
  .graph-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--color-bg);
    color: var(--color-fg);
  }

  .graph-toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface);
  }
  .toolbar-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 0 0 auto;
  }
  .eyebrow {
    font-family: var(--font-mono);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-dim);
  }
  .graph-toolbar .hint {
    color: var(--color-fg-muted);
    font-size: 12px;
  }

  .toolbar-main {
    flex: 1;
    display: flex;
    gap: var(--space-3);
    align-items: center;
    min-width: 0;
  }
  .toolbar-actions {
    display: flex;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  .segmented {
    display: inline-flex;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .segmented button {
    padding: 5px 12px;
    font-size: 13px;
    background: transparent;
    color: var(--color-fg-muted);
    border: 0;
    cursor: pointer;
    transition:
      background 0.12s,
      color 0.12s;
  }
  .segmented button.active {
    background: var(--color-accent-weak);
    color: var(--color-accent);
  }
  .segmented button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .depth {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--fs-sm);
    color: var(--color-fg-muted);
  }
  .depth input[type='range'] {
    width: 90px;
  }

  .search {
    flex: 1;
    min-width: 180px;
    max-width: 320px;
    padding: 7px 12px;
    font-size: var(--fs-sm);
    background: var(--color-bg);
    color: var(--color-fg);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .search:focus {
    outline: 2px solid var(--color-accent-weak);
    outline-offset: -1px;
  }

  .ghost {
    padding: 5px 12px;
    font-size: 13px;
    background: transparent;
    color: var(--color-fg-muted);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition:
      background 0.12s,
      color 0.12s;
  }
  .ghost:hover {
    background: var(--color-bg-hover);
    color: var(--color-fg);
  }
  .ghost.small {
    padding: 4px 10px;
    font-size: var(--fs-xs);
  }

  .graph-layout {
    flex: 1;
    display: grid;
    grid-template-columns: 196px 1fr;
    min-height: 0;
  }

  .sidebar {
    border-right: 1px solid var(--color-border);
    padding: var(--space-3);
    overflow-y: auto;
    font-size: var(--fs-sm);
    background: color-mix(in oklch, var(--color-surface) 92%, transparent);
  }
  .sidebar h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-fg-dim);
  }
  .sidebar section {
    margin-bottom: var(--space-4);
  }

  .overview-card {
    padding: var(--space-3);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .overview-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .overview-head strong {
    font-size: 14px;
    color: var(--color-fg);
  }
  .overview-meta {
    font-size: 12px;
    color: var(--color-fg-muted);
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }
  .section-meta {
    font-size: 11px;
    color: var(--color-fg-muted);
    font-family: var(--font-mono);
  }
  .type-pills {
    display: grid;
    gap: 6px;
  }
  .type-pill {
    width: 100%;
    padding: 8px 10px;
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border);
    background: var(--color-surface-raised);
    box-shadow: none;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    color: var(--color-fg);
  }
  .type-pill:hover {
    transform: none;
    background: var(--color-bg-hover);
  }
  .type-pill.is-off {
    opacity: 0.5;
    background: transparent;
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.18);
    flex-shrink: 0;
  }
  .pill-copy {
    min-width: 0;
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 8px;
    width: 100%;
  }
  .label {
    color: var(--color-fg);
    font-size: 12px;
  }
  .pill-count {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--color-fg-muted);
    flex-shrink: 0;
  }

  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    cursor: pointer;
  }
  .check.compact {
    font-size: 12px;
    color: var(--color-fg-muted);
  }

  .kbd-help {
    margin: 0;
    color: var(--color-fg-muted);
    line-height: 1.45;
    font-size: 12px;
  }
  .focus-card {
    padding: var(--space-2) var(--space-3);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .focus-label {
    margin-bottom: 4px;
    color: var(--color-fg-dim);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .focus-card strong {
    color: var(--color-fg);
    font-size: var(--fs-sm);
  }
  .focus-meta {
    margin-top: 2px;
    color: var(--color-fg-muted);
  }
  .focus-card .path {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-dim);
    word-break: break-all;
    margin-top: 4px;
  }
  .focus-card .deg {
    margin-top: 4px;
  }
  .kbd-actions {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .perf-hint {
    margin-top: var(--space-2);
    color: var(--color-fg-muted);
    font-size: 12px;
    line-height: 1.45;
  }
  .hover {
    margin-top: var(--space-3);
    padding: var(--space-2) var(--space-3);
    background: color-mix(in oklch, var(--color-accent) 7%, var(--color-surface-raised));
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }
  .hover strong {
    color: var(--color-fg);
    font-size: var(--fs-sm);
  }
  .hover .path {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--color-fg-dim);
    word-break: break-all;
    margin-top: 2px;
  }
  .hover .deg {
    margin-top: 4px;
  }

  .canvas-wrap {
    position: relative;
    overflow: hidden;
  }
  canvas {
    display: block;
    width: 100%;
    height: 100%;
    cursor: grab;
    outline: none;
  }
  canvas:focus-visible {
    box-shadow: inset 0 0 0 2px var(--color-accent);
  }
  canvas:active {
    cursor: grabbing;
  }
  .empty {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--color-fg-muted);
    font-size: var(--fs-sm);
    pointer-events: none;
  }
  .empty-card {
    max-width: 360px;
    padding: var(--space-4) var(--space-5);
    text-align: center;
    background: color-mix(in srgb, var(--color-surface) 88%, transparent);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-sm);
  }
  .empty-card strong {
    display: block;
    color: var(--color-fg);
    margin-bottom: var(--space-2);
  }
  .empty-card p {
    margin: 0;
    line-height: 1.5;
  }
  .empty.error {
    color: var(--color-danger);
  }

  @media (max-width: 980px) {
    .graph-toolbar {
      flex-wrap: wrap;
      align-items: stretch;
    }
    .toolbar-main {
      order: 3;
      width: 100%;
    }
    .search {
      max-width: none;
    }
    .graph-layout {
      grid-template-columns: 180px 1fr;
    }
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
