from pathlib import Path
import tempfile
import unittest

from PySide6.QtCore import QCoreApplication
from app.app_context import AppContext
from backend.filters.game_filters import GameFilter
from backend.parsers.pgn_parser import iter_pgn_file, parse_pgn_file
from backend.storage.file_library import FileLibrary
from controllers.extraction_worker import DatabaseExtractionWorker
from controllers.parser_controller import PgnImportWorker, ParserController
from controllers.board_controller import BoardController

ROOT = Path(__file__).resolve().parents[1]
SAMPLE = ROOT / 'sample_data/sample_games.pgn'


class LibraryTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls): cls.app = QCoreApplication.instance() or QCoreApplication([])

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.context = AppContext(Path(self.temp.name))
        self.library = FileLibrary(self.context.storage)

    def tearDown(self): self.temp.cleanup()

    def test_files_and_game_links_survive_restart_without_duplicate_rows(self):
        games = parse_pgn_file(SAMPLE)
        self.library.save_games(games, SAMPLE)
        self.library.save_games(games, SAMPLE)
        reopened = FileLibrary(AppContext(Path(self.temp.name)).storage)
        self.assertEqual(len(reopened.list_files()), 1)
        self.assertEqual(reopened.list_files()[0]['games'], 2)
        self.assertEqual(reopened.storage.count_games(), 2)

    def test_import_keeps_managed_copy_when_original_is_removed(self):
        source = Path(self.temp.name) / 'original.pgn'
        source.write_bytes(SAMPLE.read_bytes())
        worker = PgnImportWorker(source, self.context.outputs_dir / 'imports', self.library)
        errors = []
        worker.failed.connect(errors.append)
        worker.run()
        self.assertFalse(errors)
        source.unlink()
        record = self.library.list_files()[0]
        self.assertTrue(record['exists'])
        self.assertEqual(record['kind'], 'import')
        self.assertEqual(Path(record['path']).read_bytes(), SAMPLE.read_bytes())
        self.assertEqual(record['games'], 2)

    def worker(self, source=SAMPLE, limit=1):
        return DatabaseExtractionWorker(database_path=str(source), output_folder='outputs',
            base_dir=self.context.base_dir, game_filter=GameFilter(exclude_bullet=False, include_timeout=True),
            start_index=0, limit=limit, compressed_size=source.stat().st_size, library=self.library)

    def test_extraction_saves_pgn_and_games_without_opening_another_page(self):
        worker = self.worker()
        errors = []
        worker.failed.connect(errors.append)
        worker.run()
        self.assertFalse(errors)
        records = self.library.list_files()
        self.assertEqual(len(records), 2)
        exported = next(r for r in records if r['kind'] == 'export')
        self.assertEqual(len(parse_pgn_file(exported['path'])), 1)
        self.assertEqual(self.context.storage.count_games(), 1)

    def test_cancel_after_first_match_preserves_partial_result(self):
        worker = self.worker(limit=100)
        worker.progress.connect(lambda scanned, found, percent: worker.cancel() if found == 1 else None)
        worker.run()
        self.assertEqual(self.context.storage.count_games(), 1)
        self.assertEqual(len(self.library.list_files()), 2)

    def test_compressed_source_uses_same_persistent_game_store(self):
        import zstandard
        source = Path(self.temp.name) / 'sample.pgn.zst'
        source.write_bytes(zstandard.ZstdCompressor().compress(SAMPLE.read_bytes()))
        self.worker(source, limit=2).run()
        self.assertEqual(self.context.storage.count_games(), 2)
        self.assertEqual(self.library.list_files()[0]['games'], 2)

    def test_missing_file_stays_in_history(self):
        path = Path(self.temp.name) / 'gone.zst'
        path.write_bytes(b'')
        file_id = self.library.remember(path)
        path.unlink()
        self.assertEqual(self.library.list_files()[0]['id'], file_id)
        self.assertFalse(self.library.list_files()[0]['exists'])

    def test_streaming_pgn_matches_existing_parser_with_bom(self):
        path = Path(self.temp.name) / 'bom.pgn'
        path.write_text(SAMPLE.read_text(encoding='utf-8'), encoding='utf-8-sig')
        self.assertEqual([g.moves for g in iter_pgn_file(path)], [g.moves for g in parse_pgn_file(SAMPLE)])

    def test_invalid_filter_never_starts_a_worker(self):
        controller = ParserController(self.context, BoardController(ROOT))
        controller.selectSource(str(SAMPLE))
        controller.extract({'minRating': '3000', 'maxRating': '1000'})
        self.assertFalse(controller.busy)
        self.assertTrue(controller.error)
        controller.extract({'limit': '0'})
        self.assertFalse(controller.busy)
