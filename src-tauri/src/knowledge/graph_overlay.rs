//! Read-only, position-keyed study projection. Prepared repertoire coverage and
//! observed game statistics never receive these semantic references.
use super::*;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KnowledgeReference {
    pub document_id: String,
    pub document_revision: i64,
    pub chapter_id: String,
    pub node_id: String,
    pub title: String,
    pub chapter_title: String,
    pub classification: String,
    pub tags: Vec<String>,
    pub provenance: String,
}
#[derive(Debug, Clone)]
pub struct KnowledgeTransition {
    pub from_key: String,
    pub to_key: String,
    pub fen: String,
    pub uci: String,
    pub san: String,
    pub reference: KnowledgeReference,
}
#[derive(Debug, Clone)]
pub struct KnowledgePosition {
    pub position_key: String,
    pub fen: String,
    pub references: Vec<KnowledgeReference>,
}

fn has_studies(conn: &Connection) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM knowledge_documents WHERE kind='study')",
        [],
        |row| row.get(0),
    )?)
}
fn reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeReference> {
    let tags: String = row.get(7)?;
    Ok(KnowledgeReference {
        document_id: row.get(0)?,
        document_revision: row.get(1)?,
        chapter_id: row.get(2)?,
        node_id: row.get(3)?,
        title: row.get(4)?,
        chapter_title: row.get(5)?,
        classification: row.get(6)?,
        tags: serde_json::from_str(&tags).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        provenance: row.get(8)?,
    })
}
const DOCUMENT_COLUMNS: &str = "d.id,d.revision,h.chapter_id,h.node_id,d.title,h.chapter_title,
    CASE WHEN (SELECT count(*) FROM knowledge_node_index z WHERE z.document_id=d.id)=1 THEN 'position' ELSE 'theory' END,
    coalesce(json_extract(d.data_json,'$.header.metadata.tags'),'[]'),h.provenance";

/// Only the requested visible position keys are looked up. Original PGN trees
/// are not decoded or scanned for every graph node.
pub fn references(
    conn: &Connection,
    keys: &[String],
) -> Result<HashMap<String, Vec<KnowledgeReference>>, AppError> {
    if keys.is_empty() || !has_studies(conn)? {
        return Ok(HashMap::new());
    }
    let query = format!("WITH hits AS (
        SELECT i.document_id,i.chapter_id,i.node_id,i.position_key,i.chapter_title,'position_match' AS provenance,0 AS rank
        FROM knowledge_node_index i WHERE i.position_key IN (SELECT value FROM json_each(?1))
        UNION ALL
        SELECT i.document_id,i.chapter_id,i.node_id,i.position_key,i.chapter_title,i.provenance,1 AS rank
        FROM knowledge_position_links i WHERE i.position_key IN (SELECT value FROM json_each(?1))
    ), ranked AS (
        SELECT *,row_number() OVER(PARTITION BY position_key,document_id ORDER BY rank,chapter_id,node_id) AS occurrence FROM hits
    ) SELECT {DOCUMENT_COLUMNS},h.position_key FROM ranked h JOIN knowledge_documents d ON d.id=h.document_id
      WHERE d.kind='study' AND h.occurrence=1 ORDER BY h.position_key,d.updated_at_ms DESC,d.id");
    let mut statement = conn.prepare(&query)?;
    let rows = statement.query_map([serde_json::to_string(keys)?], |row| {
        Ok((row.get::<_, String>(9)?, reference(row)?))
    })?;
    let mut result: HashMap<String, Vec<KnowledgeReference>> = HashMap::new();
    for row in rows {
        let (key, value) = row?;
        result.entry(key).or_default().push(value);
    }
    Ok(result)
}
/// Matching study moves remain separate by document, so repeated cycles in one
/// PGN do not duplicate a file marker. The graph merges equal legal UCI moves.
pub fn continuations(conn: &Connection, key: &str) -> Result<Vec<KnowledgeTransition>, AppError> {
    if !has_studies(conn)? {
        return Ok(Vec::new());
    }
    let query=format!("WITH hits AS (
        SELECT i.*,'position_match' AS provenance,row_number() OVER(PARTITION BY i.uci,i.document_id ORDER BY i.chapter_id,i.node_id) AS occurrence
        FROM knowledge_node_index i WHERE i.parent_position_key=?1 AND i.uci IS NOT NULL
    ) SELECT {DOCUMENT_COLUMNS},h.position_key,h.fen,h.uci FROM hits h
        JOIN knowledge_documents d ON d.id=h.document_id WHERE d.kind='study' AND h.occurrence=1
        ORDER BY h.uci,d.updated_at_ms DESC,d.id,h.chapter_id,h.node_id");
    let mut statement = conn.prepare(&query)?;
    let rows = statement.query_map([key], |row| {
        Ok((
            reference(row)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, String>(11)?,
        ))
    })?;
    let key_fen = key
        .strip_prefix("standard|")
        .ok_or_else(|| error("Некорректный позиционный ключ знания"))?;
    let before = super::position(&format!("{key_fen} 0 1"))?;
    let mut result = Vec::new();
    for row in rows {
        let (reference, to_key, fen, uci) = row?;
        let mv = uci
            .parse::<UciMove>()
            .map_err(error)?
            .to_move(&before)
            .map_err(error)?;
        let san = SanPlus::from_move(before.clone(), mv).to_string();
        result.push(KnowledgeTransition {
            from_key: key.into(),
            to_key,
            fen,
            uci,
            san,
            reference,
        });
    }
    Ok(result)
}
pub fn position(conn: &Connection, key: &str) -> Result<Option<KnowledgePosition>, AppError> {
    if !has_studies(conn)? {
        return Ok(None);
    }
    let fen: Option<String> = conn.query_row("SELECT h.fen FROM (
        SELECT document_id,fen,0 AS rank FROM knowledge_node_index WHERE position_key=?1 AND fen<>''
        UNION ALL SELECT document_id,fen,1 AS rank FROM knowledge_position_links WHERE position_key=?1 AND fen<>''
    ) h JOIN knowledge_documents d ON d.id=h.document_id WHERE d.kind='study'
      ORDER BY h.rank,d.updated_at_ms DESC,d.id LIMIT 1",[key],|row|row.get(0)).optional()?;
    let Some(fen) = fen else {
        return Ok(None);
    };
    if position_key(&super::position(&fen)?) != key {
        return Err(error("Индекс знания не соответствует FEN"));
    }
    let references = references(conn, &[key.into()])?
        .remove(key)
        .unwrap_or_default();
    Ok(Some(KnowledgePosition {
        position_key: key.into(),
        fen,
        references,
    }))
}
