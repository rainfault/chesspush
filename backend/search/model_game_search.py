from __future__ import annotations

from pathlib import Path
from typing import Callable, Iterable

from backend.entities import GameRecord
from backend.filters.game_filters import GameFilter, matches_filter, tags_match_filter
from backend.parsers.auto_loader import is_pgn_text_path, iter_games_from_path, iter_pgn_texts_from_path
from backend.parsers.pgn_parser import parse_pgn_game, parse_tags
from backend.search.fen_tools import pgn_reaches_fen


def iter_matching_games_from_path(
    path: str | Path,
    game_filter: GameFilter,
    *,
    start_index: int = 0,
    max_games: int | None = None,
    progress_callback: Callable[[int], None] | None = None,
    progress_step: int = 1000,
    should_stop: Callable[[], bool] | None = None,
) -> Iterable[tuple[int, GameRecord]]:
    """Yield matching games after skipping ``start_index`` raw games.

    Returned indices are absolute, one-based positions in the source.  A start
    index of ``N`` therefore skips games 1..N and starts matching at N+1.  For
    compressed sources this is a logical skip: the ZST stream is still decoded
    from its beginning.
    """
    if start_index < 0:
        raise ValueError("start_index must be zero or greater")
    if max_games is not None and max_games < 0:
        raise ValueError("max_games must be zero or greater")
    if progress_step <= 0:
        raise ValueError("progress_step must be greater than zero")
    if max_games == 0:
        return

    path = Path(path)
    scanned = 0
    last_reported = 0
    source_limit = None if max_games is None else start_index + max_games

    def report_progress(*, force: bool = False) -> None:
        nonlocal last_reported
        if progress_callback is None or scanned == last_reported:
            return
        if force or scanned % progress_step == 0:
            progress_callback(scanned)
            last_reported = scanned

    if is_pgn_text_path(path):
        for pgn_text in iter_pgn_texts_from_path(path, max_games=source_limit):
            if should_stop is not None and should_stop():
                break
            scanned += 1
            report_progress()
            if scanned <= start_index:
                continue

            tags = parse_tags(pgn_text)
            if not tags_match_filter(tags, game_filter):
                continue

            game = parse_pgn_game(pgn_text, source=str(path), tags=tags)
            if matches_filter(game, game_filter):
                yield scanned, game
        report_progress(force=True)
        return

    for game in iter_games_from_path(path, max_games=source_limit):
        if should_stop is not None and should_stop():
            break
        scanned += 1
        report_progress()
        if scanned <= start_index:
            continue
        if matches_filter(game, game_filter):
            yield scanned, game
    report_progress(force=True)


class ModelGameSearch:
    def __init__(self, database_path: str):
        self.database_path = database_path

    def search(
        self,
        *,
        game_filter: GameFilter,
        fen: str = "",
        limit: int = 20,
        scan_limit: int | None = None,
    ) -> list[GameRecord]:
        if not self.database_path:
            raise ValueError("Database path is empty")
        path = Path(self.database_path)
        if not path.exists():
            raise FileNotFoundError(str(path))

        result: list[GameRecord] = []
        for _, game in iter_matching_games_from_path(path, game_filter, max_games=scan_limit):
            if fen.strip() and not pgn_reaches_fen(game.raw_pgn, fen):
                continue
            result.append(game)
            if len(result) >= limit:
                break
        return result
