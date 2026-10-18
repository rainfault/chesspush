import type { SquareName, Role } from "chessops/types";
import type { BoardSession } from "./BoardSession";
export function moveDestinations(
  moves: string[],
): Map<SquareName, SquareName[]> {
  const result = new Map<SquareName, SquareName[]>();
  for (const move of moves) {
    const from = move.slice(0, 2) as SquareName,
      to = move.slice(2, 4) as SquareName;
    const list = result.get(from) ?? [];
    if (!list.includes(to)) list.push(to);
    result.set(from, list);
  }
  return result;
}
export function uciInput(
  session: BoardSession,
  from: SquareName,
  to: SquareName,
  promotion?: Role,
): string {
  const piece = session.position.board.get(
    from.charCodeAt(0) - 97 + (Number(from[1]) - 1) * 8,
  );
  const target = session.position.board.get(
    to.charCodeAt(0) - 97 + (Number(to[1]) - 1) * 8,
  );
  if (
    piece?.role === "king" &&
    target?.role === "rook" &&
    target.color === piece.color
  )
    to = `${to[0] > from[0] ? "g" : "c"}${from[1]}` as SquareName;
  const promotions: Partial<Record<Role, string>> = {
    queen: "q",
    rook: "r",
    bishop: "b",
    knight: "n",
  };
  return from + to + (promotion ? (promotions[promotion] ?? "") : "");
}
