BEGIN IMMEDIATE;
CREATE TABLE statistics_quarantine_history (
 id INTEGER PRIMARY KEY,
 account_id TEXT NOT NULL REFERENCES statistics_accounts(id),
 external_id TEXT NOT NULL,
 payload_hash TEXT,
 raw_json TEXT NOT NULL,
 reason TEXT NOT NULL,
 received_at_ms INTEGER NOT NULL,
 UNIQUE(account_id,external_id,payload_hash)
);
CREATE INDEX statistics_quarantine_history_game ON statistics_quarantine_history(account_id,external_id,received_at_ms);
-- Existing primary evidence survives the v4 upgrade; unknown legacy hashes remain NULL.
INSERT INTO statistics_quarantine_history(account_id,external_id,raw_json,reason,received_at_ms)
 SELECT account_id,external_id,raw_json,reason,received_at_ms FROM statistics_quarantine;
CREATE TRIGGER statistics_quarantine_history_no_update BEFORE UPDATE ON statistics_quarantine_history
 BEGIN SELECT RAISE(ABORT,'Raw quarantine evidence is immutable'); END;
CREATE TRIGGER statistics_quarantine_history_no_delete BEFORE DELETE ON statistics_quarantine_history
 BEGIN SELECT RAISE(ABORT,'Raw quarantine evidence is immutable'); END;
PRAGMA user_version=5;
COMMIT;
