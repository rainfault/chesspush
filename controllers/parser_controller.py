from contextlib import suppress
from datetime import datetime
from pathlib import Path
import os
import shutil
import subprocess

from PySide6.QtCore import QObject, Property, QThread, Signal, Slot, QUrl
from PySide6.QtGui import QDesktopServices

from app.paths import normalize_path
from backend.filters.game_filters import GameFilter
from backend.parsers.pgn_parser import iter_pgn_file
from backend.storage.file_library import FileLibrary
from controllers.extraction_worker import DatabaseExtractionWorker


class PgnImportWorker(QObject):
    progress = Signal('qlonglong', int, int)
    finished = Signal(object, str, str, 'qlonglong', int)
    failed = Signal(str)

    def __init__(self, path, folder, library):
        super().__init__()
        self.path, self.folder, self.library = Path(path), folder, library
        self.cancelled = False

    def cancel(self): self.cancelled = True

    @Slot()
    def run(self):
        try:
            self.folder.mkdir(parents=True, exist_ok=True)
            copy = self.folder / (self.path.stem + '_' + datetime.now().strftime('%Y%m%d_%H%M%S_%f') + '.pgn')
            shutil.copy2(self.path, copy)
            self.library.remember(copy, 'import', original_path=str(self.path))
            batch, count = [], 0
            for game in iter_pgn_file(copy):
                if self.cancelled: break
                batch.append(game)
                count += 1
                if len(batch) == 250:
                    self._save(batch, copy)
                    batch = []
                    self.progress.emit(count, count, 0)
            self._save(batch, copy)
            status = f'{"Импорт остановлен" if self.cancelled else "Импортировано"}: {count}'
            self.finished.emit([], str(copy), status, count, count)
        except Exception as exc: self.failed.emit(str(exc))

    def _save(self, games, copy):
        self.library.storage.upsert_lab_games(games, source_kind='external_pgn')
        self.library.remember(copy, 'import', games, original_path=str(self.path))


