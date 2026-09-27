from pathlib import Path
import tempfile
import unittest

from app.app_context import AppContext
from backend.parsers.pgn_parser import parse_pgn_file
from controllers.database_loader_controller import DatabaseLoaderController
from controllers.position_lab_controller import PositionLabController
from controllers.statistics_controller import StatisticsController


PROJECT_ROOT = Path(__file__).resolve().parents[1]
SAMPLE_PGN = PROJECT_ROOT / "sample_data" / "sample_games.pgn"


class PositionLabControllerIntegrationTest(unittest.TestCase):
    def setUp(self):
        self._temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp_dir.cleanup)
        self.context = AppContext(Path(self._temp_dir.name))
        self.context.update_settings(lichess_username="JustAIM")

    def test_load_sample_and_navigate_the_game_timeline(self):
        controller = PositionLabController(self.context)

        controller.loadPgn(str(SAMPLE_PGN))

        self.assertEqual(len(controller.gameRows), 2)
        self.assertEqual(controller.currentGameIndex, 0)
        self.assertTrue(controller.hasCurrentGame)
        self.assertEqual(controller.currentGameInfo["eco"], "B12")
        self.assertEqual(controller.maxPly, 20)
        self.assertEqual(controller.currentPly, 0)
        self.assertEqual(len(controller.boardSquares), 64)
        initial_fen = controller.boardFen

        controller.nextMove()
        self.assertEqual(controller.currentPly, 1)
        self.assertNotEqual(controller.boardFen, initial_fen)

        controller.goToPly(10_000)
        self.assertEqual(controller.currentPly, controller.maxPly)
        controller.previousMove()
        self.assertEqual(controller.currentPly, controller.maxPly - 1)
        controller.firstMove()
        self.assertEqual(controller.currentPly, 0)

        controller.selectGame(1)
        self.assertEqual(controller.currentGameInfo["eco"], "D02")
        self.assertTrue(controller.flipped)
        self.assertEqual(self.context.storage.count_lab_games(), 2)

    def test_annotation_collection_crud_and_training_rating(self):
        controller = PositionLabController(self.context)
        controller.loadPgn(str(SAMPLE_PGN))
        controller.goToPly(4)
        saved_fen = controller.boardFen
        controller.toggleArrow("e2", "e4", "red")
        controller.toggleHighlight("d4", "yellow")

        controller.saveCurrentPosition(
            "структура, план; Структура",
            "Давить на центр и закончить развитие.",
        )

        self.assertEqual(self.context.storage.count_saved_positions(), 1)
        position_id = int(controller.selectedPosition["id"])
        stored = self.context.storage.get_saved_position(position_id)
        self.assertIsNotNone(stored)
        self.assertEqual(stored["fen"], saved_fen)
        self.assertEqual(stored["ply"], 4)
        self.assertEqual(stored["tags"], ["структура", "план"])
        self.assertEqual(
            stored["arrows"],
            [{"from": "e2", "to": "e4", "color": "red"}],
        )
        self.assertEqual(
            stored["highlights"],
            [{"square": "d4", "color": "yellow"}],
        )
        self.assertGreater(int(stored["game_ref"]), 0)

        controller.openPosition(position_id)
        self.assertEqual(controller.mode, "collection")
        self.assertEqual(controller.boardFen, saved_fen)
        self.assertEqual(len(controller.arrows), 1)
        self.assertEqual(len(controller.highlights), 1)

        controller.updatePosition(
            position_id,
            "центр, типовая позиция",
            "Обновлённый план.",
        )
        updated = self.context.storage.get_saved_position(position_id)
        self.assertEqual(updated["tags"], ["центр", "типовая позиция"])
        self.assertEqual(updated["comment"], "Обновлённый план.")

        controller.returnToSourceGame(position_id)
        self.assertEqual(controller.mode, "games")
        self.assertEqual(controller.currentPly, 4)
        self.assertEqual(controller.boardFen, saved_fen)
        self.assertEqual(len(controller.arrows), 1)
        self.assertEqual(len(controller.highlights), 1)

        controller.startTraining()
        self.assertEqual(controller.mode, "training")
        self.assertEqual(controller.trainingProgress, "1 / 1")
        self.assertFalse(controller.answerVisible)
        self.assertEqual(controller.arrows, [])
        self.assertEqual(controller.highlights, [])

        controller.rateAnswer("remembered")
        self.assertEqual(
            self.context.storage.list_position_reviews(position_id),
            [],
            "Оценка до показа ответа не должна сохраняться",
        )

        controller.showAnswer()
        self.assertTrue(controller.answerVisible)
        self.assertEqual(len(controller.arrows), 1)
        self.assertEqual(len(controller.highlights), 1)
        controller.rateAnswer("remembered")

        reviews = self.context.storage.list_position_reviews(position_id)
        self.assertEqual(len(reviews), 1)
        self.assertEqual(reviews[0]["rating"], "remembered")
        reviewed = self.context.storage.get_saved_position(position_id)
        self.assertEqual(reviewed["review_count"], 1)
        self.assertEqual(reviewed["last_rating"], "remembered")
        self.assertFalse(controller.answerVisible)

        controller.deletePosition(position_id)
        self.assertEqual(self.context.storage.count_saved_positions(), 0)
        self.assertEqual(controller.selectedPosition, {})
        self.assertEqual(self.context.storage.list_position_reviews(position_id), [])

    def test_switching_modes_preserves_the_game_board_draft(self):
        controller = PositionLabController(self.context)
        controller.loadPgn(str(SAMPLE_PGN))
        controller.goToPly(6)
        game_fen = controller.boardFen
        controller.toggleArrow("b1", "c3", "blue")
        controller.toggleHighlight("c3", "yellow")
        controller.saveCurrentPosition("черновик", "Проверка режимов")
        position_id = int(controller.selectedPosition["id"])

        controller.openPosition(position_id)
        self.assertEqual(controller.mode, "collection")
        controller.clearAnnotations()

        controller.setMode("games")

        self.assertEqual(controller.currentPly, 6)
        self.assertEqual(controller.boardFen, game_fen)
        self.assertEqual(
            controller.arrows,
            [{"from": "b1", "to": "c3", "color": "blue"}],
        )
        self.assertEqual(
            controller.highlights,
            [{"square": "c3", "color": "yellow"}],
        )

    def test_open_session_filters_games_and_requests_navigation(self):
        controller = PositionLabController(self.context)
        games = parse_pgn_file(SAMPLE_PGN)
        requested_pages = []
        controller.navigationRequested.connect(requested_pages.append)

        controller.openSession(
            {
                "games": games,
                "source_kind": "statistics",
                "source_label": "Мои партии · B12",
                "username": "JustAIM",
                "filters": {"eco": "B12", "color": "white"},
            }
        )

        self.assertEqual(requested_pages, ["position_lab"])
        self.assertEqual(len(controller.gameRows), 1)
        self.assertEqual(controller.currentGameInfo["eco"], "B12")
        self.assertEqual(controller.sourceLabel, "Мои партии · B12")

    def test_excluding_collection_filter_does_not_keep_hidden_saved_or_updated_selection(self):
        controller = PositionLabController(self.context)
        controller.loadPgn(str(SAMPLE_PGN))
        controller.goToPly(4)
        controller.applyPositionFilters(
            "", "нужный тег", "", "", "any", "", "", ""
        )

        controller.saveCurrentPosition("другой тег", "Скрыта активным фильтром")

        self.assertEqual(controller.positionRows, [])
        self.assertEqual(controller.selectedPosition, {})
        saved = self.context.storage.list_saved_positions()
        self.assertEqual(len(saved), 1)
        position_id = int(saved[0]["id"])

        controller.applyPositionFilters("", "", "", "", "any", "", "", "")
        controller.openPosition(position_id)
        controller.applyPositionFilters(
            "", "другой тег", "", "", "any", "", "", ""
        )
        self.assertEqual([row["id"] for row in controller.positionRows], [position_id])

        controller.updatePosition(position_id, "третий тег", "Больше не подходит")

        self.assertEqual(controller.positionRows, [])
        self.assertEqual(controller.selectedPosition, {})

    def test_training_ignores_collection_filters(self):
        controller = PositionLabController(self.context)
        controller.loadPgn(str(SAMPLE_PGN))
        controller.goToPly(4)
        controller.saveCurrentPosition("план", "Учебная позиция")
        position_id = int(controller.selectedPosition["id"])
        controller.applyPositionFilters(
            "точно не найдено", "", "", "", "any", "", "", ""
        )
        self.assertEqual(controller.positionRows, [])

        controller.startTraining()

        self.assertEqual(controller.mode, "training")
        self.assertEqual(controller.trainingProgress, "1 / 1")
        self.assertEqual(int(controller.trainingPosition["id"]), position_id)

    def test_switching_between_games_and_collection_restores_each_board_state(self):
        controller = PositionLabController(self.context)
        controller.loadPgn(str(SAMPLE_PGN))
        controller.goToPly(4)
        controller.toggleArrow("e2", "e4", "red")
        controller.saveCurrentPosition("план", "Сохранённая позиция")
        position_id = int(controller.selectedPosition["id"])

        controller.goToPly(2)
        controller.toggleArrow("a2", "a4", "blue")
        games_state = {
            "fen": controller.boardFen,
            "ply": controller.currentPly,
            "arrows": list(controller.arrows),
            "highlights": list(controller.highlights),
        }

        controller.openPosition(position_id)
        controller.toggleHighlight("c3", "yellow")
        controller.flipBoard()
        collection_state = {
            "fen": controller.boardFen,
            "ply": controller.currentPly,
            "arrows": list(controller.arrows),
            "highlights": list(controller.highlights),
            "flipped": controller.flipped,
        }

        controller.setMode("games")
        self.assertEqual(controller.boardFen, games_state["fen"])
        self.assertEqual(controller.currentPly, games_state["ply"])
        self.assertEqual(controller.arrows, games_state["arrows"])
        self.assertEqual(controller.highlights, games_state["highlights"])

        controller.setMode("collection")
        self.assertEqual(controller.boardFen, collection_state["fen"])
        self.assertEqual(controller.currentPly, collection_state["ply"])
        self.assertEqual(controller.arrows, collection_state["arrows"])
        self.assertEqual(controller.highlights, collection_state["highlights"])
        self.assertEqual(controller.flipped, collection_state["flipped"])

    def test_zst_session_does_not_require_model_games_to_include_the_user(self):
        controller = PositionLabController(self.context)
        games = parse_pgn_file(SAMPLE_PGN)
        games[0].white = "ModelWhite"
        games[0].black = "ModelBlack"

        controller.openSession(
            {
                "games": [games[0]],
                "source_kind": "zst_model",
                "source_label": "ZST models",
                "username": "JustAIM",
                "filters": {},
            }
        )

        self.assertEqual(len(controller.gameRows), 1)
        self.assertEqual(controller.currentGameInfo["white"], "ModelWhite")


