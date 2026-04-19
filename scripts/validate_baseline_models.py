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

FEATURES_WITH_ELO = [
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

FEATURES_NO_ELO = [
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


def run_stratified_cv(
    df: pd.DataFrame,
    feature_names: list[str],
    n_splits: int = 5,
) -> dict:
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


def run_leave_one_bot_out(
    df: pd.DataFrame,
    feature_names: list[str],
) -> dict:
    """
    Для каждого bot source_username:
    - test: все партии этого бота + такое же число human партий
    - train: всё остальное
    """
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

            # Берём human для теста детерминированно
            test_human = human_df.sample(n=n_bot, random_state=RANDOM_STATE)

            test_df = pd.concat([test_bot, test_human], axis=0).sample(
                frac=1.0, random_state=RANDOM_STATE
            )

            train_df = df.drop(index=test_df.index.intersection(df.index), errors="ignore").copy()

            # Чтобы в train точно не осталось тестируемого бота
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
    input_path = Path("data/features/feature_dataset_baseline.csv")
    output_path = Path("data/models/baseline_validation_results.json")
    output_path.parent.mkdir(parents=True, exist_ok=True)

    if not input_path.exists():
        raise FileNotFoundError(f"Dataset not found: {input_path}")

    df = pd.read_csv(input_path)

    required_columns = {"label", "source_username"}
    missing = required_columns - set(df.columns)
    if missing:
        raise ValueError(f"Missing required columns: {sorted(missing)}")

    results = {
        "dataset_size": int(len(df)),
        "class_counts": {
            str(k): int(v) for k, v in df["label"].value_counts().to_dict().items()
        },
        "experiments": {
            "with_elo": {
                "features": FEATURES_WITH_ELO,
                "stratified_5fold_cv": run_stratified_cv(df, FEATURES_WITH_ELO, n_splits=5),
                "leave_one_bot_out": run_leave_one_bot_out(df, FEATURES_WITH_ELO),
            },
            "without_elo": {
                "features": FEATURES_NO_ELO,
                "stratified_5fold_cv": run_stratified_cv(df, FEATURES_NO_ELO, n_splits=5),
                "leave_one_bot_out": run_leave_one_bot_out(df, FEATURES_NO_ELO),
            },
        },
    }

    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    print(f"Saved validation results to: {output_path}")

    for setup_name, setup_data in results["experiments"].items():
        print("\n" + "=" * 72)
        print(setup_name.upper())

        print("\nStratified 5-fold CV")
        for model_name, model_data in setup_data["stratified_5fold_cv"].items():
            s = model_data["summary"]
            print(
                f"{model_name}: "
                f"acc={s['accuracy_mean']:.4f}±{s['accuracy_std']:.4f}, "
                f"prec={s['precision_mean']:.4f}±{s['precision_std']:.4f}, "
                f"rec={s['recall_mean']:.4f}±{s['recall_std']:.4f}, "
                f"f1={s['f1_mean']:.4f}±{s['f1_std']:.4f}"
            )

        print("\nLeave-one-bot-out")
        for model_name, model_data in setup_data["leave_one_bot_out"].items():
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