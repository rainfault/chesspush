import type { GraphEdge, GraphPreview } from "./api";

export const GRAPH_NODE_WIDTH = 160;
export const GRAPH_NODE_HEIGHT = 118;
export const GRAPH_REPLY_HEIGHT = 100;
export const GRAPH_COLUMN = 204;
export const GRAPH_PADDING = 22;

/** Layout is tied to paths, never to PositionKey: equal positions reached by
 * different prefixes remain separate visible occurrences. */
export function layoutBranches(edges: GraphEdge[], previews: GraphPreview[]) {
  let y = GRAPH_PADDING;
  const rows = edges.map((edge) => {
    const preview = previews.find((p) => p.uci === edge.uci);
    const replies = (preview?.replies ?? []).slice(0, 2);
    const height = Math.max(
      GRAPH_NODE_HEIGHT,
      replies.length * GRAPH_REPLY_HEIGHT +
        Math.max(0, replies.length - 1) * 14,
    );
    const row = {
      edge,
      y: y + (height - GRAPH_NODE_HEIGHT) / 2,
      center: y + height / 2,
      replies: replies.map((reply, index) => ({
        edge: reply,
        y: y + index * (GRAPH_REPLY_HEIGHT + 14),
      })),
      hidden: preview?.hidden ?? 0,
    };
    y += height + 24;
    return row;
  });
  const height = Math.max(260, y);
  // Keep the selected position visible while revealing additional branches.
  // A large cohort must not push its parent below the scroll viewport.
  const center = rows.length
    ? (rows[0].center + rows.at(-1)!.center) / 2
    : height / 2;
  return { rows, height, rootY: Math.min(180, center - GRAPH_NODE_HEIGHT / 2) };
}