class ParserController(QObject):
    changed = Signal()
    filesChanged = Signal()
    jobFinished = Signal()

    def __init__(self, context, board):
        super().__init__()
        self.context, self.board = context, board
        self.library = FileLibrary(context.storage)
        self._source = context.settings.zst_database_path
        self._busy, self._error, self._status = False, '', ''
        self._scanned, self._found = 0, 0
        self._thread = self._worker = None
        if self._source and Path(self._source).is_file(): self.library.remember(self._source)
        self._files = self.library.list_files()

    @Property(str, notify=changed)
    def source(self): return self._source
    @Property(str, notify=changed)
    def sourceName(self): return Path(self._source).name if self._source else ''
    @Property(bool, notify=changed)
    def busy(self): return self._busy
    @Property(str, notify=changed)
    def error(self): return self._error
    @Property(str, notify=changed)
    def status(self): return self._status
    @Property('qlonglong', notify=changed)
    def scanned(self): return self._scanned
    @Property(int, notify=changed)
    def found(self): return self._found
    @Property('QVariantList', notify=filesChanged)
    def files(self): return self._files

    def refresh_files(self):
        self._files = self.library.list_files()
        self.filesChanged.emit()

    @Slot()
    def refreshFiles(self): self.refresh_files()

    @Slot(str)
    def fail(self, message):
        self._error = message
        self.changed.emit()

    @Slot(str)
    def selectSource(self, url):
        if self._busy: return
        try:
            path = Path(normalize_path(url)).resolve()
            if not path.is_file(): raise ValueError('Файл не найден')
            if path.suffix.lower() not in ('.zst', '.pgn'): raise ValueError('Выберите PGN или ZST')
            self.library.remember(path)
            self.context.update_settings(zst_database_path=str(path))
            self._source, self._error = str(path), ''
            self.refresh_files()
            self.changed.emit()
        except Exception as exc: self.fail(str(exc))

    @Slot(int)
    def useFile(self, file_id):
        record = self.library.get(file_id)
        if record: self.selectSource(record['path'])

    @Slot(int)
    def revealFile(self, file_id):
        record = self.library.get(file_id)
        if not record: return
        path = Path(record['path'])
        if not path.is_file():
            self.fail(f'Файл не найден: {path}')
            self.refresh_files()
            return
        if os.name == 'nt': subprocess.Popen(['explorer.exe', '/select,', str(path)])
        else: QDesktopServices.openUrl(QUrl.fromLocalFile(str(path.parent)))

    @Slot(str)
    def importPgn(self, url):
        if self._busy: return
        path = Path(normalize_path(url))
        if not path.is_file() or path.suffix.lower() != '.pgn':
            self.fail('Выберите файл PGN')
            return
        self.start_worker(PgnImportWorker(path, self.context.outputs_dir / 'imports', self.library))

    @Slot('QVariantMap')
    def extract(self, options):
        if self._busy: return
        try:
            if not self._source or not Path(self._source).is_file(): raise ValueError('Выберите базу PGN / ZST')
            def number(key, default, low=0, high=4000):
                value = int(str(options.get(key, default)).strip())
                if not low <= value <= high: raise ValueError(f'{key}: допустимо от {low} до {high}')
                return value
            low, high = number('minRating', 0), number('maxRating', 4000)
            black_low = number('minBlack', low) if options.get('separateRatings') else low
            black_high = number('maxBlack', high) if options.get('separateRatings') else high
            if low > high or black_low > black_high: raise ValueError('Минимальный рейтинг выше максимального')
            limit = number('limit', 50, 1, 1000000)
            start = number('startIndex', 0, 0, 2**63 - 1)
            state = self.board.session.snapshot()
            if not state['standardRoot']: raise ValueError('Для фильтра дебюта нужна линия из начальной позиции')
            perf = options.get('perf', 'any')
            flt = GameFilter(color=options.get('side', 'any'), result_mode=options.get('result', 'all'),
                min_white_elo=low, max_white_elo=high, min_black_elo=black_low, max_black_elo=black_high,
                min_moves=number('minMoves', 0, 0, 1000), move_prefix=state['pgn'],
                eco=options.get('eco', '') if not state['pgn'] else '', perf_type=perf,
                exclude_bullet=bool(options.get('excludeBullet', True)) and perf != 'bullet',
                include_timeout=bool(options.get('includeTimeout', False)),
                username=self.context.settings.lichess_username)
            self.start_worker(DatabaseExtractionWorker(database_path=self._source,
                output_folder=self.context.settings.export_folder, base_dir=self.context.base_dir,
                game_filter=flt, start_index=start, limit=limit,
                compressed_size=Path(self._source).stat().st_size, library=self.library))
        except Exception as exc: self.fail(str(exc))

    def start_worker(self, worker):
        self._busy, self._error, self._status = True, '', ''
        self._scanned = self._found = 0
        self._worker, self._thread = worker, QThread(self)
        worker.moveToThread(self._thread)
        self._thread.started.connect(worker.run)
        worker.progress.connect(self.progress)
        worker.finished.connect(self.finished)
        worker.failed.connect(self.fail)
        worker.finished.connect(self._thread.quit)
        worker.failed.connect(self._thread.quit)
        worker.finished.connect(worker.deleteLater)
        worker.failed.connect(worker.deleteLater)
        self._thread.finished.connect(self.cleanup)
        self._thread.finished.connect(self._thread.deleteLater)
        self.changed.emit()
        self._thread.start()

    @Slot('qlonglong', int, int)
    def progress(self, scanned, found, percent):
        self._scanned, self._found = scanned, found
        self.changed.emit()

    @Slot(object, str, str, 'qlonglong', int)
    def finished(self, games, path, status, scanned, found):
        self._scanned, self._found = scanned, found
        self._status = status if status.startswith('Импорт') else f'{"Остановлено" if status.startswith("Остановлено") else "Найдено"}: {found}'
        if games and not path: self._error = 'Партии сохранены в SQLite. Не удалось экспортировать PGN.'
        self.refresh_files()
        self.changed.emit()
        self.jobFinished.emit()

    @Slot()
    def cleanup(self):
        self._worker = self._thread = None
        self._busy = False
        self.refresh_files()
        self.changed.emit()

    @Slot()
    def cancel(self):
        if self._worker:
            with suppress(RuntimeError): self._worker.cancel()

    @Slot()
    def shutdown(self):
        self.cancel()
        if self._thread and self._thread.isRunning():
            self._thread.quit()
            self._thread.wait()
