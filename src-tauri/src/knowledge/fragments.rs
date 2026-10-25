//! Attached PGNs remain ordinary versioned KnowledgeDocuments. The combined
//! repertoire is a disposable projection of their shared VariationTrees.
use super::*;
use std::{fs, path::Path};

const ATTACHMENT: &str = "repertoire_attachment";
const MAX_ATTACHMENT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FragmentOpening {
    pub eco: String,
    pub name: String,
}

/// Original named chapters remain addressable even when the combined chess
/// projection merges their common root and prepared moves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceChapter {
    pub document_id: String,
    pub chapter_id: String,
    pub title: String,
    pub document_title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub root_node_id: String,
    pub initial_fen: String,
}

pub fn is_attachment(document: &KnowledgeDocument) -> bool {
    document.metadata.contains_key(ATTACHMENT)
}
pub fn is_active(document: &KnowledgeDocument) -> bool {
    document
        .metadata
        .get(ATTACHMENT)
        .and_then(|value| value.get("active"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}
fn filename(document: &KnowledgeDocument) -> Option<String> {
    document
        .metadata
        .get(ATTACHMENT)
        .and_then(|value| value.get("filename"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Classification follows actual legal positions in every RAV, with the most
/// specific known position on each leaf. Header claims never set these labels.
pub fn classify(document: &KnowledgeDocument) -> Vec<FragmentOpening> {
    fn walk(
        node: &VariationNode,
        previous: Option<FragmentOpening>,
        out: &mut Vec<FragmentOpening>,
    ) {
        let current = crate::statistics::chess::opening_for_position(&node.position_key)
            .map(|opening| FragmentOpening {
                eco: opening.eco,
                name: opening.name,
            })
            .or(previous);
        if node.children.is_empty() {
            if let Some(current) = current {
                out.push(current);
            }
        } else {
            for child in &node.children {
                walk(child, current.clone(), out);
            }
        }
    }
    let initial_key = position_key(&Chess::default());
    let mut result = vec![];
    for chapter in &document.chapters {
        // A FEN fragment does not prove which opening path led to that position.
        if chapter.root.position_key == initial_key {
            walk(&chapter.root, None, &mut result);
        }
    }
    result.sort();
    result.dedup();
    result
}
pub fn summary(document: &KnowledgeDocument) -> DocumentSummary {
    DocumentSummary {
        id: document.id.clone(),
        kind: document.kind.clone(),
        title: document.title.clone(),
        side: document.side.clone(),
        revision: document.revision,
        chapters: document.chapters.len(),
        updated_at_ms: document.updated_at_ms,
        attachment: is_attachment(document),
        attachment_active: is_active(document),
        filename: filename(document),
        openings: classify(document),
    }
}

pub fn attach_file(
    conn: &Connection,
    path: &Path,
    side: &str,
) -> Result<KnowledgeDocument, AppError> {
    if !["white", "black"].contains(&side) {
        return Err(error("Выберите сторону репертуара"));
    }
    if !path
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("pgn"))
    {
        return Err(error("Выберите файл PGN"));
    }
    if fs::metadata(path).map_err(error)?.len() > MAX_ATTACHMENT_BYTES {
        return Err(error(
            "Размер PGN превышает 8 МБ. Разделите репертуар на фрагменты.",
        ));
    }
    let pgn = fs::read_to_string(path).map_err(|_| error("Не удалось прочитать PGN в UTF-8"))?;
    let chapters = parse_pgn(pgn.trim_start_matches('\u{feff}'))?;
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("repertoire.pgn");
    // Repeating the same picker selection is idempotent. Edited documents are
    // preserved; attaching the original file never replaces the user's changes.
    for item in list(conn)?.into_iter().filter(|item| {
        item.attachment && item.side == side && item.filename.as_deref() == Some(filename)
    }) {
        let existing = load(conn, &item.id, None)?;
        if existing.metadata[ATTACHMENT]["source_pgn"].as_str() == Some(pgn.as_str()) {
            return if is_active(&existing) {
                Ok(existing)
            } else {
                set_attachment(conn, &existing.id, existing.revision, true)
            };
        }
    }
    let timestamp = now_ms();
    let mut metadata = BTreeMap::new();
    metadata.insert(
        ATTACHMENT.into(),
        serde_json::json!({
            "filename": filename, "active": true, "source_pgn": pgn,
            "attached_at_ms": timestamp
        }),
    );
    let document = KnowledgeDocument {
        id: new_id("knowledge")?,
        kind: "repertoire".into(),
        title: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("PGN")
            .into(),
        side: side.into(),
        revision: 1,
        created_at_ms: timestamp,
        updated_at_ms: timestamp,
        metadata,
        chapters,
    };
    persist(conn, &document, None)?;
    Ok(document)
}

pub fn set_attachment(
    conn: &Connection,
    document_id: &str,
    expected_revision: i64,
    active: bool,
) -> Result<KnowledgeDocument, AppError> {
    let mut document = load(conn, document_id, None)?;
    if document.revision != expected_revision {
        return Err(AppError::new(
            "conflict",
            "Документ уже изменён. Обновите репертуар.",
        ));
    }
    if document.kind != "repertoire" {
        return Err(error("Этот документ не является репертуаром"));
    }
    let attachment = document.metadata.entry(ATTACHMENT.into()).or_insert_with(
        || serde_json::json!({ "filename": format!("{}.pgn", document.title), "active": false }),
    );
    attachment
        .as_object_mut()
        .ok_or_else(|| error("Повреждены данные прикреплённого файла"))?
        .insert("active".into(), Value::Bool(active));
    document.revision += 1;
    document.updated_at_ms = now_ms();
    persist(conn, &document, Some(expected_revision))?;
    Ok(document)
}

/// Permanently remove one prepared document, its history and derived index.
/// Actual observed games, analysis evidence and the original file are independent.
pub fn delete_repertoire(
    conn: &Connection,
    document_id: &str,
    expected_revision: i64,
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    let current: Option<(String, i64)> = tx
        .query_row(
            "SELECT kind,revision FROM knowledge_documents WHERE id=?1",
            [document_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (kind, revision) =
        current.ok_or_else(|| AppError::new("not_found", "Репертуар уже удалён или не найден"))?;
    if kind != "repertoire" {
        return Err(error("Этот документ не является репертуаром"));
    }
    if revision != expected_revision {
        return Err(AppError::new(
            "conflict",
            "Документ уже изменён. Обновите репертуар перед удалением.",
        ));
    }
    tx.execute(
        "DELETE FROM knowledge_node_index WHERE document_id=?1",
        [document_id],
    )?;
    tx.execute(
        "DELETE FROM knowledge_document_versions WHERE document_id=?1",
        [document_id],
    )?;
    let changed = tx.execute(
        "DELETE FROM knowledge_documents WHERE id=?1 AND kind='repertoire' AND revision=?2",
        params![document_id, expected_revision],
    )?;
    if changed != 1 {
        return Err(AppError::new(
            "conflict",
            "Репертуар изменён перед удалением",
        ));
    }
    tx.commit()?;
    Ok(())
}

fn append_unique<T: Clone + PartialEq>(target: &mut Vec<T>, values: &[T]) {
    for value in values {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}
fn source_link(document: &KnowledgeDocument, chapter: &Chapter, node: &VariationNode) -> Value {
    serde_json::json!({ "document_id": document.id, "revision": document.revision,
        "chapter_id": chapter.id, "node_id": node.id, "title": document.title,
        "filename": filename(document) })
}

/// Deduplicate common prefixes without collapsing distinct occurrences or cycles.
/// Transposed paths remain separate in the PGN tree and share PositionKeys for
/// the graph's positional projection. Sources retain every original node ID.
pub fn combined(conn: &Connection, side: &str) -> Result<KnowledgeDocument, AppError> {
    if !["white", "black"].contains(&side) {
        return Err(error("Выберите сторону репертуара"));
    }
    let mut result = KnowledgeDocument {
        id: format!("repertoire-union-{side}"),
        kind: "repertoire".into(),
        title: if side == "white" {
            "Репертуар за белых"
        } else {
            "Репертуар за чёрных"
        }
        .into(),
        side: side.into(),
        revision: 1,
        created_at_ms: 0,
        updated_at_ms: 0,
        metadata: BTreeMap::new(),
        chapters: vec![],
    };
    let mut sources = serde_json::Map::new();
    fn merge(
        target: &mut VariationNode,
        source: &VariationNode,
        document: &KnowledgeDocument,
        chapter: &Chapter,
        sources: &mut serde_json::Map<String, Value>,
        node_count: &mut usize,
    ) -> Result<(), AppError> {
        use sha2::{Digest, Sha256};
        let origin = source_link(document, chapter, source);
        let entry = sources
            .entry(target.id.clone())
            .or_insert_with(|| Value::Array(vec![]));
        let origins = entry.as_array_mut().expect("origin arrays created locally");
        if !origins.contains(&origin) {
            origins.push(origin);
        }
        append_unique(&mut target.comments, &source.comments);
        append_unique(&mut target.starting_comments, &source.starting_comments);
        append_unique(&mut target.nags, &source.nags);
        append_unique(&mut target.shapes, &source.shapes);
        for child in &source.children {
            let index = target.children.iter().position(|value| {
                value.uci == child.uci && value.position_key == child.position_key
            });
            let index = if let Some(index) = index {
                index
            } else {
                *node_count += 1;
                if *node_count > MAX_NODES || target.ply as usize >= MAX_DEPTH {
                    return Err(error(
                        "Общий репертуар превышает предел 50 000 позиций или 512 полуходов в ветви",
                    ));
                }
                let mut clone = VariationNode {
                    id: child.id.clone(),
                    fen: child.fen.clone(),
                    position_key: child.position_key.clone(),
                    ply: child.ply,
                    uci: child.uci.clone(),
                    san: child.san.clone(),
                    comments: child.comments.clone(),
                    starting_comments: child.starting_comments.clone(),
                    nags: child.nags.clone(),
                    shapes: child.shapes.clone(),
                    children: vec![],
                };
                // A source FEN may have different half/fullmove counters. The
                // derived occurrence is replayed in its actual receiving path.
                let mut before = position(&target.fen)?;
                let mv = child
                    .uci
                    .as_deref()
                    .ok_or_else(|| error("У хода отсутствует UCI"))?
                    .parse::<UciMove>()
                    .map_err(error)?
                    .to_move(&before)
                    .map_err(error)?;
                clone.san = Some(SanPlus::from_move(before.clone(), mv).to_string());
                before.play_unchecked(mv);
                clone.fen = Fen::from_position(&before, EnPassantMode::Always).to_string();
                clone.ply = target.ply + 1;
                clone.id = format!(
                    "union-node-{:x}",
                    Sha256::digest(
                        format!("{}|{}", target.id, child.uci.as_deref().unwrap()).as_bytes()
                    )
                );
                target.children.push(clone);
                target.children.len() - 1
            };
            merge(
                &mut target.children[index],
                child,
                document,
                chapter,
                sources,
                node_count,
            )?;
        }
        Ok(())
    }
    fn matching_paths(
        node: &VariationNode,
        key: &str,
        prefix: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if node.position_key == key {
            out.push(prefix.clone());
        }
        for (index, child) in node.children.iter().enumerate() {
            prefix.push(index);
            matching_paths(child, key, prefix, out);
            prefix.pop();
        }
    }
    fn link(document: &KnowledgeDocument, chapter: &Chapter) -> KnowledgeLink {
        KnowledgeLink {
            target: LinkTarget::DocumentNode {
                document_id: document.id.clone(),
                revision: document.revision,
                chapter_id: chapter.id.clone(),
                node_id: chapter.root.id.clone(),
            },
            provenance: filename(document).unwrap_or_else(|| document.title.clone()),
            path_uci: vec![],
            initial_fen: Some(chapter.initial_fen.clone()),
        }
    }
    let mut summaries = list(conn)?;
    summaries.sort_by(|a, b| a.id.cmp(&b.id));
    let mut documents = vec![];
    for item in summaries.into_iter().filter(|item| {
        item.kind == "repertoire"
            && item.attachment
            && item.attachment_active
            && (item.side == side || item.side == "both")
    }) {
        documents.push(load(conn, &item.id, None)?);
    }
    let attached_ids: Vec<_> = documents
        .iter()
        .map(|document| document.id.clone())
        .collect();
    let source_chapters = documents
        .iter()
        .flat_map(|document| {
            document.chapters.iter().map(|chapter| SourceChapter {
                document_id: document.id.clone(),
                chapter_id: chapter.id.clone(),
                title: chapter.title.clone(),
                document_title: document.title.clone(),
                filename: filename(document),
                root_node_id: chapter.root.id.clone(),
                initial_fen: chapter.initial_fen.clone(),
            })
        })
        .collect::<Vec<_>>();
    let initial_key = position_key(&Chess::default());
    let mut pending = vec![];
    for (document_index, document) in documents.iter().enumerate() {
        result.updated_at_ms = result.updated_at_ms.max(document.updated_at_ms);
        result.revision = result.revision.max(document.revision);
        for chapter_index in 0..document.chapters.len() {
            pending.push((document_index, chapter_index));
        }
    }
    pending.sort_by_key(|(document, chapter)| {
        let chapter = &documents[*document].chapters[*chapter];
        (
            chapter.root.position_key != initial_key,
            chapter.initial_fen != INITIAL_FEN,
        )
    });
    let mut node_count = 0;
    while !pending.is_empty() {
        // Prefer a fragment whose root already occurs in the projection. If no
        // connection is known, retain one explicit independent starting position
        // and let subsequent fragments connect to that position.
        let mut choice = None;
        for (pending_index, (di, ci)) in pending.iter().enumerate() {
            let source = &documents[*di].chapters[*ci];
            let mut targets = vec![];
            for (target_index, target) in result.chapters.iter().enumerate() {
                let mut paths = vec![];
                matching_paths(
                    &target.root,
                    &source.root.position_key,
                    &mut vec![],
                    &mut paths,
                );
                // Standard-root documents merge only at the root. Repeated
                // start positions in a knight cycle are not a request to unfold
                // the entire opening again at every repetition.
                if source.root.position_key == initial_key {
                    paths.retain(Vec::is_empty);
                }
                for path in paths {
                    targets.push((target_index, path));
                }
            }
            if !targets.is_empty() {
                choice = Some((pending_index, targets));
                break;
            }
        }
        let seed = if result.chapters.is_empty()
            && documents[pending[0].0].chapters[pending[0].1]
                .root
                .position_key
                == initial_key
        {
            0
        } else if choice.is_none() {
            // Prefer the upstream fragment even when files were attached in the
            // opposite order (B's FEN root may already occur inside A's line).
            pending
                .iter()
                .enumerate()
                .min_by_key(|(index, (di, ci))| {
                    let key = &documents[*di].chapters[*ci].root.position_key;
                    pending
                        .iter()
                        .enumerate()
                        .filter(|(other, (odi, oci))| {
                            if other == index {
                                return false;
                            }
                            let root = &documents[*odi].chapters[*oci].root;
                            if &root.position_key == key {
                                return false;
                            }
                            let mut matches = vec![];
                            matching_paths(root, key, &mut vec![], &mut matches);
                            !matches.is_empty()
                        })
                        .count()
                })
                .map(|(index, _)| index)
                .unwrap_or(0)
        } else {
            0
        };
        let (pending_index, targets) = choice.unwrap_or((seed, vec![]));
        let (di, ci) = pending.remove(pending_index);
        let document = &documents[di];
        let source = &document.chapters[ci];
        if targets.is_empty() {
            node_count += 1;
            let mut target = source.clone();
            target.root.children.clear();
            target.source_pgn = None;
            // A combined preparation tree is not the played result or claimed
            // opening of whichever source happened to be encountered first.
            target.headers.insert("Result".into(), "*".into());
            for key in ["ECO", "Opening", "Variation"] {
                target.headers.remove(key);
            }
            target.title = if source.root.position_key == initial_key {
                "Общий репертуар".into()
            } else {
                source.title.clone()
            };
            append_unique(&mut target.links, &[link(document, source)]);
            merge(
                &mut target.root,
                &source.root,
                document,
                source,
                &mut sources,
                &mut node_count,
            )?;
            result.chapters.push(target);
        } else {
            // Targets are a snapshot taken before the graft; newly added cycle
            // occurrences are not recursively visited in this operation.
            for (chapter_index, path) in targets {
                let chapter = &mut result.chapters[chapter_index];
                append_unique(&mut chapter.links, &source.links);
                append_unique(&mut chapter.links, &[link(document, source)]);
                let target = at_mut(&mut chapter.root, &path);
                merge(
                    target,
                    &source.root,
                    document,
                    source,
                    &mut sources,
                    &mut node_count,
                )?;
            }
        }
    }
    result
        .metadata
        .insert("repertoire_sources".into(), Value::Object(sources));
    result.metadata.insert(
        "source_document_ids".into(),
        serde_json::json!(attached_ids),
    );
    result.metadata.insert(
        "source_chapters".into(),
        serde_json::to_value(source_chapters).map_err(error)?,
    );
    result
        .metadata
        .insert("read_only_projection".into(), Value::Bool(true));
    if !result.chapters.is_empty() {
        validate_document(&result)?;
    }
    Ok(result)
}

pub fn export_combined(conn: &Connection, side: &str, path: &Path) -> Result<(), AppError> {
    let document = combined(conn, side)?;
    if document.chapters.is_empty() {
        return Err(error("В репертуаре пока нет прикреплённых PGN"));
    }
    if !path
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("pgn"))
    {
        return Err(error("Укажите расширение .pgn"));
    }
    fs::write(path, export_document(&document)?).map_err(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/006_knowledge.sql"))
            .unwrap();
        conn
    }
    fn attach(conn: &Connection, pgn: &str, side: &str) -> KnowledgeDocument {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("fragment.pgn");
        fs::write(&path, pgn).unwrap();
        attach_file(conn, &path, side).unwrap()
    }
    #[test]
    fn delete_removes_only_selected_repertoire_and_versions_not_file_games_or_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(&temp.path().join("test.sqlite3")).unwrap();
        let source = temp.path().join("original.pgn");
        let pgn = "1.e4 e5 2.Nf3 *";
        fs::write(&source, pgn).unwrap();
        let original = attach_file(&conn, &source, "white").unwrap();
        let document = set_attachment(&conn, &original.id, original.revision, false).unwrap();
        let remaining = attach(&conn, "1.d4 d5 *", "black");
        conn.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES('actual','{}','{}','1.d4 d5 1-0','lichess','my_games')", []).unwrap();
        conn.execute("INSERT INTO statistics_engine_observations VALUES('actual-eval','{}','{}','info score cp 34',1)", []).unwrap();
        delete_repertoire(&conn, &document.id, document.revision).unwrap();
        for table in [
            "knowledge_documents",
            "knowledge_document_versions",
            "knowledge_node_index",
        ] {
            let key = if table == "knowledge_documents" {
                "id"
            } else {
                "document_id"
            };
            let count: i64 = conn
                .query_row(
                    &format!("SELECT count(*) FROM {table} WHERE {key}=?1"),
                    [&document.id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0);
        }
        assert!(load(&conn, &document.id, None).is_err());
        assert!(load(&conn, &document.id, Some(1)).is_err());
        assert_eq!(load(&conn, &remaining.id, None).unwrap(), remaining);
        assert_eq!(fs::read_to_string(&source).unwrap(), pgn);
        let actual: String = conn
            .query_row("SELECT pgn FROM games WHERE identity='actual'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(actual, "1.d4 d5 1-0");
        let evaluation: String = conn
            .query_row(
                "SELECT raw_info FROM statistics_engine_observations WHERE cache_key='actual-eval'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(evaluation, "info score cp 34");
        let violations: i64 = conn
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
    }

    #[test]
    fn delete_rejects_stale_revision_non_repertoire_and_missing_without_mutation() {
        let conn = db();
        let original = attach(&conn, "1.e4 e5 *", "white");
        let document = set_attachment(&conn, &original.id, original.revision, false).unwrap();
        let study = create(
            &conn,
            CreateRequest {
                title: "Study".into(),
                kind: "study".into(),
                side: "white".into(),
                initial_fen: None,
                links: vec![],
            },
        )
        .unwrap();
        let changes = conn.total_changes();
        assert_eq!(
            delete_repertoire(&conn, &document.id, original.revision)
                .unwrap_err()
                .code,
            "conflict"
        );
        assert!(delete_repertoire(&conn, &study.id, study.revision).is_err());
        assert_eq!(
            delete_repertoire(&conn, "missing", 1).unwrap_err().code,
            "not_found"
        );
        assert_eq!(conn.total_changes(), changes);
        assert_eq!(load(&conn, &document.id, None).unwrap(), document);
        assert_eq!(load(&conn, &study.id, None).unwrap(), study);
        assert_eq!(load(&conn, &original.id, Some(1)).unwrap(), original);
    }

    #[test]
    fn delete_rolls_back_versions_and_index_on_storage_failure() {
        let conn = db();
        let original = attach(&conn, "1.e4 e5 *", "white");
        let document = set_attachment(&conn, &original.id, original.revision, false).unwrap();
        let index_before: i64 = conn
            .query_row(
                "SELECT count(*) FROM knowledge_node_index WHERE document_id=?1",
                [&document.id],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute_batch("CREATE TRIGGER reject_document_delete BEFORE DELETE ON knowledge_documents BEGIN SELECT RAISE(ABORT, 'simulated storage failure'); END;").unwrap();
        assert!(delete_repertoire(&conn, &document.id, document.revision).is_err());
        assert_eq!(load(&conn, &document.id, None).unwrap(), document);
        assert_eq!(load(&conn, &original.id, Some(1)).unwrap(), original);
        let index_after: i64 = conn
            .query_row(
                "SELECT count(*) FROM knowledge_node_index WHERE document_id=?1",
                [&document.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(index_after, index_before);
    }

    #[test]
    fn files_merge_common_prefix_rav_annotations_and_provenance_without_factual_games() {
        let conn = db();
        let a = attach(
            &conn,
            "[Event \"A\"]\n\n1.e4 $1 {plan A [%cal Ge2e4]} e5 2.Nf3 *",
            "white",
        );
        let b = attach(
            &conn,
            "[Event \"B\"]\n\n1.e4 $220 {plan B} c5 (1...e5 2.Bc4) *",
            "white",
        );
        let black = attach(&conn, "1.d4 d5 *", "black");
        let combined = combined(&conn, "white").unwrap();
        assert_eq!(combined.chapters.len(), 1);
        let root = &combined.chapters[0].root;
        assert_eq!(root.children.len(), 1);
        let e4 = &root.children[0];
        assert_eq!(e4.children.len(), 2);
        assert!(e4.nags.contains(&1) && e4.nags.contains(&220));
        assert!(e4.comments.iter().any(|value| value.contains("plan A")));
        assert!(e4.comments.iter().any(|value| value.contains("plan B")));
        assert_eq!(e4.shapes.len(), 1);
        let e5 = e4
            .children
            .iter()
            .find(|node| node.san.as_deref() == Some("e5"))
            .unwrap();
        assert_eq!(e5.children.len(), 2);
        let origins = combined.metadata["repertoire_sources"][&e4.id]
            .as_array()
            .unwrap();
        assert_eq!(origins.len(), 2);
        assert_eq!(
            combined.metadata["source_document_ids"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(
            !combined.metadata["source_document_ids"]
                .as_array()
                .unwrap()
                .contains(&Value::String(black.id))
        );
        let output = export_document(&combined).unwrap();
        let parsed = parse_pgn(&output).unwrap();
        assert_eq!(parsed[0].root.children[0].children.len(), 2);
        assert!(output.contains("$220") && output.contains("plan A") && output.contains("plan B"));
        assert_eq!(load(&conn, &a.id, None).unwrap(), a);
        assert_eq!(load(&conn, &b.id, None).unwrap(), b);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn detach_is_versioned_reversible_and_keeps_original_pgn_and_node_ids() {
        let conn = db();
        let input = "[Event \"Source\"]\n\n1.e4 e5 *";
        let original = attach(&conn, input, "white");
        assert_eq!(original.metadata[ATTACHMENT]["source_pgn"], input);
        let detached = set_attachment(&conn, &original.id, 1, false).unwrap();
        assert_eq!(detached.revision, 2);
        assert!(!summary(&detached).attachment_active);
        assert!(combined(&conn, "white").unwrap().chapters.is_empty());
        assert!(
            overlay(&conn, std::slice::from_ref(&original.id))
                .unwrap()
                .is_empty()
        );
        assert_eq!(load(&conn, &original.id, Some(1)).unwrap(), original);
        let attached = set_attachment(&conn, &original.id, 2, true).unwrap();
        assert_eq!(attached.chapters, original.chapters);
        assert!(!combined(&conn, "white").unwrap().chapters.is_empty());
        assert!(set_attachment(&conn, &original.id, 1, false).is_err());
    }
    #[test]
    fn opening_labels_follow_legal_positions_not_untrusted_header_claims() {
        let conn = db();
        let document = attach(
            &conn,
            "[ECO \"Z99\"]\n[Opening \"Invented\"]\n\n1.e4 c5 2.Nf3 d6 3.d4 cxd4 4.Nxd4 Nf6 5.Nc3 a6 *",
            "white",
        );
        let labels = classify(&document);
        assert!(
            labels
                .iter()
                .any(|label| label.eco == "B90" && label.name.contains("Najdorf"))
        );
        assert!(labels.iter().all(|label| label.eco != "Z99"));
        let custom = attach(
            &conn,
            "[SetUp \"1\"]\n[FEN \"7k/8/8/8/8/8/1K6/8 w - - 0 1\"]\n\n1.Kc3 *",
            "white",
        );
        assert!(classify(&custom).is_empty());
    }
    #[test]
    fn malformed_file_does_not_create_a_document_and_manual_documents_are_not_auto_attached() {
        let conn = db();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("broken.pgn");
        fs::write(&path, "1.e4 e5 2.Ke4 *").unwrap();
        assert!(attach_file(&conn, &path, "white").is_err());
        assert!(list(&conn).unwrap().is_empty());
        let manual = create(
            &conn,
            CreateRequest {
                title: "Existing".into(),
                kind: "repertoire".into(),
                side: "white".into(),
                initial_fen: None,
                links: vec![],
            },
        )
        .unwrap();
        assert!(!summary(&manual).attachment);
        assert!(combined(&conn, "white").unwrap().chapters.is_empty());
    }
    #[test]
    fn transposed_paths_and_repeated_positions_remain_distinct_occurrences_in_union() {
        let conn = db();
        attach(&conn, "1.Nf3 Nf6 2.g3 g6 3.Bg2 Bg7 *", "white");
        attach(&conn, "1.g3 g6 2.Nf3 Nf6 3.Bg2 Bg7 *", "white");
        attach(&conn, "1.Nf3 Nf6 2.Ng1 Ng8 *", "white");
        let document = combined(&conn, "white").unwrap();
        let root = &document.chapters[0].root;
        assert_eq!(root.children.len(), 2);
        let a = &root
            .children
            .iter()
            .find(|node| node.san.as_deref() == Some("Nf3"))
            .unwrap()
            .children[0];
        let b = &root
            .children
            .iter()
            .find(|node| node.san.as_deref() == Some("g3"))
            .unwrap()
            .children[0];
        let mut a = a
            .children
            .iter()
            .find(|node| node.san.as_deref() == Some("g3"))
            .unwrap();
        a = &a.children[0];
        let b = &b.children[0].children[0];
        assert_eq!(a.position_key, b.position_key);
        assert_ne!(a.id, b.id);
        let ng1 = root
            .children
            .iter()
            .find(|node| node.san.as_deref() == Some("Nf3"))
            .unwrap()
            .children[0]
            .children
            .iter()
            .find(|node| node.san.as_deref() == Some("Ng1"))
            .unwrap();
        assert_eq!(ng1.children[0].position_key, root.position_key);
    }
    #[test]
    fn fen_fragment_grafts_to_known_position_replays_counters_and_keeps_original_ids() {
        let conn = db();
        let a = attach(&conn, "1.e4 e5 2.Nf3 Nc6 *", "white");
        let target = &a.chapters[0].root.children[0].children[0].children[0].children[0];
        let mut fen = target.fen.split_whitespace().collect::<Vec<_>>();
        // A catalogue fragment may start with reset counters; its identity still
        // matches while each resulting occurrence retains the receiving path.
        fen[4] = "0";
        fen[5] = "1";
        let source = attach(
            &conn,
            &format!(
                "[SetUp \"1\"]\n[FEN \"{}\"]\n\n1.Bc4 {{from FEN}} Bc5 (1...Nf6) *",
                fen.join(" ")
            ),
            "white",
        );
        let merged = combined(&conn, "white").unwrap();
        assert_eq!(merged.chapters.len(), 1);
        let target = &merged.chapters[0].root.children[0].children[0].children[0].children[0];
        assert_eq!(target.children[0].san.as_deref(), Some("Bc4"));
        assert_eq!(target.children[0].ply, 5);
        assert_eq!(target.children[0].children.len(), 2);
        assert!(export_document(&merged).unwrap().contains("3. Bc4"));
        assert_eq!(load(&conn, &source.id, None).unwrap(), source);
        let origin = merged.metadata["repertoire_sources"][&target.children[0].id]
            .as_array()
            .unwrap();
        assert!(
            origin
                .iter()
                .any(|value| value["node_id"] == source.chapters[0].root.children[0].id)
        );
        validate_document(&merged).unwrap();
    }
    #[test]
    fn disconnected_fen_fragments_join_in_upstream_order_and_unrelated_roots_remain_separate() {
        let conn = db();
        let position =
            position("rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2").unwrap();
        let initial = Fen::from_position(&position, EnPassantMode::Always).to_string();
        let a = attach(
            &conn,
            &format!("[FEN \"{initial}\"]\n[SetUp \"1\"]\n\n2.Nf3 Nc6 *"),
            "white",
        );
        let target = &a.chapters[0].root.children[0].children[0];
        attach(
            &conn,
            &format!("[FEN \"{}\"]\n[SetUp \"1\"]\n\n3.Bc4 Bc5 *", target.fen),
            "white",
        );
        attach(
            &conn,
            "[SetUp \"1\"]\n[FEN \"7k/8/8/8/8/8/1K6/8 w - - 0 1\"]\n\n1.Kc3 *",
            "white",
        );
        let merged = combined(&conn, "white").unwrap();
        assert_eq!(merged.chapters.len(), 2);
        let joined = merged
            .chapters
            .iter()
            .find(|chapter| chapter.root.position_key == a.chapters[0].root.position_key)
            .unwrap();
        assert_eq!(
            joined.root.children[0].children[0].children[0]
                .san
                .as_deref(),
            Some("Bc4")
        );
        validate_document(&merged).unwrap();
    }
    #[test]
    fn repeated_attachment_is_idempotent_and_reattaches_without_overwriting_edits() {
        let conn = db();
        let pgn = "1.e4 e5 *";
        let original = attach(&conn, pgn, "white");
        let again = attach(&conn, pgn, "white");
        assert_eq!(original.id, again.id);
        assert_eq!(list(&conn).unwrap().len(), 1);
        let renamed = apply(
            &conn,
            &original.id,
            1,
            EditOperation::Rename {
                title: "Edited title".into(),
                side: "white".into(),
            },
        )
        .unwrap();
        let detached = set_attachment(&conn, &original.id, renamed.revision, false).unwrap();
        let restored = attach(&conn, pgn, "white");
        assert_eq!(restored.id, original.id);
        assert_eq!(restored.title, "Edited title");
        assert_eq!(restored.revision, detached.revision + 1);
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn lichess_study_named_chapters_remain_addressable_after_common_root_merge() {
        let conn = db();
        let names = ["English", "King pawn", "Queen pawn", "Reti", "Flank"];
        let lines = [
            "1.c4 e5 *",
            "1.e4 e5 *",
            "1.d4 d5 *",
            "1.Nf3 Nf6 *",
            "1.g3 d5 *",
        ];
        let pgn=names.iter().zip(lines).map(|(name,line)|format!("[Event \"Study: {name}\"]\n[ChapterName \"{name}\"]\n[StudyName \"Five chapters\"]\n[Site \"https://lichess.org/study/example/chapter\"]\n\n{line}")).collect::<Vec<_>>().join("\n\n");
        let original = attach(&conn, &pgn, "white");
        assert_eq!(original.chapters.len(), 5);
        let union = combined(&conn, "white").unwrap();
        assert_eq!(union.chapters.len(), 1);
        let catalog: Vec<SourceChapter> =
            serde_json::from_value(union.metadata["source_chapters"].clone()).unwrap();
        assert_eq!(
            catalog
                .iter()
                .map(|chapter| chapter.title.as_str())
                .collect::<Vec<_>>(),
            names
        );
        for (reference, chapter) in catalog.iter().zip(&original.chapters) {
            assert_eq!(reference.document_id, original.id);
            assert_eq!(reference.chapter_id, chapter.id);
            assert_eq!(reference.root_node_id, chapter.root.id);
            assert_eq!(reference.initial_fen, chapter.initial_fen);
            assert_eq!(reference.filename.as_deref(), Some("fragment.pgn"));
            assert_eq!(chapter.headers["StudyName"], "Five chapters");
        }
        let renamed = apply(
            &conn,
            &original.id,
            original.revision,
            EditOperation::RenameChapter {
                chapter_id: original.chapters[2].id.clone(),
                title: "New chapter title".into(),
            },
        )
        .unwrap();
        let refreshed = combined(&conn, "white").unwrap();
        let catalog: Vec<SourceChapter> =
            serde_json::from_value(refreshed.metadata["source_chapters"].clone()).unwrap();
        assert_eq!(catalog[2].chapter_id, original.chapters[2].id);
        assert_eq!(catalog[2].title, "New chapter title");
        set_attachment(&conn, &renamed.id, renamed.revision, false).unwrap();
        assert_eq!(
            combined(&conn, "white").unwrap().metadata["source_chapters"],
            serde_json::json!([])
        );
    }
}
