-- Training stores only scheduling and attempts. Prepared moves remain in the
-- existing versioned KnowledgeDocument / VariationTree repertoire.
BEGIN IMMEDIATE;
CREATE TABLE repertoire_training_srs (
    decision_id TEXT PRIMARY KEY,
    side TEXT NOT NULL,
    position_key TEXT NOT NULL,
    solution_key TEXT NOT NULL,
    repetitions INTEGER NOT NULL DEFAULT 0,
    interval_days INTEGER NOT NULL DEFAULT 0,
    ease REAL NOT NULL DEFAULT 2.5,
    due_at_ms INTEGER NOT NULL,
    last_review_at_ms INTEGER NOT NULL
);
CREATE TABLE repertoire_training_sessions (
    id TEXT PRIMARY KEY,
    scope_key TEXT NOT NULL,
    mode TEXT NOT NULL,
    state_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX repertoire_training_session_scope ON repertoire_training_sessions(scope_key,mode,updated_at_ms DESC);
CREATE TABLE repertoire_training_reviews (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES repertoire_training_sessions(id),
    decision_id TEXT NOT NULL,
    answered_at_ms INTEGER NOT NULL,
    kind TEXT NOT NULL,
    uci TEXT,
    grade INTEGER
);
CREATE INDEX repertoire_training_review_decision ON repertoire_training_reviews(decision_id,answered_at_ms);
PRAGMA user_version=8;
COMMIT;
