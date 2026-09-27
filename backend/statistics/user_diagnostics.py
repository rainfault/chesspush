from __future__ import annotations

from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime

from backend.entities import GameRecord
from backend.statistics.pain_index import calc_pain_index, priority_from_pain


def user_score(game: GameRecord, username: str) -> float | None:
    return game.score_for_username(username)


def user_color(game: GameRecord, username: str) -> str:
    return game.color_for_username(username)


def opponent_elo(game: GameRecord, color: str) -> int:
    return game.black_elo if color == "white" else game.white_elo


def parse_game_date(value: str) -> datetime | None:
    value = (value or "").strip()
    if not value:
        return None
    if value.isdigit():
        try:
            timestamp = int(value)
            if timestamp > 10_000_000_000:
                timestamp //= 1000
            return datetime.fromtimestamp(timestamp)
        except (OSError, ValueError):
            return None
    for fmt in ("%Y.%m.%d", "%Y-%m-%d"):
        try:
            return datetime.strptime(value.replace("??", "01"), fmt)
        except ValueError:
            pass
    return None


@dataclass
class UserDiagnostics:
    summary_cards: list[dict]
    pain_rows: list[dict]
    opening_rows: list[dict]
    color_rows: list[dict]
    perf_rows: list[dict]
    status: str
    analyzed_games: int
    skipped_games: int


def empty_diagnostics(status: str) -> UserDiagnostics:
    return UserDiagnostics([], [], [], [], [], status, 0, 0)


def build_user_diagnostics(
    games: list[GameRecord],
    *,
    username: str,
    period_count: int = 0,
    min_games: int = 3,
) -> UserDiagnostics:
    username = username.strip()
    if not username:
        return empty_diagnostics("Укажи lichess username для расчёта статистики.")
    if not games:
        return empty_diagnostics("Партии не загружены. Импортируй PGN/NDJSON или обнови партии с Lichess.")

    if period_count > 0:
        games = games[:period_count]

    user_games: list[tuple[GameRecord, str, float]] = []
    skipped = 0
    for game in games:
        color = user_color(game, username)
        score = user_score(game, username)
        if color not in {"white", "black"} or score is None:
            skipped += 1
            continue
        user_games.append((game, color, score))

    if not user_games:
        return empty_diagnostics(f"В загруженных партиях не найден пользователь {username}.")

    total = len(user_games)
    wins = sum(1 for _, _, score in user_games if score == 1.0)
    draws = sum(1 for _, _, score in user_games if score == 0.5)
    losses = sum(1 for _, _, score in user_games if score == 0.0)
    score_sum = sum(score for _, _, score in user_games)

    color_stats = _aggregate_named(user_games, lambda game, color, score: color, total)
    perf_stats = _aggregate_named(user_games, lambda game, color, score: game.perf_type or "unknown", total)
    opening_stats = _aggregate_openings(user_games, total)
    pain_stats = _aggregate_pain_directions(user_games, total)

    pain_rows = []
    for row in pain_stats:
        if int(row["games"]) < min_games:
            continue
        if float(row["score_value"]) >= 0.5:
            continue
        pain = float(row["pain_index"])
        pain_rows.append(
            {
                **row,
                "recommendation": _recommendation(row),
                "priority": priority_from_pain(pain),
            }
        )
    pain_rows.sort(key=lambda row: (float(row["pain_index"]), int(row["games"])), reverse=True)

    summary_cards = [
        _card("Total games", str(total), f"{wins}W / {draws}D / {losses}L"),
        _card("Overall score", _percent(score_sum / total), f"skipped: {skipped}"),
        _card("White score", _score_for_named(color_stats, "white"), "as White"),
        _card("Black score", _score_for_named(color_stats, "black"), "as Black"),
        _card("Rapid score", _score_for_named(perf_stats, "rapid"), "rapid games"),
        _card("Blitz score", _score_for_named(perf_stats, "blitz"), "blitz games"),
    ]

    if pain_rows:
        status = f"Главные зоны тренировки: {len(pain_rows)} направлений. Начни с первых 3-5 строк Pain Map."
    else:
        status = f"Pain Map пуста: нет направлений с минимум {min_games} партиями и score ниже 50%."

    return UserDiagnostics(
        summary_cards=summary_cards,
        pain_rows=pain_rows[:50],
        opening_rows=opening_stats,
        color_rows=color_stats,
        perf_rows=perf_stats,
        status=status,
        analyzed_games=total,
        skipped_games=skipped,
    )


def _aggregate_openings(user_games: list[tuple[GameRecord, str, float]], total_games: int) -> list[dict]:
    buckets: dict[tuple[str, str, str], dict] = defaultdict(_bucket)
    for game, color, score in user_games:
        eco = (game.eco or "???").strip() or "???"
        opening = (game.opening or "Без названия дебюта").strip() or "Без названия дебюта"
        bucket = buckets[(eco, opening, color)]
        _add_game(bucket, game, color, score)

    rows = []
    for (eco, opening, color), bucket in buckets.items():
        row = _row_from_bucket(bucket, total_games)
        row.update(
            {
                "eco": eco,
                "opening": opening,
                "color": "белые" if color == "white" else "чёрные",
                "color_key": color,
                "recommendation": _recommendation({"eco": eco, "opening": opening, "color": "белые" if color == "white" else "чёрные"}),
            }
        )
        rows.append(row)
    rows.sort(key=lambda row: (float(row["pain_index"]), int(row["games"])), reverse=True)
    return rows


