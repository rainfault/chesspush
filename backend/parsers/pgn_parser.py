from __future__ import annotations

import re
from pathlib import Path
from typing import Iterable
import urllib.parse

from backend.entities import GameRecord


def split_pgn_games(text: str) -> list[str]:
    text = text.replace("\r\n", "\n").strip()
    if not text:
        return []
    # Lichess PGN games start with [Event ...]. Split before each next Event tag.
    parts = re.split(r'\n(?=\[Event\s+")', text)
    return [p.strip() for p in parts if p.strip().startswith("[")]


def parse_tags(pgn: str) -> dict[str, str]:
    header_end = pgn.find("\n\n")
    header = pgn[:header_end] if header_end >= 0 else pgn
    if "\r" in header:
        header = header.replace("\r\n", "\n")
    tags: dict[str, str] = {}
    for line in header.split("\n"):
        if not line.startswith("["):
            continue
        key_end = line.find(" ")
        if key_end <= 1:
            continue
        value_start = line.find('"', key_end)
        value_end = line.rfind('"')
        if value_start < 0 or value_end <= value_start:
            continue
        tags[line[1:key_end]] = line[value_start + 1:value_end]
    return tags


def strip_tag_section(pgn: str) -> str:
    lines = pgn.replace("\r\n", "\n").split("\n")
    move_lines: list[str] = []
    in_moves = False
    for line in lines:
        if not in_moves and line.startswith("["):
            continue
        if not in_moves and not line.strip():
            in_moves = True
            continue
        if in_moves:
            move_lines.append(line)
    if not move_lines:
        # Some PGN fragments omit the blank line.
        move_lines = [line for line in lines if not line.startswith("[")]
    return "\n".join(move_lines).strip()


def _to_int(value: str | None) -> int:
    try:
        return int(value or 0)
    except ValueError:
        return 0


def _lichess_game_id(value: str) -> str:
    """Return an id only for a direct Lichess game URL.

    A PGN ``Site`` tag is commonly a venue or a site home page, not a game
    identifier.  In particular, treating values such as ``Chess.com`` or a
    Lichess study URL as ids makes unrelated games collapse in local storage.
    """
    parsed = urllib.parse.urlparse(value.strip())
    hostname = (parsed.hostname or "").casefold()
    if hostname != "lichess.org" and not hostname.endswith(".lichess.org"):
        return ""
    parts = [part for part in parsed.path.split("/") if part]
    candidate = parts[0] if parts else ""
    return candidate if re.fullmatch(r"[A-Za-z0-9]{8,12}", candidate) else ""


def parse_pgn_game(pgn: str, source: str = "", tags: dict[str, str] | None = None) -> GameRecord:
    tags = tags or parse_tags(pgn)
    site = tags.get("Site", "")
    game_id = _lichess_game_id(site) or _lichess_game_id(tags.get("LichessURL", ""))
    return GameRecord(
        game_id=game_id,
        event=tags.get("Event", ""),
        site=site,
        date=tags.get("Date", tags.get("UTCDate", "")),
        white=tags.get("White", ""),
        black=tags.get("Black", ""),
        result=tags.get("Result", "*"),
        white_elo=_to_int(tags.get("WhiteElo")),
        black_elo=_to_int(tags.get("BlackElo")),
        eco=tags.get("ECO", ""),
        opening=tags.get("Opening", ""),
        time_control=tags.get("TimeControl", ""),
        termination=tags.get("Termination", ""),
        moves=strip_tag_section(pgn),
        raw_pgn=pgn.strip() + "\n",
        source=source,
        tags=tags,
    )


def parse_pgn_text(text: str, source: str = "") -> list[GameRecord]:
    return [parse_pgn_game(game, source=source) for game in split_pgn_games(text)]


def parse_pgn_file(path: str | Path) -> list[GameRecord]:
    path = Path(path)
    return parse_pgn_text(path.read_text(encoding="utf-8", errors="replace"), source=str(path))


def iter_pgn_file(path: str | Path) -> Iterable[GameRecord]:
    for game_text in iter_pgn_texts_file(path):
        yield parse_pgn_game(game_text, source=str(path))


def iter_pgn_texts_file(path: str | Path) -> Iterable[str]:
    # Keep the same Event boundary as split_pgn_games, without loading an entire
    # master PGN or imported file into memory.
    lines: list[str] = []
    with Path(path).open(encoding="utf-8-sig", errors="replace") as stream:
        for line in stream:
            if line.startswith('[Event ') and lines:
                text = ''.join(lines).strip()
                if text.startswith('['): yield text
                lines = []
            lines.append(line)
    text = ''.join(lines).strip()
    if text.startswith('['): yield text
