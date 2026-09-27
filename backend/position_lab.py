from __future__ import annotations

from dataclasses import dataclass
from io import StringIO
import re
from typing import Iterable

import chess
import chess.pgn

from backend.entities import GameRecord
from backend.exporters.pgn_exporter import render_minimal_pgn


ANNOTATION_COLORS = {"green", "red", "blue", "yellow"}
SQUARE_RE = re.compile(r"^[a-h][1-8]$")


@dataclass(frozen=True)
class GameTimeline:
    positions: list[str]
    moves: list[dict]

    @property
    def max_ply(self) -> int:
        return max(0, len(self.positions) - 1)


def build_game_timeline(game: GameRecord) -> GameTimeline:
    text = game.raw_pgn.strip() or render_minimal_pgn(game)
    parsed = chess.pgn.read_game(StringIO(text))
    parsed_ply = sum(1 for _ in parsed.mainline_moves()) if parsed is not None else 0
    if game.moves.strip() and (
        parsed is None or parsed.errors or parsed_ply < game.ply_count
    ):
        # Older/sparse imports can contain a header-only raw PGN while the
        # normalized movetext column is complete. Prefer the readable source.
        fallback = chess.pgn.read_game(StringIO(render_minimal_pgn(game)))
        fallback_ply = (
            sum(1 for _ in fallback.mainline_moves()) if fallback is not None else 0
        )
        if fallback is not None and not fallback.errors and fallback_ply >= parsed_ply:
            parsed = fallback
    if parsed is None:
        raise ValueError("PGN does not contain a readable game")
    if parsed.errors:
        raise ValueError(f"PGN contains an invalid move: {parsed.errors[0]}")

    board = parsed.board()
    positions = [board.fen()]
    moves = []
    for ply, move in enumerate(parsed.mainline_moves(), start=1):
        color = "white" if board.turn == chess.WHITE else "black"
        move_number = board.fullmove_number
        san = board.san(move)
        label = f"{move_number}. {san}" if color == "white" else f"{move_number}... {san}"
        board.push(move)
        positions.append(board.fen())
        moves.append(
            {
                "ply": ply,
                "move_number": move_number,
                "color": color,
                "san": san,
                "uci": move.uci(),
                "label": label,
                "fen": board.fen(),
            }
        )
    return GameTimeline(positions=positions, moves=moves)


def board_squares(fen: str, *, flipped: bool, highlights: Iterable[dict] = ()) -> list[dict]:
    board = chess.Board(fen)
    highlight_map = {
        str(item.get("square", "")): annotation_color(str(item.get("color", "green")))
        for item in highlights
        if valid_square(str(item.get("square", "")))
    }
    ranks = range(0, 8) if flipped else range(7, -1, -1)
    files = range(7, -1, -1) if flipped else range(8)
    rows = []
    for rank in ranks:
        for file_index in files:
            square = chess.square(file_index, rank)
            name = chess.square_name(square)
            piece = board.piece_at(square)
            rows.append(
                {
                    "name": name,
                    "file": chess.FILE_NAMES[file_index],
                    "rank": str(rank + 1),
                    "dark": (rank + file_index) % 2 == 1,
                    "piece_asset": (
                        f"{'w' if piece.color == chess.WHITE else 'b'}{piece.symbol().upper()}.svg"
                        if piece
                        else ""
                    ),
                    "highlight_color": highlight_map.get(name, ""),
                }
            )
    return rows


def game_summary(game: GameRecord, index: int) -> dict:
    return {
        "index": index,
        "storage_id": game.storage_id,
        "title": f"{game.white or '?'} — {game.black or '?'}",
        "subtitle": " · ".join(
            part for part in (game.date, game.result, game.eco, game.opening) if part
        ),
        "white": game.white,
        "black": game.black,
        "date": game.date,
        "result": game.result,
        "eco": game.eco,
        "opening": game.opening,
        "source": game.source,
        "source_kind": game.source_kind,
    }


def game_info(game: GameRecord) -> dict:
    return {
        "storage_id": game.storage_id,
        "players": f"{game.white or '?'} — {game.black or '?'}",
        "white": game.white,
        "black": game.black,
        "result": game.result,
        "date": game.date,
        "eco": game.eco or "???",
        "opening": game.opening or "Без названия дебюта",
        "event": game.event,
        "site": game.site,
        "source": game.source,
        "source_kind": game.source_kind,
    }


def player_color(game: GameRecord, username: str) -> str:
    normalized = username.strip().casefold()
    if normalized and game.white.strip().casefold() == normalized:
        return "white"
    if normalized and game.black.strip().casefold() == normalized:
        return "black"
    return "unknown"


def side_to_move(fen: str) -> str:
    return "white" if chess.Board(fen).turn == chess.WHITE else "black"


def move_label(timeline: GameTimeline, ply: int) -> str:
    if ply <= 0:
        return "Начальная позиция"
    index = min(ply, timeline.max_ply) - 1
    return str(timeline.moves[index]["label"])


def parse_tags_text(value: str) -> list[str]:
    seen = set()
    tags = []
    for part in re.split(r"[,;\n]", value or ""):
        tag = " ".join(part.strip().split())
        key = tag.casefold()
        if tag and key not in seen:
            seen.add(key)
            tags.append(tag)
    return tags


def normalize_arrows(items: Iterable[dict]) -> list[dict]:
    normalized = []
    seen = set()
    for item in items:
        start = str(item.get("from", "")).lower()
        end = str(item.get("to", "")).lower()
        color = annotation_color(str(item.get("color", "green")))
        key = (start, end, color)
        if start != end and valid_square(start) and valid_square(end) and key not in seen:
            seen.add(key)
            normalized.append({"from": start, "to": end, "color": color})
    return normalized


def normalize_highlights(items: Iterable[dict]) -> list[dict]:
    normalized = []
    seen = set()
    for item in items:
        square = str(item.get("square", "")).lower()
        color = annotation_color(str(item.get("color", "green")))
        key = (square, color)
        if valid_square(square) and key not in seen:
            seen.add(key)
            normalized.append({"square": square, "color": color})
    return normalized


def annotation_color(value: str) -> str:
    normalized = value.strip().lower()
    return normalized if normalized in ANNOTATION_COLORS else "green"


def valid_square(value: str) -> bool:
    return bool(SQUARE_RE.fullmatch(value.strip().lower()))
