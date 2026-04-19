from __future__ import annotations

import json
import statistics
from pathlib import Path
from typing import Optional

import pandas as pd


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


def safe_mean(values: list[float]) -> Optional[float]:
    return float(statistics.mean(values)) if values else None


def safe_std(values: list[float]) -> Optional[float]:
    if not values:
        return None
    if len(values) < 2:
        return 0.0
    return float(statistics.pstdev(values))


def safe_diff(a: Optional[float], b: Optional[float]) -> Optional[float]:
    if a is None or b is None:
        return None
    return a - b


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


def split_into_three_phases(game_rows: list[dict]) -> tuple[list[dict], list[dict], list[dict]]:
    n = len(game_rows)
    if n == 0:
        return [], [], []

    cut1 = n // 3
    cut2 = (2 * n) // 3

    early = game_rows[:cut1]
    middle = game_rows[cut1:cut2]
    late = game_rows[cut2:]

    return early, middle, late


def phase_stats(rows: list[dict]) -> dict[str, Optional[float]]:
    time_vals = [
        float(r["move_time_sec"])
        for r in rows
        if r.get("move_time_sec") is not None
    ]
    cpl_vals = [
        float(r["cpl_cp"])
        for r in rows
        if r.get("cpl_cp") is not None
    ]

    return {
        "avg_time": safe_mean(time_vals),
        "cpl_mean": safe_mean(cpl_vals),
        "cpl_std": safe_std(cpl_vals),
        "move_count": len(rows),
    }


def build_phase_feature_row(game_rows: list[dict]) -> dict:
    first = game_rows[0]

    early, middle, late = split_into_three_phases(game_rows)

    e = phase_stats(early)
    m = phase_stats(middle)
    l = phase_stats(late)

    return {
        "game_id": first.get("game_id"),
        "label": first.get("label"),
        "source_class": first.get("source_class"),
        "source_username": first.get("source_username"),

        "early_move_count": e["move_count"],
        "middle_move_count": m["move_count"],
        "late_move_count": l["move_count"],

        "early_avg_time": e["avg_time"],
        "middle_avg_time": m["avg_time"],
        "late_avg_time": l["avg_time"],

        "early_cpl_mean": e["cpl_mean"],
        "middle_cpl_mean": m["cpl_mean"],
        "late_cpl_mean": l["cpl_mean"],

        "early_cpl_std": e["cpl_std"],
        "middle_cpl_std": m["cpl_std"],
        "late_cpl_std": l["cpl_std"],

        "time_early_to_middle_diff": safe_diff(m["avg_time"], e["avg_time"]),
        "time_middle_to_late_diff": safe_diff(l["avg_time"], m["avg_time"]),

        "cpl_early_to_middle_diff": safe_diff(m["cpl_mean"], e["cpl_mean"]),
        "cpl_middle_to_late_diff": safe_diff(l["cpl_mean"], m["cpl_mean"]),
    }


def main() -> None:
    input_path = "data/engine/engine_move_level_120.jsonl"
    output_jsonl_path = "data/engine/phase_features_120.jsonl"
    output_csv_path = "data/engine/phase_features_120.csv"
    summary_path = "data/engine/phase_features_120_summary.json"

    rows = load_jsonl(input_path)
    grouped = group_rows_by_game(rows)

    phase_rows = []
    for game_id, game_rows in grouped.items():
        phase_rows.append(build_phase_feature_row(game_rows))

    phase_rows.sort(key=lambda r: r["game_id"])

    save_jsonl(output_jsonl_path, phase_rows)

    df = pd.DataFrame(phase_rows)
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

    print("Phase feature dataset built successfully.")
    print(f"games_total={len(df)}")
    print(f"jsonl={output_jsonl_path}")
    print(f"csv={output_csv_path}")
    print(f"summary={summary_path}")


if __name__ == "__main__":
    main()