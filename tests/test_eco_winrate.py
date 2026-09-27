import unittest

from backend.entities import GameRecord
from backend.statistics.eco_winrate import build_eco_winrate_report


def game(
    *,
    eco: str,
    opening: str,
    white: str = "TrainingUser",
    black: str = "Opponent",
    result: str = "1-0",
) -> GameRecord:
    return GameRecord(
        white=white,
        black=black,
        result=result,
        eco=eco,
        opening=opening,
    )


class EcoWinrateReportTest(unittest.TestCase):
    def test_groups_strictly_by_eco_across_openings_and_colors(self):
        games = [
            game(eco="B12", opening="Caro-Kann Defense: Advance Variation", result="1-0"),
            game(
                eco="B12",
                opening="Caro-Kann Defense: Classical Variation",
                white="OpponentA",
                black="TrainingUser",
                result="1/2-1/2",
            ),
            game(eco="B12", opening="Caro-Kann Defense: Two Knights Attack", result="0-1"),
        ]

        report = build_eco_winrate_report(games, "TrainingUser")

        self.assertEqual(report.analyzed_games, 3)
        self.assertEqual(report.skipped_games, 0)
        self.assertEqual(len(report.rows), 1)
        row = report.rows[0]
        self.assertTrue(
            {"eco", "opening", "games", "winrate", "winrate_value"}.issubset(row)
        )
        self.assertEqual(row["eco"], "B12")
        self.assertIsInstance(row["opening"], str)
        self.assertEqual(row["games"], 3)
        # Winrate is literal wins / games. A draw is not half a win.
        self.assertEqual(row["winrate"], "33.3%")
        self.assertAlmostEqual(row["winrate_value"], 1 / 3, places=4)

    def test_sorts_by_winrate_then_games_descending_then_eco(self):
        games = [
            game(eco="A00", opening="A", black="A1", result="0-1"),
            game(eco="B00", opening="B", black="B1", result="0-1"),
            game(eco="B00", opening="B", black="B2", result="0-1"),
            game(eco="D00", opening="D", black="D1", result="1-0"),
            game(eco="D00", opening="D", black="D2", result="0-1"),
            game(eco="C00", opening="C", black="C1", result="1-0"),
            game(eco="C00", opening="C", black="C2", result="0-1"),
            game(eco="E00", opening="E", black="E1", result="1-0"),
        ]

        report = build_eco_winrate_report(games, "TrainingUser")

        self.assertEqual(
            [row["eco"] for row in report.rows],
            ["B00", "A00", "C00", "D00", "E00"],
        )
        self.assertEqual(
            [row["winrate"] for row in report.rows],
            ["0.0%", "0.0%", "50.0%", "50.0%", "100.0%"],
        )

    def test_normalizes_unknown_eco_and_skips_foreign_or_unfinished_games(self):
        games = [
            game(eco="", opening="Unknown opening", result="1-0"),
            game(eco="   ", opening="Another unknown opening", result="0-1"),
            game(
                eco="C60",
                opening="Ruy Lopez",
                white="SomebodyElse",
                black="Opponent",
                result="1-0",
            ),
            game(eco="D02", opening="Queen's Pawn Game", result="*"),
        ]

        report = build_eco_winrate_report(games, "  traininguser  ")

        self.assertEqual(report.analyzed_games, 2)
        self.assertEqual(report.skipped_games, 2)
        self.assertEqual(len(report.rows), 1)
        self.assertEqual(report.rows[0]["eco"], "???")
        self.assertEqual(report.rows[0]["games"], 2)
        self.assertEqual(report.rows[0]["winrate"], "50.0%")
        self.assertEqual(report.rows[0]["winrate_value"], 0.5)


if __name__ == "__main__":
    unittest.main()
