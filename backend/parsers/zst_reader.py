from __future__ import annotations

import codecs
from pathlib import Path
from typing import Callable, Iterable

from backend.parsers.pgn_parser import parse_pgn_game

try:
    import zstandard as zstd
except Exception:  # pragma: no cover
    zstd = None


def _split_chunk_text(text: str, tail: str) -> tuple[list[str], str]:
    combined = tail + text
    parts = combined.split("\n\n[")
    if len(parts) == 1:
        return [], combined
    games: list[str] = []
    for part in parts[:-1]:
        part = part.strip()
        if not part:
            continue
        if not part.startswith("["):
            part = "[" + part
        games.append(part)
    new_tail = parts[-1]
    if not new_tail.startswith("["):
        new_tail = "[" + new_tail
    return games, new_tail


def iter_zst_pgn_texts(
    path: str | Path,
    *,
    chunk_size: int = 2_000_000,
    max_games: int | None = None,
    progress_callback: Callable[[int], None] | None = None,
) -> Iterable[str]:
    if zstd is None:
        raise RuntimeError("Package 'zstandard' is not installed. Run: pip install zstandard")

    path = Path(path)
    decoder = codecs.getincrementaldecoder("utf-8")()
    total = 0
    tail = ""
    with path.open("rb") as fh:
        dctx = zstd.ZstdDecompressor(max_window_size=2**31)
        with dctx.stream_reader(fh) as reader:
            while True:
                chunk = reader.read(chunk_size)
                if not chunk:
                    break
                text = decoder.decode(chunk)
                pgn_texts, tail = _split_chunk_text(text, tail)
                for pgn_text in pgn_texts:
                    total += 1
                    if progress_callback and total % 5000 == 0:
                        progress_callback(total)
                    yield pgn_text
                    if max_games is not None and total >= max_games:
                        return
    tail += decoder.decode(b"", final=True)
    if tail.strip().startswith("[") and (max_games is None or total < max_games):
        yield tail.strip()


def iter_zst_pgn_games(
    path: str | Path,
    *,
    chunk_size: int = 2_000_000,
    max_games: int | None = None,
    progress_callback: Callable[[int], None] | None = None,
):
    for pgn_text in iter_zst_pgn_texts(
        path,
        chunk_size=chunk_size,
        max_games=max_games,
        progress_callback=progress_callback,
    ):
        yield parse_pgn_game(pgn_text, source=str(path))
