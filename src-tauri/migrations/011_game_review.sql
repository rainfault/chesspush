-- Expand the existing document discriminator without touching canonical JSON,
-- node IDs, document revisions or existing foreign-key targets.
PRAGMA foreign_keys=OFF;
BEGIN IMMEDIATE;
CREATE TABLE knowledge_documents_new (
 id TEXT PRIMARY KEY,
 kind TEXT NOT NULL CHECK(kind IN ('repertoire','study','analysis')),
 title TEXT NOT NULL,
 side TEXT NOT NULL CHECK(side IN ('white','black','both')),
 revision INTEGER NOT NULL CHECK(revision>0),
 created_at_ms INTEGER NOT NULL,
 updated_at_ms INTEGER NOT NULL,
 data_json TEXT NOT NULL
);
INSERT INTO knowledge_documents_new SELECT * FROM knowledge_documents;
DROP TABLE knowledge_documents;
ALTER TABLE knowledge_documents_new RENAME TO knowledge_documents;
CREATE INDEX knowledge_documents_kind ON knowledge_documents(kind,updated_at_ms DESC);
CREATE TABLE game_review_documents (
 game_id TEXT NOT NULL REFERENCES statistics_games(id),
 account_id TEXT NOT NULL REFERENCES statistics_accounts(id),
 source_revision INTEGER NOT NULL,
 document_id TEXT NOT NULL UNIQUE REFERENCES knowledge_documents(id),
 reviewed INTEGER NOT NULL DEFAULT 0 CHECK(reviewed IN (0,1)),
 updated_at_ms INTEGER NOT NULL,
 PRIMARY KEY(game_id,account_id,source_revision)
);
CREATE INDEX game_review_source ON game_review_documents(document_id);
PRAGMA user_version=11;
COMMIT;
PRAGMA foreign_keys=ON;
