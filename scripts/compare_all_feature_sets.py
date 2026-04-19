from __future__ import annotations

import json
from pathlib import Path

import numpy as np
import pandas as pd
from sklearn.base import clone
from sklearn.ensemble import RandomForestClassifier
from sklearn.impute import SimpleImputer
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score, precision_score, recall_score, f1_score
from sklearn.model_selection import StratifiedKFold
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler


RANDOM_STATE = 42

BASELINE_FEATURES = [
    "white_elo",
    "black_elo",
    "elo_diff_abs",
    "ply_count",
    "move_count_total",
    "avg_move_time",
    "median_move_time",
    "std_move_time",
    "max_move_time",
    "cv_move_time",
    "fast_move_ratio",
    "slow_move_ratio",
    "white_move_count",
    "white_avg_move_time",
    "white_median_move_time",
    "white_std_move_time",
    "white_fast_move_ratio",
    "white_slow_move_ratio",
    "black_move_count",
    "black_avg_move_time",
    "black_median_move_time",
    "black_std_move_time",
    "black_fast_move_ratio",
    "black_slow_move_ratio",
    "wb_avg_time_diff",
    "wb_move_count_diff",
]

COGNITIVE_FEATURES = [
    "time_complexity_corr",
    "critical_time_lift",
    "cpl_mean",
    "cpl_median",
    "cpl_std",
    "post_error_time_ratio",
]

PHASE_FEATURES = [
    "early_avg_time",
    "middle_avg_time",
    "late_avg_time",
    "early_cpl_mean",
    "middle_cpl_mean",
    "late_cpl_mean",
    "early_cpl_std",
    "middle_cpl_std",
    "late_cpl_std",
    "time_early_to_middle_diff",
    "time_middle_to_late_diff",
    "cpl_early_to_middle_diff",
    "cpl_middle_to_late_diff",
]


def make_models() -> dict[str, Pipeline]:
    logistic = Pipeline(
        steps=[
            ("imputer", SimpleImputer(strategy="median")),
            ("scaler", StandardScaler()),
            ("model", LogisticRegression(max_iter=2000, random_state=RANDOM_STATE)),
        ]
    )

    rf = Pipeline(
        steps=[
            ("imputer", SimpleImputer(strategy="median")),
            ("model", RandomForestClassifier(
                n_estimators=300,
                random_state=RANDOM_STATE,
                n_jobs=-1,
            )),
        ]
    )

    return {
        "LogisticRegression": logistic,
        "RandomForestClassifier": rf,
    }


def metric_dict(y_true, y_pred) -> dict[str, float]:
    return {
        "accuracy": float(accuracy_score(y_true, y_pred)),
        "precision": float(precision_score(y_true, y_pred, zero_division=0)),
        "recall": float(recall_score(y_true, y_pred, zero_division=0)),
        "f1": float(f1_score(y_true, y_pred, zero_division=0)),
    }


def summarize_metric_list(metric_rows: list[dict[str, float]]) -> dict[str, float]:
    keys = metric_rows[0].keys()
    summary: dict[str, float] = {}
    for key in keys:
        values = np.array([row[key] for row in metric_rows], dtype=float)
        summary[f"{key}_mean"] = float(values.mean())
        summary[f"{key}_std"] = float(values.std())
    return summary


def run_stratified_cv(df: pd.DataFrame, feature_names: list[str], n_splits: int = 5) -> dict:
    x = df[feature_names].copy()
    y = df["label"].copy()

    skf = StratifiedKFold(n_splits=n_splits, shuffle=True, random_state=RANDOM_STATE)
    models = make_models()

    results: dict[str, dict] = {}

    for model_name, model in models.items():
        fold_metrics: list[dict[str, float]] = []

        for train_idx, test_idx in skf.split(x, y):
            x_train = x.iloc[train_idx]
            x_test = x.iloc[test_idx]
            y_train = y.iloc[train_idx]
            y_test = y.iloc[test_idx]

            current_model = clone(model)
            current_model.fit(x_train, y_train)
            y_pred = current_model.predict(x_test)

            fold_metrics.append(metric_dict(y_test, y_pred))

        results[model_name] = {
            "fold_metrics": fold_metrics,
            "summary": summarize_metric_list(fold_metrics),
        }

    return results


