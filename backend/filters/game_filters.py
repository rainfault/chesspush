from __future__ import annotations

from dataclasses import dataclass
import re

from backend.entities import GameRecord, detect_perf_type, result_score


@dataclass
class GameFilter:
    color: str = "any"  # any / white / black
    result_mode: str = "all"  # all / selected_wins / selected_losses / wins_draws / no_quick_losses
    min_white_elo: int = 0
    max_white_elo: int = 4000
    min_black_elo: int = 0
    max_black_elo: int = 4000
    min_both_elo: int = 0
    max_rating_diff: int = 4000
    eco: str = ""
    opening_contains: str = ""
    move_prefix: str = ""
    perf_type: str = "any"
    min_moves: int = 0
    exclude_bullet: bool = True
    include_timeout: bool = False
    username: str = ""


def _contains(haystack: str, needle: str) -> bool:
    return not needle or needle.lower() in haystack.lower()


def _to_int(value: str | None) -> int:
    try:
        return int(value or 0)
    except ValueError:
        return 0


def _eco_matches(game_eco: str, requested: str) -> bool:
    requested = requested.strip().upper()
    if not requested:
        return True
    values = [x.strip().upper() for x in requested.replace(";", ",").split(",") if x.strip()]
    if not values:
        return True
    for value in values:
        if "-" in value and len(value) >= 3:
            left, right = value.split("-", 1)
            if left[0] == right[0] == game_eco[:1]:
                try:
                    n = int(game_eco[1:])
                    if int(left[1:]) <= n <= int(right[1:]):
                        return True
                except ValueError:
                    pass
        elif game_eco.upper().startswith(value):
            return True
    return False


def _move_tokens(movetext: str) -> list[str]:
    text = re.sub(r"\{[^}]*\}", " ", movetext)
    text = re.sub(r"\([^)]*\)", " ", text)
    text = re.sub(r";[^\n]*", " ", text)
    text = re.sub(r"\$\d+", " ", text)
    text = re.sub(r"\b\d+\.(?:\.\.)?", " ", text)

    tokens: list[str] = []
    for token in text.replace("\n", " ").split():
        token = token.strip()
        if not token or token in {"1-0", "0-1", "1/2-1/2", "*"}:
            continue
        token = token.rstrip("!?+#")
        if token:
            tokens.append(token.lower())
    return tokens


def _prefix_matches(moves: str, prefix: str) -> bool:
    if not prefix.strip():
        return True
    move_tokens = _move_tokens(moves)
    prefix_tokens = _move_tokens(prefix)
    if not prefix_tokens:
        return True
    if len(move_tokens) < len(prefix_tokens):
        return False
    return move_tokens[: len(prefix_tokens)] == prefix_tokens


def tags_match_filter(tags: dict[str, str], flt: GameFilter) -> bool:
    perf_type = detect_perf_type(tags.get("Event", ""), tags.get("TimeControl", ""))
    if flt.exclude_bullet and perf_type in {"bullet", "ultraBullet"}:
        return False
    if flt.perf_type and flt.perf_type != "any" and perf_type != flt.perf_type:
        return False

    white_elo = _to_int(tags.get("WhiteElo"))
    black_elo = _to_int(tags.get("BlackElo"))
    if not (flt.min_white_elo <= white_elo <= flt.max_white_elo):
        return False
    if not (flt.min_black_elo <= black_elo <= flt.max_black_elo):
        return False
    if min(white_elo, black_elo) < flt.min_both_elo:
        return False
    if abs(white_elo - black_elo) > flt.max_rating_diff:
        return False

    if not flt.include_timeout and tags.get("Termination", "").lower() in {"time forfeit", "timeout", "abandoned"}:
        return False
    if not _eco_matches(tags.get("ECO", ""), flt.eco):
        return False
    if not _contains(tags.get("Opening", ""), flt.opening_contains):
        return False

    if flt.result_mode != "all":
        selected_color = flt.color
        if selected_color == "any" and flt.username:
            username = flt.username.lower()
            if tags.get("White", "").lower() == username:
                selected_color = "white"
            elif tags.get("Black", "").lower() == username:
                selected_color = "black"
            else:
                selected_color = "unknown"
        if selected_color not in {"white", "black"}:
            return True
        score = result_score(tags.get("Result", "*"), selected_color)
        if flt.result_mode == "selected_wins" and score != 1.0:
            return False
        if flt.result_mode == "selected_losses" and score != 0.0:
            return False
        if flt.result_mode == "wins_draws" and score not in {1.0, 0.5}:
            return False

    return True


def matches_filter(game: GameRecord, flt: GameFilter) -> bool:
    if flt.exclude_bullet and game.perf_type in {"bullet", "ultraBullet"}:
        return False
    if flt.perf_type and flt.perf_type != "any" and game.perf_type != flt.perf_type:
        return False
    if not (flt.min_white_elo <= game.white_elo <= flt.max_white_elo):
        return False
    if not (flt.min_black_elo <= game.black_elo <= flt.max_black_elo):
        return False
    if min(game.white_elo, game.black_elo) < flt.min_both_elo:
        return False
    if abs(game.white_elo - game.black_elo) > flt.max_rating_diff:
        return False
    if game.move_count < flt.min_moves:
        return False
    if not flt.include_timeout and game.termination.lower() in {"time forfeit", "timeout", "abandoned"}:
        return False
    if not _eco_matches(game.eco, flt.eco):
        return False
    if not _contains(game.opening, flt.opening_contains):
        return False
    if not _prefix_matches(game.moves, flt.move_prefix):
        return False

    if flt.result_mode != "all":
        selected_color = flt.color
        if selected_color == "any" and flt.username:
            selected_color = game.color_for_username(flt.username)
        if selected_color not in {"white", "black"}:
            return True
        score = game.score_for_username(game.white if selected_color == "white" else game.black)
        if flt.result_mode == "selected_wins" and score != 1.0:
            return False
        if flt.result_mode == "selected_losses" and score != 0.0:
            return False
        if flt.result_mode == "wins_draws" and score not in {1.0, 0.5}:
            return False
        if flt.result_mode == "no_quick_losses" and score == 0.0 and game.move_count < max(25, flt.min_moves):
            return False
    return True
