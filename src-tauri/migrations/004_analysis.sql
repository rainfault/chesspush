BEGIN IMMEDIATE;
CREATE TABLE statistics_engine_state (
 id INTEGER PRIMARY KEY CHECK(id=1), config_json TEXT, identity_json TEXT,
 paused INTEGER NOT NULL DEFAULT 0 CHECK(paused IN (0,1))
);
INSERT INTO statistics_engine_state(id) VALUES(1);
CREATE TABLE statistics_analysis_jobs (
 id INTEGER PRIMARY KEY, game_id TEXT NOT NULL REFERENCES statistics_games(id),
 movetext_revision INTEGER NOT NULL, profile_id TEXT NOT NULL,
 config_json TEXT NOT NULL, identity_json TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'queued', attempts INTEGER NOT NULL DEFAULT 0,
 progress INTEGER NOT NULL DEFAULT 0, total INTEGER NOT NULL DEFAULT 0,
 not_before_ms INTEGER NOT NULL DEFAULT 0, heartbeat_ms INTEGER,
 last_error TEXT, run_id TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
 UNIQUE(game_id,movetext_revision,profile_id)
);
CREATE INDEX statistics_analysis_job_queue ON statistics_analysis_jobs(state,not_before_ms,id);
CREATE TABLE statistics_analysis_runs (
 id TEXT PRIMARY KEY, game_id TEXT NOT NULL REFERENCES statistics_games(id),
 movetext_revision INTEGER NOT NULL, profile_id TEXT NOT NULL,
 state TEXT NOT NULL, data_json TEXT NOT NULL, completed_at_ms INTEGER,
 UNIQUE(game_id,movetext_revision,profile_id)
);
CREATE INDEX statistics_analysis_game_runs ON statistics_analysis_runs(game_id,profile_id,state);
CREATE TABLE statistics_engine_observations (
 cache_key TEXT PRIMARY KEY, context_json TEXT NOT NULL,
 evaluation_json TEXT NOT NULL, raw_info TEXT NOT NULL,
 created_at_ms INTEGER NOT NULL
);
CREATE TABLE statistics_move_evaluations (
 run_id TEXT NOT NULL REFERENCES statistics_analysis_runs(id) ON DELETE CASCADE,
 ply INTEGER NOT NULL, best_cache_key TEXT REFERENCES statistics_engine_observations(cache_key),
 played_cache_key TEXT REFERENCES statistics_engine_observations(cache_key),
 status TEXT NOT NULL, PRIMARY KEY(run_id,ply)
);
PRAGMA user_version=4;
COMMIT;
