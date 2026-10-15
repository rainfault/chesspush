BEGIN IMMEDIATE;
CREATE TABLE statistics_accounts (
 id TEXT PRIMARY KEY,
 platform TEXT NOT NULL CHECK(platform='lichess'),
 external_id TEXT NOT NULL,
 username TEXT NOT NULL,
 t0_ms INTEGER NOT NULL,
 enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
 generation INTEGER NOT NULL DEFAULT 0,
 UNIQUE(platform,external_id)
);
CREATE TABLE statistics_rating_snapshots (
 account_id TEXT NOT NULL REFERENCES statistics_accounts(id),
 pool TEXT NOT NULL,
 rating INTEGER NOT NULL CHECK(rating>0),
 timestamp_ms INTEGER NOT NULL,
 source TEXT NOT NULL CHECK(source='initial'),
 PRIMARY KEY(account_id,pool,source)
);
CREATE TABLE statistics_sync_state (
 account_id TEXT PRIMARY KEY REFERENCES statistics_accounts(id),
 highwater_ms INTEGER NOT NULL,
 last_success_ms INTEGER,
 retry_not_before_ms INTEGER,
 reconciliation_cursor_ms INTEGER NOT NULL,
 reconciliation_upper_ms INTEGER,
 last_reconciled_ms INTEGER,
 baseline_complete INTEGER NOT NULL DEFAULT 0,
 boundary_uncertain INTEGER NOT NULL DEFAULT 1,
 status TEXT NOT NULL DEFAULT 'pending',
 error_code TEXT,
 error_message TEXT,
 attempted_at_ms INTEGER
);
CREATE TABLE statistics_sync_ranges (
 id INTEGER PRIMARY KEY,
 account_id TEXT NOT NULL REFERENCES statistics_accounts(id),
 since_ms INTEGER NOT NULL,
 until_ms INTEGER NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('discovery','reconciliation')),
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','complete')),
 checked_at_ms INTEGER,
 UNIQUE(account_id,since_ms,until_ms,kind),
 CHECK(until_ms>since_ms)
);
CREATE INDEX statistics_ranges_pending ON statistics_sync_ranges(account_id,state,since_ms);
CREATE TABLE statistics_pending (
 account_id TEXT NOT NULL REFERENCES statistics_accounts(id),
 external_id TEXT NOT NULL,
 created_at_ms INTEGER,
 first_seen_ms INTEGER NOT NULL,
 last_seen_ms INTEGER NOT NULL,
 active_seen_ms INTEGER,
 status TEXT NOT NULL,
 reason TEXT,
 raw_metadata TEXT NOT NULL,
 PRIMARY KEY(account_id,external_id)
);
CREATE TABLE statistics_games (
 game_id INTEGER PRIMARY KEY REFERENCES games(id),
 id TEXT NOT NULL UNIQUE,
 source TEXT NOT NULL,
 external_id TEXT NOT NULL,
 data TEXT NOT NULL,
 movetext_hash TEXT NOT NULL,
 movetext_revision INTEGER NOT NULL DEFAULT 1,
 payload_revision INTEGER NOT NULL DEFAULT 1,
 updated_at_ms INTEGER NOT NULL,
 UNIQUE(source,external_id)
);
CREATE TABLE statistics_account_games (
 account_id TEXT NOT NULL REFERENCES statistics_accounts(id),
 game_id INTEGER NOT NULL REFERENCES games(id),
 user_color TEXT NOT NULL CHECK(user_color IN ('white','black')),
 eligibility TEXT NOT NULL CHECK(eligibility IN ('eligible','boundary_uncertain','excluded')),
 evidence TEXT NOT NULL,
 PRIMARY KEY(account_id,game_id)
);
CREATE INDEX statistics_account_membership ON statistics_account_games(account_id,eligibility,game_id);
CREATE TABLE statistics_payloads (
 id INTEGER PRIMARY KEY,
 game_id INTEGER NOT NULL REFERENCES games(id),
 payload_hash TEXT NOT NULL,
 payload_revision INTEGER NOT NULL,
 movetext_revision INTEGER NOT NULL,
 received_at_ms INTEGER NOT NULL,
 raw_json TEXT NOT NULL,
 raw_pgn TEXT NOT NULL,
 UNIQUE(game_id,payload_hash)
);
CREATE TABLE statistics_quarantine (
 account_id TEXT NOT NULL REFERENCES statistics_accounts(id),
 external_id TEXT NOT NULL,
 received_at_ms INTEGER NOT NULL,
 raw_json TEXT NOT NULL,
 reason TEXT NOT NULL,
 PRIMARY KEY(account_id,external_id)
);
PRAGMA user_version=3;
COMMIT;

