from __future__ import annotations

import json
import random
from collections import Counter
from pathlib import Path
from typing import Iterable


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


def save_jsonl(path: str, rows: Iterable[dict]) -> None:
    file_path = Path(path)
    file_path.parent.mkdir(parents=True, exist_ok=True)

    with open(file_path, "w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")


def build_summary(rows: list[dict]) -> dict:
    class_counts = Counter()
    time_controls = Counter()
    source_users = Counter()

    for row in rows:
        label = row.get("label")
        class_counts[str(label)] += 1

        tc = row.get("time_control") or "UNKNOWN"
        time_controls[tc] += 1

        if row.get("source_class") == "bot":
            source_users[row.get("source_username") or "UNKNOWN_BOT"] += 1

    return {
        "dataset_size": len(rows),
        "class_counts": dict(class_counts),
        "time_controls": dict(time_controls),
        "bot_source_distribution": dict(source_users),
    }


def build_labeled_dataset(
    human_path: str,
    bot_path: str,
    output_path: str,
    summary_path: str,
    required_time_control: str = "600+0",
    random_seed: int = 42,
) -> None:
    random.seed(random_seed)

    human_rows = load_jsonl(human_path)
    bot_rows = load_jsonl(bot_path)

    # Берём только нужный time control
    human_rows = [r for r in human_rows if (r.get("time_control") or "").strip() == required_time_control]
    bot_rows = [r for r in bot_rows if (r.get("time_control") or "").strip() == required_time_control]

    # Добавляем source_class для human, если его ещё нет
    for row in human_rows:
        row["source_class"] = "human"
        row["label"] = 0

    for row in bot_rows:
        row["label"] = 1

    # Балансировка по меньшему классу
    target_size = min(len(human_rows), len(bot_rows))

    if target_size == 0:
        raise ValueError("After filtering there are no records to build labeled dataset.")

    human_sample = random.sample(human_rows, target_size)
    bot_sample = random.sample(bot_rows, target_size)

    dataset = human_sample + bot_sample
    random.shuffle(dataset)

    save_jsonl(output_path, dataset)

    summary = build_summary(dataset)
    summary["required_time_control"] = required_time_control
    summary["human_candidates_before_sampling"] = len(human_rows)
    summary["bot_candidates_before_sampling"] = len(bot_rows)
    summary["sampled_per_class"] = target_size

    summary_file = Path(summary_path)
    summary_file.parent.mkdir(parents=True, exist_ok=True)
    with open(summary_file, "w", encoding="utf-8") as f:
        json.dump(summary, f, ensure_ascii=False, indent=2)

    print("Labeled dataset built successfully.")
    print(f"required_time_control={required_time_control}")
    print(f"human_candidates={len(human_rows)}")
    print(f"bot_candidates={len(bot_rows)}")
    print(f"sampled_per_class={target_size}")
    print(f"dataset_size={len(dataset)}")


if __name__ == "__main__":
    build_labeled_dataset(
        human_path="data/raw/games_raw_rapid_1800_2400.jsonl",
        bot_path="data/raw/bot_games_raw_rapid_1400_2800.jsonl",
        output_path="data/processed/labeled_dataset_600_0.jsonl",
        summary_path="data/processed/labeled_dataset_600_0_summary.json",
        required_time_control="600+0",
        random_seed=42,
    )