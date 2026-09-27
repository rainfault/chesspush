from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass

from backend.entities import GameRecord, result_score
from backend.statistics.pain_index import calc_pain_index, priority_from_pain


@dataclass
class OpeningStat:
    key: str
    color: str
    games: int
    score_sum: float
    avg_opponent: float
    pain_index: float

    @property
    def score(self) -> float:
        return self.score_sum / self.games if self.games else 0.0

    def to_dict(self) -> dict:
        return {
            "opening": self.key,
            "color": self.color,
            "games": self.games,
            "score": f"{self.score * 100:.1f}%",
            "avg_opponent": int(self.avg_opponent),
            "pain_index": self.pain_index,
            "priority": priority_from_pain(self.pain_index),
        }


def opening_statistics(games: list[GameRecord], username: str = "") -> list[OpeningStat]:
    buckets: dict[tuple[str, str], dict] = defaultdict(lambda: {"games": 0, "score": 0.0, "opp": 0})
    total = len(games)
    for game in games:
        if username:
            color = game.color_for_username(username)
            if color == "unknown":
                continue
            score = game.score_for_username(username)
            opponent = game.black_elo if color == "white" else game.white_elo
        else:
            color = "white"
            score = result_score(game.result, "white")
            opponent = game.black_elo
        if score is None:
            continue
        key = f"{game.eco or '???'} · {game.opening or 'Unknown opening'}"
        bucket = buckets[(key, color)]
        bucket["games"] += 1
        bucket["score"] += score
        bucket["opp"] += opponent
    stats: list[OpeningStat] = []
    for (key, color), data in buckets.items():
        games_count = data["games"]
        score = data["score"] / games_count if games_count else 0.0
        stats.append(
            OpeningStat(
                key=key,
                color=color,
                games=games_count,
                score_sum=data["score"],
                avg_opponent=data["opp"] / games_count if games_count else 0,
                pain_index=calc_pain_index(games_count, score, total_games=total),
            )
        )
    stats.sort(key=lambda x: (x.pain_index, x.games), reverse=True)
    return stats
