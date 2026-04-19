from __future__ import annotations

import json
from pathlib import Path

import pandas as pd
from sklearn.ensemble import RandomForestClassifier
from sklearn.impute import SimpleImputer
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score, f1_score, precision_score, recall_score, classification_report
from sklearn.model_selection import train_test_split
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


def evaluate_model(name: str, model, x_train, x_test, y_train, y_test) -> dict:
    model.fit(x_train, y_train)
    y_pred = model.predict(x_test)

    metrics = {
        "model": name,
        "accuracy": float(accuracy_score(y_test, y_pred)),
        "precision": float(precision_score(y_test, y_pred, zero_division=0)),
        "recall": float(recall_score(y_test, y_pred, zero_division=0)),
        "f1": float(f1_score(y_test, y_pred, zero_division=0)),
        "classification_report": classification_report(y_test, y_pred, zero_division=0),
    }
    return metrics


def main() -> None:
    input_path = Path("data/features/feature_dataset_baseline.csv")
    output_dir = Path("data/models")
    output_dir.mkdir(parents=True, exist_ok=True)

    if not input_path.exists():
        raise FileNotFoundError(f"Dataset not found: {input_path}")

    df = pd.read_csv(input_path)

    missing_features = [col for col in BASELINE_FEATURES if col not in df.columns]
    if missing_features:
        raise ValueError(f"Missing feature columns: {missing_features}")

    if "label" not in df.columns:
        raise ValueError("Target column 'label' not found")

    x = df[BASELINE_FEATURES].copy()
    y = df["label"].copy()

    x_train, x_test, y_train, y_test = train_test_split(
        x,
        y,
        test_size=0.2,
        random_state=RANDOM_STATE,
        stratify=y,
    )

    logistic_model = Pipeline(
        steps=[
            ("imputer", SimpleImputer(strategy="median")),
            ("scaler", StandardScaler()),
            ("model", LogisticRegression(max_iter=2000, random_state=RANDOM_STATE)),
        ]
    )

    rf_model = Pipeline(
        steps=[
            ("imputer", SimpleImputer(strategy="median")),
            ("model", RandomForestClassifier(
                n_estimators=300,
                max_depth=None,
                min_samples_split=2,
                min_samples_leaf=1,
                random_state=RANDOM_STATE,
                n_jobs=-1,
            )),
        ]
    )

    results = []
    results.append(
        evaluate_model(
            name="LogisticRegression",
            model=logistic_model,
            x_train=x_train,
            x_test=x_test,
            y_train=y_train,
            y_test=y_test,
        )
    )
    results.append(
        evaluate_model(
            name="RandomForestClassifier",
            model=rf_model,
            x_train=x_train,
            x_test=x_test,
            y_train=y_train,
            y_test=y_test,
        )
    )

    for result in results:
        print("=" * 60)
        print(result["model"])
        print(f'Accuracy : {result["accuracy"]:.4f}')
        print(f'Precision: {result["precision"]:.4f}')
        print(f'Recall   : {result["recall"]:.4f}')
        print(f'F1-score : {result["f1"]:.4f}')
        print()
        print(result["classification_report"])

    results_path = output_dir / "baseline_results.json"
    with open(results_path, "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)

    print(f"Saved results to: {results_path}")


if __name__ == "__main__":
    main()