class StatisticsPositionLabIntegrationTest(unittest.TestCase):
    def setUp(self):
        self._temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp_dir.cleanup)
        self.context = AppContext(Path(self._temp_dir.name))
        self.context.update_settings(lichess_username="JustAIM")

    def test_pgn_persistence_dedup_and_open_my_games_payload(self):
        controller = StatisticsController(self.context)

        controller.loadPgn(str(SAMPLE_PGN))
        controller.loadPgn(str(SAMPLE_PGN))

        self.assertEqual(self.context.storage.count_lab_games(), 2)
        stored = self.context.storage.list_lab_games(limit=None)
        self.assertTrue(all(game.moves for game in stored))
        self.assertEqual({game.source_kind for game in stored}, {"statistics_pgn"})

        restored = StatisticsController(self.context)
        restored.refresh()
        self.assertEqual([row["eco"] for row in restored.rows], ["D02", "B12"])

        payloads = []
        restored.positionLabRequested.connect(payloads.append)
        restored.openMyGames("B12", "white")

        self.assertEqual(len(payloads), 1)
        payload = payloads[0]
        self.assertEqual(payload["source_kind"], "statistics")
        self.assertEqual(payload["username"], "JustAIM")
        self.assertEqual(payload["filters"], {"eco": "B12", "color": "white"})
        self.assertEqual(len(payload["games"]), 1)
        self.assertEqual(payload["games"][0].eco, "B12")
        self.assertEqual(payload["games"][0].source_kind, "statistics_pgn")


