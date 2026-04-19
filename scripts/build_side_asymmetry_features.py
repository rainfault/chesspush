from __future__ import annotations

import json
import math
import statistics
from pathlib import Path
from typing import Optional

import pandas as pd


COMPLEXITY_CLIP = 1000
CRITICAL_QUANTILE = 0.8


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


def safe_diff(a: Optional[float], b: Optional[float]) -> Optional[float]:
    if a is None or b is None:
        return None
    return a - b


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


def safe_ratio(num: int, den: int) -> Optional[float]:
    if den == 0:
        return None
    return num / den


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


def side_metrics(side_rows: list[dict]) -> dict[str, Optional[float]]:
    time_complex_rows = [
        r for r in side_rows
        if r.get("move_time_sec") is not None and r.get("complexity_cp") is not None
    ]
    x_time = [float(r["move_time_sec"]) for r in time_complex_rows]
    y_complex = [transform_complexity(r["complexity_cp"]) for r in time_complex_rows]
    y_complex = [v for v in y_complex if v is not None]

    # нужно синхронизировать после transform
    pairs = [
        (float(r["move_time_sec"]), transform_complexity(r["complexity_cp"]))
        for r in time_complex_rows
        if transform_complexity(r["complexity_cp"]) is not None
    ]
    time_complexity_corr = pearson_corr(
        [p[0] for p in pairs],
        [p[1] for p in pairs],
    )

    cpl_vals = [
        float(r["cpl_cp"])
        for r in side_rows
        if r.get("cpl_cp") is not None
    ]
    cpl_mean = safe_mean(cpl_vals)

    nonmate_rows = [r for r in side_rows if not r.get("mate_zone", False)]
    best_hits = 0
    best_total = 0

    complexity_vals = [
        transform_complexity(r.get("complexity_cp"))
        for r in nonmate_rows
        if r.get("complexity_cp") is not None
    ]
    complexity_vals = [v for v in complexity_vals if v is not None]
    critical_threshold = quantile_threshold(complexity_vals, CRITICAL_QUANTILE)

    crit_best_hits = 0
    crit_total = 0
    crit_times = []
    noncrit_times = []

    for r in nonmate_rows:
        move_uci = r.get("move_uci")
        best_move_uci = r.get("best_move_uci")
        move_time = r.get("move_time_sec")
        complexity = transform_complexity(r.get("complexity_cp"))

        if move_uci is not None and best_move_uci is not None:
            best_total += 1
            if move_uci == best_move_uci:
                best_hits += 1

        if critical_threshold is not None and complexity is not None and move_time is not None:
            if complexity >= critical_threshold:
                crit_total += 1
                crit_times.append(float(move_time))
                if move_uci is not None and best_move_uci is not None and move_uci == best_move_uci:
                    crit_best_hits += 1
            else:
                noncrit_times.append(float(move_time))

    best_move_ratio = safe_ratio(best_hits, best_total)

    crit_mean = safe_mean(crit_times)
    noncrit_mean = safe_mean(noncrit_times)
    critical_time_lift = None
    if crit_mean is not None and noncrit_mean not in (None, 0):
        critical_time_lift = crit_mean / noncrit_mean

    return {
        "cpl_mean": cpl_mean,
        "best_move_ratio": best_move_ratio,
        "time_complexity_corr": time_complexity_corr,
        "critical_time_lift": critical_time_lift,
    }


def build_feature_row(game_rows: list[dict]) -> dict:
    first = game_rows[0]

    white_rows = [r for r in game_rows if r.get("side_to_move") == "white"]
    black_rows = [r for r in game_rows if r.get("side_to_move") == "black"]

    w = side_metrics(white_rows)
    b = side_metrics(black_rows)

    return {
        "game_id": first.get("game_id"),
        "label": first.get("label"),
        "source_class": first.get("source_class"),
        "source_username": first.get("source_username"),

        "white_cpl_mean": w["cpl_mean"],
        "black_cpl_mean": b["cpl_mean"],
        "wb_cpl_mean_diff": safe_diff(w["cpl_mean"], b["cpl_mean"]),

        "white_best_move_ratio": w["best_move_ratio"],
        "black_best_move_ratio": b["best_move_ratio"],
        "wb_best_move_ratio_diff": safe_diff(w["best_move_ratio"], b["best_move_ratio"]),

        "white_time_complexity_corr": w["time_complexity_corr"],
        "black_time_complexity_corr": b["time_complexity_corr"],
        "wb_time_complexity_corr_diff": safe_diff(w["time_complexity_corr"], b["time_complexity_corr"]),

        "white_critical_time_lift": w["critical_time_lift"],
        "black_critical_time_lift": b["critical_time_lift"],
        "wb_critical_time_lift_diff": safe_diff(w["critical_time_lift"], b["critical_time_lift"]),
    }


def main() -> None:
    input_path = "data/engine/engine_move_level_300.jsonl"
    output_jsonl_path = "data/engine/side_asymmetry_features_300.jsonl"
    output_csv_path = "data/engine/side_asymmetry_features_300.csv"
    summary_path = "data/engine/side_asymmetry_features_300_summary.json"

    rows = load_jsonl(input_path)
    grouped = group_rows_by_game(rows)

    out_rows = []
    for _, game_rows in grouped.items():
        out_rows.append(build_feature_row(game_rows))

    out_rows.sort(key=lambda r: r["game_id"])
    save_jsonl(output_jsonl_path, out_rows)

    df = pd.DataFrame(out_rows)
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

    print("Side-asymmetry feature dataset built successfully.")
    print(f"games_total={len(df)}")
    print(f"csv={output_csv_path}")
    print(f"summary={summary_path}")


if __name__ == "__main__":
    main()