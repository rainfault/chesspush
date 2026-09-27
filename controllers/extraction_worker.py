from datetime import datetime
from pathlib import Path
import re
from PySide6.QtCore import QObject, Signal, Slot
from backend.filters.game_filters import GameFilter
from backend.exporters.pgn_exporter import export_pgn
from backend.search.model_game_search import iter_matching_games_from_path


class DatabaseExtractionWorker(QObject):
    progress = Signal("qlonglong", int, int)
    finished = Signal(object, str, str, "qlonglong", int)
    failed = Signal(str)

    def __init__(
        self,
        *,
        database_path: str,
        output_folder: str,
        base_dir: Path,
        game_filter: GameFilter,
        start_index: int,
        limit: int,
        compressed_size: int,
        library=None,
    ):
        super().__init__()
        self.database_path = database_path
        self.output_folder = output_folder
        self.base_dir = base_dir
        self.game_filter = game_filter
        self.start_index = start_index
        self.limit = limit
        self.compressed_size = compressed_size
        self._cancelled = False
        self.library = library

    @Slot()
    def run(self):
        try:
            found = []
            scanned = 0

            def report_progress(scanned_games: int) -> None:
                nonlocal scanned
                scanned = scanned_games
                self.progress.emit(scanned_games, len(found), self._progress_percent())

            for scanned, game in iter_matching_games_from_path(
                self.database_path,
                self.game_filter,
                start_index=self.start_index,
                progress_callback=report_progress,
                should_stop=lambda: self._cancelled,
            ):
                found.append(game)
                self.progress.emit(scanned, len(found), self._progress_percent())
                if self._cancelled or len(found) >= self.limit:
                    break

            out_dir = Path(self.output_folder)
            if not out_dir.is_absolute():
                out_dir = self.base_dir / out_dir
            out_path = self._output_path(out_dir) if found else None
            export_warning = ""
            if out_path is not None:
                try:
                    export_pgn(found, out_path)
                except Exception as exc:
                    export_warning = f" Экспорт PGN не выполнен: {exc}"
                    out_path = None

            if out_path is not None:
                status = (
                    f"Готово. Стартовый индекс: {self.start_index}. "
                    f"Текущий индекс: {scanned}. Найдено: {len(found)}. PGN: {out_path}"
                )
            elif found:
                status = (
                    f"Готово. Стартовый индекс: {self.start_index}. "
                    f"Текущий индекс: {scanned}. Найдено: {len(found)}. PGN не создан."
                )
            else:
                status = (
                    f"Готово. Стартовый индекс: {self.start_index}. "
                    f"Текущий индекс: {scanned}. Найдено: 0. PGN не создан."
                )
            if self._cancelled:
                if out_path is not None:
                    status = (
                        f"Остановлено. Стартовый индекс: {self.start_index}. "
                        f"Текущий индекс: {scanned}. Найдено: {len(found)}. PGN: {out_path}"
                    )
                elif found:
                    status = (
                        f"Остановлено. Стартовый индекс: {self.start_index}. "
                        f"Текущий индекс: {scanned}. Найдено: {len(found)}. PGN не создан."
                    )
                else:
                    status = (
                        f"Остановлено. Стартовый индекс: {self.start_index}. "
                        f"Текущий индекс: {scanned}. Найдено: 0. PGN не создан."
                    )
            status += export_warning
            if self.library is not None:
                self.library.save_games(found, self.database_path, str(out_path or ''))
            self.finished.emit(found, str(out_path or ""), status, scanned, len(found))
        except Exception as exc:
            self.failed.emit(str(exc))

    @Slot()
    def cancel(self):
        self._cancelled = True

    def _progress_percent(self) -> int:
        if self.compressed_size <= 0:
            return 0
        try:
            current = Path(self.database_path).stat().st_size
            if current <= 0:
                return 0
        except OSError:
            return 0
        # The streaming decompressor may buffer aggressively, so exact byte progress is
        # unavailable here; keep the bar indeterminate for compressed databases.
        return 0

    def _output_path(self, out_dir: Path) -> Path:
        out_dir.mkdir(parents=True, exist_ok=True)
        parts = ["model_games"]
        opening = self._safe_file_part(self.game_filter.opening_contains)
        eco = self._safe_file_part(self.game_filter.eco)
        perf = self._safe_file_part(self.game_filter.perf_type if self.game_filter.perf_type != "any" else "")
        if opening:
            parts.append(opening)
        elif eco:
            parts.append(eco)
        if perf:
            parts.append(perf)
        if self.start_index:
            parts.append(f"from_{self.start_index + 1}")
        parts.append(datetime.now().strftime("%Y%m%d_%H%M%S"))

        stem = "_".join(parts)
        path = out_dir / f"{stem}.pgn"
        index = 2
        while path.exists():
            path = out_dir / f"{stem}_{index}.pgn"
            index += 1
        return path

    @staticmethod
    def _safe_file_part(value: str) -> str:
        value = re.sub(r"[^a-zA-Z0-9]+", "_", value.strip().lower())
        return value.strip("_")[:40]
