from __future__ import annotations

import urllib.parse
import urllib.request
from pathlib import Path

from PySide6.QtCore import Property, Signal, Slot

from app.app_context import AppContext
from app.paths import normalize_path
from backend.parsers.auto_loader import iter_games_from_path
from backend.parsers.ndjson_parser import parse_ndjson_file
from controllers.base import BaseController


class MyGamesController(BaseController):
    summaryChanged = Signal()

    def __init__(self, context: AppContext):
        super().__init__()
        self.context = context
        self._summary = ""
        self.refresh()

    @Property(str, notify=summaryChanged)
    def summary(self):
        return self._summary

    def _set_summary(self, text: str):
        self._summary = text
        self.summaryChanged.emit()

    @Slot()
    def refresh(self):
        games = self.context.storage.list_games(limit=500)
        self.set_rows([self._row(game) for game in games])
        self._set_summary(f"В локальной базе: {self.context.storage.count_games()} партий")

    @Slot(str)
    def importGames(self, path):
        path = normalize_path(path)
        if not path:
            self.set_status("Укажи путь к PGN/NDJSON/ZST файлу")
            return
        if not Path(path).exists():
            self.set_status(f"Файл не найден: {path}")
            return
        try:
            games = list(iter_games_from_path(path))
            inserted = self.context.storage.upsert_games(games)
            self.set_status(f"Импортировано новых записей: {inserted}. Прочитано партий: {len(games)}")
            self.refresh()
        except Exception as exc:
            self.set_status(f"Ошибка импорта: {exc}")

    @Slot(int, str)
    def downloadLichessGames(self, maxGames, perfType):
        username = self.context.settings.lichess_username.strip()
        if not username:
            self.set_status("В настройках не задан Lichess username")
            return
        perf = perfType.strip() or "rapid,blitz"
        query = urllib.parse.urlencode({
            "max": int(maxGames),
            "perfType": perf,
            "opening": "true",
            "clocks": "true",
            "evals": "false",
        })
        url = f"https://lichess.org/api/games/user/{urllib.parse.quote(username)}?{query}"
        out_dir = Path(self.context.settings.export_folder)
        if not out_dir.is_absolute():
            out_dir = self.context.base_dir / out_dir
        out_dir.mkdir(parents=True, exist_ok=True)
        out_path = out_dir / f"{username}_{perf.replace(',', '_')}_{maxGames}.ndjson"
        try:
            headers = {"Accept": "application/x-ndjson"}
            token = self.context.settings.lichess_api_token.strip()
            if token:
                headers["Authorization"] = f"Bearer {token}"
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req, timeout=60) as response:
                out_path.write_bytes(response.read())
            games = parse_ndjson_file(out_path)
            inserted = self.context.storage.upsert_games(games)
            self.set_status(f"Скачано: {out_path}. Новых партий: {inserted}")
            self.refresh()
        except Exception as exc:
            self.set_status(f"Ошибка загрузки Lichess: {exc}")

    def _row(self, game):
        d = game.to_dict()
        d["white"] = f"{game.white} ({game.white_elo})"
        d["black"] = f"{game.black} ({game.black_elo})"
        return d
