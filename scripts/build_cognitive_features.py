from __future__ import annotations

import json
import math
import statistics
from pathlib import Path
from typing import Optional



import pandas as pd


COMPLEXITY_CLIP = 1000
ERROR_THRESHOLD_CP = 100
CRITICAL_QUANTILE = 0.8

def safe_median(values: list[float]) -> Optional[float]:
    return float(statistics.median(values)) if values else None

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


def pearson_corr(x: list[float], y: list[float]) -> Optional[float]:
    if len(x) != len(y) or len(x) < 2:
        return None

    mean_x = sum(x) / len(x)
    mean_y = sum(y) / len(y)

    num = sum((a - mean_x) * (b - mean_y) for a, b in zip(x, y))
    den_x = math.sqrt(sum((a - mean_x) ** 2 for a in x))
    den_y = math.sqrt(sum((b - mean_y) ** 2 for b in y))

    if den_x == 0 or den_y == 0:
        return None

    return num / (den_x * den_y)


def quantile_threshold(values: list[float], q: float) -> Optional[float]:
    if not values:
        return None

    sorted_vals = sorted(values)
    if len(sorted_vals) == 1:
        return sorted_vals[0]

    idx = q * (len(sorted_vals) - 1)
    lo = int(math.floor(idx))
    hi = int(math.ceil(idx))

    if lo == hi:
        return sorted_vals[lo]

    frac = idx - lo
    return sorted_vals[lo] * (1 - frac) + sorted_vals[hi] * frac


def transform_complexity(value: Optional[float]) -> Optional[float]:
    """
    Робастная версия сложности:
    - сначала мягкий клиппинг,
    - затем log1p, чтобы очень большие значения не доминировали.
    """
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


def build_game_level_features(game_rows: list[dict]) -> dict:
    first = game_rows[0]

    valid_rows = []
    for row in game_rows:
        move_time = row.get("move_time_sec")
        complexity = transform_complexity(row.get("complexity_cp"))
        cpl = row.get("cpl_cp")
        mate_zone = bool(row.get("mate_zone"))

        valid_rows.append({
            "move_time_sec": move_time,
            "complexity_cp": complexity,
            "cpl_cp": cpl,
            "mate_zone": mate_zone,
            "ply_index": row.get("ply_index"),
        })

    # ---- 1. time_complexity_corr ----
    corr_rows = [
        r for r in valid_rows
        if r["move_time_sec"] is not None and r["complexity_cp"] is not None
    ]
    time_vals = [float(r["move_time_sec"]) for r in corr_rows]
    complexity_vals = [float(r["complexity_cp"]) for r in corr_rows]
    time_complexity_corr = pearson_corr(time_vals, complexity_vals)

    # ---- 2. critical_time_lift ----
    critical_threshold = quantile_threshold(complexity_vals, CRITICAL_QUANTILE)
    critical_positions_count = 0
    critical_time_lift = None

    if critical_threshold is not None and corr_rows:
        critical_times = []
        noncritical_times = []

        for r in corr_rows:
            if r["complexity_cp"] >= critical_threshold:
                critical_times.append(float(r["move_time_sec"]))
                critical_positions_count += 1
            else:
                noncritical_times.append(float(r["move_time_sec"]))

        crit_mean = safe_mean(critical_times)
        noncrit_mean = safe_mean(noncritical_times)

        if crit_mean is not None and noncrit_mean not in (None, 0):
            critical_time_lift = crit_mean / noncrit_mean

    # ---- 3-4. cpl_mean / cpl_std ----
    cpl_vals = [
    float(r["cpl_cp"])
    for r in valid_rows
    if r["cpl_cp"] is not None
    ]
    cpl_mean = safe_mean(cpl_vals)
    cpl_median = safe_median(cpl_vals)
    cpl_std = safe_std(cpl_vals)

    # ---- 5. post_error_time_ratio ----
    error_rows = [
        r for r in valid_rows
        if r["cpl_cp"] is not None and r["move_time_sec"] is not None
    ]

    first_error_index = None
    for r in error_rows:
        if r["cpl_cp"] >= ERROR_THRESHOLD_CP:
            first_error_index = r["ply_index"]
            break

    post_error_time_ratio = None
    error_moves_count = sum(1 for r in error_rows if r["cpl_cp"] >= ERROR_THRESHOLD_CP)

    if first_error_index is not None:
        before_times = [
            float(r["move_time_sec"])
            for r in error_rows
            if r["ply_index"] < first_error_index
        ]
        after_times = [
            float(r["move_time_sec"])
            for r in error_rows
            if r["ply_index"] > first_error_index
        ]

        # Требуем минимум 3 хода до и 3 хода после,
        # иначе признак слишком нестабилен
        if len(before_times) >= 3 and len(after_times) >= 3:
            before_mean = safe_mean(before_times)
            after_mean = safe_mean(after_times)

            if before_mean not in (None, 0) and after_mean is not None:
                post_error_time_ratio = after_mean / before_mean

    return {
        "game_id": first.get("game_id"),
        "label": first.get("label"),
        "source_class": first.get("source_class"),
        "source_username": first.get("source_username"),

        "engine_move_count": len(valid_rows),
        "critical_positions_count": critical_positions_count,
        "error_moves_count": error_moves_count,

        "time_complexity_corr": time_complexity_corr,
        "critical_time_lift": critical_time_lift,
        "cpl_mean": cpl_mean,
        "cpl_median": cpl_median,
        "cpl_std": cpl_std,
        "post_error_time_ratio": post_error_time_ratio,
    }


def main() -> None:
    input_path = "data/engine/engine_move_level_300.jsonl"
    output_jsonl_path = "data/engine/cognitive_features_300.jsonl"
    output_csv_path = "data/engine/cognitive_features_300.csv"
    summary_path = "data/engine/cognitive_features_300_summary.json"

    rows = load_jsonl(input_path)
    grouped = group_rows_by_game(rows)

    game_level_rows = []
    for game_id, game_rows in grouped.items():
        game_level_rows.append(build_game_level_features(game_rows))

    game_level_rows.sort(key=lambda r: r["game_id"])

    save_jsonl(output_jsonl_path, game_level_rows)

    df = pd.DataFrame(game_level_rows)
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

    print("Cognitive feature dataset built successfully.")
    print(f"games_total={len(df)}")
    print(f"jsonl={output_jsonl_path}")
    print(f"csv={output_csv_path}")
    print(f"summary={summary_path}")


if __name__ == "__main__":
    main()