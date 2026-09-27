from pathlib import Path
import unittest

from backend.parsers.pgn_parser import parse_pgn_file, parse_pgn_text


class PgnParserTest(unittest.TestCase):
    def test_sample_games(self):
        games = parse_pgn_file(Path(__file__).resolve().parents[1] / "sample_data" / "sample_games.pgn")
        self.assertEqual(len(games), 2)
        self.assertEqual(games[0].eco, "B12")
        self.assertEqual(games[1].opening, "Queen's Pawn Game: London System")
        self.assertGreaterEqual(games[0].move_count, 5)

    def test_move_count_ignores_lichess_clock_comments(self):
        games = parse_pgn_file(Path(__file__).resolve().parents[1] / "sample_data" / "sample_games.pgn")
        pgn = games[0].raw_pgn.replace("1. e4", "1. e4 { [%clk 0:05:00] }", 1)
        reparsed = parse_pgn_text(pgn)[0]
        self.assertEqual(reparsed.ply_count, games[0].ply_count)

    def test_lichess_game_id_ignores_board_orientation_suffix(self):
        pgn = """[Event "Rated game"]
[Site "https://lichess.org/AbCd1234/black"]
[White "A"]
[Black "B"]
[Result "*"]

1. e4 *
"""

        game = parse_pgn_text(pgn)[0]

        self.assertEqual(game.game_id, "AbCd1234")


if __name__ == "__main__":
    unittest.main()
