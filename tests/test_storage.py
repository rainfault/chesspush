from pathlib import Path
import tempfile
import unittest

from backend.parsers.pgn_parser import parse_pgn_file
from backend.storage.sqlite_storage import SQLiteStorage


class StorageTest(unittest.TestCase):
    def test_insert_games(self):
        games = parse_pgn_file(Path(__file__).resolve().parents[1] / "sample_data" / "sample_games.pgn")
        with tempfile.TemporaryDirectory() as tmp:
            storage = SQLiteStorage(Path(tmp) / "test.sqlite3")
            storage.initialize()
            inserted = storage.upsert_games(games)
            self.assertEqual(inserted, 2)
            self.assertEqual(storage.count_games(), 2)


if __name__ == "__main__":
    unittest.main()