def run_leave_one_bot_out(df: pd.DataFrame, feature_names: list[str]) -> dict:
    models = make_models()
    results: dict[str, dict] = {}

    bot_df = df[df["label"] == 1].copy()
    human_df = df[df["label"] == 0].copy()

    bot_names = sorted(bot_df["source_username"].dropna().unique().tolist())

    for model_name, model in models.items():
        split_results = []

        for bot_name in bot_names:
            test_bot = bot_df[bot_df["source_username"] == bot_name].copy()
            n_bot = len(test_bot)
            if n_bot == 0:
                continue

            test_human = human_df.sample(n=n_bot, random_state=RANDOM_STATE)

            test_df = pd.concat([test_bot, test_human], axis=0).sample(
                frac=1.0, random_state=RANDOM_STATE
            )

            train_df = df.drop(index=test_df.index.intersection(df.index), errors="ignore").copy()
            train_df = train_df[~(
                (train_df["label"] == 1) &
                (train_df["source_username"] == bot_name)
            )]

            x_train = train_df[feature_names].copy()
            y_train = train_df["label"].copy()
            x_test = test_df[feature_names].copy()
            y_test = test_df["label"].copy()

            current_model = clone(model)
            current_model.fit(x_train, y_train)
            y_pred = current_model.predict(x_test)

            metrics = metric_dict(y_test, y_pred)
            metrics["bot_name"] = bot_name
            metrics["test_size"] = int(len(test_df))
            split_results.append(metrics)

        if split_results:
            numeric_rows = [
                {k: v for k, v in row.items() if isinstance(v, (int, float)) and k != "test_size"}
                for row in split_results
            ]
            results[model_name] = {
                "per_bot_results": split_results,
                "summary": summarize_metric_list(numeric_rows),
            }

    return results


def main() -> None:
    baseline_path = Path("data/features/feature_dataset_baseline.csv")
    cognitive_path = Path("data/engine/cognitive_features_120.csv")
    phase_path = Path("data/engine/phase_features_120.csv")
    output_path = Path("data/models/all_feature_set_comparison.json")
    output_path.parent.mkdir(parents=True, exist_ok=True)

    if not baseline_path.exists():
        raise FileNotFoundError(f"Baseline dataset not found: {baseline_path}")
    if not cognitive_path.exists():
        raise FileNotFoundError(f"Cognitive dataset not found: {cognitive_path}")
    if not phase_path.exists():
        raise FileNotFoundError(f"Phase dataset not found: {phase_path}")

    baseline_df = pd.read_csv(baseline_path)
    cognitive_df = pd.read_csv(cognitive_path)
    phase_df = pd.read_csv(phase_path)

    game_ids = set(cognitive_df["game_id"].astype(str).tolist())
    baseline_subset = baseline_df[baseline_df["game_id"].astype(str).isin(game_ids)].copy()

    merged = baseline_subset.merge(
        cognitive_df,
        on=["game_id", "label", "source_class", "source_username"],
        how="inner",
        suffixes=("_base", "_cog"),
    ).merge(
        phase_df,
        on=["game_id", "label", "source_class", "source_username"],
        how="inner",
        suffixes=("", "_phase"),
    )

    baseline_only_features = [f for f in BASELINE_FEATURES if f in merged.columns]
    baseline_plus_cognitive_features = baseline_only_features + [
        f for f in COGNITIVE_FEATURES if f in merged.columns
    ]
    all_features = baseline_plus_cognitive_features + [
        f for f in PHASE_FEATURES if f in merged.columns
    ]

    experiments = {
        "baseline_only": baseline_only_features,
        "baseline_plus_cognitive": baseline_plus_cognitive_features,
        "baseline_plus_cognitive_plus_phase": all_features,
    }

    results = {
        "merged_dataset_size": int(len(merged)),
        "class_counts": {
            str(k): int(v) for k, v in merged["label"].value_counts().to_dict().items()
        },
        "feature_sets": experiments,
        "experiments": {},
    }

    for exp_name, feature_names in experiments.items():
        results["experiments"][exp_name] = {
            "stratified_5fold_cv": run_stratified_cv(merged, feature_names, n_splits=5),
            "leave_one_bot_out": run_leave_one_bot_out(merged, feature_names),
        }

    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    print(f"Saved results to: {output_path}")

    for exp_name, exp_data in results["experiments"].items():
        print("\n" + "=" * 72)
        print(exp_name.upper())

        print("\nStratified 5-fold CV")
        for model_name, model_data in exp_data["stratified_5fold_cv"].items():
            s = model_data["summary"]
            print(
                f"{model_name}: "
                f"acc={s['accuracy_mean']:.4f}±{s['accuracy_std']:.4f}, "
                f"prec={s['precision_mean']:.4f}±{s['precision_std']:.4f}, "
                f"rec={s['recall_mean']:.4f}±{s['recall_std']:.4f}, "
                f"f1={s['f1_mean']:.4f}±{s['f1_std']:.4f}"
            )

        print("\nLeave-one-bot-out")
        for model_name, model_data in exp_data["leave_one_bot_out"].items():
            s = model_data["summary"]
            print(
                f"{model_name}: "
                f"acc={s['accuracy_mean']:.4f}±{s['accuracy_std']:.4f}, "
                f"prec={s['precision_mean']:.4f}±{s['precision_std']:.4f}, "
                f"rec={s['recall_mean']:.4f}±{s['recall_std']:.4f}, "
                f"f1={s['f1_mean']:.4f}±{s['f1_std']:.4f}"
            )


if __name__ == "__main__":
    main()