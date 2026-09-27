from __future__ import annotations

from dataclasses import dataclass, asdict
from pathlib import Path
import json


@dataclass
class AppSettings:
    lichess_username: str = ""
    lichess_api_token: str = ""
    zst_database_path: str = ""
    export_folder: str = "outputs"
    local_storage_path: str = "data/chess_training_tool.sqlite3"
    default_min_rating: int = 2300
    default_max_rating: int = 4000
    default_min_moves: int = 25
    exclude_bullet: bool = True
    default_limit: int = 50
    theme: str = "light"


class SettingsStore:
    def __init__(self, settings_path: Path):
        self.settings_path = settings_path

    def load(self) -> AppSettings:
        if not self.settings_path.exists():
            return AppSettings()
        try:
            data = json.loads(self.settings_path.read_text(encoding="utf-8"))
            base = asdict(AppSettings())
            base.update({k: v for k, v in data.items() if k in base})
            base['theme'] = 'dark' if base['theme'] == 'dark' else 'light'
            return AppSettings(**base)
        except Exception:
            return AppSettings()

    def save(self, settings: AppSettings) -> None:
        self.settings_path.parent.mkdir(parents=True, exist_ok=True)
        self.settings_path.write_text(
            json.dumps(asdict(settings), ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
