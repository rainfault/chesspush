use super::*;

fn fixture() -> (tempfile::TempDir, Connection) {
    let directory = tempfile::tempdir().unwrap();
    let conn = crate::storage::open(&directory.path().join("knowledge.sqlite3")).unwrap();
    (directory, conn)
}
fn study(conn: &Connection) -> KnowledgeDocument {
    create(
        conn,
        CreateRequest {
            title: "Позиция".into(),
            kind: "study".into(),
            side: "both".into(),
            initial_fen: None,
            links: vec![],
        },
    )
    .unwrap()
}
fn import_study(conn: &Connection, pgn: &str) -> KnowledgeDocument {
    import(
        conn,
        ImportRequest {
            title: "Заметка".into(),
            kind: "study".into(),
            side: "both".into(),
            pgn: pgn.into(),
            document_id: None,
            expected_revision: None,
        },
    )
    .unwrap()
}

#[test]
fn classification_follows_content_and_reuses_stable_tree_identifiers() {
    let (_directory, conn) = fixture();
    let original = study(&conn);
    assert_eq!(classification(&original).as_deref(), Some("position"));
    let chapter = original.chapters[0].id.clone();
    let root = original.chapters[0].root.id.clone();
    let edited = apply(
        &conn,
        &original.id,
        original.revision,
        EditOperation::AddMove {
            chapter_id: chapter.clone(),
            parent_id: root.clone(),
            uci: "e2e4".into(),
        },
    )
    .unwrap();
    let summary = fragments::summary(&edited);
    assert_eq!(summary.classification.as_deref(), Some("theory"));
    assert_eq!(summary.positions, 2);
    assert_eq!(edited.chapters[0].root.id, root);
    let child = edited.chapters[0].root.children[0].id.clone();
    let removed = apply(
        &conn,
        &edited.id,
        edited.revision,
        EditOperation::RemoveVariation {
            chapter_id: chapter,
            node_id: child,
        },
    )
    .unwrap();
    assert_eq!(classification(&removed).as_deref(), Some("position"));
    assert_eq!(
        load(&conn, &removed.id, Some(original.revision)).unwrap(),
        original
    );
}

#[test]
fn study_import_preserves_kind_when_chapters_are_appended_and_legacy_default_is_repertoire() {
    let (_directory, conn) = fixture();
    let request: ImportRequest =
        serde_json::from_value(serde_json::json!({"title":"Legacy","side":"white","pgn":"1.e4 *"}))
            .unwrap();
    assert_eq!(request.kind, "repertoire");
    let original = import_study(&conn, "[Event \"Original\"]\n\n1.e4 e5 (1...c5) *");
    let edited = import(
        &conn,
        ImportRequest {
            title: "Ignored".into(),
            kind: "repertoire".into(),
            side: "white".into(),
            pgn: "[Event \"Ending\"]\n\n1.d4 *".into(),
            document_id: Some(original.id.clone()),
            expected_revision: Some(original.revision),
        },
    )
    .unwrap();
    assert_eq!(edited.kind, "study");
    assert_eq!(edited.chapters.len(), 2);
    assert_eq!(edited.chapters[0], original.chapters[0]);
    assert!(overlay(&conn, &[edited.id]).unwrap().is_empty());
}

#[test]
fn normalized_unicode_tags_remain_suggested_after_removal_deletion_and_restart() {
    let (directory, conn) = fixture();
    let original = study(&conn);
    let tagged = apply(
        &conn,
        &original.id,
        original.revision,
        EditOperation::SetTags {
            tags: vec![
                "  План   в центре ".into(),
                "план в центре".into(),
                "Ферзевый гамбит".into(),
            ],
        },
    )
    .unwrap();
    assert_eq!(
        document_tags(&tagged),
        vec!["План в центре", "Ферзевый гамбит"]
    );
    assert_eq!(
        tag_suggestions(&conn, "ПЛА").unwrap(),
        vec!["План в центре"]
    );
    assert!(tag_suggestions(&conn, "%").unwrap().is_empty());
    let cleared = apply(
        &conn,
        &tagged.id,
        tagged.revision,
        EditOperation::SetTags { tags: vec![] },
    )
    .unwrap();
    assert!(delete_study(&conn, &cleared.id, 1).is_err());
    delete_study(&conn, &cleared.id, cleared.revision).unwrap();
    drop(conn);
    let reopened = crate::storage::open(&directory.path().join("knowledge.sqlite3")).unwrap();
    assert_eq!(
        tag_suggestions(&reopened, "ферз").unwrap(),
        vec!["Ферзевый гамбит"]
    );
    assert!(list(&reopened).unwrap().is_empty());
}

