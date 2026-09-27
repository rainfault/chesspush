from __future__ import annotations

from PySide6.QtCore import Property, Signal, Slot

from app.app_context import AppContext
from backend.statistics.opening_stats import opening_statistics
from controllers.base import BaseController


class DashboardController(BaseController):
    recommendationChanged = Signal()

    def __init__(self, context: AppContext):
        super().__init__()
        self.context = context
        self._recommendation = "Импортируй партии и обнови статистику."
        self.refresh()

    @Property(str, notify=recommendationChanged)
    def recommendation(self):
        return self._recommendation

    def _set_recommendation(self, value: str):
        self._recommendation = value
        self.recommendationChanged.emit()

    @Slot()
    def refresh(self):
        info = self.context.storage.storage_info()
        games = self.context.storage.list_games(limit=5000)
        stats = opening_statistics(games, username=self.context.settings.lichess_username)
        rows = [
            {"title": "Lichess", "value": self.context.settings.lichess_username or "не задан"},
            {"title": "Локальные партии", "value": str(info["games"])},
            {"title": "Training Cards", "value": str(info["cards"])},
            {"title": "База ZST", "value": "задана" if self.context.settings.zst_database_path else "не задана"},
        ]
        self.set_rows(rows)
        if stats:
            top = stats[0].to_dict()
            self._set_recommendation(
                f"Сегодня выгоднее всего разобрать: {top['opening']} ({top['color']}). "
                f"Партий: {top['games']}, score: {top['score']}, PainIndex: {top['pain_index']}."
            )
        else:
            self._set_recommendation("Импортируй 40–80 своих rapid/blitz партий, затем открой статистику.")
        self.set_status("Dashboard обновлён")
