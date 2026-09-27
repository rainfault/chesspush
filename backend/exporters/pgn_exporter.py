from __future__ import annotations

from pathlib import Path

from backend.entities import GameRecord


def export_pgn(games: list[GameRecord], path: str | Path) -> Path:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as fh:
        for game in games:
            if game.raw_pgn.strip():
                fh.write(game.raw_pgn.strip())
            else:
                fh.write(render_minimal_pgn(game))
            fh.write("\n\n")
    return path


def render_minimal_pgn(game: GameRecord) -> str:
    tags = {
        "Event": game.event or "?",
        "Site": game.site or "?",
        "Date": game.date or "????.??.??",
        "White": game.white or "?",
        "Black": game.black or "?",
        "Result": game.result or "*",
        "WhiteElo": str(game.white_elo or ""),
        "BlackElo": str(game.black_elo or ""),
        "ECO": game.eco or "",
        "Opening": game.opening or "",
        "TimeControl": game.time_control or "",
        "Termination": game.termination or "",
    }
    lines = [f'[{k} "{v}"]' for k, v in tags.items()]
    return "\n".join(lines) + "\n\n" + (game.moves or "*")
