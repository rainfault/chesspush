-- Existing due dates and review history remain unchanged. SM-2 state is
-- converted with FSRS's official migration helper on the next scored review.
BEGIN IMMEDIATE;
ALTER TABLE repertoire_training_srs ADD COLUMN stability REAL;
ALTER TABLE repertoire_training_srs ADD COLUMN difficulty REAL;
ALTER TABLE repertoire_training_srs ADD COLUMN scheduler_version INTEGER NOT NULL DEFAULT 0;
PRAGMA user_version=9;
COMMIT;
