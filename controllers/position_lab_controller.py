from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import Property, Signal, Slot

from app.app_context import AppContext
from app.paths import normalize_path
from backend.entities import GameRecord
from backend.parsers.pgn_parser import parse_pgn_file
from backend.position_lab import (
    GameTimeline,
    annotation_color,
    board_squares,
    build_game_timeline,
    game_info,
    game_summary,
    move_label,
    normalize_arrows,
    normalize_highlights,
    parse_tags_text,
    player_color,
    side_to_move,
    valid_square,
)
from controllers.base import BaseController


class PositionLabController(BaseController):
    navigationRequested = Signal(str)
    modeChanged = Signal()
    gamesChanged = Signal()
    boardChanged = Signal()
    collectionChanged = Signal()
    selectedPositionChanged = Signal()
    trainingChanged = Signal()
    sourceChanged = Signal()

    def __init__(self, context: AppContext):
        super().__init__()
        self.context = context
        self._mode = "games"
        self._games: list[GameRecord] = []
        self._game_rows: list[dict] = []
        self._current_game_index = -1
        self._current_game_info: dict = {}
        self._timeline: GameTimeline | None = None
        self._move_rows: list[dict] = []
        self._current_ply = 0
        self._fen = ""
        self._flipped = False
        self._arrows: list[dict] = []
        self._highlights: list[dict] = []
        self._board_squares: list[dict] = []
        self._source_label = ""
        self._board_states: dict[str, dict] = {}
        self._positions: list[dict] = []
        self._selected_position: dict = {}
        self._selected_position_id = 0
        self._filters = {
            "search": "",
            "tags": [],
            "eco": "",
            "opening": "",
            "color": "",
            "source": "",
            "date_from": "",
            "date_to": "",
        }
        self._training_ids: list[int] = []
        self._training_index = -1
        self._training_position: dict = {}
        self._answer_visible = False
        self.refreshCollection()
        self.set_status("Открой локальные партии, PGN или выборку из Statistics / ZST Parser.")

    @Property(str, notify=modeChanged)
    def mode(self) -> str:
        return self._mode

    @Property("QVariantList", notify=gamesChanged)
    def gameRows(self):
        return self._game_rows

    @Property(int, notify=gamesChanged)
    def currentGameIndex(self) -> int:
        return self._current_game_index

    @Property("QVariantMap", notify=gamesChanged)
    def currentGameInfo(self):
        return self._current_game_info

    @Property("QVariantList", notify=gamesChanged)
    def moveRows(self):
        return self._move_rows

    @Property(int, notify=boardChanged)
    def currentPly(self) -> int:
        return self._current_ply

    @Property(int, notify=boardChanged)
    def maxPly(self) -> int:
        return self._timeline.max_ply if self._timeline is not None else 0

    @Property(str, notify=boardChanged)
    def boardFen(self) -> str:
        return self._fen

    @Property("QVariantList", notify=boardChanged)
    def boardSquares(self):
        return self._board_squares

    @Property("QVariantList", notify=boardChanged)
    def arrows(self):
        if self._mode == "training" and not self._answer_visible:
            return []
        return self._arrows

    @Property("QVariantList", notify=boardChanged)
    def highlights(self):
        if self._mode == "training" and not self._answer_visible:
            return []
        return self._highlights

    @Property(bool, notify=boardChanged)
    def flipped(self) -> bool:
        return self._flipped

    @Property(bool, notify=gamesChanged)
    def hasCurrentGame(self) -> bool:
        return self._timeline is not None and 0 <= self._current_game_index < len(self._games)

    @Property(str, notify=sourceChanged)
    def sourceLabel(self) -> str:
        return self._source_label

    @Property("QVariantList", notify=collectionChanged)
    def positionRows(self):
        return self._positions

    @Property("QVariantMap", notify=collectionChanged)
    def positionFilters(self):
        return {
            **self._filters,
            "tags_text": ", ".join(self._filters.get("tags", [])),
            "color": self._filters.get("color", "") or "any",
        }

    @Property("QVariantMap", notify=selectedPositionChanged)
    def selectedPosition(self):
        return self._selected_position

    @Property("QVariantMap", notify=trainingChanged)
    def trainingPosition(self):
        return self._training_position

    @Property(bool, notify=trainingChanged)
    def answerVisible(self) -> bool:
        return self._answer_visible

    @Property(str, notify=trainingChanged)
    def trainingProgress(self) -> str:
        if not self._training_ids or self._training_index < 0:
            return "0 / 0"
        return f"{self._training_index + 1} / {len(self._training_ids)}"

    @Slot(str, result=str)
    def pathFromUrl(self, value) -> str:
        return normalize_path(value)

    @Slot(str)
    def setMode(self, mode: str) -> None:
        normalized = mode.strip().lower()
        if normalized not in {"games", "collection", "training"}:
            return
        if self._mode == normalized:
            return
        training_id = int(self._training_position.get("id", 0) or 0)
        if normalized == "training" and (
            not training_id
            or self.context.storage.get_saved_position(training_id) is None
        ):
            self.startTraining()
            return
        self._set_mode_direct(normalized)
        if normalized == "collection":
            self.refreshCollection()
        self._refresh_board()

    @Slot(object)
    def openSession(self, payload) -> None:
        if not isinstance(payload, dict):
            self.set_status("Position Lab получил некорректную сессию.")
            return
        games = [game for game in payload.get("games", []) if isinstance(game, GameRecord)]
        source_kind = str(payload.get("source_kind", "session") or "session")
        source_label = str(payload.get("source_label", "Текущая выборка") or "Текущая выборка")
        username = str(payload.get("username", "") or "")
        filters = payload.get("filters", {}) if isinstance(payload.get("filters", {}), dict) else {}
        eco = str(filters.get("eco", "") or "").strip().upper()
        color = str(filters.get("color", "any") or "any").strip().lower()
        require_user = bool(payload.get("user_only", source_kind == "statistics"))

        storage_groups: dict[str, list[GameRecord]] = {}
        for game in games:
            if not game.source_kind:
                game.source_kind = source_kind
            storage_groups.setdefault(game.source_kind, []).append(game)
        for stored_kind, stored_games in storage_groups.items():
            self.context.storage.upsert_lab_games(
                stored_games,
                source_kind=stored_kind,
            )

        filtered = []
        for game in games:
            if eco and game.eco.strip().upper() != eco:
                continue
            game_color = player_color(game, username)
            if require_user and username and game_color == "unknown":
                continue
            if color in {"white", "black"} and game_color != color:
                continue
            filtered.append(game)

        self._set_mode_direct("games")
        self._set_games(filtered, source_label)
        self.navigationRequested.emit("position_lab")
        self.set_status(f"Открыто партий: {len(filtered)} · источник: {source_label}")

    @Slot()
    def refreshLocalGames(self) -> None:
        games = self.context.storage.list_lab_games(limit=None)
        self._set_mode_direct("games")
        self._set_games(games, f"Локальная библиотека · {len(games)}")
        self.set_status(f"Локальных партий: {len(games)}")

    @Slot(str)
    def loadPgn(self, path: str) -> None:
        normalized = normalize_path(path)
        pgn_path = Path(normalized) if normalized else None
        if pgn_path is None or not pgn_path.exists():
            self.set_status("Выбери существующий PGN-файл.")
            return
        if pgn_path.suffix.lower() != ".pgn":
            self.set_status("Position Lab принимает внешний файл с расширением .pgn.")
            return
        try:
            games = parse_pgn_file(pgn_path)
            inserted = self.context.storage.upsert_lab_games(
                games,
                source_kind="external_pgn",
            )
            self._set_mode_direct("games")
            self._set_games(games, f"PGN: {pgn_path.name}")
            self.set_status(
                f"PGN открыт: {len(games)} партий · новых в библиотеке: {inserted}."
            )
        except Exception as exc:
            self.set_status(f"Не удалось открыть PGN: {exc}")

    @Slot(int)
    def selectGame(self, index: int) -> None:
        if index < 0 or index >= len(self._games):
            return
        game = self._games[index]
        try:
            timeline = build_game_timeline(game)
        except Exception as exc:
            self._timeline = None
            self._move_rows = []
            self._fen = ""
            self._current_game_index = index
            self._current_game_info = game_info(game)
            self._clear_annotations(emit=False)
            self.gamesChanged.emit()
            self._refresh_board()
            self.set_status(f"Не удалось разобрать ходы партии: {exc}")
            return

        self._current_game_index = index
        self._current_game_info = game_info(game)
        self._timeline = timeline
        self._move_rows = timeline.moves
        self._current_ply = 0
        self._fen = timeline.positions[0]
        self._flipped = player_color(game, self.context.settings.lichess_username) == "black"
        self._clear_annotations(emit=False)
        self.gamesChanged.emit()
        self._refresh_board()
        self.set_status(
            f"{game.white or '?'} — {game.black or '?'} · ходов: {timeline.max_ply}."
        )

    @Slot(int)
    def goToPly(self, ply: int) -> None:
        if self._timeline is None:
            return
        normalized = max(0, min(int(ply), self._timeline.max_ply))
        if normalized == self._current_ply:
            return
        self._current_ply = normalized
        self._fen = self._timeline.positions[normalized]
        self._clear_annotations(emit=False)
        self._refresh_board()

    @Slot()
    def firstMove(self) -> None:
        self.goToPly(0)

    @Slot()
    def previousMove(self) -> None:
        self.goToPly(self._current_ply - 1)

    @Slot()
    def nextMove(self) -> None:
        self.goToPly(self._current_ply + 1)

    @Slot()
    def lastMove(self) -> None:
        if self._timeline is not None:
            self.goToPly(self._timeline.max_ply)

    @Slot()
    def flipBoard(self) -> None:
        self._flipped = not self._flipped
        self._refresh_board()

    @Slot(str, str, str)
    def toggleArrow(self, from_square: str, to_square: str, color: str) -> None:
        start = from_square.strip().lower()
        end = to_square.strip().lower()
        normalized_color = annotation_color(color)
        if start == end or not valid_square(start) or not valid_square(end):
            return
        exact = any(
            arrow.get("from") == start
            and arrow.get("to") == end
            and arrow.get("color") == normalized_color
            for arrow in self._arrows
        )
        self._arrows = [
            arrow
            for arrow in self._arrows
            if not (arrow.get("from") == start and arrow.get("to") == end)
        ]
        if not exact:
            self._arrows.append({"from": start, "to": end, "color": normalized_color})
        self._arrows = normalize_arrows(self._arrows)
        self._refresh_board()

    @Slot(str, str)
    def toggleHighlight(self, square: str, color: str) -> None:
        normalized_square = square.strip().lower()
        normalized_color = annotation_color(color)
        if not valid_square(normalized_square):
            return
        exact = any(
            item.get("square") == normalized_square
            and item.get("color") == normalized_color
            for item in self._highlights
        )
        self._highlights = [
            item for item in self._highlights if item.get("square") != normalized_square
        ]
        if not exact:
            self._highlights.append(
                {"square": normalized_square, "color": normalized_color}
            )
        self._highlights = normalize_highlights(self._highlights)
        self._refresh_board()

    @Slot()
    def clearAnnotations(self) -> None:
        self._clear_annotations(emit=True)

    @Slot(str, str)
    def saveCurrentPosition(self, tags_text: str, comment: str) -> None:
        if not self.hasCurrentGame or self._timeline is None:
            self.set_status("Сначала открой партию и выбери позицию.")
            return
        game = self._games[self._current_game_index]
        if not game.storage_id:
            self.context.storage.upsert_lab_games(
                [game],
                source_kind=game.source_kind or "position_lab",
            )
        saved_color = player_color(game, self.context.settings.lichess_username)
        if saved_color == "unknown":
            saved_color = side_to_move(self._fen)
        position_id = self.context.storage.add_saved_position(
            game_ref=game.storage_id or None,
            fen=self._fen,
            ply=self._current_ply,
            move_label=move_label(self._timeline, self._current_ply),
            side_to_move=side_to_move(self._fen),
            white=game.white,
            black=game.black,
            game_date=game.date,
            eco=game.eco,
            opening=game.opening,
            color=saved_color,
            source=game.source or self._source_label,
            source_kind=game.source_kind or "position_lab",
            tags=parse_tags_text(tags_text),
            comment=comment,
            arrows=normalize_arrows(self._arrows),
            highlights=normalize_highlights(self._highlights),
        )
        self.refreshCollection()
        if self._is_position_visible(position_id):
            self._select_position_record(position_id)
        self.set_status(f"Позиция сохранена · {move_label(self._timeline, self._current_ply)}.")

    @Slot(str, str, str, str, str, str, str, str)
    def applyPositionFilters(
        self,
        search: str,
        tags_text: str,
        eco: str,
        opening: str,
        color: str,
        source: str,
        date_from: str,
        date_to: str,
    ) -> None:
        self._filters = {
            "search": search,
            "tags": parse_tags_text(tags_text),
            "eco": eco,
            "opening": opening,
            "color": "" if color in {"", "any"} else color,
            "source": source,
            "date_from": date_from,
            "date_to": date_to,
        }
        self.refreshCollection()

    @Slot()
    def refreshCollection(self) -> None:
        self._positions = self.context.storage.list_saved_positions(**self._filters)
        self.collectionChanged.emit()
        visible_ids = {int(position["id"]) for position in self._positions}
        if self._selected_position_id and self._selected_position_id not in visible_ids:
            self._selected_position = {}
            self._selected_position_id = 0
            self._board_states.pop("collection", None)
            self.selectedPositionChanged.emit()
        if self._mode == "collection" and not self._selected_position_id:
            if self._positions:
                self.openPosition(int(self._positions[0]["id"]))
            else:
                self._fen = ""
                self._current_ply = 0
                self._clear_annotations(emit=False)
                self._refresh_board()

    @Slot(int)
    def openPosition(self, position_id: int) -> None:
        self._select_position_record(position_id)
        if not self._selected_position:
            return
        self._set_mode_direct("collection")
        self._fen = str(self._selected_position.get("fen", ""))
        self._current_ply = int(self._selected_position.get("ply", 0) or 0)
        self._arrows = normalize_arrows(self._selected_position.get("arrows", []))
        self._highlights = normalize_highlights(
            self._selected_position.get("highlights", [])
        )
        self._flipped = self._selected_position.get("color") == "black"
        self._refresh_board()

    @Slot(int, str, str)
    def updatePosition(self, position_id: int, tags_text: str, comment: str) -> None:
        updated = self.context.storage.update_saved_position(
            position_id,
            tags=parse_tags_text(tags_text),
            comment=comment,
            arrows=normalize_arrows(self._arrows),
            highlights=normalize_highlights(self._highlights),
        )
        if not updated:
            self.set_status("Позиция для редактирования не найдена.")
            return
        self.refreshCollection()
        if self._is_position_visible(position_id):
            self._select_position_record(position_id)
        self.set_status("Позиция обновлена.")

    @Slot(int)
    def deletePosition(self, position_id: int) -> None:
        if not self.context.storage.delete_saved_position(position_id):
            self.set_status("Позиция уже удалена.")
            return
        deleted_training_position = (
            int(self._training_position.get("id", 0) or 0) == int(position_id)
        )
        self._training_ids = [
            item for item in self._training_ids if item != int(position_id)
        ]
        if deleted_training_position:
            self._training_position = {}
            self._training_index = -1
            self._answer_visible = False
            self._board_states.pop("training", None)
            self.trainingChanged.emit()
        self._selected_position = {}
        self._selected_position_id = 0
        self._board_states.pop("collection", None)
        self.selectedPositionChanged.emit()
        self.refreshCollection()
        if self._mode == "training":
            self.startTraining()
        elif self._mode == "collection" and not self._positions:
            self._fen = ""
            self._current_ply = 0
            self._clear_annotations(emit=False)
            self._refresh_board()
        self.set_status("Позиция удалена.")

    @Slot(int)
    def returnToSourceGame(self, position_id: int) -> None:
        position = self.context.storage.get_saved_position(position_id)
        if not position or not position.get("game_ref"):
            self.set_status("Исходная партия для этой позиции недоступна.")
            return
        game = self.context.storage.get_lab_game(int(position["game_ref"]))
        if game is None:
            self.set_status("Исходная партия не найдена в локальной библиотеке.")
            return
        self._set_mode_direct("games")
        self._set_games([game], "Исходная партия сохранённой позиции")
        self.goToPly(int(position.get("ply", 0) or 0))
        self._arrows = normalize_arrows(position.get("arrows", []))
        self._highlights = normalize_highlights(position.get("highlights", []))
        self._refresh_board()
        self.set_status("Открыта исходная партия и сохранённый ход.")

    @Slot()
    def startTraining(self) -> None:
        # Collection filters are browsing tools.  Basic training always covers
        # the complete saved library so a hidden search cannot make positions
        # appear to have disappeared.
        training_positions = self.context.storage.list_saved_positions()
        self._training_ids = [int(position["id"]) for position in training_positions]
        self._training_index = 0 if self._training_ids else -1
        self._set_mode_direct("training")
        if not self._training_ids:
            self._training_position = {}
            self._answer_visible = False
            self._board_states.pop("training", None)
            self._fen = ""
            self._current_ply = 0
            self._clear_annotations(emit=False)
            self.trainingChanged.emit()
            self._refresh_board()
            self.set_status("Для тренировки сначала сохрани хотя бы одну позицию.")
            return
        self._load_training_position()

    @Slot()
    def showAnswer(self) -> None:
        if not self._training_position:
            return
        self._answer_visible = True
        self.trainingChanged.emit()
        self._refresh_board()

    @Slot(str)
    def rateAnswer(self, rating: str) -> None:
        if not self._training_position:
            return
        if not self._answer_visible:
            self.set_status("Сначала покажи ответ, затем оцени воспоминание.")
            return
        try:
            self.context.storage.record_position_review(
                int(self._training_position["id"]),
                rating,
            )
        except ValueError as exc:
            self.set_status(str(exc))
            return
        self.refreshCollection()
        self._training_index = (self._training_index + 1) % len(self._training_ids)
        self._load_training_position()
        self.set_status("Оценка сохранена. Следующая позиция.")

    def _set_games(self, games: list[GameRecord], source_label: str) -> None:
        self._games = list(games)
        self._game_rows = [game_summary(game, index) for index, game in enumerate(self._games)]
        self._source_label = source_label
        self.sourceChanged.emit()
        if self._games:
            self.selectGame(0)
            return
        self._current_game_index = -1
        self._current_game_info = {}
        self._timeline = None
        self._move_rows = []
        self._current_ply = 0
        self._fen = ""
        self._clear_annotations(emit=False)
        self.gamesChanged.emit()
        self._refresh_board()

    def _set_mode_direct(self, mode: str) -> None:
        if self._mode != mode:
            self._capture_board_state()
            self._mode = mode
            self._restore_board_state(mode)
            self.modeChanged.emit()

    def _capture_board_state(self) -> None:
        self._board_states[self._mode] = {
            "fen": self._fen,
            "ply": self._current_ply,
            "flipped": self._flipped,
            "arrows": [dict(item) for item in self._arrows],
            "highlights": [dict(item) for item in self._highlights],
        }

    def _restore_board_state(self, mode: str) -> None:
        state = self._board_states.get(mode)
        if state is None and mode == "games" and self._timeline is not None:
            state = {
                "fen": self._timeline.positions[self._current_ply],
                "ply": self._current_ply,
                "flipped": self._flipped,
                "arrows": [],
                "highlights": [],
            }
        elif state is None and mode == "collection" and self._selected_position:
            state = self._state_from_position(self._selected_position)
        elif state is None and mode == "training" and self._training_position:
            state = self._state_from_position(self._training_position)
        if state is None:
            return
        self._fen = str(state.get("fen", ""))
        self._current_ply = int(state.get("ply", 0) or 0)
        self._flipped = bool(state.get("flipped", False))
        self._arrows = normalize_arrows(state.get("arrows", []))
        self._highlights = normalize_highlights(state.get("highlights", []))

    @staticmethod
    def _state_from_position(position: dict) -> dict:
        return {
            "fen": str(position.get("fen", "")),
            "ply": int(position.get("ply", 0) or 0),
            "flipped": position.get("color") == "black",
            "arrows": position.get("arrows", []),
            "highlights": position.get("highlights", []),
        }

    def _select_position_record(self, position_id: int) -> None:
        position = self.context.storage.get_saved_position(position_id)
        self._selected_position = position or {}
        self._selected_position_id = int(position_id) if position else 0
        if position:
            self._board_states["collection"] = self._state_from_position(position)
        else:
            self._board_states.pop("collection", None)
        self.selectedPositionChanged.emit()

    def _is_position_visible(self, position_id: int) -> bool:
        return any(int(position.get("id", 0) or 0) == int(position_id) for position in self._positions)

    def _load_training_position(self) -> None:
        if self._training_index < 0 or self._training_index >= len(self._training_ids):
            return
        position = self.context.storage.get_saved_position(
            self._training_ids[self._training_index]
        )
        if position is None:
            self._training_ids.pop(self._training_index)
            if not self._training_ids:
                self.startTraining()
                return
            self._training_index %= len(self._training_ids)
            self._load_training_position()
            return
        self._training_position = position
        self._answer_visible = False
        self._fen = str(position.get("fen", ""))
        self._current_ply = int(position.get("ply", 0) or 0)
        self._arrows = normalize_arrows(position.get("arrows", []))
        self._highlights = normalize_highlights(position.get("highlights", []))
        self._flipped = position.get("color") == "black"
        self.trainingChanged.emit()
        self._refresh_board()

    def _clear_annotations(self, *, emit: bool) -> None:
        self._arrows = []
        self._highlights = []
        if emit:
            self._refresh_board()

    def _refresh_board(self) -> None:
        display_highlights = self.highlights
        if not self._fen:
            self._board_squares = []
        else:
            try:
                self._board_squares = board_squares(
                    self._fen,
                    flipped=self._flipped,
                    highlights=display_highlights,
                )
            except ValueError:
                self._board_squares = []
        self.boardChanged.emit()
