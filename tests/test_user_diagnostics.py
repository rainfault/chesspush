import unittest

from backend.entities import GameRecord
from backend.statistics.user_diagnostics import build_user_diagnostics


class UserDiagnosticsTest(unittest.TestCase):
    def test_summary_is_from_user_perspective(self):
        games = [
            GameRecord(white="JustAIM", black="A", result="1-0", eco="B12", opening="Caro-Kann", event="Rated Rapid Game", white_elo=1900, black_elo=1900),
            GameRecord(white="B", black="JustAIM", result="1-0", eco="D02", opening="London System", event="Rated Blitz Game", white_elo=1900, black_elo=1900),
        ]
        diagnostics = build_user_diagnostics(games, username="JustAIM", period_count=0, min_games=1)
        score_card = next(card for card in diagnostics.summary_cards if card["title"] == "Overall score")
        self.assertEqual(score_card["value"], "50.0%")
        self.assertEqual(diagnostics.analyzed_games, 2)

    def test_pain_map_requires_min_games_and_low_score(self):
        games = [
            GameRecord(white="JustAIM", black=f"A{i}", result="0-1", eco="C60", opening="Ruy Lopez", event="Rated Rapid Game", white_elo=2000, black_elo=2000)
            for i in range(3)
        ]
        games.append(GameRecord(white="JustAIM", black="B", result="0-1", eco="B12", opening="Caro-Kann", event="Rated Rapid Game"))
        diagnostics = build_user_diagnostics(games, username="JustAIM", period_count=0, min_games=3)
        self.assertEqual(len(diagnostics.pain_rows), 1)
        self.assertEqual(diagnostics.pain_rows[0]["eco"], "C60")
        self.assertGreater(diagnostics.pain_rows[0]["pain_index"], 0)

    def test_pain_map_groups_opening_family(self):
        games = [
            GameRecord(white="JustAIM", black="A", result="0-1", eco="B10", opening="Caro-Kann Defense: Two Knights Attack", event="Rated Blitz Game"),
            GameRecord(white="JustAIM", black="B", result="0-1", eco="B12", opening="Caro-Kann Defense: Advance Variation", event="Rated Blitz Game"),
            GameRecord(white="JustAIM", black="C", result="1/2-1/2", eco="B13", opening="Caro-Kann Defense: Exchange Variation", event="Rated Blitz Game"),
        ]
        diagnostics = build_user_diagnostics(games, username="JustAIM", period_count=0, min_games=3)
        self.assertEqual(len(diagnostics.pain_rows), 1)
        self.assertEqual(diagnostics.pain_rows[0]["opening"], "Caro-Kann Defense")
        self.assertEqual(diagnostics.pain_rows[0]["games"], 3)
        self.assertEqual(diagnostics.pain_rows[0]["eco"], "B10-B13")


if __name__ == "__main__":
    unittest.main()
