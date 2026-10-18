export interface Route {
  initialFen: string;
  line: string[];
  positionKey?: string | null;
  reference?: { game_id: string; revision: number; ply: number };
}
export interface History {
  entries: Route[];
  cursor: number;
}
export function pushRoute(history: History, route: Route): History {
  const current = history.entries[history.cursor];
  if (
    current?.initialFen === route.initialFen &&
    current.positionKey === route.positionKey &&
    current.line.join(" ") === route.line.join(" ") &&
    JSON.stringify(current.reference ?? null) ===
      JSON.stringify(route.reference ?? null)
  )
    return history;
  const entries = [
    ...history.entries.slice(0, history.cursor + 1),
    route,
  ].slice(-200);
  return { entries, cursor: entries.length - 1 };
}
export function restoreHistory(value: unknown, initialFen: string): History {
  if (
    value &&
    typeof value === "object" &&
    "entries" in value &&
    Array.isArray(value.entries)
  ) {
    const entries = value.entries
      .filter(
        (x): x is Route =>
          !!x &&
          typeof x.initialFen === "string" &&
          x.initialFen.length < 200 &&
          Array.isArray(x.line) &&
          x.line.length <= 1000 &&
          x.line.every(
            (m: unknown) =>
              typeof m === "string" && /^[a-h][1-8][a-h][1-8][qrbn]?$/.test(m),
          ),
      )
      .slice(-200);
    if (entries.length)
      return {
        entries,
        cursor: Math.min(
          entries.length - 1,
          Math.max(0, Math.floor(Number((value as History).cursor)) || 0),
        ),
      };
  }
  return { entries: [{ initialFen, line: [] }], cursor: 0 };
}

/** Restore the next half-move selected on this exact prefix, not an arbitrary
 * incoming edge of a transposed position or a new guess at a fork. */
export function chosenContinuation(history: History): Route | undefined {
  const current = history.entries[history.cursor];
  const candidates = [
    ...history.entries.slice(history.cursor + 1),
    ...history.entries.slice(0, history.cursor).reverse(),
  ];
  const chosen = candidates.find(
    (route) =>
      route.initialFen === current.initialFen &&
      route.line.length > current.line.length &&
      route.line.slice(0, current.line.length).join(" ") ===
        current.line.join(" "),
  );
  if (!chosen) return undefined;
  return {
    initialFen: current.initialFen,
    line: chosen.line.slice(0, current.line.length + 1),
  };
}