def _aggregate_pain_directions(user_games: list[tuple[GameRecord, str, float]], total_games: int) -> list[dict]:
    buckets: dict[tuple[str, str], dict] = defaultdict(_bucket)
    ecos: dict[tuple[str, str], Counter] = defaultdict(Counter)
    examples: dict[tuple[str, str], Counter] = defaultdict(Counter)

    for game, color, score in user_games:
        family = _opening_family(game.opening)
        key = (family, color)
        _add_game(buckets[key], game, color, score)
        eco = (game.eco or "???").strip() or "???"
        ecos[key][eco] += 1
        if game.opening:
            examples[key][game.opening] += 1

    rows = []
    for (family, color), bucket in buckets.items():
        row = _row_from_bucket(bucket, total_games)
        row.update(
            {
                "eco": _eco_summary(ecos[(family, color)]),
                "opening": family,
                "color": "белые" if color == "white" else "чёрные",
                "color_key": color,
                "recommendation": _recommendation(
                    {
                        "eco": _eco_summary(ecos[(family, color)]),
                        "opening": family,
                        "color": "белые" if color == "white" else "чёрные",
                    }
                ),
                "examples": ", ".join(name for name, _ in examples[(family, color)].most_common(3)),
            }
        )
        rows.append(row)

    rows.sort(key=lambda row: (float(row["pain_index"]), int(row["games"])), reverse=True)
    return rows


def _aggregate_named(user_games: list[tuple[GameRecord, str, float]], key_fn, total_games: int) -> list[dict]:
    buckets: dict[str, dict] = defaultdict(_bucket)
    for game, color, score in user_games:
        key = key_fn(game, color, score) or "unknown"
        _add_game(buckets[str(key)], game, color, score)
    rows = []
    for key, bucket in buckets.items():
        row = _row_from_bucket(bucket, total_games)
        row.update({"name": _display_group_name(key), "key": key})
        rows.append(row)
    rows.sort(key=lambda row: (float(row["pain_index"]), int(row["games"])), reverse=True)
    return rows


def _bucket() -> dict:
    return {"games": 0, "wins": 0, "draws": 0, "losses": 0, "score": 0.0, "opp": 0, "moves": 0}


def _add_game(bucket: dict, game: GameRecord, color: str, score: float) -> None:
    bucket["games"] += 1
    bucket["score"] += score
    bucket["opp"] += opponent_elo(game, color)
    bucket["moves"] += game.move_count
    if score == 1.0:
        bucket["wins"] += 1
    elif score == 0.5:
        bucket["draws"] += 1
    else:
        bucket["losses"] += 1


def _row_from_bucket(bucket: dict, total_games: int) -> dict:
    games = int(bucket["games"])
    score_value = float(bucket["score"]) / games if games else 0.0
    pain = calc_pain_index(games, score_value, total_games=total_games)
    return {
        "games": games,
        "wins": int(bucket["wins"]),
        "draws": int(bucket["draws"]),
        "losses": int(bucket["losses"]),
        "score": _percent(score_value),
        "score_value": round(score_value, 4),
        "avg_opponent": int(bucket["opp"] / games) if games else 0,
        "avg_moves": round(bucket["moves"] / games, 1) if games else 0,
        "pain_index": pain,
        "priority": priority_from_pain(pain),
    }


def _recommendation(row: dict) -> str:
    eco = row.get("eco", "")
    opening = row.get("opening", "это направление")
    color = row.get("color", "")
    return f"Открой выгрузку модельных партий и подбери референсы: {eco} {opening}, цвет: {color}."


def _opening_family(opening: str) -> str:
    opening = (opening or "").strip()
    if not opening:
        return "Без названия дебюта"
    return opening.split(":", 1)[0].strip() or opening


def _eco_summary(counter: Counter) -> str:
    ecos = sorted(eco for eco in counter if eco and eco != "???")
    if not ecos:
        return "???"
    if len(ecos) == 1:
        return ecos[0]
    return f"{ecos[0]}-{ecos[-1]}"


def _display_group_name(key: str) -> str:
    if key == "white":
        return "Белыми"
    if key == "black":
        return "Чёрными"
    return key or "unknown"


def _score_for_named(rows: list[dict], key: str) -> str:
    for row in rows:
        if row.get("key") == key:
            return str(row.get("score", "0.0%"))
    return "0.0%"


def _percent(value: float) -> str:
    return f"{value * 100:.1f}%"


def _card(title: str, value: str, hint: str = "") -> dict:
    return {"title": title, "value": value, "hint": hint}
