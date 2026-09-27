from __future__ import annotations

try:
    import chess
    import chess.pgn
except Exception:  # pragma: no cover
    chess = None


def is_python_chess_available() -> bool:
    return chess is not None


def normalize_fen(fen: str) -> str:
    return " ".join((fen or "").strip().split()[:4])


def pgn_reaches_fen(raw_pgn: str, target_fen: str, max_ply: int = 80) -> bool:
    if chess is None or not raw_pgn or not target_fen.strip():
        return False
    import io

    try:
        game = chess.pgn.read_game(io.StringIO(raw_pgn))
        if game is None:
            return False
        target = normalize_fen(target_fen)
        board = game.board()
        if normalize_fen(board.fen()) == target:
            return True
        for idx, move in enumerate(game.mainline_moves()):
            if idx > max_ply:
                break
            board.push(move)
            if normalize_fen(board.fen()) == target:
                return True
    except Exception:
        return False
    return False
