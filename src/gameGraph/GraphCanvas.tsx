import { useEffect, useLayoutEffect, useRef } from "react";
import cytoscape, { type Core, type ElementDefinition } from "cytoscape";
import type { GraphNode, GraphEdge } from "./api";
export function GraphCanvas({
  nodes,
  edges,
  selected,
  onNode,
  onEdge,
  onViewport,
  initialViewport,
  focusToken,
  fitToken,
  pathEdges,
}: {
  nodes: GraphNode[];
  edges: GraphEdge[];
  selected: string;
  onNode: (id: string) => void;
  onEdge: (id: string) => void;
  onViewport: (value: { zoom: number; pan: { x: number; y: number } }) => void;
  initialViewport?: { zoom: number; pan: { x: number; y: number } };
  focusToken: number;
  fitToken: number;
  pathEdges?: string[];
}) {
  const container = useRef<HTMLDivElement>(null),
    cy = useRef<Core | null>(null),
    positions = useRef<Record<string, { x: number; y: number }>>({}),
    worker = useRef<Worker | null>(null);
  const callbacks = useRef({ onNode, onEdge, onViewport });
  callbacks.current = { onNode, onEdge, onViewport };
  const selectedRef = useRef(selected);
  selectedRef.current = selected;
  const layoutSequence = useRef(0),
    layoutQueue = useRef<number[]>([]),
    layoutPending = useRef(false),
    pendingAppear = useRef(new Set<string>()),
    cameraReady = useRef(false),
    cameraAction = useRef<"fit" | "focus" | null>(null),
    applyCamera = useRef<() => void>(() => {}),
    lastSelected = useRef(""),
    lastFocusToken = useRef(focusToken),
    lastFitToken = useRef(fitToken);
  useLayoutEffect(() => {
    // React StrictMode replays effects in development; a terminated worker's
    // request queue must not consume the replacement worker's first reply.
    layoutQueue.current = [];
    layoutPending.current = false;
    pendingAppear.current.clear();
    cameraReady.current = false;
    cameraAction.current = null;
    lastSelected.current = "";
    lastFocusToken.current = focusToken;
    lastFitToken.current = fitToken;
    const graph = cytoscape({
      container: container.current!,
      elements: [],
      layout: { name: "preset" },
      minZoom: 0.15,
      maxZoom: 2.5,
      wheelSensitivity: 0.25,
      boxSelectionEnabled: false,
      autoungrabify: true,
    });
    cy.current = graph;
    graph.on("tap", "node", (e) => callbacks.current.onNode(e.target.id()));
    graph.on("tap", "edge", (e) => callbacks.current.onEdge(e.target.id()));
    graph.on("viewport", () =>
      callbacks.current.onViewport({ zoom: graph.zoom(), pan: graph.pan() }),
    );
    const restored =
      initialViewport &&
      Number.isFinite(initialViewport.zoom) &&
      initialViewport.zoom > 0 &&
      Number.isFinite(initialViewport.pan.x) &&
      Number.isFinite(initialViewport.pan.y);
    if (restored) {
      graph.viewport(initialViewport);
      cameraReady.current = true;
    }
    applyCamera.current = () => {
      if (layoutPending.current || !graph.nodes().length) return;
      const action =
        cameraAction.current ?? (cameraReady.current ? null : "fit");
      if (!action) return;
      if (action === "fit") {
        graph.stop(true, false);
        graph.fit(undefined, 32);
      } else {
        const target = graph.getElementById(selectedRef.current);
        if (!target.length) return;
        graph.stop(true, false);
        const zoom = Math.max(0.9, graph.zoom());
        const position = target.position();
        const pan = {
          x: graph.width() / 2 - position.x * zoom,
          y: graph.height() / 2 - position.y * zoom,
        };
        if (window.matchMedia("(prefers-reduced-motion: reduce)").matches)
          graph.viewport({ zoom, pan });
        else graph.animate({ zoom, pan, duration: 240 });
      }
      cameraReady.current = true;
      cameraAction.current = null;
    };
    const resize = new ResizeObserver(() => graph.resize());
    resize.observe(container.current!);
    const recolor = () => {
      const css = getComputedStyle(document.documentElement),
        text = css.getPropertyValue("--text").trim(),
        surface = css.getPropertyValue("--surface").trim(),
        border = css.getPropertyValue("--border").trim(),
        accent = css.getPropertyValue("--accent").trim(),
        success = css.getPropertyValue("--success").trim(),
        warning = css.getPropertyValue("--warning").trim();
      graph
        .style()
        .fromJson([
          {
            selector: "node",
            style: {
              label: "data(label)",
              "text-wrap": "wrap",
              "font-size": 12,
              "text-valign": "center",
              "text-halign": "center",
              width: 142,
              height: 62,
              shape: "round-rectangle",
              "background-color": surface,
              "border-color": border,
              "border-width": 1,
              color: text,
            },
          },
          {
            selector: "edge",
            style: {
              label: "data(label)",
              width: "data(width)",
              "curve-style": "bezier",
              "target-arrow-shape": "triangle",
              "line-color": border,
              "target-arrow-color": border,
              "font-size": 11,
              color: text,
              "text-background-color": surface,
              "text-background-opacity": 0.85,
              "text-background-padding": "3px",
              "loop-direction": "-45deg",
            },
          },
          {
            selector: ".prepared",
            style: {
              "line-color": success,
              "target-arrow-color": success,
              "line-style": "dashed",
            },
          },
          {
            selector: ".both",
            style: { "line-color": success, "target-arrow-color": success },
          },
          {
            selector: ".deviation",
            style: { "line-color": warning, "target-arrow-color": warning },
          },
          {
            selector: ".selected",
            style: { "border-width": 3, "border-color": accent, "z-index": 10 },
          },
          {
            selector: ".route",
            style: { "line-color": accent, "target-arrow-color": accent },
          },
        ] as any)
        .update();
    };
    recolor();
    const theme = new MutationObserver(recolor);
    theme.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    worker.current = new Worker(
      new URL("./layout.worker.ts", import.meta.url),
      { type: "module" },
    );
    worker.current.onmessage = (e) => {
      // Worker replies are ordered; discard a superseded projection's layout.
      if (layoutQueue.current.shift() !== layoutSequence.current) return;
      layoutPending.current = false;
      positions.current = e.data.positions;
      graph.batch(() =>
        graph.nodes().forEach((n) => {
          const p = positions.current[n.id()];
          if (p) n.position(p);
        }),
      );
      const reduced = window.matchMedia(
        "(prefers-reduced-motion: reduce)",
      ).matches;
      for (const id of pendingAppear.current) {
        const element = graph.getElementById(id);
        if (!element.length) continue;
        if (reduced) element.style("opacity", 1);
        else
          element.animate(
            { style: { opacity: 1 } },
            { duration: 180, queue: false },
          );
      }
      pendingAppear.current.clear();
      applyCamera.current();
    };
    return () => {
      if (worker.current) worker.current.onmessage = null;
      worker.current?.terminate();
      resize.disconnect();
      theme.disconnect();
      graph.destroy();
      cy.current = null;
      applyCamera.current = () => {};
    };
  }, []);
  useEffect(() => {
    const graph = cy.current;
    if (!graph) return;
    const elements: ElementDefinition[] = [
      ...nodes.map((n) => ({
        data: {
          id: n.id,
          label: `${n.id === selected ? "Текущая позиция" : "Позиция"}\n${n.games} партий${n.hidden_continuations ? ` · +${n.hidden_continuations}` : ""}`,
        },
        position: positions.current[n.id],
      })),
      ...edges.map((e) => ({
        data: {
          id: e.id,
          source: e.from,
          target: e.to,
          label: `${e.san}${e.visits ? ` · ${e.frequency.toFixed(0)}%` : ""}`,
          width: Math.min(7, 1 + Math.sqrt(e.visits)),
        },
        classes: e.prepared.length
          ? e.visits
            ? "both"
            : "prepared"
          : e.coverage === "own_deviation"
            ? "deviation"
            : "",
      })),
    ];
    const ids = new Set(elements.map((e) => e.data.id));
    graph.batch(() => {
      graph
        .elements()
        .filter((e) => !ids.has(e.id()))
        .remove();
      for (const e of elements) {
        const old = graph.getElementById(e.data.id!);
        if (old.length) {
          old.data(e.data);
          old.classes((e.classes as string) ?? "");
        } else {
          graph.add(e).style("opacity", 0);
          pendingAppear.current.add(e.data.id!);
        }
      }
      graph.nodes().removeClass("selected");
      graph.getElementById(selected).addClass("selected");
    });
    layoutPending.current = true;
    layoutQueue.current.push(++layoutSequence.current);
    worker.current?.postMessage({
      nodes,
      edges,
      root: selected,
      previous: positions.current,
    });
  }, [nodes, edges, selected]);
  useEffect(() => {
    const changed = !!lastSelected.current && selected !== lastSelected.current;
    if (selected) lastSelected.current = selected;
    if (changed || focusToken !== lastFocusToken.current)
      cameraAction.current = "focus";
    lastFocusToken.current = focusToken;
    applyCamera.current();
  }, [focusToken, selected]);
  useEffect(() => {
    const graph = cy.current;
    if (!graph) return;
    graph.edges().removeClass("route");
    for (const id of pathEdges ?? [])
      graph.getElementById(id).addClass("route");
  }, [pathEdges, edges]);
  useEffect(() => {
    if (fitToken !== lastFitToken.current) {
      cameraAction.current = "fit";
      lastFitToken.current = fitToken;
      applyCamera.current();
    }
  }, [fitToken]);
  return (
    <div
      className="game-graph-canvas"
      ref={container}
      role="img"
      aria-label="Граф позиций и ходов"
      tabIndex={0}
    />
  );
}
