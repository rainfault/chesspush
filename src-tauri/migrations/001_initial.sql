BEGIN IMMEDIATE;
CREATE TABLE games (
    id INTEGER PRIMARY KEY, identity TEXT NOT NULL UNIQUE,
    metadata TEXT NOT NULL, tags TEXT NOT NULL, pgn TEXT NOT NULL,
    source TEXT NOT NULL, source_kind TEXT NOT NULL
);
CREATE TABLE files (
    id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, kind TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE file_games (
    file_id INTEGER NOT NULL REFERENCES files(id), game_id INTEGER NOT NULL REFERENCES games(id),
    PRIMARY KEY(file_id,game_id)
);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
PRAGMA user_version=1;
COMMIT;
