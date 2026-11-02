//! Personal game review uses the shared editable document model. Played games,
//! their mainlines, results, and engine evidence remain immutable sources.
use crate::{
    chess_domain::OccurrenceRef,
    domain::AppError,
    knowledge::{self, Chapter, KnowledgeDocument, KnowledgeLink, LinkTarget, VariationNode},
    statistics::{engine, store, types::StoredGame},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

fn error(message: impl ToString) -> AppError {
    AppError::new("game_review", message)
}

#[derive(Debug, Default, Deserialize)]
pub struct ListRequest {
    pub account_id: Option<String>,
    pub query: Option<String>,
    pub color: Option<String>,
    pub speed: Option<String>,
    pub tag: Option<String>,
    pub reviewed: Option<bool>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}
#[derive(Debug, Serialize)]
pub struct ReviewProgress {
    pub progress: i64,
    pub total: i64,
}
#[derive(Debug, Serialize)]
pub struct GameRow {
    pub game_id: String,
    pub account_id: String,
    pub source_revision: i64,
    pub white: String,
    pub black: String,
    pub white_rating: Option<i32>,
    pub black_rating: Option<i32>,
    pub result: String,
    pub own_color: String,
    pub started_at_ms: Option<i64>,
    pub speed: String,
    pub opening: Option<String>,
    pub eco: Option<String>,
    pub analysis_status: String,
    pub analysis_error: Option<String>,
    pub analysis_progress: Option<ReviewProgress>,
    pub tags: Vec<String>,
    pub reviewed: bool,
    pub document_id: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct GamePage {
    pub accounts: Vec<store::AccountObservation>,
    pub selected_account_id: Option<String>,
    pub total: usize,
    pub rows: Vec<GameRow>,
    pub tags: Vec<String>,
}
fn selected_account(
    conn: &Connection,
    requested: Option<&str>,
) -> Result<Option<String>, AppError> {
    if let Some(id) = requested {
        if store::account(conn, id)?.is_none() {
            return Err(error("Аккаунт статистики не найден"));
        }
        return Ok(Some(id.into()));
    }
    Ok(store::active_account(conn)?
        .or(store::accounts(conn)?.into_iter().next())
        .map(|a| a.id))
}
pub fn list(conn: &Connection, request: ListRequest) -> Result<GamePage, AppError> {
    if request
        .color
        .as_deref()
        .is_some_and(|s| !["white", "black"].contains(&s))
        || request.limit.is_some_and(|n| n == 0 || n > 100)
        || request.offset.is_some_and(|n| n > 1_000_000)
        || request.query.as_ref().is_some_and(|s| s.len() > 256)
        || request.tag.as_ref().is_some_and(|s| s.chars().count() > 64)
        || matches!((request.since_ms,request.until_ms),(Some(a),Some(b)) if a>b)
    {
        return Err(error("Проверьте фильтры списка партий"));
    }
    let accounts = store::accounts(conn)?;
    let account_id = selected_account(conn, request.account_id.as_deref())?;
    let Some(id) = account_id.as_deref() else {
        return Ok(GamePage {
            accounts,
            selected_account_id: None,
            total: 0,
            rows: vec![],
            tags: vec![],
        });
    };
    let tags=conn.prepare("SELECT DISTINCT t.value FROM game_review_documents r JOIN knowledge_documents d ON d.id=r.document_id,json_each(d.data_json,'$.header.metadata.tags') t WHERE r.account_id=?1 AND r.source_revision=(SELECT max(previous.source_revision) FROM game_review_documents previous WHERE previous.game_id=r.game_id AND previous.account_id=r.account_id) AND t.type='text' ORDER BY t.value COLLATE NOCASE")?.query_map([id],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
    let query = request.query.as_deref().unwrap_or("").trim().to_lowercase();
    let filter = "FROM statistics_games g JOIN statistics_account_games a ON a.game_id=g.game_id
      LEFT JOIN game_review_documents r ON r.game_id=g.id AND r.account_id=a.account_id AND r.source_revision=g.movetext_revision
      LEFT JOIN knowledge_documents d ON d.id=coalesce(r.document_id,(SELECT previous.document_id FROM game_review_documents previous WHERE previous.game_id=g.id AND previous.account_id=a.account_id ORDER BY previous.source_revision DESC LIMIT 1))
      WHERE a.account_id=?1 AND a.user_color IN ('white','black')
      AND json_extract(g.data,'$.variant')='standard'
      AND json_extract(g.data,'$.result') IN ('1-0','0-1','1/2-1/2')
      AND coalesce(json_extract(g.data,'$.status'),'') NOT IN ('created','started')
      AND (?2='' OR instr(lower(coalesce(json_extract(g.data,'$.white.name'),'')||' '||coalesce(json_extract(g.data,'$.black.name'),'')||' '||coalesce(json_extract(g.data,'$.replay.opening.name'),'')||' '||coalesce(json_extract(g.data,'$.replay.opening.eco'),'')),?2)>0)
      AND (?3 IS NULL OR a.user_color=?3) AND (?4 IS NULL OR json_extract(g.data,'$.speed')=?4)
      AND (?5 IS NULL OR EXISTS(SELECT 1 FROM json_each(d.data_json,'$.header.metadata.tags') t WHERE t.value=?5))
      AND (?6 IS NULL OR coalesce(r.reviewed,0)=?6)
      AND (?7 IS NULL OR json_extract(g.data,'$.started_at_ms')>=?7)
      AND (?8 IS NULL OR json_extract(g.data,'$.started_at_ms')<=?8)";
    let values = params![
        id,
        query,
        request.color,
        request.speed,
        request.tag,
        request.reviewed,
        request.since_ms,
        request.until_ms
    ];
    let total = conn.query_row(&format!("SELECT count(*) {filter}"), values, |r| r.get(0))?;
    let sql=format!("SELECT g.id,a.account_id,g.movetext_revision,
      coalesce(json_extract(g.data,'$.white.name'),''),coalesce(json_extract(g.data,'$.black.name'),''),
      json_extract(g.data,'$.white.rating'),json_extract(g.data,'$.black.rating'),json_extract(g.data,'$.result'),a.user_color,
      json_extract(g.data,'$.started_at_ms'),json_extract(g.data,'$.speed'),json_extract(g.data,'$.replay.opening.name'),json_extract(g.data,'$.replay.opening.eco'),
      r.document_id,coalesce(r.reviewed,0),coalesce(json_extract(d.data_json,'$.header.metadata.tags'),'[]'),
      (SELECT json_object('state',j.state,'progress',j.progress,'total',j.total,'error',j.last_error) FROM statistics_analysis_jobs j WHERE j.game_id=g.id AND j.movetext_revision=g.movetext_revision AND j.profile_id=(SELECT json_extract(identity_json,'$.profile_id') FROM statistics_engine_state WHERE id=1) ORDER BY j.id DESC LIMIT 1),
      (SELECT identity_json IS NOT NULL FROM statistics_engine_state WHERE id=1)
      {filter} ORDER BY json_extract(g.data,'$.started_at_ms') DESC,g.id DESC LIMIT ?9 OFFSET ?10");
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(
            params![
                id,
                query,
                request.color,
                request.speed,
                request.tag,
                request.reviewed,
                request.since_ms,
                request.until_ms,
                request.limit.unwrap_or(25),
                request.offset.unwrap_or(0)
            ],
            |r| {
                let tags: String = r.get(15)?;
                let job: Option<String> = r.get(16)?;
                let job =
                    job.and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok());
                let configured: bool = r.get(17)?;
                Ok(GameRow {
                    game_id: r.get(0)?,
                    account_id: r.get(1)?,
                    source_revision: r.get(2)?,
                    white: r.get(3)?,
                    black: r.get(4)?,
                    white_rating: r.get(5)?,
                    black_rating: r.get(6)?,
                    result: r.get(7)?,
                    own_color: r.get(8)?,
                    started_at_ms: r.get(9)?,
                    speed: r.get(10)?,
                    opening: r.get(11)?,
                    eco: r.get(12)?,
                    document_id: r.get(13)?,
                    reviewed: r.get(14)?,
                    tags: serde_json::from_str(&tags).unwrap_or_default(),
                    analysis_status: job
                        .as_ref()
                        .and_then(|j| j["state"].as_str())
                        .unwrap_or(if configured {
                            "not_queued"
                        } else {
                            "not_configured"
                        })
                        .into(),
                    analysis_error: job
                        .as_ref()
                        .and_then(|j| j["error"].as_str())
                        .map(str::to_owned),
                    analysis_progress: job.as_ref().map(|j| ReviewProgress {
                        progress: j["progress"].as_i64().unwrap_or(0),
                        total: j["total"].as_i64().unwrap_or(0),
                    }),
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GamePage {
        accounts,
        selected_account_id: account_id,
        total,
        rows,
        tags,
    })
}

fn owned_game(
    conn: &Connection,
    id: &str,
    account_id: Option<&str>,
) -> Result<StoredGame, AppError> {
    let account =
        selected_account(conn, account_id)?.ok_or_else(|| error("Выберите аккаунт партии"))?;
    let row:Option<(String,i64,String,String)>=conn.query_row("SELECT g.data,g.movetext_revision,a.user_color,a.eligibility FROM statistics_games g JOIN statistics_account_games a ON a.game_id=g.game_id WHERE g.id=?1 AND a.account_id=?2",params![id,account],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let (data, revision, color, eligibility) =
        row.ok_or_else(|| AppError::new("not_found", "Партия этого аккаунта не найдена"))?;
    let mut game: StoredGame = serde_json::from_str(&data)?;
    game.account_id = account;
    game.own_color = color;
    game.eligibility = eligibility;
    game.movetext_revision = revision;
    if game.variant != "standard"
        || !["white", "black"].contains(&game.own_color.as_str())
        || !["1-0", "0-1", "1/2-1/2"].contains(&game.result.as_str())
        || ["started", "created"].contains(&game.status.as_str())
    {
        return Err(error(
            "Разбор доступен для завершённых собственных партий в стандартные шахматы",
        ));
    }
    Ok(game)
}
#[derive(Debug, Serialize)]
pub struct SourceNode {
    pub ply: u32,
    pub chapter_id: String,
    pub node_id: String,
}
#[derive(Debug, Serialize)]
pub struct Review {
    pub game: StoredGame,
    pub document: KnowledgeDocument,
    pub chapter_id: String,
    pub node_id: String,
    pub source_nodes: Vec<SourceNode>,
    pub reviewed: bool,
    pub analysis_status: String,
    #[serde(skip)]
    pub(crate) created: bool,
}
fn source_nodes(game: &StoredGame, document: &KnowledgeDocument) -> Vec<SourceNode> {
    let mut result = vec![];
    let Some(chapter) = source_chapter(document) else {
        return result;
    };
    let mut current = &chapter.root;
    if current
        .fen
        .split_whitespace()
        .take(4)
        .ne(game.replay.initial_fen.split_whitespace().take(4))
    {
        return result;
    }
    result.push(SourceNode {
        ply: 0,
        chapter_id: chapter.id.clone(),
        node_id: current.id.clone(),
    });
    for played in game.replay.nodes.iter().skip(1) {
        let Some(child) = current.children.iter().find(|c| c.uci == played.uci) else {
            break;
        };
        result.push(SourceNode {
            ply: played.ply,
            chapter_id: chapter.id.clone(),
            node_id: child.id.clone(),
        });
        current = child;
    }
    result
}
fn source_chapter(document: &KnowledgeDocument) -> Option<&Chapter> {
    let id = document
        .metadata
        .get("game_review")
        .and_then(|value| value.get("chapter_id"))
        .and_then(|value| value.as_str());
    document
        .chapters
        .iter()
        .find(|chapter| Some(chapter.id.as_str()) == id)
        .or_else(|| document.chapters.first())
}
pub fn open(
    conn: &Connection,
    game_id: &str,
    account_id: Option<&str>,
    source_revision: Option<i64>,
) -> Result<Review, AppError> {
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let game = owned_game(&tx, game_id, account_id)?;
    if source_revision.is_some_and(|revision| revision != game.movetext_revision) {
        return Err(AppError::new(
            "conflict",
            "Главная линия партии изменилась. Откройте её актуальную версию.",
        ));
    }
    let mapped:Option<(String,bool)>=tx.query_row("SELECT document_id,reviewed FROM game_review_documents WHERE game_id=?1 AND account_id=?2 AND source_revision=?3",params![game.id,game.account_id,game.movetext_revision],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let created = mapped.is_none();
    let (document, reviewed) = if let Some((id, reviewed)) = mapped {
        (knowledge::load(&tx, &id, None)?, reviewed)
    } else {
        let now = store::now_ms();
        let mut chapters = knowledge::parse_pgn(&game.raw_pgn)?;
        if chapters.len() != 1 {
            return Err(error("У сыгранной партии должна быть одна главная линия"));
        }
        let chapter = &mut chapters[0];
        chapter.title = format!("{} — {}", game.white.name, game.black.name);
        chapter.links.push(KnowledgeLink {
            target: LinkTarget::Occurrence {
                occurrence: OccurrenceRef {
                    game_id: game.id.clone(),
                    revision: game.movetext_revision,
                    ply: 0,
                },
            },
            provenance: "game_review".into(),
            path_uci: vec![],
            initial_fen: Some(game.replay.initial_fen.clone()),
        });
        let mut metadata = BTreeMap::new();
        // Tags describe the party, while comments/variations and the reviewed
        // mark belong to a particular factual movetext revision.
        let previous:Option<String>=tx.query_row("SELECT document_id FROM game_review_documents WHERE game_id=?1 AND account_id=?2 ORDER BY source_revision DESC LIMIT 1",params![game.id,game.account_id],|r|r.get(0)).optional()?;
        if let Some(previous) = previous {
            metadata.insert(
                "tags".into(),
                json!(knowledge::document_tags(&knowledge::load(
                    &tx, &previous, None
                )?)),
            );
        }
        metadata.insert("game_review".into(),json!({"game_id":game.id,"account_id":game.account_id,"source_revision":game.movetext_revision,"chapter_id":chapter.id}));
        let document = KnowledgeDocument {
            id: knowledge::new_id("analysis")?,
            kind: "analysis".into(),
            title: chapter.title.clone(),
            side: game.own_color.clone(),
            revision: 1,
            created_at_ms: now,
            updated_at_ms: now,
            metadata,
            chapters,
        };
        knowledge::persist_uncommitted(&tx, &document, None)?;
        tx.execute("INSERT INTO game_review_documents(game_id,account_id,source_revision,document_id,updated_at_ms) VALUES(?1,?2,?3,?4,?5)",params![game.id,game.account_id,game.movetext_revision,document.id,now])?;
        (document, false)
    };
    let nodes = source_nodes(&game, &document);
    let chapter = source_chapter(&document).ok_or_else(|| error("В разборе нет главы"))?;
    let chapter_id = chapter.id.clone();
    let node_id = chapter.root.id.clone();
    let configured: bool = tx.query_row(
        "SELECT identity_json IS NOT NULL FROM statistics_engine_state WHERE id=1",
        [],
        |row| row.get(0),
    )?;
    let status:Option<String>=tx.query_row("SELECT state FROM statistics_analysis_jobs WHERE game_id=?1 AND movetext_revision=?2 AND profile_id=(SELECT json_extract(identity_json,'$.profile_id') FROM statistics_engine_state WHERE id=1) ORDER BY id DESC LIMIT 1",params![game.id,game.movetext_revision],|r|r.get(0)).optional()?;
    tx.commit()?;
    Ok(Review {
        game,
        document,
        chapter_id,
        node_id,
        source_nodes: nodes,
        reviewed,
        analysis_status: status.unwrap_or_else(|| {
            if configured {
                "not_queued"
            } else {
                "not_configured"
            }
            .into()
        }),
        created,
    })
}
pub fn mark(
    conn: &Connection,
    game_id: &str,
    account_id: Option<&str>,
    source_revision: i64,
    reviewed: bool,
) -> Result<(), AppError> {
    let review = open(conn, game_id, account_id, Some(source_revision))?;
    let changed=conn.execute("UPDATE game_review_documents SET reviewed=?1,updated_at_ms=?2 WHERE game_id=?3 AND account_id=?4 AND source_revision=?5 AND source_revision=(SELECT movetext_revision FROM statistics_games WHERE id=?3)",params![reviewed,store::now_ms(),game_id,review.game.account_id,source_revision])?;
    if changed != 1 {
        return Err(AppError::new(
            "conflict",
            "Исходная партия изменилась. Откройте её актуальную версию.",
        ));
    }
    Ok(())
}
/// Revalidate the owning factual game whenever a derived review is used.
pub fn review_source(conn: &Connection, document_id: &str) -> Result<StoredGame, AppError> {
    let mapped:Option<(String,String,i64)>=conn.query_row("SELECT game_id,account_id,source_revision FROM game_review_documents WHERE document_id=?1",[document_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let (id, account, revision) =
        mapped.ok_or_else(|| error("Документ не связан с разбором собственной партии"))?;
    let game = owned_game(conn, &id, Some(&account))?;
    if game.movetext_revision != revision {
        return Err(AppError::new(
            "conflict",
            "Исходная партия изменилась. Сохранённый разбор доступен отдельно; откройте актуальную партию.",
        ));
    }
    Ok(game)
}
fn find_node<'a>(node: &'a VariationNode, id: &str) -> Option<&'a VariationNode> {
    if node.id == id {
        Some(node)
    } else {
        node.children.iter().find_map(|child| find_node(child, id))
    }
}
fn copied_node(
    node: &VariationNode,
    root_ply: u32,
    is_root: bool,
    children: bool,
) -> Result<VariationNode, AppError> {
    let mut copy = node.clone();
    copy.id = knowledge::new_id("node")?;
    copy.ply = node.ply - root_ply;
    if is_root {
        copy.uci = None;
        copy.san = None;
        // The extracted root has no preceding move. Keep prose that originally
        // introduced that move as root commentary, before its following notes.
        copy.comments = copy
            .starting_comments
            .iter()
            .chain(copy.comments.iter())
            .cloned()
            .collect();
        copy.starting_comments.clear();
    }
    copy.children = if children {
        node.children
            .iter()
            .map(|child| copied_node(child, root_ply, false, true))
            .collect::<Result<_, _>>()?
    } else {
        vec![]
    };
    Ok(copy)
}
#[derive(Debug, Deserialize)]
pub struct SaveKnowledgeRequest {
    pub document_id: String,
    pub expected_revision: i64,
    pub chapter_id: String,
    pub node_id: String,
    pub title: String,
    pub scope: String,
}
pub fn save_knowledge(
    conn: &Connection,
    request: SaveKnowledgeRequest,
) -> Result<KnowledgeDocument, AppError> {
    if !["position", "branch", "game"].contains(&request.scope.as_str()) {
        return Err(error("Выберите позицию, вариант или весь разбор"));
    }
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let source = knowledge::load(&tx, &request.document_id, None)?;
    if source.revision != request.expected_revision {
        return Err(AppError::new(
            "conflict",
            "Разбор уже изменён. Сохраните его и повторите действие.",
        ));
    }
    let game = review_source(&tx, &source.id)?;
    let chapter = source
        .chapters
        .iter()
        .find(|c| c.id == request.chapter_id)
        .ok_or_else(|| error("Глава разбора не найдена"))?;
    let node = find_node(&chapter.root, &request.node_id)
        .ok_or_else(|| error("Узел разбора не найден"))?;
    let nav = knowledge::navigate(
        &tx,
        &source.id,
        &chapter.id,
        &node.id,
        Some(source.revision),
    )?;
    // An occurrence refers to the longest real prefix of this analysis route,
    // never to a hypothetical move merely sharing the same ply/position.
    let actual = game
        .replay
        .nodes
        .iter()
        .skip(1)
        .filter_map(|n| n.uci.as_ref())
        .collect::<Vec<_>>();
    let common = if nav.initial_fen == game.replay.initial_fen {
        nav.path_uci
            .iter()
            .zip(actual)
            .take_while(|(a, b)| a == b)
            .count()
    } else {
        0
    };
    let links = vec![
        KnowledgeLink {
            target: LinkTarget::Occurrence {
                occurrence: OccurrenceRef {
                    game_id: game.id.clone(),
                    revision: game.movetext_revision,
                    ply: common as u32,
                },
            },
            provenance: "game_review".into(),
            path_uci: nav.path_uci[..common].to_vec(),
            initial_fen: Some(game.replay.initial_fen.clone()),
        },
        KnowledgeLink {
            target: LinkTarget::DocumentNode {
                document_id: source.id.clone(),
                revision: source.revision,
                chapter_id: chapter.id.clone(),
                node_id: node.id.clone(),
            },
            provenance: "game_review".into(),
            path_uci: nav.path_uci.clone(),
            initial_fen: Some(nav.initial_fen.clone()),
        },
        KnowledgeLink {
            target: LinkTarget::Position {
                position_key: node.position_key.clone(),
            },
            provenance: "game_review".into(),
            path_uci: nav.path_uci.clone(),
            initial_fen: Some(nav.initial_fen.clone()),
        },
    ];
    let mut chapters = if request.scope == "game" {
        source
            .chapters
            .iter()
            .map(|c| {
                Ok(Chapter {
                    id: knowledge::new_id("chapter")?,
                    root: copied_node(&c.root, 0, true, true)?,
                    ..c.clone()
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?
    } else {
        let mut headers = chapter.headers.clone();
        headers.remove("FEN");
        headers.remove("SetUp");
        headers.insert("Result".into(), "*".into());
        vec![Chapter {
            id: knowledge::new_id("chapter")?,
            title: request.title.trim().into(),
            initial_fen: node.fen.clone(),
            headers,
            root: copied_node(node, node.ply, true, request.scope == "branch")?,
            source_pgn: None,
            links: vec![],
        }]
    };
    for chapter in &mut chapters {
        chapter.links.extend(links.clone());
    }
    let now = store::now_ms();
    let mut metadata = BTreeMap::new();
    metadata.insert("tags".into(), json!(knowledge::document_tags(&source)));
    metadata.insert("review_source".into(),json!({"game_id":game.id,"account_id":game.account_id,"source_revision":game.movetext_revision,"document_id":source.id,"document_revision":source.revision}));
    let document = KnowledgeDocument {
        id: knowledge::new_id("knowledge")?,
        kind: "study".into(),
        title: request.title.trim().into(),
        side: source.side,
        revision: 1,
        created_at_ms: now,
        updated_at_ms: now,
        metadata,
        chapters,
    };
    knowledge::persist_uncommitted(&tx, &document, None)?;
    tx.commit()?;
    Ok(document)
}
pub fn use_repertoire(
    conn: &Connection,
    id: &str,
    expected_revision: i64,
    side: &str,
) -> Result<KnowledgeDocument, AppError> {
    if !["white", "black"].contains(&side) {
        return Err(error("Выберите сторону подготовки"));
    }
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let mut document = knowledge::load(&tx, id, None)?;
    if document.revision != expected_revision {
        return Err(AppError::new(
            "conflict",
            "Знание изменено. Откройте актуальную версию.",
        ));
    }
    if document.kind != "study" {
        return Err(error("Подключить можно сохранённое знание"));
    }
    document.side = side.into();
    document.metadata.insert("repertoire_attachment".into(),json!({"filename":format!("{}.pgn",document.title),"active":true,"source":"knowledge","attached_at_ms":store::now_ms()}));
    document.revision += 1;
    document.updated_at_ms = store::now_ms();
    knowledge::persist_uncommitted(&tx, &document, Some(expected_revision))?;
    tx.commit()?;
    Ok(document)
}

#[derive(Debug, Serialize)]
pub struct LiveEvaluation {
    pub evaluation: crate::statistics::types::PositionEvaluation,
    pub engine: engine::EngineIdentity,
}
pub fn evaluate(
    conn: &Connection,
    id: &str,
    revision: i64,
    chapter: &str,
    node: &str,
    cancel: impl FnMut() -> bool,
) -> Result<LiveEvaluation, AppError> {
    review_source(conn, id)?;
    let current = knowledge::load(conn, id, None)?;
    if current.revision != revision {
        return Err(AppError::new(
            "conflict",
            "Разбор изменён. Повторите анализ текущей позиции.",
        ));
    }
    let nav = knowledge::navigate(conn, id, chapter, node, Some(revision))?;
    let (evaluation, identity) =
        engine::evaluate_context(conn, &nav.initial_fen, &nav.path_uci, &nav.fen, cancel)?;
    review_source(conn, id)?;
    if knowledge::load(conn, id, None)?.revision != revision {
        return Err(AppError::new(
            "conflict",
            "Разбор изменён. Повторите анализ текущей позиции.",
        ));
    }
    Ok(LiveEvaluation {
        evaluation,
        engine: identity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lichess::VerifiedIdentity, statistics::chess, storage};
    fn fixture() -> (tempfile::TempDir, Connection, StoredGame) {
        let dir = tempfile::tempdir().unwrap();
        let conn = storage::open(&dir.path().join("review.sqlite3")).unwrap();
        let observation = store::observe(
            &conn,
            &VerifiedIdentity {
                id: "review_fixture".into(),
                username: "ReviewFixture".into(),
                generation: 1,
            },
            1000,
        )
        .unwrap();
        let game = seed(
            &conn,
            &observation.id,
            "review01",
            "1.e4 e5 2.Nf3 Nc6 3.Bb5 a6",
            "1-0",
            2000,
        );
        (dir, conn, game)
    }
    fn seed(
        conn: &Connection,
        account: &str,
        id: &str,
        moves: &str,
        result: &str,
        at: i64,
    ) -> StoredGame {
        let pgn = format!(
            "[Event \"Review test\"]\n[White \"ReviewFixture\"]\n[Black \"Opponent\"]\n[Result \"{result}\"]\n\n{moves} {result}"
        );
        let payload = json!({"id":id,"createdAt":at,"lastMoveAt":at+200,"status":"resign","winner":if result=="1-0" {"white"}else{"black"},"speed":"rapid","variant":"standard","rated":true,"players":{"white":{"user":{"id":"review_fixture","name":"ReviewFixture"},"rating":1800},"black":{"user":{"id":"opponent","name":"Opponent"},"rating":1750}},"pgn":pgn});
        let game = chess::from_lichess(&payload, account, "review_fixture").unwrap();
        store::save_game(conn, account, &game, &payload.to_string(), "eligible").unwrap();
        owned_game(conn, &game.id, Some(account)).unwrap()
    }
    fn facts(conn: &Connection) -> Vec<String> {
        ["SELECT data FROM statistics_games ORDER BY id","SELECT raw_json||raw_pgn FROM statistics_payloads ORDER BY game_id,payload_revision","SELECT account_id||game_id||user_color||eligibility||evidence FROM statistics_account_games ORDER BY account_id,game_id","SELECT pgn FROM games ORDER BY id"].iter().flat_map(|sql|conn.prepare(sql).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap()).collect()
    }
    #[test]
    fn review_is_idempotent_and_edits_never_change_played_material() {
        let (_dir, conn, game) = fixture();
        let before = facts(&conn);
        let review = open(
            &conn,
            &game.id,
            Some(&game.account_id),
            Some(game.movetext_revision),
        )
        .unwrap();
        assert_eq!(review.document.kind, "analysis");
        assert!(review.created);
        assert_eq!(review.source_nodes.len(), game.replay.nodes.len());
        let same = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        assert!(!same.created);
        assert_eq!(same.document, review.document);
        let node = review.source_nodes[1].node_id.clone();
        let changed = knowledge::apply(
            &conn,
            &review.document.id,
            review.document.revision,
            knowledge::EditOperation::Annotate {
                chapter_id: review.chapter_id.clone(),
                node_id: node,
                comments: vec!["My own plan".into()],
                nags: vec![5],
                shapes: vec![],
            },
        )
        .unwrap();
        let changed = knowledge::apply(
            &conn,
            &changed.id,
            changed.revision,
            knowledge::EditOperation::SetTags {
                tags: vec!["Структура".into()],
            },
        )
        .unwrap();
        mark(
            &conn,
            &game.id,
            Some(&game.account_id),
            game.movetext_revision,
            true,
        )
        .unwrap();
        let reopened = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        assert_eq!(reopened.document, changed);
        assert!(reopened.reviewed);
        assert_eq!(
            knowledge::tag_suggestions(&conn, "стру").unwrap(),
            vec!["Структура"]
        );
        assert_eq!(facts(&conn), before);
        let page = list(
            &conn,
            ListRequest {
                tag: Some("Структура".into()),
                reviewed: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].tags, vec!["Структура"]);
        assert_eq!(
            page.rows[0].document_id.as_deref(),
            Some(changed.id.as_str())
        );
    }
    #[test]
    fn list_filters_page_and_account_ownership_are_enforced() {
        let (_dir, conn, game) = fixture();
        let review = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        knowledge::apply(
            &conn,
            &review.document.id,
            review.document.revision,
            knowledge::EditOperation::SetTags {
                tags: vec!["Off-page tag".into()],
            },
        )
        .unwrap();
        seed(
            &conn,
            &game.account_id,
            "review02",
            "1.d4 d5 2.c4 e6",
            "0-1",
            3000,
        );
        let page = list(
            &conn,
            ListRequest {
                limit: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.total, 2);
        assert!(page.rows[0].game_id.ends_with("review02"));
        assert!(page.rows[0].tags.is_empty());
        assert_eq!(page.tags, vec!["Off-page tag"]);
        let filtered = list(
            &conn,
            ListRequest {
                query: Some("no matching opponent".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(filtered.total, 0);
        assert_eq!(filtered.tags, page.tags);
        let second = list(
            &conn,
            ListRequest {
                limit: Some(1),
                offset: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(second.rows[0].game_id, game.id);
        assert_eq!(
            list(
                &conn,
                ListRequest {
                    since_ms: Some(2500),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            1
        );
        assert_eq!(
            list(
                &conn,
                ListRequest {
                    query: Some("Opponent".into()),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            2
        );
        assert_eq!(
            list(
                &conn,
                ListRequest {
                    color: Some("black".into()),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            0
        );
        assert!(
            list(
                &conn,
                ListRequest {
                    limit: Some(1000),
                    ..Default::default()
                }
            )
            .is_err()
        );
        let other = store::observe(
            &conn,
            &VerifiedIdentity {
                id: "different".into(),
                username: "Different".into(),
                generation: 1,
            },
            1000,
        )
        .unwrap();
        assert_eq!(
            list(
                &conn,
                ListRequest {
                    account_id: Some(other.id.clone()),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            0
        );
        assert!(open(&conn, &game.id, Some(&other.id), None).is_err());
    }
    #[test]
    fn new_source_revision_preserves_old_review_and_rejects_stale_source_address() {
        let (_dir, conn, game) = fixture();
        let mut original = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        original.document = knowledge::apply(
            &conn,
            &original.document.id,
            original.document.revision,
            knowledge::EditOperation::SetTags {
                tags: vec!["Persistent party tag".into()],
            },
        )
        .unwrap();
        mark(
            &conn,
            &game.id,
            Some(&game.account_id),
            game.movetext_revision,
            true,
        )
        .unwrap();
        let next = seed(
            &conn,
            &game.account_id,
            "review01",
            "1.d4 d5 2.c4 e6",
            "1-0",
            2000,
        );
        assert!(next.movetext_revision > game.movetext_revision);
        assert_eq!(
            mark(
                &conn,
                &game.id,
                Some(&game.account_id),
                game.movetext_revision,
                true
            )
            .unwrap_err()
            .code,
            "conflict"
        );
        assert_eq!(conn.query_row("SELECT count(*) FROM game_review_documents WHERE game_id=?1 AND source_revision=?2",params![game.id,next.movetext_revision],|r|r.get::<_,i64>(0)).unwrap(),0);
        let pending = list(
            &conn,
            ListRequest {
                tag: Some("Persistent party tag".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(pending.total, 1);
        assert_eq!(pending.tags, vec!["Persistent party tag"]);
        assert_eq!(pending.rows[0].tags, pending.tags);
        assert!(pending.rows[0].document_id.is_none());
        assert!(!pending.rows[0].reviewed);
        assert_eq!(
            open(
                &conn,
                &game.id,
                Some(&game.account_id),
                Some(game.movetext_revision)
            )
            .unwrap_err()
            .code,
            "conflict"
        );
        assert_eq!(
            review_source(&conn, &original.document.id)
                .unwrap_err()
                .code,
            "conflict"
        );
        let updated = open(&conn, &next.id, Some(&next.account_id), None).unwrap();
        assert_ne!(updated.document.id, original.document.id);
        assert_eq!(
            knowledge::document_tags(&updated.document),
            vec!["Persistent party tag"]
        );
        assert!(!updated.reviewed);
        assert_eq!(
            knowledge::load(&conn, &original.document.id, None).unwrap(),
            original.document
        );
    }
    #[test]
    fn source_addresses_survive_chapter_reordering() {
        let (_dir, conn, game) = fixture();
        let review = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        let changed = knowledge::apply(
            &conn,
            &review.document.id,
            review.document.revision,
            knowledge::EditOperation::AddChapter {
                title: "Other line".into(),
                initial_fen: None,
                links: vec![],
            },
        )
        .unwrap();
        let changed = knowledge::apply(
            &conn,
            &changed.id,
            changed.revision,
            knowledge::EditOperation::MoveChapter {
                chapter_id: changed.chapters[1].id.clone(),
                index: 0,
            },
        )
        .unwrap();
        assert_ne!(changed.chapters[0].id, review.chapter_id);
        let reopened = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        assert_eq!(reopened.chapter_id, review.chapter_id);
        assert_eq!(reopened.source_nodes.len(), game.replay.nodes.len());
        assert_eq!(
            reopened.source_nodes[3].node_id,
            review.source_nodes[3].node_id
        );
    }
    #[test]
    fn saved_branch_has_new_ids_and_real_occurrence_prefix_for_hypothetical_route() {
        let (_dir, conn, game) = fixture();
        let original = facts(&conn);
        let review = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        let e4 = review.source_nodes[1].node_id.clone();
        let doc = knowledge::apply(
            &conn,
            &review.document.id,
            review.document.revision,
            knowledge::EditOperation::AddMove {
                chapter_id: review.chapter_id.clone(),
                parent_id: e4,
                uci: "c7c5".into(),
            },
        )
        .unwrap();
        let alternate = doc.chapters[0].root.children[0]
            .children
            .iter()
            .find(|n| n.uci.as_deref() == Some("c7c5"))
            .unwrap();
        let saved = save_knowledge(
            &conn,
            SaveKnowledgeRequest {
                document_id: doc.id.clone(),
                expected_revision: doc.revision,
                chapter_id: review.chapter_id,
                node_id: alternate.id.clone(),
                title: "Сицилианская структура".into(),
                scope: "branch".into(),
            },
        )
        .unwrap();
        assert_eq!(saved.kind, "study");
        assert_eq!(saved.chapters[0].initial_fen, alternate.fen);
        assert_ne!(saved.chapters[0].root.id, alternate.id);
        assert!(
            matches!(&saved.chapters[0].links[0].target,LinkTarget::Occurrence{occurrence} if occurrence.ply==1 && occurrence.revision==game.movetext_revision)
        );
        assert_eq!(saved.chapters[0].links[0].path_uci, vec!["e2e4"]);
        assert!(
            matches!(&saved.chapters[0].links[1].target,LinkTarget::DocumentNode{document_id,..} if document_id==&doc.id)
        );
        assert_eq!(facts(&conn), original);
        assert_eq!(
            knowledge::parse_pgn(&knowledge::export_document(&saved).unwrap()).unwrap()[0]
                .root
                .fen,
            saved.chapters[0].root.fen
        );
    }
    #[test]
    fn extraction_preserves_prose_before_and_after_a_variation_move() {
        let (_dir, conn, first) = fixture();
        let game = seed(
            &conn,
            &first.account_id,
            "review01",
            "1.e4 e5 ({Before Sicilian} 1...c5 {After Sicilian} 2.Nf3) 2.Nf3 Nc6",
            "1-0",
            2000,
        );
        let review = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        let chapter = &review.document.chapters[0];
        let node = chapter.root.children[0]
            .children
            .iter()
            .find(|node| node.uci.as_deref() == Some("c7c5"))
            .unwrap();
        assert_eq!(node.starting_comments, vec!["Before Sicilian"]);
        assert_eq!(node.comments, vec!["After Sicilian"]);
        for scope in ["position", "branch"] {
            let saved = save_knowledge(
                &conn,
                SaveKnowledgeRequest {
                    document_id: review.document.id.clone(),
                    expected_revision: review.document.revision,
                    chapter_id: chapter.id.clone(),
                    node_id: node.id.clone(),
                    title: format!("Sicilian {scope}"),
                    scope: scope.into(),
                },
            )
            .unwrap();
            let root = &saved.chapters[0].root;
            assert!(root.starting_comments.is_empty());
            assert_eq!(root.comments, vec!["Before Sicilian", "After Sicilian"]);
            let exported = knowledge::export_document(&saved).unwrap();
            let imported = knowledge::parse_pgn(&exported).unwrap();
            assert_eq!(imported[0].root.comments, root.comments);
            assert_eq!(root.children.is_empty(), scope == "position");
        }
        assert_eq!(
            knowledge::load(&conn, &review.document.id, None).unwrap(),
            review.document
        );
    }
    #[test]
    fn study_repertoire_role_reuses_document_and_training_history() {
        let (_dir, conn, game) = fixture();
        let before = facts(&conn);
        let review = open(&conn, &game.id, Some(&game.account_id), None).unwrap();
        let study = save_knowledge(
            &conn,
            SaveKnowledgeRequest {
                document_id: review.document.id.clone(),
                expected_revision: review.document.revision,
                chapter_id: review.chapter_id,
                node_id: review.node_id,
                title: "My preparation".into(),
                scope: "game".into(),
            },
        )
        .unwrap();
        let prepared = use_repertoire(&conn, &study.id, study.revision, "white").unwrap();
        assert_eq!(prepared.id, study.id);
        assert_eq!(prepared.kind, "study");
        assert_eq!(prepared.chapters, study.chapters);
        let combined = knowledge::fragments::combined(&conn, "white").unwrap();
        assert!(!combined.chapters.is_empty());
        let scope = knowledge::training::Scope {
            side: "white".into(),
            source_document_id: Some(study.id.clone()),
            chapter_id: Some(study.chapters[0].id.clone()),
            node_id: None,
        };
        let overview = knowledge::training::overview(&conn, scope.clone()).unwrap();
        assert!(overview.new_positions > 0);
        let initial = knowledge::training::start(
            &conn,
            knowledge::training::StartRequest {
                scope: scope.clone(),
                mode: "srs".into(),
                restart: false,
                new_limit: 5,
            },
        )
        .unwrap();
        // Learn the source choices, then score an actual recall. A nonempty
        // FSRS memory and attempts must survive attachment lifecycle changes.
        let mut view = initial.clone();
        for _ in 0..30 {
            let scheduled: i64 = conn
                .query_row(
                    "SELECT count(*) FROM repertoire_training_srs WHERE stability IS NOT NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            if scheduled > 0 {
                break;
            }
            view = if matches!(view.feedback.as_str(), "correct" | "assisted") {
                knowledge::training::next(&conn, &view.id).unwrap()
            } else {
                let prompt = view.prompt.as_ref().unwrap();
                let uci = find_node(&study.chapters[0].root, &prompt.node_id)
                    .expect("source-scoped training preserves knowledge node IDs")
                    .children[0]
                    .uci
                    .clone()
                    .unwrap();
                knowledge::training::answer(&conn, &view.id, &uci).unwrap()
            };
        }
        let training_snapshot = |conn: &Connection| {
            ["SELECT json_array(decision_id,side,position_key,solution_key,repetitions,interval_days,ease,due_at_ms,last_review_at_ms,stability,difficulty,scheduler_version) FROM repertoire_training_srs ORDER BY decision_id", "SELECT json_array(id,session_id,decision_id,answered_at_ms,kind,uci,grade) FROM repertoire_training_reviews ORDER BY id"].iter().map(|sql|conn.prepare(sql).unwrap().query_map([], |r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap()).collect::<Vec<_>>()
        };
        let memory = training_snapshot(&conn);
        assert!(!memory[0].is_empty());
        assert!(memory[1].len() > 1);
        let before_detach = knowledge::training::overview(&conn, scope.clone()).unwrap();
        let retained = conn
            .query_row(
                "SELECT state_json FROM repertoire_training_sessions WHERE id=?1",
                [&initial.id],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        let detached =
            knowledge::fragments::set_attachment(&conn, &prepared.id, prepared.revision, false)
                .unwrap();
        assert!(knowledge::training::overview(&conn, scope.clone()).is_err());
        let restored = use_repertoire(&conn, &study.id, detached.revision, "white").unwrap();
        assert_eq!(restored.id, prepared.id);
        assert_eq!(
            knowledge::training::overview(&conn, scope)
                .unwrap()
                .new_positions,
            before_detach.new_positions
        );
        assert_eq!(
            conn.query_row(
                "SELECT state_json FROM repertoire_training_sessions WHERE id=?1",
                [&initial.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            retained
        );
        assert_eq!(training_snapshot(&conn), memory);
        knowledge::delete_study(&conn, &restored.id, restored.revision).unwrap();
        assert_eq!(training_snapshot(&conn), memory);
        assert!(
            knowledge::fragments::combined(&conn, "white")
                .unwrap()
                .chapters
                .is_empty()
        );
        assert_eq!(facts(&conn), before);
    }
}
