from __future__ import annotations

from pathlib import Path
from typing import Iterable

from backend.entities import GameRecord
from backend.parsers.ndjson_parser import iter_ndjson_file
from backend.parsers.pgn_parser import iter_pgn_file, iter_pgn_texts_file
from backend.parsers.zst_reader import iter_zst_pgn_games, iter_zst_pgn_texts


def is_pgn_text_path(path: str | Path) -> bool:
    path = Path(path)
    suffixes = "".join(path.suffixes).lower()
    return suffixes.endswith(".pgn.zst") or path.suffix.lower() in {".zst", ".pgn"}


def iter_games_from_path(path: str | Path, *, max_games: int | None = None) -> Iterable[GameRecord]:
    path = Path(path)
    suffixes = "".join(path.suffixes).lower()
    if suffixes.endswith(".pgn.zst") or path.suffix.lower() == ".zst":
        yield from iter_zst_pgn_games(path, max_games=max_games)
    elif path.suffix.lower() in {".ndjson", ".jsonl"}:
        count = 0
        for game in iter_ndjson_file(path):
            yield game
            count += 1
            if max_games is not None and count >= max_games:
                break
    else:
        count = 0
        for game in iter_pgn_file(path):
            yield game
            count += 1
            if max_games is not None and count >= max_games:
                break


def iter_pgn_texts_from_path(path: str | Path, *, max_games: int | None = None) -> Iterable[str]:
    path = Path(path)
    suffixes = "".join(path.suffixes).lower()
    if suffixes.endswith(".pgn.zst") or path.suffix.lower() == ".zst":
        yield from iter_zst_pgn_texts(path, max_games=max_games)
    elif path.suffix.lower() == ".pgn":
        count = 0
        for pgn_text in iter_pgn_texts_file(path):
            yield pgn_text
            count += 1
            if max_games is not None and count >= max_games:
                break
    else:
        raise ValueError(f"Path does not contain PGN text: {path}")
