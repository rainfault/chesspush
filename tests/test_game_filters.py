import unittest

from backend.entities import GameRecord
from backend.filters.game_filters import GameFilter, _prefix_matches, matches_filter


class GameFiltersTest(unittest.TestCase):
    def test_move_prefix_ignores_lichess_clock_comments(self):
        moves = (
            "1. e4 { [%clk 0:05:00] } 1... e5 { [%clk 0:05:00] } "
            "2. Nf3 { [%clk 0:04:59] } 2... Nc6 { [%clk 0:04:59] } "
            "3. Bb5 { [%clk 0:04:57] } 3... a6"
        )
        self.assertTrue(_prefix_matches(moves, "1. e4 e5 2. Nf3 Nc6 3. Bb5"))

    def test_ruy_lopez_prefix_matches_later_eco_code(self):
        game = GameRecord(
            event="Rated Blitz game",
            white_elo=2400,
            black_elo=2410,
            eco="C68",
            opening="Ruy Lopez: Exchange Variation",
            termination="Normal",
            moves=(
                "1. e4 { [%clk 0:05:00] } 1... e5 { [%clk 0:05:00] } "
                "2. Nf3 { [%clk 0:04:59] } 2... Nc6 { [%clk 0:04:59] } "
                "3. Bb5 { [%clk 0:04:57] } 3... a6 4. Bxc6"
            ),
        )
        flt = GameFilter(
            min_white_elo=2300,
            max_white_elo=4000,
            min_black_elo=2300,
            max_black_elo=4000,
            min_both_elo=2300,
            opening_contains="Ruy Lopez",
            move_prefix="1. e4 e5 2. Nf3 Nc6 3. Bb5",
            min_moves=0,
            exclude_bullet=True,
            include_timeout=False,
        )
        self.assertTrue(matches_filter(game, flt))


if __name__ == "__main__":
    unittest.main()
