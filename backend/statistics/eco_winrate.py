from __future__ import annotations

from collections import Counter, defaultdict
from dataclasses import dataclass

from backend.entities import GameRecord


@dataclass(frozen=True)
class EcoWinrateReport:
    rows: list[dict]
    analyzed_games: int
    skipped_games: int


def build_eco_winrate_report(games: list[GameRecord], username: str) -> EcoWinrateReport:
    """Aggregate completed games from the user's perspective, strictly by ECO."""
    normalized_username = username.strip().casefold()
    if not normalized_username:
        return EcoWinrateReport([], 0, len(games))

    buckets: dict[str, dict] = defaultdict(
        lambda: {"games": 0, "wins": 0, "openings": Counter()}
    )
    analyzed_games = 0
    skipped_games = 0

    for game in games:
        color = _user_color(game, normalized_username)
        if color is None or game.result not in {"1-0", "0-1", "1/2-1/2"}:
            skipped_games += 1
            continue

        eco = (game.eco or "").strip().upper() or "???"
        opening = (game.opening or "").strip() or "Без названия дебюта"
        bucket = buckets[eco]
        bucket["games"] += 1
        bucket["wins"] += int(_is_user_win(game.result, color))
        bucket["openings"][opening] += 1
        analyzed_games += 1

    rows = []
    for eco, bucket in buckets.items():
        games_count = int(bucket["games"])
        winrate_value = int(bucket["wins"]) / games_count if games_count else 0.0
        rows.append(
            {
                "eco": eco,
                "opening": _most_common_opening(bucket["openings"]),
                "games": games_count,
                "winrate": f"{winrate_value * 100:.1f}%",
                "winrate_value": winrate_value,
            }
        )

    rows.sort(
        key=lambda row: (
            float(row["winrate_value"]),
            -int(row["games"]),
            str(row["eco"]),
        )
    )
    return EcoWinrateReport(rows, analyzed_games, skipped_games)


def _user_color(game: GameRecord, normalized_username: str) -> str | None:
    if game.white.strip().casefold() == normalized_username:
        return "white"
    if game.black.strip().casefold() == normalized_username:
        return "black"
    return None


def _is_user_win(result: str, color: str) -> bool:
    return (result == "1-0" and color == "white") or (result == "0-1" and color == "black")


def _most_common_opening(openings: Counter) -> str:
    if not openings:
        return "Без названия дебюта"
    return sorted(openings.items(), key=lambda item: (-item[1], item[0].casefold()))[0][0]
