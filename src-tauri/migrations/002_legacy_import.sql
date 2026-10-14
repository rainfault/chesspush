BEGIN IMMEDIATE;
-- Lossless transfer records from the retired Qt app, not a Knowledge subsystem.
CREATE TABLE legacy_imports (
    source_path TEXT PRIMARY KEY,
    report TEXT NOT NULL,
    imported_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE legacy_rows (
    source_path TEXT NOT NULL,
    table_name TEXT NOT NULL,
    row_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    game_id INTEGER REFERENCES games(id),
    PRIMARY KEY(source_path, table_name, row_id)
);
PRAGMA user_version=2;
COMMIT;
