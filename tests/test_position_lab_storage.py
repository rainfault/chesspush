from pathlib import Path
from contextlib import closing
import sqlite3
import tempfile
import unittest

from backend.entities import GameRecord
from backend.parsers.pgn_parser import parse_pgn_file, parse_pgn_text
from backend.position_lab import board_squares, build_game_timeline
from backend.storage.sqlite_storage import SQLiteStorage
from controllers.statistics_controller import StatisticsController


ROOT = Path(__file__).resolve().parents[1]
SAMPLE_PGN = ROOT / "sample_data" / "sample_games.pgn"


class PositionLabGameStorageTest(unittest.TestCase):
    def setUp(self):
        self._temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp_dir.cleanup)
        self.storage = SQLiteStorage(Path(self._temp_dir.name) / "position_lab.sqlite3")
        self.storage.initialize()

    def test_sample_pgn_fingerprint_upsert_is_idempotent(self):
        first_import = parse_pgn_file(SAMPLE_PGN)
        inserted = self.storage.upsert_lab_games(
            first_import,
            source_kind="lichess_user",
        )

        self.assertEqual(inserted, 2)
        self.assertEqual(self.storage.count_lab_games(), 2)
        first_ids = [game.storage_id for game in first_import]
        self.assertTrue(all(storage_id > 0 for storage_id in first_ids))

        repeated_import = parse_pgn_file(SAMPLE_PGN)
        inserted_again = self.storage.upsert_lab_games(
            repeated_import,
            source_kind="lichess_user",
        )

        self.assertEqual(inserted_again, 0)
        self.assertEqual(self.storage.count_lab_games(), 2)
        self.assertEqual(
            [game.storage_id for game in repeated_import],
            first_ids,
        )

    def test_different_games_without_site_or_id_do_not_collapse(self):
        first = parse_pgn_text(
            """
[Event "Local game"]
[Date "2026.08.15"]
[White "Alice"]
[Black "Bob"]
[Result "*"]

1. e4 e5 *
""".strip()
        )[0]
        second = parse_pgn_text(
            """
[Event "Local game"]
[Date "2026.08.15"]
[White "Alice"]
[Black "Bob"]
[Result "*"]

1. d4 d5 *
""".strip()
        )[0]

        self.assertEqual(first.game_id, "")
        self.assertEqual(first.site, "")
        self.assertEqual(second.game_id, "")
        self.assertEqual(second.site, "")
        self.assertEqual(
            self.storage.upsert_lab_games(
                [first, second],
                source_kind="external_pgn",
            ),
            2,
        )
        self.assertEqual(self.storage.count_lab_games(), 2)
        self.assertNotEqual(first.storage_id, second.storage_id)

    def test_different_games_with_a_shared_venue_site_do_not_collapse(self):
        games = [
            parse_pgn_text(
                f"""
[Event "Online tournament"]
[Site "Chess.com"]
[Date "2026.08.15"]
[White "Alice"]
[Black "Bob"]
[Result "*"]

1. {moves} *
""".strip()
            )[0]
            for moves in ("e4 e5", "d4 d5")
        ]

        # A venue-like Site is deliberately not a stable game identifier.
        self.assertEqual([game.site for game in games], ["Chess.com", "Chess.com"])
        self.assertEqual(
            self.storage.upsert_lab_games(games, source_kind="external_pgn"),
            2,
        )
        self.assertEqual(self.storage.count_lab_games(), 2)
        self.assertNotEqual(games[0].storage_id, games[1].storage_id)

    def test_same_lichess_game_id_with_different_site_is_one_game(self):
        first = GameRecord(
            game_id="AbCd1234",
            site="https://lichess.org/AbCd1234",
            white="Alice",
            black="Bob",
            result="1-0",
            moves="1. e4 e5 1-0",
            raw_pgn="[Site \"https://lichess.org/AbCd1234\"]\n\n1. e4 e5 1-0\n",
        )
        repeated = GameRecord(
            game_id="abcd1234",
            site="https://lichess.org/AbCd1234/black",
            white="Alice",
            black="Bob",
            result="1-0",
            moves="1. e4 e5 2. Nf3 1-0",
            raw_pgn=(
                "[Site \"https://lichess.org/AbCd1234/black\"]\n\n"
                "1. e4 e5 2. Nf3 1-0\n"
            ),
        )

        self.assertEqual(
            self.storage.upsert_lab_games([first], source_kind="lichess_user"),
            1,
        )
        self.assertEqual(
            self.storage.upsert_lab_games([repeated], source_kind="lichess_user"),
            0,
        )
        self.assertEqual(self.storage.count_lab_games(), 1)
        self.assertEqual(repeated.storage_id, first.storage_id)

        stored = self.storage.get_lab_game(first.storage_id)
        self.assertIsNotNone(stored)
        self.assertEqual(stored.game_id, "abcd1234")
        self.assertIn("2. Nf3", stored.moves)

    def test_sparse_lichess_upsert_preserves_rich_metadata_and_tags(self):
        rich = GameRecord(
            game_id="Rich1234",
            event="Rated Rapid Game",
            site="https://lichess.org/Rich1234",
            date="2026.08.15",
            white="Alice",
            black="Bob",
            result="1-0",
            white_elo=2111,
            black_elo=2077,
            eco="B12",
            opening="Caro-Kann Defense: Advance Variation",
            time_control="600+5",
            termination="Normal",
            moves="1. e4 c6 2. d4 d5 3. e5 1-0",
            raw_pgn=(
                '[Event "Rated Rapid Game"]\n'
                '[Site "https://lichess.org/Rich1234"]\n'
                '[White "Alice"]\n[Black "Bob"]\n'
                '[ECO "B12"]\n[Opening "Caro-Kann Defense: Advance Variation"]\n\n'
                "1. e4 c6 2. d4 d5 3. e5 1-0\n"
            ),
            source="Lichess: Alice",
            tags={"UTCDate": "2026.08.15", "CustomTag": "keep me"},
        )
        sparse = GameRecord(
            game_id="rich1234",
            site="https://lichess.org/Rich1234/black",
        )

        self.assertEqual(
            self.storage.upsert_lab_games([rich], source_kind="lichess"),
            1,
        )
        self.assertEqual(
            self.storage.upsert_lab_games([sparse], source_kind="lichess"),
            0,
        )

        stored = self.storage.get_lab_game(rich.storage_id)
        self.assertIsNotNone(stored)
        self.assertEqual(stored.white, "Alice")
        self.assertEqual(stored.black, "Bob")
        self.assertEqual(stored.result, "1-0")
        self.assertEqual(stored.white_elo, 2111)
        self.assertEqual(stored.black_elo, 2077)
        self.assertEqual(stored.eco, "B12")
        self.assertEqual(stored.opening, "Caro-Kann Defense: Advance Variation")
        self.assertEqual(stored.time_control, "600+5")
        self.assertEqual(stored.termination, "Normal")
        self.assertEqual(stored.tags["UTCDate"], "2026.08.15")
        self.assertEqual(stored.tags["CustomTag"], "keep me")
        self.assertEqual(stored.moves, rich.moves)
        self.assertEqual(stored.raw_pgn, rich.raw_pgn)

    def test_header_only_raw_upsert_does_not_break_the_stored_timeline(self):
        complete = GameRecord(
            game_id="AbCd1234",
            event="Rapid",
            site="https://lichess.org/AbCd1234",
            white="Alice",
            black="Bob",
            result="1-0",
            moves="1. e4 e5 2. Nf3 Nc6 1-0",
            raw_pgn=(
                '[Event "Rapid"]\n'
                '[Site "https://lichess.org/AbCd1234"]\n'
                '[White "Alice"]\n[Black "Bob"]\n[Result "1-0"]\n\n'
                "1. e4 e5 2. Nf3 Nc6 1-0\n"
            ),
        )
        header_only = GameRecord(
            game_id="AbCd1234",
            event="Metadata " + ("x" * 300),
            site="https://lichess.org/AbCd1234",
            white="Alice",
            black="Bob",
            result="1-0",
            moves="",
            raw_pgn=(
                f'[Event "Metadata {"x" * 300}"]\n'
                '[Site "https://lichess.org/AbCd1234"]\n'
                '[White "Alice"]\n[Black "Bob"]\n[Result "1-0"]\n\n*\n'
            ),
        )
        self.assertGreater(len(header_only.raw_pgn), len(complete.raw_pgn))

        self.storage.upsert_lab_games([complete], source_kind="lichess")
        self.storage.upsert_lab_games([header_only], source_kind="lichess")

        stored = self.storage.get_lab_game(complete.storage_id)
        self.assertIsNotNone(stored)
        self.assertEqual(stored.moves, complete.moves)
        self.assertEqual(stored.raw_pgn, complete.raw_pgn)
        self.assertEqual(build_game_timeline(stored).max_ply, 4)

    def test_lichess_url_tag_and_direct_site_share_one_fingerprint(self):
        tagged_url = parse_pgn_text(
            """
[Event "Rated Rapid Game"]
[Site "?"]
[LichessURL "https://lichess.org/AbCd1234"]
[Date "2026.08.15"]
[White "Alice"]
[Black "Bob"]
[Result "1-0"]

1. e4 e5 1-0
""".strip()
        )[0]
        direct_site = parse_pgn_text(
            """
[Event "Rated Rapid Game"]
[Site "https://lichess.org/AbCd1234"]
[Date "2026.08.15"]
[White "Alice"]
[Black "Bob"]
[Result "1-0"]

1. e4 e5 1-0
""".strip()
        )[0]
        self.assertEqual(tagged_url.game_id, direct_site.game_id)
        self.assertEqual(
            self.storage.game_fingerprint(tagged_url),
            self.storage.game_fingerprint(direct_site),
        )

        self.assertEqual(
            self.storage.upsert_lab_games(
                [tagged_url], source_kind="statistics_pgn"
            ),
            1,
        )
        self.assertEqual(
            self.storage.upsert_lab_games([direct_site], source_kind="lichess"),
            0,
        )
        self.assertEqual(self.storage.count_lab_games(), 1)
        self.assertEqual(tagged_url.storage_id, direct_site.storage_id)

    def test_legacy_provenance_survives_external_and_zst_reimports(self):
        legacy = parse_pgn_file(SAMPLE_PGN)[0]
        self.storage.upsert_lab_games([legacy], source_kind="legacy")

        for source_kind in ("external_pgn", "zst_model"):
            repeated = parse_pgn_file(SAMPLE_PGN)[0]
            self.assertEqual(
                self.storage.upsert_lab_games(
                    [repeated], source_kind=source_kind
                ),
                0,
            )
            stored = self.storage.get_lab_game(legacy.storage_id)
            self.assertIsNotNone(stored)
            self.assertEqual(stored.source_kind, "legacy")
            statistics_games = self.storage.list_lab_games(
                limit=None,
                source_kinds=StatisticsController.USER_SOURCE_KINDS,
            )
            self.assertEqual([game.storage_id for game in statistics_games], [legacy.storage_id])

    def test_bulk_upsert_assigns_ids_beyond_sqlite_parameter_batches(self):
        games = [
            GameRecord(
                game_id=f"bulk-{index}",
                site=f"https://lichess.org/bulk-{index}",
                white="A",
                black="B",
                moves="1. e4 *",
            )
            for index in range(1_205)
        ]

        inserted = self.storage.upsert_lab_games(games, source_kind="lichess")

        self.assertEqual(inserted, len(games))
        self.assertEqual(self.storage.count_lab_games(), len(games))
        self.assertTrue(all(game.storage_id > 0 for game in games))

    def test_raw_pgn_and_all_tags_round_trip(self):
        game = parse_pgn_file(SAMPLE_PGN)[0]
        game.tags["CustomTag"] = "Значение с Unicode"
        original_raw_pgn = game.raw_pgn
        original_tags = dict(game.tags)

        self.storage.upsert_lab_games([game], source_kind="external_pgn")
        stored = self.storage.get_lab_game(game.storage_id)

        self.assertIsNotNone(stored)
        self.assertEqual(stored.raw_pgn, original_raw_pgn)
        self.assertEqual(stored.tags, original_tags)
        self.assertEqual(stored.source_kind, "external_pgn")

    def test_initialize_migrates_existing_legacy_games(self):
        legacy_path = Path(self._temp_dir.name) / "legacy.sqlite3"
        raw_pgn = SAMPLE_PGN.read_text(encoding="utf-8").split("\n\n[Event", 1)[0]
        with closing(sqlite3.connect(legacy_path)) as conn, conn:
            conn.execute(
                """
                CREATE TABLE games (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    game_id TEXT, white TEXT, black TEXT, moves TEXT,
                    raw_pgn TEXT, source TEXT
                )
                """
            )
            conn.execute(
                """
                INSERT INTO games (game_id, white, black, moves, raw_pgn, source)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                ("legacy1", "JustAIM", "Opponent", "1. e4 c6", raw_pgn, "old import"),
            )

        migrated = SQLiteStorage(legacy_path)
        migrated.initialize()

        self.assertEqual(migrated.count_lab_games(), 1)
        game = migrated.list_lab_games(limit=None)[0]
        self.assertEqual(game.game_id, "legacy1")
        self.assertEqual(game.source_kind, "legacy")
        self.assertEqual(game.tags.get("ECO"), "B12")


class SavedPositionStorageTest(unittest.TestCase):
    def setUp(self):
        self._temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._temp_dir.cleanup)
        self.storage = SQLiteStorage(Path(self._temp_dir.name) / "position_lab.sqlite3")
        self.storage.initialize()

        self.game = parse_pgn_file(SAMPLE_PGN)[0]
        self.storage.upsert_lab_games([self.game], source_kind="lichess_user")
        self.timeline = build_game_timeline(self.game)

    def _add_position(self) -> int:
        return self.storage.add_saved_position(
            game_ref=self.game.storage_id,
            fen=self.timeline.positions[4],
            ply=4,
            move_label=self.timeline.moves[3]["label"],
            side_to_move="white",
            white=self.game.white,
            black=self.game.black,
            game_date=self.game.date,
            eco=self.game.eco,
            opening=self.game.opening,
            color="white",
            source="Lichess: JustAIM",
            source_kind="lichess_user",
            tags=["Изолированная пешка", "План", "план"],
            comment="Давить на слабое поле d5.",
            arrows=[{"from": "c3", "to": "d5", "color": "green"}],
            highlights=[{"square": "d5", "color": "red"}],
        )

    def test_saved_position_crud_annotations_and_filters(self):
        position_id = self._add_position()

        position = self.storage.get_saved_position(position_id)
        self.assertIsNotNone(position)
        self.assertEqual(position["game_ref"], self.game.storage_id)
        self.assertEqual(position["fen"], self.timeline.positions[4])
        self.assertEqual(position["ply"], 4)
        self.assertEqual(position["tags"], ["Изолированная пешка", "План"])
        self.assertEqual(
            position["arrows"],
            [{"from": "c3", "to": "d5", "color": "green"}],
        )
        self.assertEqual(
            position["highlights"],
            [{"square": "d5", "color": "red"}],
        )

        matches = self.storage.list_saved_positions(
            search="слабое поле",
            tags=["план"],
            eco="b12",
            opening="advance",
            color="white",
            source="lichess",
            date_from="2026-05-01",
            date_to="2026-05-31",
        )
        self.assertEqual([item["id"] for item in matches], [position_id])
        self.assertEqual(
            self.storage.list_saved_positions(tags=["несуществующий тег"]),
            [],
        )

        updated = self.storage.update_saved_position(
            position_id,
            tags=["Эндшпиль"],
            comment="Новый комментарий",
            arrows=[{"from": "a1", "to": "a8", "color": "blue"}],
            highlights=[{"square": "e4", "color": "yellow"}],
        )
        self.assertTrue(updated)
        position = self.storage.get_saved_position(position_id)
        self.assertEqual(position["tags"], ["Эндшпиль"])
        self.assertEqual(position["comment"], "Новый комментарий")
        self.assertEqual(position["arrows"][0]["color"], "blue")
        self.assertEqual(position["highlights"][0]["color"], "yellow")

        self.assertTrue(self.storage.delete_saved_position(position_id))
        self.assertIsNone(self.storage.get_saved_position(position_id))
        self.assertEqual(self.storage.count_saved_positions(), 0)
        self.assertFalse(self.storage.delete_saved_position(position_id))

    def test_three_review_ratings_are_kept_as_history(self):
        position_id = self._add_position()

        review_ids = [
            self.storage.record_position_review(position_id, rating)
            for rating in ("forgot", "partial", "remembered")
        ]

        self.assertEqual(len(set(review_ids)), 3)
        reviews = self.storage.list_position_reviews(position_id)
        self.assertEqual(
            [review["rating"] for review in reviews],
            ["remembered", "partial", "forgot"],
        )
        position = self.storage.get_saved_position(position_id)
        self.assertEqual(position["last_rating"], "remembered")
        self.assertEqual(position["review_count"], 3)
        self.assertTrue(position["last_reviewed_at"])

        with self.assertRaises(ValueError):
            self.storage.record_position_review(position_id, "perfect")

        self.storage.delete_saved_position(position_id)
        self.assertEqual(self.storage.list_position_reviews(position_id), [])

    def test_date_filter_excludes_positions_without_a_game_date(self):
        dated_id = self._add_position()
        undated_id = self.storage.add_saved_position(
            game_ref=None,
            fen="8/8/8/8/8/8/8/K6k w - - 0 1",
            ply=0,
            move_label="Начальная позиция",
            side_to_move="white",
            white="",
            black="",
            game_date="",
            eco="",
            opening="",
            color="unknown",
            source="External PGN",
            source_kind="external_pgn",
            tags=[],
            comment="Без даты",
            arrows=[],
            highlights=[],
        )

        self.assertNotEqual(dated_id, undated_id)
        self.assertEqual(
            [item["id"] for item in self.storage.list_saved_positions(
                date_from="2026-01-01",
            )],
            [dated_id],
        )
        self.assertEqual(
            [item["id"] for item in self.storage.list_saved_positions(
                date_to="2026-12-31",
            )],
            [dated_id],
        )


class PositionLabTimelineTest(unittest.TestCase):
    def test_sample_timeline_fen_squares_and_flip(self):
        game = parse_pgn_file(SAMPLE_PGN)[0]
        timeline = build_game_timeline(game)

        self.assertEqual(timeline.max_ply, game.ply_count)
        self.assertEqual(
            timeline.positions[0],
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        )
        self.assertEqual(
            timeline.positions[1],
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
        )
        self.assertEqual(timeline.moves[0]["uci"], "e2e4")
        self.assertEqual(timeline.moves[0]["label"], "1. e4")
        self.assertEqual(timeline.moves[1]["uci"], "c7c6")
        self.assertEqual(timeline.moves[1]["label"], "1... c6")

        normal = board_squares(
            timeline.positions[1],
            flipped=False,
            highlights=[{"square": "e4", "color": "yellow"}],
        )
        flipped = board_squares(timeline.positions[1], flipped=True)

        self.assertEqual(len(normal), 64)
        self.assertEqual(len(flipped), 64)
        self.assertEqual(normal[0]["name"], "a8")
        self.assertEqual(normal[-1]["name"], "h1")
        self.assertEqual(flipped[0]["name"], "h1")
        self.assertEqual(flipped[-1]["name"], "a8")
        self.assertEqual(
            [item["name"] for item in flipped],
            list(reversed([item["name"] for item in normal])),
        )
        self.assertEqual(normal[0]["piece_asset"], "bR.svg")
        self.assertEqual(flipped[0]["piece_asset"], "wR.svg")
        e4 = next(item for item in normal if item["name"] == "e4")
        self.assertEqual(e4["piece_asset"], "wP.svg")
        self.assertEqual(e4["highlight_color"], "yellow")

    def test_invalid_pgn_move_is_rejected_instead_of_silently_truncated(self):
        game = GameRecord(
            raw_pgn=(
                '[Event "Broken game"]\n'
                '[Result "*"]\n\n'
                "1. e4 e5 2. Bh6 *\n"
            )
        )

        with self.assertLogs("chess.pgn", level="ERROR"):
            with self.assertRaisesRegex(ValueError, "invalid move"):
                build_game_timeline(game)


if __name__ == "__main__":
    unittest.main()
