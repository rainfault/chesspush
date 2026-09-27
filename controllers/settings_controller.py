from __future__ import annotations

from PySide6.QtCore import Property, Signal, Slot

from app.app_context import AppContext
from app.paths import normalize_path
from controllers.base import BaseController


class SettingsController(BaseController):
    settingsChanged = Signal()

    def __init__(self, context: AppContext):
        super().__init__()
        self.context = context
        self.refresh()

    def _get(self, name: str):
        return getattr(self.context.settings, name)

    @Slot(str, str, str, result=bool)
    def saveProfile(self, username, token, theme):
        try:
            self.context.update_settings(lichess_username=username.strip(),
                lichess_api_token=token.strip(), theme='dark' if theme == 'dark' else 'light')
            self.settingsChanged.emit()
            self.set_status('')
            return True
        except OSError:
            self.set_status('Не удалось сохранить настройки')
            return False

    @Property(str, notify=settingsChanged)
    def lichessUsername(self):
        return self._get("lichess_username")

    @Property(str, notify=settingsChanged)
    def lichessApiToken(self):
        return self._get("lichess_api_token")

    @Property(str, notify=settingsChanged)
    def zstDatabasePath(self):
        return self._get("zst_database_path")

    @Property(str, notify=settingsChanged)
    def exportFolder(self):
        return self._get("export_folder")

    @Property(str, notify=settingsChanged)
    def localStoragePath(self):
        return self._get("local_storage_path")

    @Property(int, notify=settingsChanged)
    def defaultMinRating(self):
        return self._get("default_min_rating")

    @Property(int, notify=settingsChanged)
    def defaultMaxRating(self):
        return self._get("default_max_rating")

    @Property(int, notify=settingsChanged)
    def defaultMinMoves(self):
        return self._get("default_min_moves")

    @Property(bool, notify=settingsChanged)
    def excludeBullet(self):
        return self._get("exclude_bullet")

    @Property(int, notify=settingsChanged)
    def defaultLimit(self):
        return self._get("default_limit")

    @Property(str, notify=settingsChanged)
    def theme(self):
        return self._get("theme")

    @Slot(str, str, str, str, str, str, int, int, int, bool, int)
    def saveSettings(self, username, token, theme, zstPath, exportFolder, storagePath, minRating, maxRating, minMoves, excludeBullet, defaultLimit):
        self.context.update_settings(
            lichess_username=username.strip(),
            lichess_api_token=token.strip(),
            theme="lichess" if theme == "lichess" else "dark",
            zst_database_path=normalize_path(zstPath),
            export_folder=normalize_path(exportFolder) or "outputs",
            local_storage_path=normalize_path(storagePath) or "data/chess_training_tool.sqlite3",
            default_min_rating=int(minRating),
            default_max_rating=int(maxRating),
            default_min_moves=int(minMoves),
            exclude_bullet=bool(excludeBullet),
            default_limit=int(defaultLimit),
        )
        self.settingsChanged.emit()
        self.set_status("Настройки сохранены. Для смены SQLite-пути лучше перезапустить приложение.")

    @Slot()
    def refresh(self):
        info = self.context.storage.storage_info()
        self.set_rows([
            {"name": "Theme", "value": self.context.settings.theme},
            {"name": "Профиль Lichess", "value": self.context.settings.lichess_username},
            {"name": "Lichess token", "value": "задан" if self.context.settings.lichess_api_token else "не задан"},
            {"name": "Путь к базе", "value": self.context.settings.zst_database_path or "не задан"},
            {"name": "Экспорт", "value": self.context.settings.export_folder},
            {"name": "SQLite", "value": info["path"]},
            {"name": "Партий в локальном хранилище", "value": str(info["games"])},
            {"name": "Позиции в Position Lab", "value": str(info["positions"])},
            {"name": "Training cards", "value": str(info["cards"])},
        ])
