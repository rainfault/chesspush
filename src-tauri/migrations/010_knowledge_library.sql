-- Only derived lookup and reusable user tags are added. Versioned PGN trees,
-- repertoire schedules, factual games and their identifiers remain unchanged.
BEGIN IMMEDIATE;
CREATE TABLE knowledge_tag_history (
    tag_key TEXT PRIMARY KEY,
    tag TEXT NOT NULL,
    last_used_at_ms INTEGER NOT NULL
);
CREATE INDEX knowledge_documents_kind ON knowledge_documents(kind,updated_at_ms DESC);
ALTER TABLE knowledge_node_index ADD COLUMN fen TEXT NOT NULL DEFAULT '';
ALTER TABLE knowledge_node_index ADD COLUMN chapter_title TEXT NOT NULL DEFAULT '';
WITH source AS MATERIALIZED (
    SELECT d.id AS document_id,
           json_extract(c.value,'$.header.id') AS chapter_id,
           json_extract(n.value,'$.value.id') AS node_id,
           json_extract(n.value,'$.value.fen') AS fen,
           json_extract(c.value,'$.header.title') AS chapter_title
    FROM knowledge_documents d,
         json_each(d.data_json,'$.chapters') c,
         json_each(c.value,'$.nodes') n
)
UPDATE knowledge_node_index AS i SET fen=source.fen, chapter_title=source.chapter_title
FROM source
WHERE i.document_id=source.document_id AND i.chapter_id=source.chapter_id AND i.node_id=source.node_id;
CREATE TABLE knowledge_position_links (
    document_id TEXT NOT NULL REFERENCES knowledge_documents(id) ON DELETE CASCADE,
    chapter_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    position_key TEXT NOT NULL,
    fen TEXT NOT NULL,
    chapter_title TEXT NOT NULL,
    provenance TEXT NOT NULL,
    PRIMARY KEY(document_id,chapter_id,position_key)
);
CREATE INDEX knowledge_position_links_position ON knowledge_position_links(position_key,document_id);
INSERT OR IGNORE INTO knowledge_position_links(document_id,chapter_id,node_id,position_key,fen,chapter_title,provenance)
SELECT d.id,
       json_extract(c.value,'$.header.id'),
       json_extract(c.value,'$.header.root.id'),
       json_extract(l.value,'$.target.position_key'),
       CASE WHEN json_extract(c.value,'$.header.root.position_key')=json_extract(l.value,'$.target.position_key')
            THEN json_extract(c.value,'$.header.root.fen') ELSE '' END,
       json_extract(c.value,'$.header.title'),
       json_extract(l.value,'$.provenance')
FROM knowledge_documents d,
     json_each(d.data_json,'$.chapters') c,
     json_each(c.value,'$.header.links') l
WHERE json_extract(l.value,'$.target.type')='position';
PRAGMA user_version=10;
COMMIT;
