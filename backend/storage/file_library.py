"""Persistent file catalog sharing the existing game store."""
from contextlib import closing
from pathlib import Path
import os


class FileLibrary:
    def __init__(self, storage):
        self.storage = storage
        with closing(storage.connect()) as db, db:
            db.executescript('''
                CREATE TABLE IF NOT EXISTS library_files (
                    id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE,
                    original_path TEXT NOT NULL DEFAULT '', kind TEXT NOT NULL,
                    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                CREATE TABLE IF NOT EXISTS library_file_games (
                    file_id INTEGER NOT NULL REFERENCES library_files(id) ON DELETE CASCADE,
                    game_id INTEGER NOT NULL REFERENCES position_lab_games(id) ON DELETE CASCADE,
                    PRIMARY KEY(file_id, game_id)
                );
            ''')

    def remember(self, path, kind='source', games=(), original_path=''):
        path = os.path.normcase(str(Path(path).resolve()))
        with closing(self.storage.connect()) as db, db:
            db.execute('''INSERT INTO library_files(path,original_path,kind) VALUES(?,?,?)
                ON CONFLICT(path) DO UPDATE SET updated_at=CURRENT_TIMESTAMP''',
                (path, str(original_path), kind))
            file_id = db.execute('SELECT id FROM library_files WHERE path=?', (path,)).fetchone()['id']
            db.executemany('INSERT OR IGNORE INTO library_file_games VALUES(?,?)',
                           [(file_id, g.storage_id) for g in games if g.storage_id])
        return file_id

    def list_files(self):
        with closing(self.storage.connect()) as db:
            records = db.execute('''SELECT f.*, COUNT(g.game_id) AS games FROM library_files f
                LEFT JOIN library_file_games g ON g.file_id=f.id
                GROUP BY f.id ORDER BY f.updated_at DESC, f.id DESC''').fetchall()
        return [dict(row, name=Path(row['path']).name, exists=Path(row['path']).is_file()) for row in records]

    def get(self, file_id):
        with closing(self.storage.connect()) as db:
            row = db.execute('SELECT * FROM library_files WHERE id=?', (file_id,)).fetchone()
        return dict(row) if row else None

    def save_games(self, games, source, output=''):
        games = list(games)
        self.storage.upsert_lab_games(games, source_kind='zst_model' if str(source).lower().endswith('.zst') else 'external_pgn')
        self.remember(source, games=games)
        if output: self.remember(output, 'export', games)
