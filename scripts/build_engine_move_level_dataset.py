from __future__ import annotations

import io
import json
import re
from pathlib import Path
from typing import Optional

import chess
import chess.engine
import chess.pgn


CLK_RE = re.compile(r"\[%clk\s+([0-9]+:[0-9]{2}:[0-9]{2})\]")

STOCKFISH_PATH = r"C:\chess-engine\stockfish-windows-x86-64-avx2\stockfish\stockfish-windows-x86-64-avx2.exe"
ENGINE_DEPTH = 10
ENGINE_MULTIPV = 3
MATE_CP = 10000


def load_jsonl(path: str) -> list[dict]:
    rows: list[dict] = []
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def save_jsonl(path: str, rows: list[dict]) -> None:
    file_path = Path(path)
    file_path.parent.mkdir(parents=True, exist_ok=True)

    with open(file_path, "w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")


def parse_clock_to_seconds(clock_str: str) -> int:
    h, m, s = map(int, clock_str.split(":"))
    return h * 3600 + m * 60 + s


def extract_clock_times(moves_text: str) -> list[int]:
    return [parse_clock_to_seconds(x) for x in CLK_RE.findall(moves_text)]


def split_side_clocks(clock_times: list[int]) -> tuple[list[int], list[int]]:
    return clock_times[0::2], clock_times[1::2]


def parse_base_time_from_time_control(time_control: str) -> Optional[int]:
    if not time_control or "+" not in time_control:
        return None
    try:
        base_str, _ = time_control.split("+", 1)
        return int(base_str)
    except ValueError:
        return None


def clocks_to_move_durations(side_clocks: list[int], initial_time_sec: int) -> list[float]:
    if not side_clocks:
        return []

    durations: list[float] = []
    prev_clock = initial_time_sec

    for i, current_clock in enumerate(side_clocks):
        duration = prev_clock - current_clock

        # первый нулевой ход часто артефакт округления часов
        if i == 0 and duration == 0:
            prev_clock = current_clock
            continue

        if duration >= 0:
            durations.append(float(duration))

        prev_clock = current_clock

    return durations


def score_to_cp_for_color(score: chess.engine.PovScore, color: chess.Color) -> Optional[int]:
    """
    Оценка в centipawns в перспективе заданной стороны.
    Для матовых позиций возвращает условное число через mate_score.
    """
    side_score = score.white() if color == chess.WHITE else score.black()
    cp = side_score.score(mate_score=MATE_CP)
    return None if cp is None else int(cp)


def score_is_mate_for_color(score: chess.engine.PovScore, color: chess.Color) -> bool:
    side_score = score.white() if color == chess.WHITE else score.black()
    return side_score.is_mate()


def analyse_position(
    engine: chess.engine.SimpleEngine,
    board: chess.Board,
) -> dict:
    """
    Возвращает top-3 кандидатов из одной и той же позиции
    в перспективе стороны, которая ходит.
    """
    info = engine.analyse(
        board,
        chess.engine.Limit(depth=ENGINE_DEPTH),
        multipv=ENGINE_MULTIPV,
    )

    result = {
        "best_eval_cp": None,
        "second_best_eval_cp": None,
        "third_best_eval_cp": None,
        "best_is_mate": False,
        "second_is_mate": False,
        "third_is_mate": False,
        "best_move_uci": None,
        "second_best_move_uci": None,
        "third_best_move_uci": None,
    }

    if not info:
        return result

    if isinstance(info, dict):
        info = [info]

    perspective = board.turn
    slots = [
        ("best", 0),
        ("second_best", 1),
        ("third_best", 2),
    ]

    for prefix, idx in slots:
        if len(info) <= idx:
            continue
        node = info[idx]

        if "score" in node:
            result[f"{prefix}_eval_cp"] = score_to_cp_for_color(node["score"], perspective)
            result[f"{prefix}_is_mate"] = score_is_mate_for_color(node["score"], perspective)

        pv = node.get("pv")
        if pv and len(pv) > 0:
            result[f"{prefix}_move_uci"] = pv[0].uci()

    return result


def analyse_played_move_eval(
    engine: chess.engine.SimpleEngine,
    board_before: chess.Board,
    played_move: chess.Move,
) -> tuple[Optional[int], bool]:
    """
    Оценка фактически сыгранного хода, рассчитанная из той же исходной позиции
    через root_moves=[played_move], в перспективе игрока, который делает ход.
    """
    perspective = board_before.turn

    info = engine.analyse(
        board_before,
        chess.engine.Limit(depth=ENGINE_DEPTH),
        multipv=1,
        root_moves=[played_move],
    )

    if not info:
        return None, False

    if isinstance(info, list):
        if len(info) == 0:
            return None, False
        info = info[0]

    if "score" not in info:
        return None, False

    played_eval_cp = score_to_cp_for_color(info["score"], perspective)
    played_is_mate = score_is_mate_for_color(info["score"], perspective)

    return played_eval_cp, played_is_mate


def safe_abs_diff(a: Optional[int], b: Optional[int]) -> Optional[int]:
    if a is None or b is None:
        return None
    return abs(a - b)


