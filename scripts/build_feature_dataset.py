from __future__ import annotations

import json
import math
import re
import statistics
from pathlib import Path
from typing import Optional

import pandas as pd


CLK_RE = re.compile(r"\[%clk\s+([0-9]+:[0-9]{2}:[0-9]{2})\]")


def load_jsonl(path: str) -> list[dict]:
    file_path = Path(path)
    if not file_path.exists():
        raise FileNotFoundError(f"File not found: {file_path}")

    rows: list[dict] = []
    with open(file_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rows.append(json.loads(line))

    return rows


def save_jsonl(path: str, rows: list[dict]) -> None:
    file_path = Path(path)
    file_path.parent.mkdir(parents=True, exist_ok=True)

    with open(file_path, "w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")


def parse_clock_to_seconds(clock_str: str) -> int:
    """
    '0:09:58' -> 598
    """
    parts = clock_str.split(":")
    if len(parts) != 3:
        raise ValueError(f"Unexpected clock format: {clock_str}")

    hours, minutes, seconds = map(int, parts)
    return hours * 3600 + minutes * 60 + seconds


def extract_clock_times(moves_text: str) -> list[int]:
    """
    Извлекает оставшееся время после каждого полухода.
    """
    matches = CLK_RE.findall(moves_text)
    return [parse_clock_to_seconds(x) for x in matches]


def split_side_clocks(clock_times: list[int]) -> tuple[list[int], list[int]]:
    """
    Чётные индексы -> белые, нечётные -> чёрные.
    """
    white = clock_times[0::2]
    black = clock_times[1::2]
    return white, black


def clocks_to_move_durations(side_clocks: list[int], initial_time_sec: int) -> list[float]:
    """
    Преобразует оставшееся время игрока после каждого его хода
    в затраченное время на каждом его ходе.

    Для 600+0:
      duration_1 = initial_time - clock_after_move_1
      duration_i = clock_after_prev_move - clock_after_current_move

    Первый нулевой ход часто является артефактом округления часов,
    поэтому он пропускается.
    """
    if not side_clocks:
        return []

    durations: list[float] = []
    prev_clock = initial_time_sec

    for i, current_clock in enumerate(side_clocks):
        duration = prev_clock - current_clock

        # Частый случай: после первого хода часы всё ещё показывают 10:00
        # Из-за дискретности секунд это даёт ложный duration = 0
        if i == 0 and duration == 0:
            prev_clock = current_clock
            continue

        # Защита от некорректных значений
        if duration >= 0:
            durations.append(float(duration))

        prev_clock = current_clock

    return durations


def safe_mean(values: list[float]) -> Optional[float]:
    return float(statistics.mean(values)) if values else None


def safe_median(values: list[float]) -> Optional[float]:
    return float(statistics.median(values)) if values else None


def safe_std(values: list[float]) -> Optional[float]:
    if len(values) < 2:
        return 0.0 if values else None
    return float(statistics.pstdev(values))


def safe_min(values: list[float]) -> Optional[float]:
    return float(min(values)) if values else None


def safe_max(values: list[float]) -> Optional[float]:
    return float(max(values)) if values else None


def safe_ratio(condition_count: int, total_count: int) -> Optional[float]:
    if total_count == 0:
        return None
    return condition_count / total_count


def coefficient_of_variation(values: list[float]) -> Optional[float]:
    if not values:
        return None
    mean_val = safe_mean(values)
    std_val = safe_std(values)
    if mean_val is None or std_val is None or mean_val == 0:
        return None
    return std_val / mean_val


def parse_base_time_from_time_control(time_control: str) -> Optional[int]:
    """
    '600+0' -> 600
    """
    if not time_control or "+" not in time_control:
        return None
    try:
        base_str, _inc_str = time_control.split("+", 1)
        return int(base_str)
    except ValueError:
        return None


def opening_family(opening: Optional[str]) -> str:
    """
    Очень грубая безопасная агрегация дебюта.
    Не используем точное название дебюта как признак.
    """
    if not opening:
        return "UNKNOWN"

    opening_lower = opening.lower()

    families = [
        "sicilian",
        "french",
        "caro-kann",
        "pirc",
        "modern defense",
        "scandinavian",
        "queen's pawn",
        "queen's gambit",
        "king's pawn",
        "english",
        "bird",
        "indian defense",
        "ruy lopez",
        "philidor",
        "petrov",
        "slav",
        "benoni",
        "hungarian opening",
        "saragossa",
        "zukertort",
        "center game",
    ]

    for fam in families:
        if fam in opening_lower:
            return fam.upper().replace(" ", "_").replace("-", "_")

    return "OTHER"


def build_feature_row(row: dict) -> dict:
    moves_text = row.get("moves_text") or ""
    time_control = (row.get("time_control") or "").strip()
    base_time_sec = parse_base_time_from_time_control(time_control)

    clock_times = extract_clock_times(moves_text)
    white_clocks, black_clocks = split_side_clocks(clock_times)

    if base_time_sec is None:
        white_durations = []
        black_durations = []
    else:
        white_durations = clocks_to_move_durations(white_clocks, base_time_sec)
        black_durations = clocks_to_move_durations(black_clocks, base_time_sec)

    all_durations = white_durations + black_durations

    fast_threshold = 2.0
    slow_threshold = 10.0

    fast_moves = sum(1 for x in all_durations if x <= fast_threshold)
    slow_moves = sum(1 for x in all_durations if x >= slow_threshold)

    white_fast = sum(1 for x in white_durations if x <= fast_threshold)
    black_fast = sum(1 for x in black_durations if x <= fast_threshold)
    white_slow = sum(1 for x in white_durations if x >= slow_threshold)
    black_slow = sum(1 for x in black_durations if x >= slow_threshold)

    white_avg = safe_mean(white_durations)
    black_avg = safe_mean(black_durations)

    feature_row = {
        # target
        "label": row.get("label"),

        # identifiers / audit fields
        "game_id": row.get("game_id"),
        "source_class": row.get("source_class"),
        "source_username": row.get("source_username"),

        # safe metadata
        "white_elo": row.get("white_elo"),
        "black_elo": row.get("black_elo"),
        "elo_diff_abs": abs((row.get("white_elo") or 0) - (row.get("black_elo") or 0)),
        "ply_count": row.get("ply_count"),
        "opening_family": opening_family(row.get("opening")),

        # clock availability
        "clock_tag_count": row.get("clock_tag_count"),
        "has_clock_data": row.get("has_clock_data"),

        # all moves
        "move_count_total": len(all_durations),
        "avg_move_time": safe_mean(all_durations),
        "median_move_time": safe_median(all_durations),
        "std_move_time": safe_std(all_durations),
        # "min_move_time": safe_min(all_durations),
        "max_move_time": safe_max(all_durations),
        "cv_move_time": coefficient_of_variation(all_durations),
        "fast_move_ratio": safe_ratio(fast_moves, len(all_durations)),
        "slow_move_ratio": safe_ratio(slow_moves, len(all_durations)),

        # white-only
        "white_move_count": len(white_durations),
        "white_avg_move_time": white_avg,
        "white_median_move_time": safe_median(white_durations),
        "white_std_move_time": safe_std(white_durations),
        "white_fast_move_ratio": safe_ratio(white_fast, len(white_durations)),
        "white_slow_move_ratio": safe_ratio(white_slow, len(white_durations)),

        # black-only
        "black_move_count": len(black_durations),
        "black_avg_move_time": black_avg,
        "black_median_move_time": safe_median(black_durations),
        "black_std_move_time": safe_std(black_durations),
        "black_fast_move_ratio": safe_ratio(black_fast, len(black_durations)),
        "black_slow_move_ratio": safe_ratio(black_slow, len(black_durations)),

        # symmetry / imbalance
        "wb_avg_time_diff": (
            None if white_avg is None or black_avg is None else white_avg - black_avg
        ),
        "wb_move_count_diff": len(white_durations) - len(black_durations),
    }

    return feature_row


def build_feature_dataset(
    input_path: str,
    output_csv_path: str,
    output_jsonl_path: str,
    summary_path: str,
) -> None:
    rows = load_jsonl(input_path)
    feature_rows = [build_feature_row(row) for row in rows]

    df = pd.DataFrame(feature_rows)

    # Сохраняем полный feature dataset
    Path(output_csv_path).parent.mkdir(parents=True, exist_ok=True)
    df.to_csv(output_csv_path, index=False, encoding="utf-8")

    save_jsonl(output_jsonl_path, feature_rows)

    # Сводка
    summary = {
        "dataset_size": int(len(df)),
        "class_counts": {
            str(k): int(v) for k, v in df["label"].value_counts(dropna=False).to_dict().items()
        },
        "columns": list(df.columns),
        "null_counts": {
            col: int(val) for col, val in df.isnull().sum().to_dict().items()
        },
        "numeric_means": {
            col: (None if pd.isna(val) else float(val))
            for col, val in df.select_dtypes(include=["number"]).mean(numeric_only=True).to_dict().items()
        },
    }

    summary_file = Path(summary_path)
    summary_file.parent.mkdir(parents=True, exist_ok=True)
    with open(summary_file, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)

    print("Feature dataset built successfully.")
    print(f"rows={len(df)}")
    print(f"csv={output_csv_path}")
    print(f"jsonl={output_jsonl_path}")
    print(f"summary={summary_path}")


if __name__ == "__main__":
    build_feature_dataset(
        input_path="data/processed/labeled_dataset_600_0.jsonl",
        output_csv_path="data/features/feature_dataset_baseline.csv",
        output_jsonl_path="data/features/feature_dataset_baseline.jsonl",
        summary_path="data/features/feature_dataset_baseline_summary.json",
    )