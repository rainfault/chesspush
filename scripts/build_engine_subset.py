from __future__ import annotations

import json
import random
from pathlib import Path


RANDOM_STATE = 42


def load_jsonl(path: str) -> list[dict]:
    rows = []
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


def main() -> None:
    random.seed(RANDOM_STATE)

    input_path = "data/processed/labeled_dataset_600_0.jsonl"
    output_path = "data/engine/engine_input_subset_300.jsonl"

    rows = load_jsonl(input_path)

    humans = [r for r in rows if r.get("label") == 0]
    bots = [r for r in rows if r.get("label") == 1]

    human_sample = random.sample(humans, 150)
    bot_sample = random.sample(bots, 150)

    subset = human_sample + bot_sample
    random.shuffle(subset)

    save_jsonl(output_path, subset)

    print(f"Saved subset: {len(subset)} rows")
    print(f"Human: {len(human_sample)}, Bot: {len(bot_sample)}")


if __name__ == "__main__":
    main()