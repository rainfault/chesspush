from __future__ import annotations

from datetime import datetime
from io import StringIO
from pathlib import Path
import re

from PySide6.QtCore import QObject, Property, QThread, Signal, Slot

from app.app_context import AppContext
from app.paths import normalize_path
from backend.exporters.pgn_exporter import export_pgn
from backend.filters.game_filters import GameFilter
from backend.search.model_game_search import iter_matching_games_from_path
from backend.search.opening_catalog import builtin_opening_choices, opening_book, scan_database_openings
from controllers.base import BaseController

try:
    import chess
    import chess.pgn
except Exception:  # pragma: no cover
    chess = None


from controllers.extraction_worker import DatabaseExtractionWorker


class DatabaseLoaderController(BaseController):
    outputPathChanged = Signal()
    openingChoicesChanged = Signal()
    openingFamiliesChanged = Signal()
    openingVariantsChanged = Signal()
    openingEcoOptionsChanged = Signal()
    boardChanged = Signal()
    extractionStateChanged = Signal()
    extractionProgressChanged = Signal()
    lastGamesChanged = Signal()
    positionLabRequested = Signal(object)

    def __init__(self, context: AppContext):
        super().__init__()
        self.context = context
        self._book = opening_book()
        self._output_path = ""
        self._last_games = []
        self._opening_choices = builtin_opening_choices()
        self._opening_families = self._book.search_families("", limit=120)
        self._opening_variants: list[dict] = []
        self._opening_eco_options: list[dict] = []
        self._selected_family_key = ""
        self._selected_eco = ""
        self._selected_square = ""
        self._board = chess.Board() if chess is not None else None
        self._board_squares: list[dict] = []
        self._board_matches: list[dict] = []
        self._board_pgn = ""
        self._board_fen = ""
        self._is_extracting = False
        self._progress_percent = 0
        self._scanned_games = 0
        self._found_games = 0
        self._start_index = 0
        self._worker_thread: QThread | None = None
        self._worker: DatabaseExtractionWorker | None = None
        self._refresh_board_state()

    @Property(str, notify=outputPathChanged)
    def outputPath(self):
        return self._output_path

    @Property("QVariantList", notify=openingChoicesChanged)
    def openingChoices(self):
        return self._opening_choices

    @Property("QVariantList", notify=openingFamiliesChanged)
    def openingFamilies(self):
        return self._opening_families

    @Property("QVariantList", notify=openingVariantsChanged)
    def openingVariants(self):
        return self._opening_variants

    @Property("QVariantList", notify=openingEcoOptionsChanged)
    def openingEcoOptions(self):
        return self._opening_eco_options

    @Property("QVariantList", notify=boardChanged)
    def boardSquares(self):
        return self._board_squares

    @Property("QVariantList", notify=boardChanged)
    def boardMatches(self):
        return self._board_matches

    @Property(str, notify=boardChanged)
    def boardPgn(self):
        return self._board_pgn

    @Property(str, notify=boardChanged)
    def boardFen(self):
        return self._board_fen

    @Property(bool, notify=extractionStateChanged)
    def isExtracting(self):
        return self._is_extracting

    @Property(int, notify=extractionProgressChanged)
    def progressPercent(self):
        return self._progress_percent

    @Property("qlonglong", notify=extractionProgressChanged)
    def scannedGames(self):
        return self._scanned_games

    @Property(int, notify=extractionProgressChanged)
    def foundGames(self):
        return self._found_games

    @Property(bool, notify=lastGamesChanged)
    def canOpenLastInPositionLab(self):
        return bool(self._last_games)

    def _set_output_path(self, value: str):
        self._output_path = value
        self.outputPathChanged.emit()

    def _set_opening_choices(self, choices: list[dict]) -> None:
        seen = set()
        merged = []
        for choice in choices:
            key = choice.get("key") or choice.get("label")
            if key in seen:
                continue
            seen.add(key)
            merged.append(choice)
        self._opening_choices = merged
        self.openingChoicesChanged.emit()

    def _set_opening_families(self, rows: list[dict]) -> None:
        self._opening_families = rows
        self.openingFamiliesChanged.emit()

    def _set_opening_variants(self, rows: list[dict]) -> None:
        self._opening_variants = rows
        self.openingVariantsChanged.emit()

    def _set_opening_eco_options(self, rows: list[dict]) -> None:
        self._opening_eco_options = rows
        self.openingEcoOptionsChanged.emit()

    def _refresh_variants(self, query: str = "") -> None:
        if not self._selected_family_key:
            self._set_opening_variants([])
            return
        self._set_opening_variants(
            self._book.variants_for_family(
                self._selected_family_key,
                eco=self._selected_eco,
                query=query,
            )
        )

    def _select_opening_family_row(self, family: dict) -> None:
        self._selected_family_key = str(family.get("key", ""))
        self._selected_eco = ""
        self._set_opening_eco_options(self._book.ecos_for_family(self._selected_family_key))
        self._refresh_variants()

    @Slot(str, result=str)
    def pathFromUrl(self, value):
        return normalize_path(value)

    @Slot(str)
    def rememberDatabasePath(self, databasePath):
        database_path = normalize_path(databasePath)
        if not database_path:
            return
        self.context.update_settings(zst_database_path=database_path)
        self.set_status(f"Database path saved: {database_path}")

    @Slot(str, int)
    def scanOpeningChoices(self, databasePath, maxGames):
        database_path = normalize_path(databasePath) or self.context.settings.zst_database_path
        if not database_path:
            self.set_status("Select a PGN/ZST database first.")
            return
        if not Path(database_path).exists():
            self.set_status(f"Database not found: {database_path}")
            return
        try:
            scan_limit = max(500, int(maxGames) if int(maxGames) > 0 else 5000)
            scanned_choices = scan_database_openings(
                database_path,
                max_games=scan_limit,
                exclude_bullet=self.context.settings.exclude_bullet,
            )
            self._set_opening_choices(builtin_opening_choices() + scanned_choices)
            self.set_status(
                f"Opening list updated: {len(scanned_choices)} database openings from up to {scan_limit} games."
            )
        except Exception as exc:
            self.set_status(f"Opening scan failed: {exc}")

    @Slot(int, str, result=str)
    def openingChoiceValue(self, index, field):
        if index < 0 or index >= len(self._opening_choices):
            return ""
        return str(self._opening_choices[index].get(field, ""))

    @Slot(str)
    def searchOpeningFamilies(self, query):
        rows = self._book.search_families(query, limit=120)
        self._set_opening_families(rows)
        self.set_status(f"Opening search: {len(rows)} families")

    @Slot(int)
    def selectOpeningFamily(self, index):
        if index < 0 or index >= len(self._opening_families):
            return
        family = self._opening_families[index]
        self._select_opening_family_row(family)
        self.set_status(f"Selected opening family: {family.get('family', family.get('label', ''))}")

    @Slot(int)
    def selectOpeningEco(self, index):
        if index < 0 or index >= len(self._opening_eco_options):
            return
        self._selected_eco = str(self._opening_eco_options[index].get("eco", ""))
        self._refresh_variants()

    @Slot(str)
    def searchOpeningVariants(self, query):
        self._refresh_variants(query)

    @Slot(int, str, result=str)
    def openingVariantValue(self, index, field):
        if index < 0 or index >= len(self._opening_variants):
            return ""
        return str(self._opening_variants[index].get(field, ""))

    @Slot(int, str, result=str)
    def openingFamilyValue(self, index, field):
        if index < 0 or index >= len(self._opening_families):
            return ""
        return str(self._opening_families[index].get(field, ""))

    @Slot(str, str, str)
    def syncOpeningSelection(self, eco, openingContains, movePrefix):
        query = (openingContains or "").strip()
        if not query and eco:
            query = eco
        families = self._book.search_families(query, limit=120)
        if not families:
            self._set_opening_families([])
            self._set_opening_eco_options([])
            self._set_opening_variants([])
            return

        selected_index = 0
        normalized_opening = query.lower()
        for index, family in enumerate(families):
            if str(family.get("family", "")).lower() == normalized_opening:
                selected_index = index
                break

        self._set_opening_families(families)
        self._select_opening_family_row(families[selected_index])

        eco = (eco or "").strip().upper()
        if eco:
            for index, option in enumerate(self._opening_eco_options):
                if str(option.get("eco", "")).upper() == eco:
                    self._selected_eco = eco
                    self._set_opening_variants(self._book.variants_for_family(self._selected_family_key, eco=eco))
                    break

        prefix = (movePrefix or "").strip()
        if prefix:
            matching_variants = [
                row for row in self._opening_variants
                if str(row.get("move_prefix", "")).strip() == prefix
            ]
            if matching_variants:
                remaining = [
                    row for row in self._opening_variants
                    if str(row.get("move_prefix", "")).strip() != prefix
                ]
                self._set_opening_variants(matching_variants + remaining)

    @Slot()
    def resetBoardOpening(self):
        if chess is None:
            self.set_status("python-chess is not installed; board selection is unavailable.")
            return
        self._board = chess.Board()
        self._selected_square = ""
        self._refresh_board_state()

    @Slot()
    def undoBoardOpeningMove(self):
        if self._board is None or not self._board.move_stack:
            return
        self._board.pop()
        self._selected_square = ""
        self._refresh_board_state()

    @Slot(str)
    def clickBoardSquare(self, squareName):
        if chess is None or self._board is None:
            self.set_status("python-chess is not installed; board selection is unavailable.")
            return
        try:
            square = chess.parse_square(squareName)
        except ValueError:
            return

        piece = self._board.piece_at(square)
        if not self._selected_square:
            if piece and piece.color == self._board.turn:
                self._selected_square = squareName
                self._refresh_board_state()
            return

        from_square = chess.parse_square(self._selected_square)
        move = chess.Move(from_square, square)
        moving_piece = self._board.piece_at(from_square)
        if moving_piece and moving_piece.piece_type == chess.PAWN and chess.square_rank(square) in {0, 7}:
            move = chess.Move(from_square, square, promotion=chess.QUEEN)

        if move in self._board.legal_moves:
            self._board.push(move)
            self._selected_square = ""
            self._refresh_board_state()
            return

        if piece and piece.color == self._board.turn:
            self._selected_square = squareName
            self._refresh_board_state()
            return

        self._selected_square = ""
        self._refresh_board_state()

    @Slot(str, str)
    def moveBoardPiece(self, fromSquareName, toSquareName):
        if chess is None or self._board is None:
            self.set_status("python-chess is not installed; board selection is unavailable.")
            return
        try:
            from_square = chess.parse_square(fromSquareName)
            to_square = chess.parse_square(toSquareName)
        except ValueError:
            return
        if from_square == to_square:
            self.clickBoardSquare(fromSquareName)
            return

        moving_piece = self._board.piece_at(from_square)
        if not moving_piece or moving_piece.color != self._board.turn:
            return

        move = chess.Move(from_square, to_square)
        if moving_piece.piece_type == chess.PAWN and chess.square_rank(to_square) in {0, 7}:
            move = chess.Move(from_square, to_square, promotion=chess.QUEEN)

        if move in self._board.legal_moves:
            self._board.push(move)
        self._selected_square = ""
        self._refresh_board_state()

    @Slot(str)
    def setBoardFromPgn(self, pgnPrefix):
        if chess is None:
            self.set_status("python-chess is not installed; board selection is unavailable.")
            return
        self._board = chess.Board()
        self._selected_square = ""
        text = (pgnPrefix or "").strip()
        if not text:
            self._refresh_board_state()
            return

        game = chess.pgn.read_game(StringIO(text + " *"))
        if game is None:
            self._refresh_board_state()
            return
        try:
            for move in game.mainline_moves():
                if move in self._board.legal_moves:
                    self._board.push(move)
                else:
                    break
        except Exception:
            pass
        self._refresh_board_state()

    @Slot(int, str, result=str)
    def boardMatchValue(self, index, field):
        if index < 0 or index >= len(self._board_matches):
            return ""
        return str(self._board_matches[index].get(field, ""))

    def _refresh_board_state(self) -> None:
        if chess is None or self._board is None:
            self._board_squares = []
            self._board_matches = []
            self._board_pgn = ""
            self._board_fen = ""
            self.boardChanged.emit()
            return

        legal_targets = set()
        if self._selected_square:
            try:
                selected = chess.parse_square(self._selected_square)
                legal_targets = {chess.square_name(move.to_square) for move in self._board.legal_moves if move.from_square == selected}
            except ValueError:
                legal_targets = set()

        squares = []
        for rank in range(7, -1, -1):
            for file_index in range(8):
                square = chess.square(file_index, rank)
                name = chess.square_name(square)
                piece = self._board.piece_at(square)
                piece_asset = ""
                if piece:
                    piece_asset = f"{'w' if piece.color == chess.WHITE else 'b'}{piece.symbol().upper()}.svg"
                squares.append(
                    {
                        "name": name,
                        "piece": piece.unicode_symbol() if piece else "",
                        "piece_asset": piece_asset,
                        "turn_piece": bool(piece and piece.color == self._board.turn),
                        "dark": (rank + file_index) % 2 == 1,
                        "selected": name == self._selected_square,
                        "target": name in legal_targets,
                    }
                )

        self._board_squares = squares
        self._board_fen = self._board.fen()
        self._board_pgn = self._current_board_pgn()
        self._board_matches = self._book.match_line(self._board_pgn, limit=80)
        self.boardChanged.emit()

    def _current_board_pgn(self) -> str:
        if chess is None or self._board is None or not self._board.move_stack:
            return ""
        game = chess.pgn.Game.from_board(self._board)
        exporter = chess.pgn.StringExporter(headers=False, variations=False, comments=False)
        text = game.accept(exporter).strip()
        return text.removesuffix("*").strip()

    @Slot(str, str, str, int, int, int, int, str, str, str, str, str, int, str, bool, bool)
    def extractGames(
        self,
        databasePath,
        color,
        resultMode,
        minWhiteElo,
        maxWhiteElo,
        minBlackElo,
        maxBlackElo,
        eco,
        openingContains,
        movePrefix,
        perfType,
        startIndex,
        limit,
        outputFolder,
        excludeBullet,
        includeTimeout,
    ):
        if self._is_extracting:
            self.set_status("Извлечение уже выполняется.")
            return
        database_path = normalize_path(databasePath) or self.context.settings.zst_database_path
        output_folder = normalize_path(outputFolder) or self.context.settings.export_folder
        if not database_path:
            self.set_status("Не указан путь к базе PGN/ZST")
            return
        if not Path(database_path).exists():
            self.set_status(f"База не найдена: {database_path}")
            return
        try:
            start_index = int(str(startIndex).strip() or "0")
            if start_index < 0:
                raise ValueError("Стартовый индекс не может быть отрицательным")
            flt = GameFilter(
                color=color or "any",
                result_mode=resultMode or "all",
                min_white_elo=int(minWhiteElo),
                max_white_elo=int(maxWhiteElo),
                min_black_elo=int(minBlackElo),
                max_black_elo=int(maxBlackElo),
                min_both_elo=min(int(minWhiteElo), int(minBlackElo)),
                eco="" if str(movePrefix).strip() else eco,
                opening_contains=openingContains,
                move_prefix=movePrefix,
                perf_type=perfType or "any",
                min_moves=self.context.settings.default_min_moves,
                exclude_bullet=bool(excludeBullet),
                include_timeout=bool(includeTimeout),
                username=self.context.settings.lichess_username,
            )
            self._start_extraction_worker(
                database_path=database_path,
                output_folder=output_folder,
                game_filter=flt,
                start_index=start_index,
                limit=max(1, int(limit)),
            )
        except Exception as exc:
            self.set_status(f"Ошибка извлечения: {exc}")

    @Slot()
    def cancelExtraction(self):
        if self._worker is not None:
            self._worker.cancel()
            self.set_status(
                f"Останавливаю извлечение... текущий индекс: {self._scanned_games}, найдено: {self._found_games}"
            )

    def _start_extraction_worker(
        self,
        *,
        database_path: str,
        output_folder: str,
        game_filter: GameFilter,
        start_index: int,
        limit: int,
    ) -> None:
        self._is_extracting = True
        self._progress_percent = 0
        self._scanned_games = 0
        self._found_games = 0
        self._start_index = start_index
        self._last_games = []
        self.lastGamesChanged.emit()
        self.extractionStateChanged.emit()
        self.extractionProgressChanged.emit()
        self.set_rows([])
        self._set_output_path("")
        if start_index:
            self.set_status(
                f"Извлечение запущено. ZST читается с начала; пропускаю первые {start_index} партий..."
            )
        else:
            self.set_status("Извлечение запущено с начала базы...")

        path = Path(database_path)
        compressed_size = path.stat().st_size if path.exists() else 0
        self._worker_thread = QThread(self)
        self._worker = DatabaseExtractionWorker(
            database_path=database_path,
            output_folder=output_folder,
            base_dir=self.context.base_dir,
            game_filter=game_filter,
            start_index=start_index,
            limit=limit,
            compressed_size=compressed_size,
        )
        self._worker.moveToThread(self._worker_thread)
        self._worker_thread.started.connect(self._worker.run)
        self._worker.progress.connect(self._on_extraction_progress)
        self._worker.finished.connect(self._on_extraction_finished)
        self._worker.failed.connect(self._on_extraction_failed)
        self._worker.finished.connect(self._worker_thread.quit)
        self._worker.failed.connect(self._worker_thread.quit)
        self._worker.finished.connect(self._worker.deleteLater)
        self._worker.failed.connect(self._worker.deleteLater)
        self._worker_thread.finished.connect(self._cleanup_worker)
        self._worker_thread.finished.connect(self._worker_thread.deleteLater)
        self._worker_thread.start()

    @Slot("qlonglong", int, int)
    def _on_extraction_progress(self, scanned: int, found: int, percent: int) -> None:
        self._scanned_games = scanned
        self._found_games = found
        self._progress_percent = max(0, min(100, int(percent)))
        self.extractionProgressChanged.emit()
        if scanned <= self._start_index:
            self.set_status(
                f"Проматываю ZST: текущий индекс {scanned} из {self._start_index}; найдено {found}"
            )
        else:
            self.set_status(f"Идёт извлечение: текущий индекс {scanned}, найдено {found}")

    @Slot(object, str, str, "qlonglong", int)
    def _on_extraction_finished(self, games, output_path: str, status: str, scanned: int, found: int) -> None:
        self._last_games = list(games)
        self.lastGamesChanged.emit()
        self.set_rows([g.to_dict() for g in self._last_games])
        self._set_output_path(output_path)
        self._scanned_games = scanned
        self._found_games = found
        self._progress_percent = 100
        self.set_status(status)
        self.extractionProgressChanged.emit()

    @Slot(str)
    def _on_extraction_failed(self, message: str) -> None:
        self._last_games = []
        self.lastGamesChanged.emit()
        self.set_status(f"Ошибка извлечения: {message}")

    @Slot()
    def _cleanup_worker(self) -> None:
        self._worker = None
        self._worker_thread = None
        self._is_extracting = False
        self.extractionStateChanged.emit()

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

    @Slot(str)
    def exportLast(self, path):
        path = normalize_path(path)
        if not path:
            self.set_status("Не указан путь экспорта")
            return
        try:
            export_pgn(self._last_games, path)
            self._set_output_path(path)
            self.set_status(f"Экспортировано партий: {len(self._last_games)}")
        except Exception as exc:
            self.set_status(f"Ошибка экспорта: {exc}")

    @Slot()
    def openLastInPositionLab(self) -> None:
        if not self._last_games:
            self.set_status("Сначала собери выборку модельных партий.")
            return
        self.positionLabRequested.emit(
            {
                "games": list(self._last_games),
                "source_kind": "zst_model",
                "source_label": f"ZST Parser · {len(self._last_games)} модельных партий",
                "username": self.context.settings.lichess_username,
                "filters": {},
            }
        )