class DatabaseLoaderPositionLabIntegrationTest(unittest.TestCase):
    def setUp(self):
        self._temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp_dir.cleanup)
        self.context = AppContext(Path(self._temp_dir.name))
        self.context.update_settings(lichess_username="JustAIM")

    def test_open_current_selection_and_clear_stale_selection_after_failure(self):
        controller = DatabaseLoaderController(self.context)
        games = parse_pgn_file(SAMPLE_PGN)
        payloads = []
        controller.positionLabRequested.connect(payloads.append)

        controller._on_extraction_finished(
            games,
            str(Path(self._temp_dir.name) / "model_games.pgn"),
            "Готово",
            25,
            len(games),
        )
        self.assertTrue(controller.canOpenLastInPositionLab)
        controller.openLastInPositionLab()

        self.assertEqual(len(payloads), 1)
        payload = payloads[0]
        self.assertEqual(payload["source_kind"], "zst_model")
        self.assertEqual(payload["username"], "JustAIM")
        self.assertEqual(len(payload["games"]), 2)
        self.assertIsNot(payload["games"], controller._last_games)

        controller._on_extraction_failed("broken stream")
        self.assertFalse(controller.canOpenLastInPositionLab)
        controller.openLastInPositionLab()
        self.assertEqual(len(payloads), 1)
        self.assertIn("Сначала собери", controller.status)


if __name__ == "__main__":
    unittest.main()
