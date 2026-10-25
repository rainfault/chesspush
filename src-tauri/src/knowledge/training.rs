//! Position-based training projected from the existing combined VariationTree.
//! No repertoire lines or independently editable chess content are stored here.
use super::*;
use rusqlite::{Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, VecDeque};

const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Scope {
    pub side: String,
    #[serde(default)]
    pub chapter_id: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    // Omitted for legacy merged scopes so their persisted scope keys remain
    // byte-for-byte compatible. When present, chapter/node IDs are source IDs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_document_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartRequest {
    pub scope: Scope,
    pub mode: String,
    #[serde(default)]
    pub restart: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub completed: usize,
    pub total: usize,
    pub mistakes: usize,
    pub finished: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overview {
    pub total: usize,
    pub due: usize,
    pub next_due_at_ms: Option<i64>,
    pub active_srs: Option<SessionSummary>,
    pub active_full_drill: Option<SessionSummary>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMove {
    pub uci: String,
    pub san: String,
    pub fen: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerMove {
    pub uci: String,
    pub san: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompt {
    pub decision_id: String,
    pub chapter_id: String,
    pub node_id: String,
    pub initial_fen: String,
    pub fen: String,
    pub position_key: String,
    pub context: Vec<ContextMove>,
    pub legal_moves: Vec<AnswerMove>,
    pub accepted_moves: Vec<AnswerMove>,
    pub comments: Vec<String>,
    pub attempted_uci: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionView {
    pub id: String,
    pub mode: String,
    pub scope: Scope,
    pub total: usize,
    pub completed: usize,
    pub mistakes: usize,
    pub finished: bool,
    pub feedback: String,
    pub prompt: Option<Prompt>,
}

#[derive(Debug, Clone)]
struct Occurrence {
    chapter_id: String,
    node_id: String,
    initial_fen: String,
    path: Vec<String>,
}
#[derive(Debug, Clone)]
struct Edge {
    uci: String,
    san: String,
    to: String,
    comments: Vec<String>,
}
#[derive(Debug, Clone)]
struct Decision {
    id: String,
    key: String,
    solution_key: String,
    answers: Vec<Edge>,
    comments: Vec<String>,
    occurrence: Occurrence,
}
struct Projection {
    decisions: Vec<Decision>,
    graph: HashMap<String, Vec<Edge>>,
    fingerprint: String,
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn scope_key(scope: &Scope) -> Result<String, AppError> {
    Ok(digest(&serde_json::to_string(scope).map_err(error)?))
}
fn push_comments(target: &mut Vec<String>, values: &[String]) {
    for comment in values {
        if !target.contains(comment) {
            target.push(comment.clone());
        }
    }
}

/// All occurrences contribute legal prepared answers. A scope selects positions
/// to test; transposed occurrences still recognize every prepared alternative.
fn project(document: &KnowledgeDocument, scope: &Scope) -> Result<Projection, AppError> {
    if !["white", "black"].contains(&scope.side.as_str()) {
        return Err(error("Выберите сторону репертуара"));
    }
    if scope.node_id.is_some() && scope.chapter_id.is_none() {
        return Err(error("Для ветви требуется глава репертуара"));
    }
    let mut graph: HashMap<String, Vec<Edge>> = HashMap::new();
    let mut occurrences: HashMap<String, Occurrence> = HashMap::new();
    let mut comments: HashMap<String, Vec<String>> = HashMap::new();
    let mut selected = Vec::new();
    let mut selected_seen = HashSet::new();
    let mut found_chapter = scope.chapter_id.is_none();
    let mut found_node = scope.node_id.is_none();
    struct Walk<'a> {
        scope: &'a Scope,
        graph: &'a mut HashMap<String, Vec<Edge>>,
        occurrences: &'a mut HashMap<String, Occurrence>,
        comments: &'a mut HashMap<String, Vec<String>>,
        selected: &'a mut Vec<String>,
        selected_seen: &'a mut HashSet<String>,
        found_node: &'a mut bool,
    }
    impl Walk<'_> {
        fn visit(
            &mut self,
            chapter: &Chapter,
            node: &VariationNode,
            path: &mut Vec<String>,
            chapter_selected: bool,
            parent_selected: bool,
        ) -> Result<(), AppError> {
            let is_root = self.scope.node_id.as_deref() == Some(node.id.as_str());
            if chapter_selected && is_root {
                *self.found_node = true;
            }
            let selected =
                chapter_selected && (parent_selected || self.scope.node_id.is_none() || is_root);
            let edges = self.graph.entry(node.position_key.clone()).or_default();
            for child in &node.children {
                let uci = child
                    .uci
                    .clone()
                    .ok_or_else(|| error("У подготовленного хода отсутствует UCI"))?;
                let comment_values = child
                    .starting_comments
                    .iter()
                    .chain(child.comments.iter())
                    .cloned()
                    .collect::<Vec<_>>();
                if let Some(existing) = edges.iter_mut().find(|edge| edge.uci == uci) {
                    push_comments(&mut existing.comments, &comment_values);
                } else {
                    edges.push(Edge {
                        uci,
                        san: child.san.clone().unwrap_or_default(),
                        to: child.position_key.clone(),
                        comments: comment_values,
                    });
                }
            }
            push_comments(
                self.comments.entry(node.position_key.clone()).or_default(),
                &node.comments,
            );
            let own = position(&node.fen)?.turn()
                == if self.scope.side == "white" {
                    Color::White
                } else {
                    Color::Black
                };
            if own && !node.children.is_empty() {
                let occurrence = Occurrence {
                    chapter_id: chapter.id.clone(),
                    node_id: node.id.clone(),
                    initial_fen: chapter.initial_fen.clone(),
                    path: path.clone(),
                };
                // Selected representatives keep the requested branch's context.
                if selected && self.selected_seen.insert(node.position_key.clone()) {
                    self.selected.push(node.position_key.clone());
                    self.occurrences
                        .insert(node.position_key.clone(), occurrence);
                } else {
                    self.occurrences
                        .entry(node.position_key.clone())
                        .or_insert(occurrence);
                }
            }
            for child in &node.children {
                path.push(
                    child
                        .uci
                        .clone()
                        .ok_or_else(|| error("У хода отсутствует UCI"))?,
                );
                self.visit(chapter, child, path, chapter_selected, selected)?;
                path.pop();
            }
            Ok(())
        }
    }
    for chapter in &document.chapters {
        let chapter_selected = scope
            .chapter_id
            .as_deref()
            .is_none_or(|id| id == chapter.id);
        if chapter_selected {
            found_chapter = true;
        }
        Walk {
            scope,
            graph: &mut graph,
            occurrences: &mut occurrences,
            comments: &mut comments,
            selected: &mut selected,
            selected_seen: &mut selected_seen,
            found_node: &mut found_node,
        }
        .visit(chapter, &chapter.root, &mut vec![], chapter_selected, false)?;
    }
    if !found_chapter || !found_node {
        return Err(AppError::new(
            "training_scope",
            "Выбранная ветвь изменена или удалена. Выберите её заново.",
        ));
    }
    for edges in graph.values_mut() {
        edges.sort_by(|a, b| a.uci.cmp(&b.uci));
    }
    let mut decisions = Vec::new();
    for key in selected {
        let answers = graph.get(&key).cloned().unwrap_or_default();
        let solution_key = answers
            .iter()
            .map(|edge| edge.uci.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let id = digest(&format!("{}|{key}|{solution_key}", scope.side));
        decisions.push(Decision {
            id,
            key: key.clone(),
            solution_key,
            answers,
            comments: comments.remove(&key).unwrap_or_default(),
            occurrence: occurrences.remove(&key).expect("selected occurrence"),
        });
    }
    let mut ids = decisions
        .iter()
        .map(|decision| {
            format!(
                "{}|{}|{}",
                decision.id,
                decision.occurrence.initial_fen,
                decision.occurrence.path.join(",")
            )
        })
        .collect::<Vec<_>>();
    ids.sort();
    let mut graph_keys = graph
        .iter()
        .map(|(key, edges)| {
            format!(
                "{key}>{}",
                edges
                    .iter()
                    .map(|edge| edge.uci.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>();
    graph_keys.sort();
    let fingerprint = digest(&format!("{}|{}", ids.join("|"), graph_keys.join("|")));
    Ok(Projection {
        decisions,
        graph,
        fingerprint,
    })
}

/// The merged repertoire and a named source chapter are two projections of the
/// same editable documents. A source scope keeps the chapter's own preparation
/// and opponent replies; it never copies those lines into training storage.
fn project_scope(conn: &Connection, scope: &Scope) -> Result<Projection, AppError> {
    let Some(document_id) = &scope.source_document_id else {
        return project(&fragments::combined(conn, &scope.side)?, scope);
    };
    let chapter_id = scope.chapter_id.as_deref().ok_or_else(|| {
        AppError::new(
            "training_scope",
            "Выберите главу исследования для тренировки",
        )
    })?;
    let mut document = load(conn, document_id, None).map_err(|_| {
        AppError::new(
            "training_scope",
            "Исследование удалено. Выберите другую главу.",
        )
    })?;
    if document.kind != "repertoire"
        || !fragments::is_attachment(&document)
        || !fragments::is_active(&document)
        || !(document.side == scope.side || document.side == "both")
    {
        return Err(AppError::new(
            "training_scope",
            "Исследование не подключено к этому репертуару. Подключите PGN или выберите другую главу.",
        ));
    }
    document.chapters.retain(|chapter| chapter.id == chapter_id);
    if document.chapters.is_empty() {
        return Err(AppError::new(
            "training_scope",
            "Глава исследования изменена или удалена. Выберите другую главу.",
        ));
    }
    project(&document, scope)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    id: String,
    scope: Scope,
    mode: String,
    fingerprint: String,
    items: Vec<String>,
    completed: BTreeSet<String>,
    failed: BTreeSet<String>,
    current: Option<String>,
    initial_fen: String,
    path: Vec<String>,
    feedback: String,
    attempted_uci: Option<String>,
    created_at_ms: i64,
}
impl Session {
    fn summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id.clone(),
            completed: self.completed.len(),
            total: self.items.len(),
            mistakes: self.failed.len(),
            finished: self.current.is_none(),
        }
    }
}
fn save(conn: &Connection, session: &Session) -> Result<(), AppError> {
    conn.execute("INSERT INTO repertoire_training_sessions(id,scope_key,mode,state_json,created_at_ms,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET state_json=excluded.state_json,updated_at_ms=excluded.updated_at_ms", params![session.id,scope_key(&session.scope)?,session.mode,serde_json::to_string(session).map_err(error)?,session.created_at_ms,now_ms()])?;
    Ok(())
}
fn load_session(conn: &Connection, id: &str) -> Result<Session, AppError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT state_json FROM repertoire_training_sessions WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    serde_json::from_str(&json.ok_or_else(|| error("Тренировка не найдена"))?).map_err(error)
}
fn latest(conn: &Connection, scope: &Scope, mode: &str) -> Result<Option<Session>, AppError> {
    let mut query = conn.prepare("SELECT state_json FROM repertoire_training_sessions WHERE scope_key=?1 AND mode=?2 ORDER BY created_at_ms DESC,rowid DESC LIMIT 1")?;
    for json in query.query_map(params![scope_key(scope)?, mode], |row| {
        row.get::<_, String>(0)
    })? {
        let session: Session = serde_json::from_str(&json?).map_err(error)?;
        if session.current.is_some() {
            return Ok(Some(session));
        }
    }
    Ok(None)
}
fn due_at(conn: &Connection, id: &str) -> Result<Option<i64>, AppError> {
    Ok(conn
        .query_row(
            "SELECT due_at_ms FROM repertoire_training_srs WHERE decision_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?)
}
fn eligible(
    conn: &Connection,
    projection: &Projection,
    mode: &str,
    now: i64,
) -> Result<Vec<String>, AppError> {
    let mut result = vec![];
    for decision in &projection.decisions {
        if mode == "full_drill" || due_at(conn, &decision.id)?.is_none_or(|due| due <= now) {
            result.push(decision.id.clone());
        }
    }
    Ok(result)
}
fn set_current(session: &mut Session, decision: Option<&Decision>) {
    session.current = decision.map(|value| value.id.clone());
    session.attempted_uci = None;
    session.feedback = if decision.is_some() {
        "awaiting"
    } else {
        "finished"
    }
    .into();
    if let Some(decision) = decision {
        session.initial_fen = decision.occurrence.initial_fen.clone();
        session.path = decision.occurrence.path.clone();
        if session.failed.contains(&decision.id) {
            session.feedback = "retry".into();
        }
    }
}
fn refresh(
    conn: &Connection,
    session: &mut Session,
    projection: &Projection,
) -> Result<(), AppError> {
    let content_changed = session.fingerprint != projection.fingerprint;
    if !content_changed && session.mode != "srs" {
        return Ok(());
    }
    let valid = projection
        .decisions
        .iter()
        .map(|item| item.id.clone())
        .collect::<HashSet<_>>();
    session.items.retain(|id| valid.contains(id));
    session.completed.retain(|id| valid.contains(id));
    session.failed.retain(|id| valid.contains(id));
    if session.mode == "srs" {
        let due = eligible(conn, projection, "srs", now_ms())?
            .into_iter()
            .collect::<HashSet<_>>();
        session.items.retain(|id| {
            session.completed.contains(id) || session.failed.contains(id) || due.contains(id)
        });
    }
    if content_changed {
        let mut included = session.items.iter().cloned().collect::<HashSet<_>>();
        for id in eligible(conn, projection, &session.mode, now_ms())? {
            if included.insert(id.clone()) {
                session.items.push(id);
            }
        }
    }
    session.fingerprint = projection.fingerprint.clone();
    let included = session
        .items
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    // Rebuild context from current PGN: a removed route must never be replayed.
    let current = session
        .current
        .as_deref()
        .filter(|id| included.contains(id))
        .and_then(|id| projection.decisions.iter().find(|item| item.id == id));
    if let Some(current) = current {
        if content_changed {
            session.initial_fen = current.occurrence.initial_fen.clone();
            session.path = current.occurrence.path.clone();
        }
    } else {
        let next = projection.decisions.iter().find(|item| {
            included.contains(item.id.as_str()) && !session.completed.contains(&item.id)
        });
        set_current(session, next);
    }
    Ok(())
}
fn replay(initial_fen: &str, path: &[String]) -> Result<(Chess, Vec<ContextMove>), AppError> {
    let mut pos = position(initial_fen)?;
    let mut context = vec![];
    for uci in path {
        let mv = uci
            .parse::<UciMove>()
            .map_err(error)?
            .to_move(&pos)
            .map_err(error)?;
        let san = SanPlus::from_move(pos.clone(), mv).to_string();
        pos.play_unchecked(mv);
        context.push(ContextMove {
            uci: uci.clone(),
            san,
            fen: Fen::from_position(&pos, EnPassantMode::Always).to_string(),
        });
    }
    Ok((pos, context))
}
fn view(session: &Session, projection: &Projection) -> Result<SessionView, AppError> {
    let prompt = if let Some(current) = &session.current {
        let decision = projection
            .decisions
            .iter()
            .find(|item| &item.id == current)
            .ok_or_else(|| error("Позиция тренировки изменена"))?;
        let (pos, context) = replay(&session.initial_fen, &session.path)?;
        if position_key(&pos) != decision.key {
            return Err(error("Контекст тренировки изменён"));
        }
        let revealed = session.feedback != "awaiting";
        let mut comments = decision.comments.clone();
        for answer in &decision.answers {
            push_comments(&mut comments, &answer.comments);
        }
        Some(Prompt {
            decision_id: decision.id.clone(),
            chapter_id: decision.occurrence.chapter_id.clone(),
            node_id: decision.occurrence.node_id.clone(),
            initial_fen: session.initial_fen.clone(),
            fen: Fen::from_position(&pos, EnPassantMode::Always).to_string(),
            position_key: decision.key.clone(),
            context,
            legal_moves: pos
                .legal_moves()
                .iter()
                .map(|mv| AnswerMove {
                    uci: UciMove::from_move(*mv, CastlingMode::Standard).to_string(),
                    san: SanPlus::from_move(pos.clone(), *mv).to_string(),
                })
                .collect(),
            accepted_moves: if revealed {
                decision
                    .answers
                    .iter()
                    .map(|edge| AnswerMove {
                        uci: edge.uci.clone(),
                        san: edge.san.clone(),
                    })
                    .collect()
            } else {
                vec![]
            },
            comments: if revealed { comments } else { vec![] },
            attempted_uci: session.attempted_uci.clone(),
        })
    } else {
        None
    };
    Ok(SessionView {
        id: session.id.clone(),
        mode: session.mode.clone(),
        scope: session.scope.clone(),
        total: session.items.len(),
        completed: session.completed.len(),
        mistakes: session.failed.len(),
        finished: session.current.is_none(),
        feedback: session.feedback.clone(),
        prompt,
    })
}

pub fn overview(conn: &Connection, scope: Scope) -> Result<Overview, AppError> {
    let tx = conn.unchecked_transaction()?;
    let projection = project_scope(&tx, &scope)?;
    let now = now_ms();
    let mut due = 0;
    let mut next_due_at_ms: Option<i64> = None;
    for decision in &projection.decisions {
        match due_at(&tx, &decision.id)? {
            Some(at) if at > now => {
                next_due_at_ms = Some(next_due_at_ms.map_or(at, |existing| existing.min(at)))
            }
            _ => due += 1,
        }
    }
    let active = |mode: &str| -> Result<Option<SessionSummary>, AppError> {
        if let Some(mut session) = latest(&tx, &scope, mode)? {
            refresh(&tx, &mut session, &projection)?;
            // Overview keeps a read snapshot; the next mutation reconciles and
            // saves the session, so viewing progress never needs a writer lock.
            Ok(session.current.is_some().then(|| session.summary()))
        } else {
            Ok(None)
        }
    };
    let result = Overview {
        total: projection.decisions.len(),
        due,
        next_due_at_ms,
        active_srs: active("srs")?,
        active_full_drill: active("full_drill")?,
    };
    tx.commit()?;
    Ok(result)
}
pub fn start(conn: &Connection, request: StartRequest) -> Result<SessionView, AppError> {
    if !["srs", "full_drill"].contains(&request.mode.as_str()) {
        return Err(error("Неизвестный режим тренировки"));
    }
    // Acquire the writer before taking a WAL read snapshot. A deferred
    // read-to-write upgrade can fail immediately after a background commit,
    // bypassing the connection's busy timeout.
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let projection = project_scope(&tx, &request.scope)?;
    let existing = if request.restart {
        None
    } else {
        latest(&tx, &request.scope, &request.mode)?
    };
    let session = if let Some(mut session) = existing {
        refresh(&tx, &mut session, &projection)?;
        session
    } else {
        let mut session = Session {
            id: new_id("training")?,
            scope: request.scope,
            mode: request.mode,
            fingerprint: projection.fingerprint.clone(),
            items: vec![],
            completed: BTreeSet::new(),
            failed: BTreeSet::new(),
            current: None,
            initial_fen: INITIAL_FEN.into(),
            path: vec![],
            feedback: "finished".into(),
            attempted_uci: None,
            created_at_ms: now_ms(),
        };
        session.items = eligible(&tx, &projection, &session.mode, now_ms())?;
        let first = projection
            .decisions
            .iter()
            .find(|item| session.items.contains(&item.id));
        set_current(&mut session, first);
        session
    };
    save(&tx, &session)?;
    let result = view(&session, &projection)?;
    tx.commit()?;
    Ok(result)
}

/// Published SuperMemo SM-2 with fixed grades: 4 = unaided recall,
/// 1 = failure / hint. Guided correction never schedules a second review.
/// https://www.super-memory.com/english/ol/sm2.htm
fn schedule(
    conn: &Connection,
    session: &Session,
    decision: &Decision,
    grade: u8,
    timestamp: i64,
) -> Result<(), AppError> {
    let previous: Option<(i64,i64,f64)> = conn.query_row("SELECT repetitions,interval_days,ease FROM repertoire_training_srs WHERE decision_id=?1",[&decision.id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    let (repetitions, interval, ease) = previous.unwrap_or((0, 0, 2.5));
    let next_repetitions = if grade < 3 { 0 } else { repetitions + 1 };
    let next_interval = if grade < 3 || next_repetitions == 1 {
        1
    } else if next_repetitions == 2 {
        6
    } else {
        ((interval as f64) * ease).ceil().min(36_500.0) as i64
    };
    let delta = 5.0 - grade as f64;
    let next_ease = (ease + 0.1 - delta * (0.08 + delta * 0.02)).max(1.3);
    let due = timestamp.saturating_add(next_interval * DAY_MS);
    conn.execute("INSERT INTO repertoire_training_srs(decision_id,side,position_key,solution_key,repetitions,interval_days,ease,due_at_ms,last_review_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(decision_id) DO UPDATE SET repetitions=excluded.repetitions,interval_days=excluded.interval_days,ease=excluded.ease,due_at_ms=excluded.due_at_ms,last_review_at_ms=excluded.last_review_at_ms",params![decision.id,session.scope.side,decision.key,decision.solution_key,next_repetitions,next_interval,next_ease,due,timestamp])?;
    Ok(())
}
fn record(
    conn: &Connection,
    session: &Session,
    decision: &Decision,
    kind: &str,
    uci: Option<&str>,
    grade: Option<u8>,
) -> Result<(), AppError> {
    let timestamp = now_ms();
    conn.execute("INSERT INTO repertoire_training_reviews(session_id,decision_id,answered_at_ms,kind,uci,grade) VALUES(?1,?2,?3,?4,?5,?6)",params![session.id,decision.id,timestamp,kind,uci,grade])?;
    if session.mode == "srs"
        && let Some(grade) = grade
    {
        schedule(conn, session, decision, grade, timestamp)?;
    }
    Ok(())
}
fn current_projection(conn: &Connection, session: &mut Session) -> Result<Projection, AppError> {
    let projection = project_scope(conn, &session.scope)?;
    refresh(conn, session, &projection)?;
    Ok(projection)
}
pub fn answer(conn: &Connection, session_id: &str, uci: &str) -> Result<SessionView, AppError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let mut session = load_session(&tx, session_id)?;
    let before = session.current.clone();
    let projection = current_projection(&tx, &mut session)?;
    if before != session.current {
        save(&tx, &session)?;
        let result = view(&session, &projection)?;
        tx.commit()?;
        return Ok(result);
    }
    let current = session
        .current
        .as_deref()
        .ok_or_else(|| error("Тренировка завершена"))?;
    let decision = projection
        .decisions
        .iter()
        .find(|item| item.id == current)
        .expect("validated current");
    if session.completed.contains(current) {
        return view(&session, &projection);
    }
    let (pos, _) = replay(&session.initial_fen, &session.path)?;
    uci.parse::<UciMove>()
        .map_err(error)?
        .to_move(&pos)
        .map_err(|_| error("Недопустимый ход"))?;
    let correct = decision.answers.iter().any(|edge| edge.uci == uci);
    if correct {
        let assisted = session.failed.contains(current);
        record(
            &tx,
            &session,
            decision,
            if assisted { "retry" } else { "correct" },
            Some(uci),
            if assisted { None } else { Some(4) },
        )?;
        session.completed.insert(current.into());
        session.feedback = if assisted { "assisted" } else { "correct" }.into();
    } else {
        let first = !session.failed.contains(current);
        record(
            &tx,
            &session,
            decision,
            "error",
            Some(uci),
            first.then_some(1),
        )?;
        session.failed.insert(current.into());
        session.feedback = "retry".into();
    }
    session.attempted_uci = Some(uci.into());
    save(&tx, &session)?;
    let result = view(&session, &projection)?;
    tx.commit()?;
    Ok(result)
}
pub fn hint(conn: &Connection, session_id: &str) -> Result<SessionView, AppError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let mut session = load_session(&tx, session_id)?;
    let before = session.current.clone();
    let projection = current_projection(&tx, &mut session)?;
    if before != session.current {
        save(&tx, &session)?;
        let result = view(&session, &projection)?;
        tx.commit()?;
        return Ok(result);
    }
    let current = session
        .current
        .as_deref()
        .ok_or_else(|| error("Тренировка завершена"))?;
    let decision = projection
        .decisions
        .iter()
        .find(|item| item.id == current)
        .expect("validated current");
    if !session.completed.contains(current) && !session.failed.contains(current) {
        record(&tx, &session, decision, "hint", None, Some(1))?;
        session.failed.insert(current.into());
        session.feedback = "retry".into();
        session.attempted_uci = None;
        save(&tx, &session)?;
    }
    let result = view(&session, &projection)?;
    tx.commit()?;
    Ok(result)
}

/// Choose the nearest pending decision reachable after the user's accepted
/// move. Visited PositionKeys terminate transpositions and repetition cycles;
/// undiscovered branches are later entered from their canonical PGN context.
pub fn next(conn: &Connection, session_id: &str) -> Result<SessionView, AppError> {
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let mut session = load_session(&tx, session_id)?;
    let before = session.current.clone();
    let projection = current_projection(&tx, &mut session)?;
    if before != session.current {
        save(&tx, &session)?;
        let result = view(&session, &projection)?;
        tx.commit()?;
        return Ok(result);
    }
    if session.current.is_none() {
        return view(&session, &projection);
    }
    if !session
        .completed
        .contains(session.current.as_deref().unwrap())
    {
        return Err(error("Сначала воспроизведите подготовленный ход"));
    }
    let included = session
        .items
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let pending = projection
        .decisions
        .iter()
        .filter(|item| included.contains(item.id.as_str()) && !session.completed.contains(&item.id))
        .collect::<Vec<_>>();
    let pending_positions = pending
        .iter()
        .map(|decision| (decision.key.as_str(), *decision))
        .collect::<HashMap<_, _>>();
    let (mut after, _) = replay(&session.initial_fen, &session.path)?;
    let accepted = session
        .attempted_uci
        .clone()
        .ok_or_else(|| error("Отсутствует сыгранный ход"))?;
    let mv = accepted
        .parse::<UciMove>()
        .map_err(error)?
        .to_move(&after)
        .map_err(error)?;
    after.play_unchecked(mv);
    let mut base = session.path.clone();
    base.push(accepted);
    let mut queue = VecDeque::from([(position_key(&after), vec![])]);
    let mut visited = HashSet::new();
    let mut chosen = None;
    while let Some((key, path)) = queue.pop_front() {
        if !visited.insert(key.clone()) {
            continue;
        }
        if let Some(decision) = pending_positions.get(key.as_str()) {
            chosen = Some((*decision, path));
            break;
        }
        for edge in projection.graph.get(&key).into_iter().flatten() {
            let mut next_path = path.clone();
            next_path.push(edge.uci.clone());
            queue.push_back((edge.to.clone(), next_path));
        }
    }
    if let Some((decision, path)) = chosen {
        let initial = session.initial_fen.clone();
        set_current(&mut session, Some(decision));
        session.initial_fen = initial;
        session.path = base;
        session.path.extend(path);
    } else {
        set_current(&mut session, pending.first().copied());
    }
    save(&tx, &session)?;
    let result = view(&session, &projection)?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    const REPERTOIRE: &str = "[Event \"Queen pawn\"]\n\n1.d4 d5 2.Nf3 Nf6 3.c4 {Central break} e6 (3...g6 4.Nc3 Bg7) 4.Nc3 *\n\n[Event \"Transposition\"]\n\n1.Nf3 Nf6 2.d4 d5 3.e3 {Quiet development} e6 4.Bd3 *";
    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
        conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/006_knowledge.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/008_repertoire_training.sql"))
            .unwrap();
        conn
    }
    fn attached(conn: &Connection, pgn: &str, side: &str) -> KnowledgeDocument {
        let document = super::super::import(
            conn,
            ImportRequest {
                title: "Preparation".into(),
                side: side.into(),
                pgn: pgn.into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        fragments::set_attachment(conn, &document.id, document.revision, true).unwrap()
    }
    fn scope(side: &str) -> Scope {
        Scope {
            side: side.into(),
            chapter_id: None,
            node_id: None,
            source_document_id: None,
        }
    }
    fn begin(conn: &Connection, mode: &str) -> SessionView {
        start(
            conn,
            StartRequest {
                scope: scope("white"),
                mode: mode.into(),
                restart: false,
            },
        )
        .unwrap()
    }
    fn projection(conn: &Connection) -> Projection {
        project(
            &fragments::combined(conn, "white").unwrap(),
            &scope("white"),
        )
        .unwrap()
    }
    fn choose(conn: &Connection, view: &SessionView) -> String {
        project_scope(conn, &view.scope)
            .unwrap()
            .decisions
            .iter()
            .find(|item| item.id == view.prompt.as_ref().unwrap().decision_id)
            .unwrap()
            .answers[0]
            .uci
            .clone()
    }
    fn finish(conn: &Connection, mut view: SessionView) -> SessionView {
        let mut iterations = 0;
        while !view.finished {
            iterations += 1;
            assert!(iterations < 100, "must terminate even with cycles");
            if view.feedback == "awaiting" || view.feedback == "retry" {
                let uci = choose(conn, &view);
                view = answer(conn, &view.id, &uci).unwrap();
            }
            view = next(conn, &view.id).unwrap();
        }
        view
    }
    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    #[test]
    fn transpositions_combine_all_prepared_answers_and_scope_keeps_context() {
        let conn = db();
        attached(&conn, REPERTOIRE, "white");
        let document = fragments::combined(&conn, "white").unwrap();
        let all = project(&document, &scope("white")).unwrap();
        let pos = replay(
            INITIAL_FEN,
            &["d2d4", "d7d5", "g1f3", "g8f6"].map(str::to_owned),
        )
        .unwrap()
        .0;
        let matching = all
            .decisions
            .iter()
            .filter(|item| item.key == position_key(&pos))
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].solution_key, "c2c4,e2e3");
        assert_eq!(
            all.decisions
                .iter()
                .map(|item| &item.key)
                .collect::<HashSet<_>>()
                .len(),
            all.decisions.len()
        );
        let root = &document.chapters[0].root;
        let d4 = root
            .children
            .iter()
            .find(|node| node.uci.as_deref() == Some("d2d4"))
            .unwrap();
        let branch_scope = Scope {
            side: "white".into(),
            chapter_id: Some(document.chapters[0].id.clone()),
            node_id: Some(d4.id.clone()),
            source_document_id: None,
        };
        let branch = project(&document, &branch_scope).unwrap();
        assert!(
            !branch
                .decisions
                .iter()
                .any(|item| item.key == root.position_key)
        );
        assert!(
            branch
                .decisions
                .iter()
                .all(|item| item.occurrence.path.first().map(String::as_str) == Some("d2d4"))
        );
        assert!(
            branch
                .decisions
                .iter()
                .any(|item| item.id == matching[0].id)
        );
        assert!(
            project(
                &document,
                &Scope {
                    side: "white".into(),
                    chapter_id: Some("removed".into()),
                    node_id: None,
                    source_document_id: None,
                }
            )
            .is_err()
        );
    }

    #[test]
    fn full_drill_covers_every_unique_decision_including_rare_branch_without_srs_changes() {
        let conn = db();
        attached(&conn, REPERTOIRE, "white");
        let expected = projection(&conn)
            .decisions
            .iter()
            .map(|item| item.id.clone())
            .collect::<HashSet<_>>();
        let mut view = begin(&conn, "full_drill");
        let mut seen = HashSet::new();
        let mut saw_rare = false;
        while !view.finished {
            let prompt = view.prompt.as_ref().unwrap();
            assert!(
                seen.insert(prompt.decision_id.clone()),
                "same positional decision tested twice"
            );
            saw_rare |= prompt.context.iter().any(|mv| mv.uci == "g7g6");
            assert!(prompt.accepted_moves.is_empty());
            assert!(prompt.comments.is_empty());
            let uci = choose(&conn, &view);
            view = answer(&conn, &view.id, &uci).unwrap();
            assert_eq!(view.feedback, "correct");
            assert!(!view.prompt.as_ref().unwrap().accepted_moves.is_empty());
            // Duplicate requests cannot schedule or complete a position twice.
            assert_eq!(
                answer(&conn, &view.id, &uci).unwrap().completed,
                view.completed
            );
            view = next(&conn, &view.id).unwrap();
        }
        assert_eq!(seen, expected);
        assert!(saw_rare);
        assert_eq!(view.completed, view.total);
        assert_eq!(count(&conn, "repertoire_training_srs"), 0);
        assert_eq!(
            count(&conn, "repertoire_training_reviews"),
            view.total as i64
        );
    }

    #[test]
    fn srs_wrong_hint_and_guided_correction_grade_once_and_preserve_resumable_state() {
        let conn = db();
        attached(&conn, REPERTOIRE, "white");
        let original = begin(&conn, "srs");
        let id = original.id.clone();
        let failed = answer(&conn, &id, "a2a3").unwrap();
        assert_eq!(failed.feedback, "retry");
        assert_eq!(failed.completed, 0);
        assert_eq!(failed.mistakes, 1);
        assert!(next(&conn, &id).is_err());
        let rows: (i64, i64, f64) = conn
            .query_row(
                "SELECT repetitions,interval_days,ease FROM repertoire_training_srs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((rows.0, rows.1), (0, 1));
        assert!((rows.2 - 1.96).abs() < 1e-10);
        hint(&conn, &id).unwrap();
        hint(&conn, &id).unwrap();
        assert_eq!(count(&conn, "repertoire_training_reviews"), 1);
        let resumed = begin(&conn, "srs");
        assert_eq!(resumed.id, id);
        assert_eq!(resumed.feedback, "retry");
        let correction = choose(&conn, &resumed);
        let corrected = answer(&conn, &id, &correction).unwrap();
        assert_eq!(corrected.feedback, "assisted");
        assert_eq!(corrected.completed, 1);
        let graded: i64 = conn
            .query_row(
                "SELECT count(*) FROM repertoire_training_reviews WHERE grade IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(graded, 1);
        let next_view = next(&conn, &id).unwrap();
        assert_eq!(hint(&conn, &id).unwrap().feedback, "retry");
        let guided = answer(&conn, &id, &choose(&conn, &next_view)).unwrap();
        assert_eq!(guided.feedback, "assisted");
        assert_eq!(guided.mistakes, 2);
        assert_eq!(count(&conn, "repertoire_training_srs"), 2);
    }

    #[test]
    fn srs_due_selection_and_full_drill_are_independent() {
        let conn = db();
        attached(&conn, REPERTOIRE, "white");
        let start_view = begin(&conn, "srs");
        let finished = finish(&conn, start_view);
        let before:String=conn.query_row("SELECT group_concat(decision_id||':'||due_at_ms||':'||repetitions||':'||ease) FROM repertoire_training_srs",[],|row|row.get(0)).unwrap();
        assert_eq!(overview(&conn, scope("white")).unwrap().due, 0);
        assert!(begin(&conn, "srs").finished);
        let drill = begin(&conn, "full_drill");
        assert_eq!(drill.total, finished.total);
        finish(&conn, drill);
        let after:String=conn.query_row("SELECT group_concat(decision_id||':'||due_at_ms||':'||repetitions||':'||ease) FROM repertoire_training_srs",[],|row|row.get(0)).unwrap();
        assert_eq!(before, after);
        conn.execute("UPDATE repertoire_training_srs SET due_at_ms=0 WHERE decision_id=(SELECT decision_id FROM repertoire_training_srs LIMIT 1)",[]).unwrap();
        assert_eq!(begin(&conn, "srs").total, 1);
    }

    #[test]
    fn sm2_uses_published_intervals_and_ease_minimum() {
        let conn = db();
        attached(&conn, "1.e4 *", "white");
        let view = begin(&conn, "srs");
        let session = load_session(&conn, &view.id).unwrap();
        let projected = projection(&conn);
        let decision = &projected.decisions[0];
        for (index, expected) in [1, 6, 15, 38].iter().enumerate() {
            schedule(&conn, &session, decision, 4, 1000 + index as i64).unwrap();
            let interval: i64 = conn
                .query_row(
                    "SELECT interval_days FROM repertoire_training_srs",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(interval, *expected);
        }
        for _ in 0..10 {
            schedule(&conn, &session, decision, 1, 1000).unwrap();
        }
        let result: (i64, i64, f64) = conn
            .query_row(
                "SELECT repetitions,interval_days,ease FROM repertoire_training_srs",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(result, (0, 1, 1.3));
    }

    #[test]
    fn changed_pgn_keeps_unchanged_solution_history_and_refreshes_coverage() {
        let conn = db();
        let document = attached(&conn, REPERTOIRE, "white");
        let session = begin(&conn, "srs");
        let old_root = session.prompt.as_ref().unwrap().decision_id.clone();
        answer(&conn, &session.id, "d2d4").unwrap();
        let old_projection = projection(&conn);
        let root = &document.chapters[0].root;
        let changed = super::super::apply(
            &conn,
            &document.id,
            document.revision,
            EditOperation::AddMove {
                chapter_id: document.chapters[0].id.clone(),
                parent_id: root.id.clone(),
                uci: "c2c4".into(),
            },
        )
        .unwrap();
        let new_projection = projection(&conn);
        let new_root = &new_projection.decisions[0];
        assert_ne!(old_root, new_root.id);
        assert!(due_at(&conn, &new_root.id).unwrap().is_none());
        assert!(due_at(&conn, &old_root).unwrap().is_some());
        for old in old_projection.decisions.iter().skip(1) {
            assert!(
                new_projection
                    .decisions
                    .iter()
                    .any(|item| item.id == old.id)
            );
        }
        let resumed = begin(&conn, "srs");
        assert_eq!(resumed.id, session.id);
        assert_eq!(resumed.completed, 0);
        assert_eq!(resumed.prompt.as_ref().unwrap().decision_id, new_root.id);
        // A comment edit cannot invalidate an unchanged decision or its schedule.
        answer(&conn, &resumed.id, "d2d4").unwrap();
        let before = due_at(&conn, &new_root.id).unwrap();
        super::super::apply(
            &conn,
            &changed.id,
            changed.revision,
            EditOperation::Annotate {
                chapter_id: changed.chapters[0].id.clone(),
                node_id: changed.chapters[0].root.id.clone(),
                comments: vec!["New comment".into()],
                nags: vec![],
                shapes: vec![],
            },
        )
        .unwrap();
        assert_eq!(begin(&conn, "srs").completed, 1);
        assert_eq!(due_at(&conn, &new_root.id).unwrap(), before);
    }

    #[test]
    fn repetition_cycles_and_black_repertoire_terminate_and_restart_supersedes_old_run() {
        let conn = db();
        attached(&conn, "1.Nf3 Nf6 2.Ng1 Ng8 3.e4 e5 4.Nf3 *", "white");
        let all = projection(&conn);
        let root = all
            .decisions
            .iter()
            .find(|item| item.key == position_key(&Chess::default()))
            .unwrap();
        assert_eq!(root.solution_key, "e2e4,g1f3");
        let abandoned = begin(&conn, "full_drill");
        let restarted = start(
            &conn,
            StartRequest {
                scope: scope("white"),
                mode: "full_drill".into(),
                restart: true,
            },
        )
        .unwrap();
        assert_ne!(abandoned.id, restarted.id);
        finish(&conn, restarted);
        assert!(
            overview(&conn, scope("white"))
                .unwrap()
                .active_full_drill
                .is_none()
        );
        attached(&conn, "1.e4 e5 2.Nf3 Nc6 (2...Nf6) 3.Bb5 *", "black");
        let black = start(
            &conn,
            StartRequest {
                scope: scope("black"),
                mode: "full_drill".into(),
                restart: false,
            },
        )
        .unwrap();
        assert_eq!(black.prompt.as_ref().unwrap().context[0].uci, "e2e4");
        assert_eq!(black.total, 2);
        let first = answer(&conn, &black.id, "e7e5").unwrap();
        assert_eq!(first.feedback, "correct");
        let second = next(&conn, &black.id).unwrap();
        assert_eq!(
            second.prompt.as_ref().unwrap().context.last().unwrap().uci,
            "g1f3"
        );
        assert_eq!(
            answer(&conn, &black.id, "g8f6").unwrap().feedback,
            "correct"
        );
        assert!(next(&conn, &black.id).unwrap().finished);
    }

    #[test]
    fn overlapping_srs_scopes_skip_decisions_already_reviewed_elsewhere() {
        let conn = db();
        attached(&conn, REPERTOIRE, "white");
        let whole = begin(&conn, "srs");
        let total = whole.total;
        let document = fragments::combined(&conn, "white").unwrap();
        let selected = &document.chapters[0].root;
        let scoped = start(
            &conn,
            StartRequest {
                scope: Scope {
                    side: "white".into(),
                    chapter_id: Some(document.chapters[0].id.clone()),
                    node_id: Some(selected.id.clone()),
                    source_document_id: None,
                },
                mode: "srs".into(),
                restart: false,
            },
        )
        .unwrap();
        answer(&conn, &scoped.id, "d2d4").unwrap();
        let resumed = begin(&conn, "srs");
        assert_eq!(resumed.id, whole.id);
        assert_eq!(resumed.total, total - 1);
        assert_ne!(
            resumed.prompt.as_ref().unwrap().decision_id,
            whole.prompt.as_ref().unwrap().decision_id
        );
        let grade_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM repertoire_training_reviews WHERE grade IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(grade_count, 1);
        // An in-flight old answer is discarded after reconciliation, not graded
        // against the different current task chosen during the refresh.
        let other = start(
            &conn,
            StartRequest {
                scope: Scope {
                    side: "white".into(),
                    chapter_id: Some(document.chapters[0].id.clone()),
                    node_id: Some(selected.id.clone()),
                    source_document_id: None,
                },
                mode: "srs".into(),
                restart: true,
            },
        )
        .unwrap();
        let uci = choose(&conn, &other);
        answer(&conn, &other.id, &uci).unwrap();
        let stale = answer(&conn, &whole.id, &choose(&conn, &resumed)).unwrap();
        assert_eq!(stale.feedback, "awaiting");
        let count_after: i64 = conn
            .query_row(
                "SELECT count(*) FROM repertoire_training_reviews WHERE grade IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count_after, 2);
    }

    #[test]
    fn durable_drill_resumes_failed_correction_after_database_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("training.sqlite3");
        let id;
        {
            let conn = crate::storage::open(&path).unwrap();
            attached(&conn, REPERTOIRE, "white");
            let view = begin(&conn, "full_drill");
            id = view.id.clone();
            hint(&conn, &id).unwrap();
        }
        let conn = crate::storage::open(&path).unwrap();
        let view = begin(&conn, "full_drill");
        assert_eq!(view.id, id);
        assert_eq!(view.feedback, "retry");
        assert_eq!(view.mistakes, 1);
        assert_eq!(view.completed, 0);
        let uci = choose(&conn, &view);
        assert_eq!(answer(&conn, &id, &uci).unwrap().feedback, "assisted");
        assert_eq!(count(&conn, "repertoire_training_srs"), 0);
    }

    const STUDY: &str = "[Event \"Study: English\"]\n[ChapterName \"English\"]\n\n1.c4 e5 2.Nc3 Nf6 (2...Nc6 3.g3) 3.g3 *\n\n[Event \"Study: King pawn\"]\n[ChapterName \"King pawn\"]\n\n1.e4 e5 2.Nf3 Nc6 3.Bb5 *\n\n[Event \"Study: Queen pawn\"]\n[ChapterName \"Queen pawn\"]\n\n1.d4 d5 2.Nf3 Nf6 3.c4 *\n\n[Event \"Study: Reti\"]\n[ChapterName \"Reti\"]\n\n1.Nf3 Nf6 2.d4 d5 3.c4 *\n\n[Event \"Study: Flank\"]\n[ChapterName \"Flank\"]\n\n1.g3 d5 2.Bg2 *";
    fn source_scope(document: &KnowledgeDocument, index: usize) -> Scope {
        Scope {
            side: "white".into(),
            source_document_id: Some(document.id.clone()),
            chapter_id: Some(document.chapters[index].id.clone()),
            node_id: None,
        }
    }
    fn source_begin(
        conn: &Connection,
        document: &KnowledgeDocument,
        index: usize,
        mode: &str,
    ) -> SessionView {
        start(
            conn,
            StartRequest {
                scope: source_scope(document, index),
                mode: mode.into(),
                restart: false,
            },
        )
        .unwrap()
    }

    #[test]
    fn legacy_scope_serialization_and_keys_remain_unchanged() {
        let serialized = r#"{"side":"white","chapter_id":null,"node_id":null}"#;
        assert_eq!(serde_json::to_string(&scope("white")).unwrap(), serialized);
        assert_eq!(scope_key(&scope("white")).unwrap(), digest(serialized));
        let old: Scope = serde_json::from_str(serialized).unwrap();
        assert_eq!(old, scope("white"));
    }

    #[test]
    fn named_chapter_isolates_answers_replies_and_branch_coverage() {
        let conn = db();
        let document = attached(&conn, STUDY, "white");
        assert_eq!(document.chapters.len(), 5);
        let all = projection(&conn);
        let all_root = all
            .decisions
            .iter()
            .find(|item| item.occurrence.path.is_empty())
            .unwrap();
        assert_eq!(all_root.answers.len(), 5);
        let first = source_begin(&conn, &document, 0, "full_drill");
        assert_eq!(first.total, 4);
        assert_eq!(
            first.prompt.as_ref().unwrap().chapter_id,
            document.chapters[0].id
        );
        let rejected = answer(&conn, &first.id, "e2e4").unwrap();
        assert_eq!(rejected.feedback, "retry");
        assert_eq!(
            rejected
                .prompt
                .as_ref()
                .unwrap()
                .accepted_moves
                .iter()
                .map(|mv| mv.uci.as_str())
                .collect::<Vec<_>>(),
            ["c2c4"]
        );
        let corrected = answer(&conn, &first.id, "c2c4").unwrap();
        assert_eq!(corrected.feedback, "assisted");
        let second = next(&conn, &first.id).unwrap();
        assert_eq!(
            second.prompt.as_ref().unwrap().context.last().unwrap().uci,
            "e7e5"
        );
        let completed = finish(&conn, second);
        assert_eq!(completed.completed, 4);
        assert_eq!(count(&conn, "repertoire_training_srs"), 0);
        let chapter = &document.chapters[0];
        let nc6 = &chapter.root.children[0].children[0].children[0].children[1];
        assert_eq!(nc6.uci.as_deref(), Some("b8c6"));
        let mut branch = source_scope(&document, 0);
        branch.node_id = Some(nc6.id.clone());
        let branch_projection = project_scope(&conn, &branch).unwrap();
        assert_eq!(branch_projection.decisions.len(), 1);
        assert!(
            branch_projection.decisions[0]
                .occurrence
                .path
                .contains(&"b8c6".into())
        );
    }

    #[test]
    fn identical_source_decisions_share_schedule_and_merged_sessions_remain_valid() {
        let conn = db();
        let document = attached(&conn, STUDY, "white");
        let global = begin(&conn, "full_drill");
        let global_id = global.id.clone();
        let global_root = global.prompt.as_ref().unwrap().decision_id.clone();
        answer(&conn, &global_id, "e2e4").unwrap();
        let source = source_begin(&conn, &document, 2, "srs");
        answer(&conn, &source.id, "d2d4").unwrap();
        next(&conn, &source.id).unwrap();
        answer(&conn, &source.id, "g1f3").unwrap();
        let common = next(&conn, &source.id).unwrap();
        let common_id = common.prompt.as_ref().unwrap().decision_id.clone();
        answer(&conn, &source.id, "c2c4").unwrap();
        let merged = projection(&conn);
        assert!(
            merged
                .decisions
                .iter()
                .any(|decision| decision.id == common_id)
        );
        let transposed = project_scope(&conn, &source_scope(&document, 3)).unwrap();
        assert!(
            transposed
                .decisions
                .iter()
                .any(|decision| decision.id == common_id)
        );
        assert!(due_at(&conn, &common_id).unwrap().is_some());
        assert_eq!(overview(&conn, source_scope(&document, 3)).unwrap().due, 2);
        let resumed = begin(&conn, "full_drill");
        assert_eq!(resumed.id, global_id);
        assert_eq!(resumed.completed, 1);
        assert_eq!(resumed.prompt.as_ref().unwrap().decision_id, global_root);
        assert_eq!(resumed.feedback, "correct");
        let renamed = super::super::apply(
            &conn,
            &document.id,
            document.revision,
            EditOperation::RenameChapter {
                chapter_id: document.chapters[2].id.clone(),
                title: "Renamed".into(),
            },
        )
        .unwrap();
        let continued = source_begin(&conn, &renamed, 2, "srs");
        assert_eq!(continued.id, source.id);
        assert_eq!(continued.completed, 3);
        assert_eq!(due_at(&conn, &common_id).unwrap().is_some(), true);
    }

    #[test]
    fn source_scope_rejects_missing_removed_detached_and_wrong_side_sources() {
        let conn = db();
        let document = attached(&conn, STUDY, "white");
        let mut invalid = source_scope(&document, 0);
        invalid.chapter_id = None;
        assert_eq!(overview(&conn, invalid).unwrap_err().code, "training_scope");
        let mut invalid = source_scope(&document, 0);
        invalid.side = "black".into();
        assert_eq!(overview(&conn, invalid).unwrap_err().code, "training_scope");
        let view = source_begin(&conn, &document, 0, "full_drill");
        let removed = super::super::apply(
            &conn,
            &document.id,
            document.revision,
            EditOperation::RemoveChapter {
                chapter_id: document.chapters[0].id.clone(),
            },
        )
        .unwrap();
        assert_eq!(
            answer(&conn, &view.id, "c2c4").unwrap_err().code,
            "training_scope"
        );
        let scope = source_scope(&removed, 0);
        let detached =
            fragments::set_attachment(&conn, &removed.id, removed.revision, false).unwrap();
        assert_eq!(
            overview(&conn, scope.clone()).unwrap_err().code,
            "training_scope"
        );
        fragments::delete_repertoire(&conn, &detached.id, detached.revision).unwrap();
        assert_eq!(overview(&conn, scope).unwrap_err().code, "training_scope");
    }
}
