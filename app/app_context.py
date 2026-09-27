from __future__ import annotations

from dataclasses import replace
from pathlib import Path

from app.config import AppSettings, SettingsStore
from app.paths import ensure_folder
from backend.storage.sqlite_storage import SQLiteStorage


class AppContext:
    def __init__(self, base_dir: Path):
        self.base_dir = base_dir
        self.data_dir = ensure_folder(base_dir / "data")
        self.outputs_dir = ensure_folder(base_dir / "outputs")
        self.settings_store = SettingsStore(self.data_dir / "settings.json")
        self.settings: AppSettings = self.settings_store.load()

        # Resolve relative paths against the project root.
        storage_path = Path(self.settings.local_storage_path)
        if not storage_path.is_absolute():
            storage_path = base_dir / storage_path
        storage_path.parent.mkdir(parents=True, exist_ok=True)
        self.storage = SQLiteStorage(storage_path)
        self.storage.initialize()

    def save_settings(self) -> None:
        self.settings_store.save(self.settings)

    def update_settings(self, **kwargs) -> None:
        self.settings = replace(self.settings, **kwargs)
        self.save_settings()
