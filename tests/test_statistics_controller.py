from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from app.app_context import AppContext
from controllers.statistics_controller import StatisticsController


class StatisticsControllerTest(unittest.TestCase):
    def setUp(self):
        self._temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp_dir.cleanup)
        self.context = AppContext(Path(self._temp_dir.name))

    def test_load_pgn_saves_user_games_in_the_local_library(self):
        self.context.update_settings(lichess_username="JustAIM")
        controller = StatisticsController(self.context)
        sample_path = (
            Path(__file__).resolve().parents[1]
            / "sample_data"
            / "sample_games.pgn"
        )

        controller.loadPgn(str(sample_path))

        self.assertEqual(
            [row["eco"] for row in controller.rows],
            ["D02", "B12"],
        )
        self.assertEqual(
            [row["winrate"] for row in controller.rows],
            ["0.0%", "100.0%"],
        )
        self.assertIn("Локальная библиотека", controller.source)
        self.assertIn(sample_path.name, controller.status)
        self.assertEqual(self.context.storage.count_games(), 2)

    def test_load_pgn_requires_username_from_settings(self):
        self.context.update_settings(lichess_username="")
        controller = StatisticsController(self.context)
        sample_path = (
            Path(__file__).resolve().parents[1]
            / "sample_data"
            / "sample_games.pgn"
        )

        controller.loadPgn(str(sample_path))

        self.assertEqual(controller.rows, [])
        self.assertIn("Lichess username", controller.status)

    def test_failed_pgn_load_keeps_the_last_successful_table(self):
        self.context.update_settings(lichess_username="JustAIM")
        controller = StatisticsController(self.context)
        sample_path = (
            Path(__file__).resolve().parents[1]
            / "sample_data"
            / "sample_games.pgn"
        )
        controller.loadPgn(str(sample_path))
        previous_rows = list(controller.rows)
        previous_source = controller.source

        with patch(
            "controllers.statistics_controller.parse_pgn_file",
            side_effect=ValueError("broken PGN"),
        ):
            controller.loadPgn(str(sample_path))

        self.assertEqual(controller.rows, previous_rows)
        self.assertEqual(controller.source, previous_source)
        self.assertIn("broken PGN", controller.status)

    def test_refresh_clears_session_data_when_username_is_removed(self):
        self.context.update_settings(lichess_username="JustAIM")
        controller = StatisticsController(self.context)
        sample_path = (
            Path(__file__).resolve().parents[1]
            / "sample_data"
            / "sample_games.pgn"
        )
        controller.loadPgn(str(sample_path))

        self.context.update_settings(lichess_username="")
        controller.refresh()

        self.assertEqual(controller.rows, [])
        self.assertEqual(controller.source, "")


if __name__ == "__main__":
    unittest.main()
