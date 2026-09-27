from __future__ import annotations

import json
from pathlib import Path
from typing import Iterable

from backend.entities import GameRecord
from backend.parsers.pgn_parser import parse_pgn_game


def game_from_lichess_json(obj: dict, source: str = "") -> GameRecord:
    if obj.get("pgn"):
        game = parse_pgn_game(obj["pgn"], source=source)
        if not game.game_id:
            game.game_id = obj.get("id", "")
        return game

    players = obj.get("players", {})
    white = players.get("white", {})
    black = players.get("black", {})
    opening = obj.get("opening", {})
    perf = obj.get("perf", "")
    winner = obj.get("winner")
    result = "1/2-1/2"
    if winner == "white":
        result = "1-0"
    elif winner == "black":
        result = "0-1"
    return GameRecord(
        game_id=obj.get("id", ""),
        event=f"Rated {perf.title()} Game" if obj.get("rated") else f"Casual {perf.title()} Game",
        site=f"https://lichess.org/{obj.get('id', '')}",
        date=str(obj.get("createdAt", "")),
        white=white.get("user", {}).get("name", ""),
        black=black.get("user", {}).get("name", ""),
        result=result,
        white_elo=int(white.get("rating", 0) or 0),
        black_elo=int(black.get("rating", 0) or 0),
        eco=opening.get("eco", ""),
        opening=opening.get("name", ""),
        time_control=str(obj.get("clock", {}).get("initial", "")) + "+" + str(obj.get("clock", {}).get("increment", "")) if obj.get("clock") else "",
        termination=obj.get("status", ""),
        moves=obj.get("moves", ""),
        raw_pgn=obj.get("pgn", ""),
        source=source,
        tags={},
    )


def iter_ndjson_file(path: str | Path) -> Iterable[GameRecord]:
    path = Path(path)
    with path.open("r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                yield game_from_lichess_json(json.loads(line), source=str(path))
            except json.JSONDecodeError:
                continue


def parse_ndjson_file(path: str | Path) -> list[GameRecord]:
    return list(iter_ndjson_file(path))
