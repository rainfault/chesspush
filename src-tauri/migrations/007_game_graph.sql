BEGIN IMMEDIATE;
CREATE TABLE chess_positions(position_key TEXT PRIMARY KEY,variant TEXT NOT NULL,representative_fen TEXT NOT NULL);
CREATE TABLE game_graph_games (
 game_id INTEGER PRIMARY KEY REFERENCES games(id),revision INTEGER NOT NULL,payload_revision INTEGER NOT NULL,updated_at_ms INTEGER NOT NULL,
 external_ref TEXT NOT NULL UNIQUE,source TEXT NOT NULL,variant TEXT NOT NULL,result TEXT NOT NULL,status TEXT NOT NULL,speed TEXT NOT NULL,
 rated INTEGER,control TEXT NOT NULL,timestamp_ms INTEGER,white TEXT NOT NULL,black TEXT NOT NULL,white_rating INTEGER,black_rating INTEGER,
 opening_name TEXT,opening_eco TEXT,opening_family TEXT,plies INTEGER NOT NULL,initial_fen TEXT NOT NULL
);
CREATE INDEX game_graph_games_filter ON game_graph_games(source,speed,timestamp_ms);
CREATE TABLE game_graph_revisions(game_id INTEGER NOT NULL REFERENCES games(id),revision INTEGER NOT NULL,initial_fen TEXT NOT NULL,PRIMARY KEY(game_id,revision));
CREATE TABLE game_occurrences (
 game_id INTEGER NOT NULL REFERENCES games(id),revision INTEGER NOT NULL,ply INTEGER NOT NULL,
 position_key TEXT NOT NULL REFERENCES chess_positions(position_key),fen TEXT NOT NULL,from_key TEXT REFERENCES chess_positions(position_key),
 uci TEXT,san TEXT,phase TEXT NOT NULL,clock_seconds REAL,spent_seconds REAL,
 PRIMARY KEY(game_id,revision,ply),FOREIGN KEY(game_id,revision) REFERENCES game_graph_revisions(game_id,revision)
);
CREATE INDEX game_occurrences_position ON game_occurrences(position_key,game_id,revision,ply);
CREATE INDEX game_occurrences_edge ON game_occurrences(from_key,uci,game_id,revision,ply);
CREATE TABLE game_graph_opening_labels(game_id INTEGER NOT NULL REFERENCES games(id),color TEXT NOT NULL,label TEXT NOT NULL,PRIMARY KEY(game_id,color,label));
PRAGMA user_version=7;
COMMIT;
