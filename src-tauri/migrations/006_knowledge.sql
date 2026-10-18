BEGIN IMMEDIATE;
CREATE TABLE knowledge_documents (
 id TEXT PRIMARY KEY,
 kind TEXT NOT NULL CHECK(kind IN ('repertoire','study')),
 title TEXT NOT NULL,
 side TEXT NOT NULL CHECK(side IN ('white','black','both')),
 revision INTEGER NOT NULL CHECK(revision>0),
 created_at_ms INTEGER NOT NULL,
 updated_at_ms INTEGER NOT NULL,
 data_json TEXT NOT NULL
);
CREATE TABLE knowledge_document_versions (
 document_id TEXT NOT NULL REFERENCES knowledge_documents(id),
 revision INTEGER NOT NULL,
 data_json TEXT NOT NULL,
 created_at_ms INTEGER NOT NULL,
 PRIMARY KEY(document_id,revision)
);
-- Derived lookup only. The versioned document remains the editable source.
CREATE TABLE knowledge_node_index (
 document_id TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,
 chapter_id TEXT NOT NULL,
 node_id TEXT NOT NULL,
 document_revision INTEGER NOT NULL,
 position_key TEXT NOT NULL,
 parent_position_key TEXT,
 uci TEXT,
 PRIMARY KEY(document_id,chapter_id,node_id)
);
CREATE INDEX knowledge_nodes_position ON knowledge_node_index(position_key,document_id);
PRAGMA user_version=6;
COMMIT;