#[test]
fn invalid_tags_are_atomic_and_suggestions_are_bounded() {
    let (_directory, conn) = fixture();
    let original = study(&conn);
    for tags in [
        vec![" ".into()],
        vec!["a\nb".into()],
        vec!["Ж".repeat(65)],
        vec!["tag".into(); 33],
    ] {
        assert!(
            apply(
                &conn,
                &original.id,
                original.revision,
                EditOperation::SetTags { tags }
            )
            .is_err()
        );
        assert_eq!(load(&conn, &original.id, None).unwrap(), original);
        assert!(tag_suggestions(&conn, "").unwrap().is_empty());
    }
    let tagged = apply(
        &conn,
        &original.id,
        original.revision,
        EditOperation::SetTags {
            tags: (0..32).map(|index| format!("Тег {index:02}")).collect(),
        },
    )
    .unwrap();
    assert_eq!(document_tags(&tagged).len(), 32);
    assert_eq!(tag_suggestions(&conn, "ТЕГ").unwrap().len(), 20);
    assert!(tag_suggestions(&conn, &"Ж".repeat(65)).unwrap().is_empty());
}

#[test]
fn pgn_roundtrip_preserves_user_tags_headers_and_rav_content() {
    let (_directory, conn) = fixture();
    let original = import_study(
        &conn,
        "[Event \"Named event\"]\n[X-Custom \"retained\"]\n\n1.e4 {Plan} e5 (1...c5) *",
    );
    let tagged = apply(
        &conn,
        &original.id,
        original.revision,
        EditOperation::SetTags {
            tags: vec!["Король \"в центре\"".into(), "Прорыв\\центр".into()],
        },
    )
    .unwrap();
    let exported = export_document(&tagged).unwrap();
    assert!(exported.contains("[Event \"Named event\"]"));
    assert!(exported.contains("[X-Custom \"retained\"]"));
    let imported = import_study(&conn, &exported);
    assert_eq!(document_tags(&imported), document_tags(&tagged));
    assert_eq!(imported.chapters[0].root.children[0].children.len(), 2);
    assert_eq!(imported.chapters[0].root.children[0].comments, vec!["Plan"]);
    assert_eq!(
        tag_suggestions(&conn, "король").unwrap(),
        vec!["Король \"в центре\""]
    );
}

#[test]
fn header_edits_and_chapter_reordering_preserve_ids_and_protect_starting_position() {
    let (_directory, conn) = fixture();
    let original = import_study(
        &conn,
        "[Event \"One\"]\n\n1.e4 *\n\n[Event \"Two\"]\n\n1.d4 *",
    );
    let chapter = original.chapters[0].id.clone();
    let mut headers = original.chapters[0].headers.clone();
    headers.insert("Event".into(), "Edited event".into());
    let edited = apply(
        &conn,
        &original.id,
        original.revision,
        EditOperation::SetChapterHeaders {
            chapter_id: chapter.clone(),
            headers: headers.clone(),
        },
    )
    .unwrap();
    assert_eq!(edited.chapters[0].root, original.chapters[0].root);
    assert_eq!(
        parse_pgn(&export_document(&edited).unwrap()).unwrap()[0].headers["Event"],
        "Edited event"
    );
    headers.insert("FEN".into(), INITIAL_FEN.into());
    assert!(
        apply(
            &conn,
            &edited.id,
            edited.revision,
            EditOperation::SetChapterHeaders {
                chapter_id: chapter.clone(),
                headers
            }
        )
        .is_err()
    );
    let moved = apply(
        &conn,
        &edited.id,
        edited.revision,
        EditOperation::MoveChapter {
            chapter_id: chapter.clone(),
            index: 1,
        },
    )
    .unwrap();
    assert_eq!(moved.chapters[1].id, chapter);
    assert_eq!(moved.chapters[0].id, original.chapters[1].id);
    assert_eq!(moved.chapters[1].root, original.chapters[0].root);
    assert_eq!(load(&conn, &original.id, None).unwrap(), moved);
}

