from __future__ import annotations

import csv
from pathlib import Path

from backend.entities import GameRecord


def export_csv(games: list[GameRecord], path: str | Path) -> Path:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    rows = [g.to_dict() for g in games]
    fields = list(rows[0].keys()) if rows else ["game_id"]
    with path.open("w", encoding="utf-8", newline="") as fh:
        writer = csv.DictWriter(fh, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)
    return path