def safe_cpl(
    best_eval_cp: Optional[int],
    played_eval_cp: Optional[int],
    best_is_mate: bool,
    played_is_mate: bool,
) -> Optional[int]:
    """
    CPL считаем только вне mate-позиций.
    Для mate-позиций возвращаем None, чтобы не ломать статистику.
    """
    if best_eval_cp is None or played_eval_cp is None:
        return None

    if best_is_mate or played_is_mate:
        return None

    return max(best_eval_cp - played_eval_cp, 0)


def build_move_level_rows_for_game(
    game_row: dict,
    engine: chess.engine.SimpleEngine,
) -> list[dict]:
    game_id = game_row.get("game_id")
    label = game_row.get("label")
    source_class = game_row.get("source_class")
    source_username = game_row.get("source_username")
    time_control = game_row.get("time_control") or ""
    moves_text = game_row.get("moves_text") or ""
    pgn_text = game_row.get("pgn") or ""

    game = chess.pgn.read_game(io.StringIO(pgn_text))
    if game is None:
        return []

    board = game.board()
    mainline_moves = list(game.mainline_moves())

    base_time_sec = parse_base_time_from_time_control(time_control)
    clock_times = extract_clock_times(moves_text)

    white_clocks, black_clocks = split_side_clocks(clock_times)
    white_durations = clocks_to_move_durations(white_clocks, base_time_sec) if base_time_sec is not None else []
    black_durations = clocks_to_move_durations(black_clocks, base_time_sec) if base_time_sec is not None else []

    white_move_index = 0
    black_move_index = 0

    rows: list[dict] = []

    for ply_index, move in enumerate(mainline_moves, start=1):
        mover_color = board.turn
        mover_side = "white" if mover_color == chess.WHITE else "black"

        position_info = analyse_position(engine, board)

        best_eval_cp = position_info["best_eval_cp"]
        second_best_eval_cp = position_info["second_best_eval_cp"]
        third_best_eval_cp = position_info["third_best_eval_cp"]

        best_is_mate = position_info["best_is_mate"]
        second_is_mate = position_info["second_is_mate"]
        third_is_mate = position_info["third_is_mate"]

        best_move_uci = position_info["best_move_uci"]
        second_best_move_uci = position_info["second_best_move_uci"]
        third_best_move_uci = position_info["third_best_move_uci"]

        played_eval_cp, played_is_mate = analyse_played_move_eval(engine, board, move)

        complexity_cp = safe_abs_diff(best_eval_cp, second_best_eval_cp)
        cpl_cp = safe_cpl(
            best_eval_cp=best_eval_cp,
            played_eval_cp=played_eval_cp,
            best_is_mate=best_is_mate,
            played_is_mate=played_is_mate,
        )
        mate_zone = best_is_mate or second_is_mate or third_is_mate or played_is_mate

        if mover_color == chess.WHITE:
            move_time_sec = white_durations[white_move_index] if white_move_index < len(white_durations) else None
            white_move_index += 1
        else:
            move_time_sec = black_durations[black_move_index] if black_move_index < len(black_durations) else None
            black_move_index += 1

        rows.append({
            "game_id": game_id,
            "label": label,
            "source_class": source_class,
            "source_username": source_username,
            "ply_index": ply_index,
            "side_to_move": mover_side,
            "move_uci": move.uci(),
            "move_time_sec": move_time_sec,

            "best_move_uci": best_move_uci,
            "second_best_move_uci": second_best_move_uci,
            "third_best_move_uci": third_best_move_uci,

            "best_eval_cp": best_eval_cp,
            "second_best_eval_cp": second_best_eval_cp,
            "third_best_eval_cp": third_best_eval_cp,

            "best_is_mate": best_is_mate,
            "second_is_mate": second_is_mate,
            "third_is_mate": third_is_mate,
            "played_is_mate": played_is_mate,
            "mate_zone": mate_zone,

            "played_eval_cp": played_eval_cp,
            "complexity_cp": complexity_cp,
            "cpl_cp": cpl_cp,
        })

        board.push(move)

    return rows


def main() -> None:
    input_path = "data/engine/engine_input_subset_300.jsonl"
    output_path = "data/engine/engine_move_level_300.jsonl"
    summary_path = "data/engine/engine_move_level_300_summary.json"

    games = load_jsonl(input_path)

    all_rows: list[dict] = []
    processed_games = 0
    failed_games = 0

    with chess.engine.SimpleEngine.popen_uci(STOCKFISH_PATH) as engine:
        engine.configure({"Threads": 1})

        for idx, game_row in enumerate(games, start=1):
            try:
                rows = build_move_level_rows_for_game(game_row, engine)
                if not rows:
                    failed_games += 1
                    continue

                all_rows.extend(rows)
                processed_games += 1

                if idx % 10 == 0:
                    print(f"Processed games: {idx}/{len(games)} | move rows: {len(all_rows)}")

            except Exception as e:
                failed_games += 1
                print(f"Failed game {game_row.get('game_id')}: {e}")

    save_jsonl(output_path, all_rows)

    summary = {
        "games_input": len(games),
        "games_processed": processed_games,
        "games_failed": failed_games,
        "move_rows_total": len(all_rows),
    }

    with open(summary_path, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)

    print("Done.")
    print(summary)


if __name__ == "__main__":
    main()