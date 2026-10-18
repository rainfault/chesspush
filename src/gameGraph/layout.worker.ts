// Deterministic bounded layout in a worker. Existing coordinates survive refresh and expansion.
self.onmessage = (event: MessageEvent) => {
  const { nodes, edges, root, previous } = event.data as {
    nodes: { id: string; visits: number }[];
    edges: { from: string; to: string }[];
    root: string;
    previous: Record<string, { x: number; y: number }>;
  };
  const depths = new Map<string, number>([[root, 0]]);
  const adjacent = new Map<string, string[]>();
  for (const e of edges) {
    const list = adjacent.get(e.from) ?? [];
    list.push(e.to);
    adjacent.set(e.from, list);
  }
  const queue = [root];
  for (let i = 0; i < queue.length; i++)
    for (const to of adjacent.get(queue[i]) ?? [])
      if (!depths.has(to)) {
        depths.set(to, (depths.get(queue[i]) ?? 0) + 1);
        queue.push(to);
      }
  const keep = new Set([
    ...Object.keys(previous).slice(-2000),
    ...nodes.map((n) => n.id),
  ]);
  const positions = Object.fromEntries(
    Object.entries(previous).filter(([id]) => keep.has(id)),
  );
  const used = new Set(Object.values(positions).map((p) => `${p.x},${p.y}`));
  for (const n of [...nodes].sort(
    (a, b) => b.visits - a.visits || a.id.localeCompare(b.id),
  )) {
    if (positions[n.id]) continue;
    const x = (depths.get(n.id) ?? 0) * 190;
    let y = 0;
    while (used.has(`${x},${y}`)) y += 94;
    positions[n.id] = { x, y };
    used.add(`${x},${y}`);
  }
  self.postMessage({ positions });
};
export {};
