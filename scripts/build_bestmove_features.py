from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Optional

import pandas as pd


COMPLEXITY_CLIP = 1000
CRITICAL_QUANTILE = 0.8
TIME_PRESSURE_SEC = 2.0


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


def safe_ratio(num: int, den: int) -> Optional[float]:
    if den == 0:
        return None
    return num / den


def quantile_threshold(values: list[float], q: float) -> Optional[float]:
    if not values:
        return None

    values = sorted(values)
    if len(values) == 1:
        return values[0]

    idx = q * (len(values) - 1)
    lo = int(math.floor(idx))
    hi = int(math.ceil(idx))

    if lo == hi:
        return values[lo]

    frac = idx - lo
    return values[lo] * (1 - frac) + values[hi] * frac


def transform_complexity(value: Optional[float]) -> Optional[float]:
    if value is None:
        return None
    clipped = float(min(value, COMPLEXITY_CLIP))
    return math.log1p(clipped)


def group_rows_by_game(rows: list[dict]) -> dict[str, list[dict]]:
    grouped: dict[str, list[dict]] = {}
    for row in rows:
        game_id = row.get("game_id")
        if not game_id:
            continue
        grouped.setdefault(game_id, []).append(row)

    for game_id in grouped:
        grouped[game_id].sort(key=lambda r: r.get("ply_index", 0))

    return grouped


def build_bestmove_feature_row(game_rows: list[dict]) -> dict:
    first = game_rows[0]

    valid_rows = []
    for r in game_rows:
        if r.get("mate_zone"):
            continue

        move_uci = r.get("move_uci")
        best_move_uci = r.get("best_move_uci")
        second_best_move_uci = r.get("second_best_move_uci")
        third_best_move_uci = r.get("third_best_move_uci")
        move_time_sec = r.get("move_time_sec")
        complexity = transform_complexity(r.get("complexity_cp"))

        valid_rows.append({
            "move_uci": move_uci,
            "best_move_uci": best_move_uci,
            "second_best_move_uci": second_best_move_uci,
            "third_best_move_uci": third_best_move_uci,
            "move_time_sec": move_time_sec,
            "complexity": complexity,
        })

    best_hits = 0
    top2_hits = 0
    top3_hits = 0

    complexity_vals = [r["complexity"] for r in valid_rows if r["complexity"] is not None]
    critical_threshold = quantile_threshold(complexity_vals, CRITICAL_QUANTILE)

    crit_best_hits = 0
    crit_total = 0

    pressure_best_hits = 0
    pressure_total = 0

    for r in valid_rows:
        move_uci = r["move_uci"]
        best1 = r["best_move_uci"]
        best2 = r["second_best_move_uci"]
        best3 = r["third_best_move_uci"]

        if move_uci is not None and best1 is not None and move_uci == best1:
            best_hits += 1

        if move_uci is not None and move_uci in {best1, best2}:
            top2_hits += 1

        if move_uci is not None and move_uci in {best1, best2, best3}:
            top3_hits += 1

        if critical_threshold is not None and r["complexity"] is not None and r["complexity"] >= critical_threshold:
            crit_total += 1
            if move_uci is not None and best1 is not None and move_uci == best1:
                crit_best_hits += 1

        if r["move_time_sec"] is not None and float(r["move_time_sec"]) <= TIME_PRESSURE_SEC:
            pressure_total += 1
            if move_uci is not None and best1 is not None and move_uci == best1:
                pressure_best_hits += 1

    total = len(valid_rows)

    return {
        "game_id": first.get("game_id"),
        "label": first.get("label"),
        "source_class": first.get("source_class"),
        "source_username": first.get("source_username"),

        "engine_nonmate_move_count": total,
        "critical_nonmate_positions_count": crit_total,
        "time_pressure_positions_count": pressure_total,

        "best_move_ratio": safe_ratio(best_hits, total),
        "top2_move_ratio": safe_ratio(top2_hits, total),
        "top3_move_ratio": safe_ratio(top3_hits, total),
        "best_move_ratio_in_critical_positions": safe_ratio(crit_best_hits, crit_total),
        "best_move_ratio_under_time_pressure": safe_ratio(pressure_best_hits, pressure_total),
    }


def main() -> None:
    input_path = "data/engine/engine_move_level_120.jsonl"
    output_jsonl_path = "data/engine/bestmove_features_120.jsonl"
    output_csv_path = "data/engine/bestmove_features_120.csv"
    summary_path = "data/engine/bestmove_features_120_summary.json"

    rows = load_jsonl(input_path)
    grouped = group_rows_by_game(rows)

    feature_rows = []
    for _, game_rows in grouped.items():
        feature_rows.append(build_bestmove_feature_row(game_rows))

    feature_rows.sort(key=lambda r: r["game_id"])

    save_jsonl(output_jsonl_path, feature_rows)

    df = pd.DataFrame(feature_rows)
    Path(output_csv_path).parent.mkdir(parents=True, exist_ok=True)
    df.to_csv(output_csv_path, index=False, encoding="utf-8")

    summary = {
        "games_total": int(len(df)),
        "class_counts": {
            str(k): int(v) for k, v in df["label"].value_counts(dropna=False).to_dict().items()
        },
        "null_counts": {
            col: int(v) for col, v in df.isnull().sum().to_dict().items()
        },
        "numeric_means": {
            col: (None if pd.isna(val) else float(val))
            for col, val in df.select_dtypes(include=["number"]).mean(numeric_only=True).to_dict().items()
        },
    }

    with open(summary_path, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)

    print("Best-move feature dataset built successfully.")
    print(f"games_total={len(df)}")
    print(f"jsonl={output_jsonl_path}")
    print(f"csv={output_csv_path}")
    print(f"summary={summary_path}")


if __name__ == "__main__":
    main()