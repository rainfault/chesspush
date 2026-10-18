import type { DecisionEvent, GameSummary } from "./api";

export interface ReviewCase {
  event: DecisionEvent;
  game: GameSummary | undefined;
  count: number;
  key: string;
}

/** Orders existing confirmed evidence for review. This is not a skill score.
 * The full pre-move FEN is only a conservative grouping label, never a new
 * position identity. Source game / ply / run remain the navigation address.
 */
export function reviewCases(
  events: DecisionEvent[],
  games: GameSummary[],
): ReviewCase[] {
  const byGame = new Map(games.map((game) => [game.id, game]));
  const seen = new Set<string>();
  const groups = new Map<string, ReviewCase>();
  const reviewKinds = new Set([
    "significant",
    "significant_loss",
    "opportunity_missed",
    "missed_opportunity",
    "advantage_lost",
    "mate_lost",
  ]);
  for (const event of events) {
    const game = byGame.get(event.game_id);
    if (
      !game ||
      event.actor !== game.own_color ||
      event.loss_q == null ||
      event.kind === "unstable"
    )
      continue;
    if (!reviewKinds.has(event.kind) || event.loss_q <= 0) continue;
    const address = `${event.game_id}:${event.ply}:${event.run_id}`;
    if (seen.has(address)) continue;
    seen.add(address);
    const key = `${event.fen}|${event.san}|${event.profile_id}|${event.method}`;
    const group = groups.get(key);
    if (group) {
      group.count++;
      if ((event.loss_q ?? 0) > (group.event.loss_q ?? 0)) {
        group.event = event;
        group.game = game;
      }
    } else groups.set(key, { event, game, count: 1, key });
  }
  return [...groups.values()].sort(
    (a, b) =>
      b.count - a.count ||
      (b.event.loss_q ?? 0) - (a.event.loss_q ?? 0) ||
      a.key.localeCompare(b.key),
  );
}
