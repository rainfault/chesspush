from __future__ import annotations


def calc_pain_index(game_count: int, score: float, total_games: int | None = None) -> float:
    """Practical diagnostic index: frequent and underperforming themes go up.

    score is expected in [0, 1]. Values above 50% are not considered painful.
    """
    if game_count <= 0:
        return 0.0
    underperformance = max(0.0, 0.5 - score)
    return round(game_count * underperformance, 3)


def priority_from_pain(value: float) -> str:
    if value >= 5:
        return "срочно"
    if value >= 2:
        return "высокий"
    if value > 0:
        return "средний"
    return "норма"
