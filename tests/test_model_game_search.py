from pathlib import Path
import tempfile
import unittest

import zstandard as zstd

from backend.filters.game_filters import GameFilter
from backend.search.model_game_search import iter_matching_games_from_path


SAMPLE_GAMES = Path(__file__).resolve().parents[1] / "sample_data" / "sample_games.pgn"


class ModelGameSearchStartIndexTest(unittest.TestCase):
    def test_start_index_zero_does_not_skip_games(self):
        matches = list(
            iter_matching_games_from_path(
                SAMPLE_GAMES,
                GameFilter(),
                start_index=0,
            )
        )

        self.assertEqual([index for index, _ in matches], [1, 2])

    def test_start_index_skips_raw_games_before_filtering(self):
        matches = list(
            iter_matching_games_from_path(
                SAMPLE_GAMES,
                GameFilter(eco="D02"),
                start_index=1,
            )
        )

        self.assertEqual(len(matches), 1)
        index, game = matches[0]
        self.assertEqual(index, 2)
        self.assertEqual(game.eco, "D02")

    def test_start_index_keeps_returned_indices_absolute_and_one_based(self):
        matches = list(
            iter_matching_games_from_path(
                SAMPLE_GAMES,
                GameFilter(),
                start_index=1,
            )
        )

        self.assertEqual([index for index, _ in matches], [2])

    def test_negative_start_index_is_rejected(self):
        with self.assertRaises(ValueError):
            list(
                iter_matching_games_from_path(
                    SAMPLE_GAMES,
                    GameFilter(),
                    start_index=-1,
                )
            )

    def test_progress_callback_reports_absolute_indices_during_skip(self):
        progress: list[int] = []

        list(
            iter_matching_games_from_path(
                SAMPLE_GAMES,
                GameFilter(),
                start_index=1,
                progress_callback=progress.append,
                progress_step=1,
            )
        )

        self.assertEqual(progress, [1, 2])

    def test_start_index_on_zst_returns_second_absolute_index(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            zst_path = Path(temp_dir) / "sample_games.pgn.zst"
            zst_path.write_bytes(zstd.ZstdCompressor().compress(SAMPLE_GAMES.read_bytes()))

            matches = list(
                iter_matching_games_from_path(
                    zst_path,
                    GameFilter(),
                    start_index=1,
                )
            )

        self.assertEqual([index for index, _ in matches], [2])

    def test_progress_callback_reports_exact_eof_when_no_games_match(self):
        progress: list[int] = []

        matches = list(
            iter_matching_games_from_path(
                SAMPLE_GAMES,
                GameFilter(eco="A00"),
                progress_callback=progress.append,
                progress_step=1000,
            )
        )

        self.assertEqual(matches, [])
        self.assertEqual(progress, [2])

    def test_max_games_counts_games_after_start_index(self):
        progress: list[int] = []

        matches = list(
            iter_matching_games_from_path(
                SAMPLE_GAMES,
                GameFilter(),
                start_index=1,
                max_games=1,
                progress_callback=progress.append,
                progress_step=1,
            )
        )

        self.assertEqual([index for index, _ in matches], [2])
        self.assertEqual(progress, [1, 2])

    def test_stop_requested_by_progress_applies_before_next_raw_game(self):
        progress: list[int] = []
        stop_requested = False

        def record_progress(index: int) -> None:
            nonlocal stop_requested
            progress.append(index)
            stop_requested = True

        matches = list(
            iter_matching_games_from_path(
                SAMPLE_GAMES,
                GameFilter(),
                progress_callback=record_progress,
                progress_step=1,
                should_stop=lambda: stop_requested,
            )
        )

        self.assertEqual([index for index, _ in matches], [1])
        self.assertEqual(progress, [1])

    def test_immediate_stop_does_not_scan_or_report_progress(self):
        progress: list[int] = []

        matches = list(
            iter_matching_games_from_path(
                SAMPLE_GAMES,
                GameFilter(),
                progress_callback=progress.append,
                progress_step=1,
                should_stop=lambda: True,
            )
        )

        self.assertEqual(matches, [])
        self.assertEqual(progress, [])


if __name__ == "__main__":
    unittest.main()
