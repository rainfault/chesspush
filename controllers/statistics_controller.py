from __future__ import annotations

from pathlib import Path
import urllib.error

from PySide6.QtCore import QObject, Property, QThread, Signal, Slot

from app.app_context import AppContext
from app.paths import normalize_path
from backend.lichess_api import download_user_games_pgn, normalize_max_games
from backend.parsers.pgn_parser import parse_pgn_file, parse_pgn_text
from backend.position_lab import player_color
from backend.statistics.eco_winrate import build_eco_winrate_report
from controllers.base import BaseController


class LichessDownloadWorker(QObject):
    finished = Signal(object, str)
    failed = Signal(str)

    def __init__(self, *, username: str, token: str, max_games: int):
        super().__init__()
        self.username = username
        self.token = token
        self.max_games = normalize_max_games(max_games)
        self._cancelled = False

    @Slot()
    def run(self) -> None:
        try:
            body, url = download_user_games_pgn(
                self.username,
                token=self.token,
                max_games=self.max_games,
            )
            if self._cancelled:
                self.failed.emit("Загрузка с Lichess остановлена.")
                return
            games = parse_pgn_text(body, source=url)
            self.finished.emit(
                games,
                f"Lichess вернул {len(games)} из последних {self.max_games} партий.",
            )
        except urllib.error.URLError as exc:
            self.failed.emit(f"Нет доступа к серверам Lichess: {exc}")
        except Exception as exc:
            self.failed.emit(f"Ошибка загрузки Lichess: {exc}")

    @Slot()
    def cancel(self) -> None:
        self._cancelled = True


