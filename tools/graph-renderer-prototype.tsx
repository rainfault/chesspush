// Reproducible synthetic renderer comparison, never imported by the product.
import React from "react";
import { createRoot } from "react-dom/client";
import { ReactFlow } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import cytoscape from "cytoscape";
import Sigma from "sigma";
import Graph from "graphology";
import ELK from "elkjs/lib/elk.bundled.js";
const stage = document.getElementById("stage")!;
const frame = () =>
  new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
async function frames(n = 3) {
  for (let i = 0; i < n; i++) await frame();
}
function data(n: number) {
  const nodes = Array.from({ length: n }, (_, i) => ({
    id: `n${i}`,
    position: { x: (i % 20) * 120, y: Math.floor(i / 20) * 80 },
    data: { label: `N${i}` },
  }));
  const edges = Array.from({ length: n - 1 }, (_, i) => ({
    id: `e${i}`,
    source: `n${Math.floor(i / 2)}`,
    target: `n${i + 1}`,
  }));
  for (let i = 5; i < n; i += 17)
    edges.push({ id: `t${i}`, source: `n${i}`, target: `n${i - 5}` });
  return { nodes, edges };
}
(window as any).runRendererComparison = async (n: number) => {
  const { nodes, edges } = data(n);
  const results: any[] = [];
  for (const name of ["Cytoscape", "React Flow", "Sigma"]) {
    stage.replaceChildren();
    const start = performance.now();
    let dispose = () => {},
      pan = () => {};
    if (name === "Cytoscape") {
      const cy = cytoscape({
        container: stage,
        elements: [
          ...nodes.map((x) => ({
            data: { id: x.id, label: x.data.label },
            position: x.position,
          })),
          ...edges.map((x) => ({ data: { ...x } })),
        ],
        layout: { name: "preset" },
        style: [
          {
            selector: "node",
            style: { label: "data(label)", width: 35, height: 35 },
          },
          {
            selector: "edge",
            style: { width: 1, "target-arrow-shape": "triangle" },
          },
        ],
      });
      dispose = () => cy.destroy();
      pan = () => cy.panBy({ x: 1, y: 1 });
    } else if (name === "React Flow") {
      const root = createRoot(stage);
      root.render(
        <ReactFlow
          nodes={nodes}
          edges={edges}
          fitView
          nodesDraggable={false}
          onInit={(flow) => {
            pan = () => {
              const v = flow.getViewport();
              void flow.setViewport({ ...v, x: v.x + 1 });
            };
          }}
        />,
      );
      dispose = () => root.unmount();
    } else {
      const graph = new Graph({ type: "directed", multi: true });
      nodes.forEach((x) =>
        graph.addNode(x.id, {
          x: x.position.x,
          y: x.position.y,
          size: 4,
          label: x.data.label,
          color: "#3692e7",
        }),
      );
      edges.forEach((x) =>
        graph.addDirectedEdgeWithKey(x.id, x.source, x.target),
      );
      const renderer = new Sigma(graph, stage, { renderEdgeLabels: false });
      dispose = () => renderer.kill();
      pan = () => {
        const c = renderer.getCamera();
        c.setState({ x: c.x + 0.001 });
      };
    }
    await frames(3);
    const ready = performance.now() - start;
    const durations: number[] = [];
    for (let i = 0; i < 20; i++) {
      const t = performance.now();
      pan();
      await frame();
      durations.push(performance.now() - t);
    }
    results.push({
      renderer: name,
      nodes: n,
      edges: edges.length,
      ready_ms: ready,
      pan_frame_p50_ms: durations.sort((a, b) => a - b)[10],
      pan_frame_max_ms: Math.max(...durations),
    });
    dispose();
    await frames(2);
  }
  const elk = new ELK();
  const start = performance.now();
  await elk.layout({
    id: "root",
    layoutOptions: { "elk.algorithm": "layered" },
    children: nodes.map((x) => ({ id: x.id, width: 100, height: 50 })),
    edges: edges.map((x) => ({
      id: x.id,
      sources: [x.source],
      targets: [x.target],
    })),
  });
  results.push({
    renderer: "ELK layout (no renderer)",
    nodes: n,
    edges: edges.length,
    layout_ms: performance.now() - start,
  });
  return results;
};