#[test]
fn graph_projection_deduplicates_files_preserving_occurrences_exact_fen_and_link_provenance() {
    let (_directory, conn) = fixture();
    let repeated = import_study(&conn, "1.Nf3 Nf6 2.Ng1 Ng8 3.Nf3 *");
    let root_key = repeated.chapters[0].root.position_key.clone();
    let refs = graph_overlay::references(&conn, &[root_key.clone()]).unwrap();
    assert_eq!(refs[&root_key].len(), 1);
    assert!(
        refs[&root_key]
            .iter()
            .all(|reference| reference.document_id == repeated.id)
    );
    let saved = load(&conn, &repeated.id, None).unwrap();
    let root = &saved.chapters[0].root;
    let last = &root.children[0].children[0].children[0].children[0];
    assert_eq!(root.position_key, last.position_key);
    assert_ne!(root.id, last.id);
    let transitions = graph_overlay::continuations(&conn, &root_key).unwrap();
    assert_eq!(transitions.len(), 1);
    assert_eq!(transitions[0].san, "Nf3");
    let fen = "4k3/8/8/8/8/8/8/4K2R b K - 98 71";
    let isolated = create(
        &conn,
        CreateRequest {
            title: "Ending".into(),
            kind: "study".into(),
            side: "both".into(),
            initial_fen: Some(fen.into()),
            links: vec![],
        },
    )
    .unwrap();
    let hydrated = graph_overlay::position(&conn, &isolated.chapters[0].root.position_key)
        .unwrap()
        .unwrap();
    assert_eq!(hydrated.fen, fen);
    let outside = position_key(&position("4k3/8/8/8/8/8/8/R3K3 b Q - 12 8").unwrap());
    let linked = create(
        &conn,
        CreateRequest {
            title: "Linked".into(),
            kind: "study".into(),
            side: "both".into(),
            initial_fen: None,
            links: vec![KnowledgeLink {
                target: LinkTarget::Position {
                    position_key: outside.clone(),
                },
                provenance: "manual_position_link".into(),
                path_uci: vec![],
                initial_fen: None,
            }],
        },
    )
    .unwrap();
    let linked_refs = graph_overlay::references(&conn, &[outside.clone()]).unwrap();
    assert_eq!(linked_refs[&outside][0].document_id, linked.id);
    assert_eq!(linked_refs[&outside][0].provenance, "manual_position_link");
    assert!(graph_overlay::position(&conn, &outside).unwrap().is_none());
}

#[test]
fn study_deletion_does_not_change_repertoires_or_factual_games() {
    let (_directory, conn) = fixture();
    conn.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES('fixture','{}','{}','1.e4 e5 *','fixture','\"local\"')",[]).unwrap();
    let repertoire = import(
        &conn,
        ImportRequest {
            title: "Repertoire".into(),
            kind: "repertoire".into(),
            side: "white".into(),
            pgn: "1.e4 e5 *".into(),
            document_id: None,
            expected_revision: None,
        },
    )
    .unwrap();
    let note = study(&conn);
    assert!(delete_study(&conn, &repertoire.id, repertoire.revision).is_err());
    delete_study(&conn, &note.id, note.revision).unwrap();
    assert_eq!(load(&conn, &repertoire.id, None).unwrap(), repertoire);
    assert_eq!(
        conn.query_row("SELECT pgn FROM games", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "1.e4 e5 *"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM knowledge_documents", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn file_import_export_preserves_pgn_and_rejects_invalid_files_without_partial_documents() {
    let (directory, conn) = fixture();
    let source = directory.path().join("Ending.pgn");
    std::fs::write(&source,format!("\u{feff}[Event \"Model\"]\n[FEN \"4k3/8/8/8/8/8/8/4K2R b K - 18 17\"]\n[SetUp \"1\"]\n\n*")).unwrap();
    let note = import_file(&conn, &source).unwrap();
    assert_eq!(note.kind, "study");
    assert_eq!(
        fragments::summary(&note).filename.as_deref(),
        Some("Ending.pgn")
    );
    assert_eq!(classification(&note).as_deref(), Some("position"));
    let exported = directory.path().join("Exported.pgn");
    export_file(&conn, &note.id, None, &exported).unwrap();
    assert_eq!(
        parse_pgn(&std::fs::read_to_string(exported).unwrap()).unwrap()[0]
            .root
            .fen,
        note.chapters[0].root.fen
    );
    let invalid = directory.path().join("Invalid.pgn");
    std::fs::write(&invalid, [0xff, 0xfe]).unwrap();
    assert!(import_file(&conn, &invalid).is_err());
    assert!(export_file(&conn, &note.id, None, &directory.path().join("invalid.txt")).is_err());
    assert_eq!(list(&conn).unwrap().len(), 1);
}