class StatisticsController(BaseController):
    busyChanged = Signal()
    sourceChanged = Signal()
    positionLabRequested = Signal(object)

    USER_SOURCE_KINDS = ("lichess", "statistics_pgn", "legacy")

    def __init__(self, context: AppContext):
        super().__init__()
        self.context = context
        self._busy = False
        self._source = ""
        self._current_games = []
        self._loaded_username = ""
        self._worker_thread: QThread | None = None
        self._worker: LichessDownloadWorker | None = None
        self._pending_username = ""
        self._set_initial_status()

    @Property(bool, notify=busyChanged)
    def busy(self) -> bool:
        return self._busy

    @Property(str, notify=sourceChanged)
    def source(self) -> str:
        return self._source

    @Slot(str, result=str)
    def pathFromUrl(self, value) -> str:
        return normalize_path(value)

    @Slot()
    def refresh(self) -> None:
        username = self._username()
        if not username:
            self._clear_current()
            self.set_status("Укажи Lichess username в настройках.")
            return
        if self._loaded_username and username.casefold() != self._loaded_username.casefold():
            self._clear_current()
        if self._current_games and username.casefold() == self._loaded_username.casefold():
            self._apply_games(self._current_games, self._source, username)
            return
        games = self._local_user_games(username)
        if not games:
            self._set_initial_status()
            return
        self._current_games = games
        self._loaded_username = username
        self._apply_games(
            games,
            f"Локальная библиотека: {len(games)} партий",
            username,
        )

    @Slot(str)
    def loadPgn(self, path: str) -> None:
        if self._busy:
            self.set_status("Дождитесь завершения загрузки с Lichess.")
            return
        username = self._username()
        if not username:
            self.set_status("Укажи Lichess username в настройках перед загрузкой PGN.")
            return

        normalized_path = normalize_path(path)
        if not normalized_path:
            self.set_status("Выбери PGN-файл.")
            return
        pgn_path = Path(normalized_path)
        if not pgn_path.exists():
            self.set_status(f"Файл не найден: {pgn_path}")
            return
        if pgn_path.suffix.lower() != ".pgn":
            self.set_status("Для статистики выбери файл с расширением .pgn.")
            return

        try:
            games = parse_pgn_file(pgn_path)
            user_games = self._games_for_username(games, username)
            inserted = self.context.storage.upsert_lab_games(
                user_games,
                source_kind="statistics_pgn",
            )
            local_games = self._local_user_games(username)
            source = f"Локальная библиотека: {len(local_games)} партий"
            self._apply_games(
                local_games,
                source,
                username,
                prefix=(
                    f"PGN {pgn_path.name}: найдено партий игрока {len(user_games)}, "
                    f"добавлено новых {inserted}."
                ),
            )
            self._current_games = local_games
            self._loaded_username = username
        except Exception as exc:
            self.set_status(f"Ошибка загрузки PGN: {exc}")

    @Slot(int)
    def fetchLatestGames(self, maxGames: int) -> None:
        if self._busy:
            self.set_status("Загрузка с Lichess уже выполняется.")
            return
        username = self._username()
        if not username:
            self.set_status("Укажи Lichess username в настройках перед загрузкой.")
            return

        max_games = normalize_max_games(maxGames)
        self._busy = True
        self.busyChanged.emit()
        self._pending_username = username
        self.set_status(f"Загружаю последние {max_games} партий игрока {username}...")

        self._worker_thread = QThread(self)
        self._worker = LichessDownloadWorker(
            username=username,
            token=self.context.settings.lichess_api_token,
            max_games=max_games,
        )
        self._worker.moveToThread(self._worker_thread)
        self._worker_thread.started.connect(self._worker.run)
        self._worker.finished.connect(self._online_finished)
        self._worker.failed.connect(self._online_failed)
        self._worker.finished.connect(self._worker_thread.quit)
        self._worker.failed.connect(self._worker_thread.quit)
        self._worker.finished.connect(self._worker.deleteLater)
        self._worker.failed.connect(self._worker.deleteLater)
        self._worker_thread.finished.connect(self._cleanup_worker)
        self._worker_thread.finished.connect(self._worker_thread.deleteLater)
        self._worker_thread.start()

    @Slot(object, str)
    def _online_finished(self, games: list, message: str) -> None:
        current_username = self._username()
        if current_username.casefold() != self._pending_username.casefold():
            self._clear_current()
            self.set_status("Username изменён во время загрузки. Повтори запрос с Lichess.")
            return

        try:
            user_games = self._games_for_username(list(games), self._pending_username)
            inserted = self.context.storage.upsert_lab_games(
                user_games,
                source_kind="lichess",
            )
            self._current_games = self._local_user_games(self._pending_username)
            self._loaded_username = self._pending_username
            source = f"Локальная библиотека: {len(self._current_games)} партий"
            self._apply_games(
                self._current_games,
                source,
                self._pending_username,
                prefix=f"{message} Новых партий сохранено: {inserted}.",
            )
        except Exception as exc:
            self.set_status(f"Партии получены, но не сохранены локально: {exc}")

    @Slot(str, str)
    def openMyGames(self, eco: str, color: str) -> None:
        username = self._username()
        if not username:
            self.set_status("Укажи Lichess username в настройках.")
            return
        games = self._current_games or self._local_user_games(username)
        normalized_eco = eco.strip().upper()
        normalized_color = color if color in {"white", "black"} else "any"
        matching = [
            game
            for game in games
            if (not normalized_eco or game.eco.strip().upper() == normalized_eco)
            and player_color(game, username) != "unknown"
            and (
                normalized_color == "any"
                or player_color(game, username) == normalized_color
            )
        ]
        if not matching:
            color_label = {
                "white": "белыми",
                "black": "чёрными",
            }.get(normalized_color, "за оба цвета")
            self.set_status(
                f"Для ECO {normalized_eco or '???'} не найдено локальных партий {color_label}."
            )
            return
        self.positionLabRequested.emit(
            {
                "games": matching,
                "source_kind": "statistics",
                "source_label": f"Мои партии · {normalized_eco or 'без ECO'}",
                "username": username,
                "filters": {
                    "eco": normalized_eco,
                    "color": normalized_color,
                },
            }
        )

    @Slot(str)
    def _online_failed(self, message: str) -> None:
        self.set_status(message)

    @Slot()
    def _cleanup_worker(self) -> None:
        self._worker = None
        self._worker_thread = None
        self._pending_username = ""
        self._busy = False
        self.busyChanged.emit()

    @Slot()
    def shutdown(self) -> None:
        worker = self._worker
        thread = self._worker_thread
        if worker is not None:
            try:
                worker.cancel()
            except RuntimeError:
                pass
        if thread is not None and thread.isRunning():
            thread.quit()
            thread.wait()

    def _apply_games(
        self,
        games: list,
        source: str,
        username: str,
        *,
        prefix: str = "",
    ) -> None:
        report = build_eco_winrate_report(games, username)
        self.set_rows(report.rows)
        self._set_source(source)

        if not games:
            detail = "В источнике нет партий."
        elif not report.analyzed_games:
            detail = f"В источнике не найдено завершённых партий игрока {username}."
        else:
            detail = (
                f"Проанализировано: {report.analyzed_games}. "
                f"ECO в таблице: {len(report.rows)}. Пропущено: {report.skipped_games}."
            )
        self.set_status(" ".join(part for part in (prefix, detail) if part))

    def _username(self) -> str:
        return self.context.settings.lichess_username.strip()

    def _local_user_games(self, username: str) -> list:
        games = self.context.storage.list_lab_games(
            limit=None,
            source_kinds=self.USER_SOURCE_KINDS,
        )
        return self._games_for_username(games, username)

    @staticmethod
    def _games_for_username(games: list, username: str) -> list:
        return [game for game in games if player_color(game, username) != "unknown"]

    def _set_source(self, value: str) -> None:
        if self._source == value:
            return
        self._source = value
        self.sourceChanged.emit()

    def _clear_current(self) -> None:
        self._current_games = []
        self._loaded_username = ""
        self._set_source("")
        self.set_rows([])

    def _set_initial_status(self) -> None:
        if self._username():
            self.set_status("Загрузите последние N партий с Lichess или выберите PGN-файл.")
        else:
            self.set_status("Укажи Lichess username в настройках.")
