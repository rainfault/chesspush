from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Property, Signal, Slot

from app.app_context import AppContext
from app.paths import normalize_path
from backend.exporters.pgn_exporter import export_pgn
from backend.filters.game_filters import GameFilter
from backend.search.fen_tools import is_python_chess_available
from backend.search.model_game_search import ModelGameSearch
from controllers.base import BaseController


class ModelFinderController(BaseController):
    engineInfoChanged = Signal()

    def __init__(self, context: AppContext):
        super().__init__()
        self.context = context
        self._last_games = []

    @Property(str, notify=engineInfoChanged)
    def engineInfo(self):
        return "python-chess доступен" if is_python_chess_available() else "python-chess не установлен: FEN-поиск ограничен"

    @Slot(str, str, str, str, int, int, str, int, int)
    def searchModelGames(
        self,
        databasePath,
        fen,
        eco,
        openingContains,
        minRating,
        maxRating,
        perfType,
        limit,
        scanLimit,
    ):
        database_path = normalize_path(databasePath) or self.context.settings.zst_database_path
        if not database_path:
            self.set_status("Не указан путь к базе")
            return
        if not Path(database_path).exists():
            self.set_status(f"Файл не найден: {database_path}")
            return
        flt = GameFilter(
            min_white_elo=int(minRating),
            min_black_elo=int(minRating),
            max_white_elo=int(maxRating),
            max_black_elo=int(maxRating),
            min_both_elo=int(minRating),
            eco=eco,
            opening_contains=openingContains,
            perf_type=perfType or "any",
            min_moves=self.context.settings.default_min_moves,
            exclude_bullet=self.context.settings.exclude_bullet,
            include_timeout=False,
        )
        try:
            searcher = ModelGameSearch(database_path)
            games = searcher.search(
                game_filter=flt,
                fen=fen,
                limit=int(limit),
                scan_limit=int(scanLimit) if int(scanLimit) > 0 else None,
            )
            self._last_games = games
            self.set_rows([g.to_dict() for g in games])
            self.set_status(f"Найдено модельных партий: {len(games)}")
        except Exception as exc:
            self.set_status(f"Ошибка поиска: {exc}")

    @Slot(str)
    def exportLast(self, path):
        path = normalize_path(path)
        if not path:
            out_dir = Path(self.context.settings.export_folder)
            if not out_dir.is_absolute():
                out_dir = self.context.base_dir / out_dir
            path = str(out_dir / "model_finder_export.pgn")
        try:
            export_pgn(self._last_games, path)
            self.set_status(f"Экспортировано в PGN: {path}")
        except Exception as exc:
            self.set_status(f"Ошибка экспорта: {exc}")
