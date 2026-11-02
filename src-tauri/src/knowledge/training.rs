//! Position-based training projected from the existing combined VariationTree.
//! No repertoire lines or independently editable chess content are stored here.
use super::*;
use fsrs::{FSRS, FSRS6_DEFAULT_DECAY, MemoryState, current_retrievability};
use rusqlite::{Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, VecDeque};

const DAY_MS: i64 = 86_400_000;
const SESSION_LIMIT: usize = 10;
const RETEST_GAP: u64 = 3;
const DESIRED_RETENTION: f32 = 0.9;
const STRATEGY_VERSION: u8 = 1;
fn default_new_limit() -> usize {
    5
}

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
    #[serde(default = "default_new_limit")]
    pub new_limit: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub completed: usize,
    pub total: usize,
    pub mistakes: usize,
    pub finished: bool,
    pub new_count: usize,
    pub review_count: usize,
    pub first_try_correct: usize,
    pub recovered: usize,
    pub pending_retests: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overview {
    pub total: usize,
    pub due: usize,
    pub new_positions: usize,
    pub learning: usize,
    pub scheduled: usize,
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
    pub task: Option<String>,
    pub step: u64,
    pub new_count: usize,
    pub review_count: usize,
    pub first_try_correct: usize,
    pub recovered: usize,
    pub pending_retests: usize,
    pub next_review_at_ms: Option<i64>,
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
    if !["repertoire", "study"].contains(&document.kind.as_str())
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
struct QueuedTask {
    decision_id: String,
    task: String,
    ready_at_step: u64,
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
    #[serde(default)]
    strategy_version: u8,
    #[serde(default = "default_new_limit")]
    new_limit: usize,
    #[serde(default)]
    queue: Vec<QueuedTask>,
    #[serde(default)]
    task: String,
    #[serde(default)]
    step: u64,
    #[serde(default)]
    visit_assisted: bool,
    #[serde(default)]
    scored: BTreeSet<String>,
    #[serde(default)]
    new_items: BTreeSet<String>,
    #[serde(default)]
    introduced: BTreeSet<String>,
    #[serde(default)]
    first_try_correct: BTreeSet<String>,
    #[serde(default)]
    recovered: BTreeSet<String>,
}
impl Session {
    fn summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id.clone(),
            completed: self.completed.len(),
            total: self.items.len(),
            mistakes: self.failed.len(),
            finished: self.current.is_none(),
            new_count: self.new_items.len(),
            review_count: self.items.len().saturating_sub(self.new_items.len()),
            first_try_correct: self.first_try_correct.len(),
            recovered: self.recovered.len(),
            pending_retests: self
                .queue
                .iter()
                .filter(|entry| matches!(entry.task.as_str(), "recall" | "relearn"))
                .count()
                + usize::from(
                    self.current.is_some()
                        && self.mode == "srs"
                        && (matches!(self.task.as_str(), "recall" | "relearn")
                            || self.feedback == "assisted")
                        && !self
                            .completed
                            .contains(self.current.as_deref().unwrap_or_default()),
                ),
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
#[cfg(test)]
fn due_at(conn: &Connection, id: &str) -> Result<Option<i64>, AppError> {
    Ok(conn
        .query_row(
            "SELECT due_at_ms FROM repertoire_training_srs WHERE decision_id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()?)
}
#[derive(Debug, Clone)]
struct Schedule {
    repetitions: i64,
    interval_days: i64,
    ease: f64,
    due_at_ms: i64,
    last_review_at_ms: i64,
    stability: Option<f32>,
    difficulty: Option<f32>,
    scheduler_version: i64,
}
impl Schedule {
    fn memory(&self) -> Option<MemoryState> {
        let stability = self.stability?;
        let difficulty = self.difficulty?;
        (self.scheduler_version == 6
            && stability.is_finite()
            && stability > 0.0
            && difficulty.is_finite()
            && (1.0..=10.0).contains(&difficulty))
        .then_some(MemoryState {
            stability,
            difficulty,
        })
    }
    fn urgency(&self, now: i64) -> f32 {
        if let Some(memory) = self.memory() {
            current_retrievability(
                memory,
                now.saturating_sub(self.last_review_at_ms).max(0) as f32 / DAY_MS as f32,
                FSRS6_DEFAULT_DECAY,
            )
        } else {
            // Legacy cards retain their due dates until the next real review.
            // Relative overdue time orders them without inventing memory state.
            1.0 / (1.0
                + now.saturating_sub(self.due_at_ms).max(0) as f32
                    / (self.interval_days.max(1) as f32 * DAY_MS as f32))
        }
    }
}
fn schedules(conn: &Connection, side: &str) -> Result<HashMap<String, Schedule>, AppError> {
    let mut statement = conn.prepare("SELECT decision_id,repetitions,interval_days,ease,due_at_ms,last_review_at_ms,stability,difficulty,scheduler_version FROM repertoire_training_srs WHERE side=?1")?;
    Ok(statement
        .query_map([side], |row| {
            Ok((
                row.get(0)?,
                Schedule {
                    repetitions: row.get(1)?,
                    interval_days: row.get(2)?,
                    ease: row.get(3)?,
                    due_at_ms: row.get(4)?,
                    last_review_at_ms: row.get(5)?,
                    stability: row.get(6)?,
                    difficulty: row.get(7)?,
                    scheduler_version: row.get(8)?,
                },
            ))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?)
}
fn planned_items(
    projection: &Projection,
    state: &HashMap<String, Schedule>,
    now: i64,
    new_limit: usize,
    excluded: &HashSet<String>,
) -> Vec<(String, bool)> {
    let mut due = projection
        .decisions
        .iter()
        .filter(|decision| !excluded.contains(&decision.id))
        .filter_map(|decision| {
            state
                .get(&decision.id)
                .filter(|schedule| schedule.due_at_ms <= now)
                .map(|schedule| (decision, schedule))
        })
        .collect::<Vec<_>>();
    due.sort_by(|(left, a), (right, b)| {
        a.urgency(now)
            .total_cmp(&b.urgency(now))
            .then_with(|| a.due_at_ms.cmp(&b.due_at_ms))
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut selected = due
        .into_iter()
        .take(SESSION_LIMIT)
        .map(|(decision, _)| (decision.id.clone(), false))
        .collect::<Vec<_>>();
    let mut fresh = projection
        .decisions
        .iter()
        .enumerate()
        .filter(|(_, decision)| {
            !excluded.contains(&decision.id) && !state.contains_key(&decision.id)
        })
        .collect::<Vec<_>>();
    fresh.sort_by_key(|(index, decision)| (decision.occurrence.path.len(), *index));
    selected.extend(
        fresh
            .into_iter()
            .take(new_limit.min(SESSION_LIMIT.saturating_sub(selected.len())))
            .map(|(_, decision)| (decision.id.clone(), true)),
    );
    selected
}
fn eligible(
    conn: &Connection,
    projection: &Projection,
    mode: &str,
    now: i64,
) -> Result<Vec<String>, AppError> {
    // Full drill has no scheduling filter. SRS selection uses planned_items.
    if mode == "full_drill" {
        return Ok(projection
            .decisions
            .iter()
            .map(|decision| decision.id.clone())
            .collect());
    }
    let state = schedules(conn, "white")?
        .into_iter()
        .chain(schedules(conn, "black")?)
        .collect::<HashMap<_, _>>();
    let mut result = vec![];
    for decision in &projection.decisions {
        if state
            .get(&decision.id)
            .is_none_or(|schedule| schedule.due_at_ms <= now)
        {
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
        session.step = session.step.saturating_add(1);
        session.task = "review".into();
        session.initial_fen = decision.occurrence.initial_fen.clone();
        session.path = decision.occurrence.path.clone();
        if session.failed.contains(&decision.id) {
            session.feedback = "retry".into();
        }
    }
}
fn choose_adaptive(session: &mut Session, projection: &Projection) {
    session
        .queue
        .retain(|entry| !session.completed.contains(&entry.decision_id));
    let next_step = session.step.saturating_add(1);
    // If a small deck cannot supply three intervening tasks, continue without
    // an artificial wait. Same-session success is not long-term mastery.
    let index = session
        .queue
        .iter()
        .position(|entry| entry.ready_at_step <= next_step)
        .or_else(|| {
            session
                .queue
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.ready_at_step)
                .map(|(index, _)| index)
        });
    let Some(index) = index else {
        session.current = None;
        session.task.clear();
        session.feedback = "finished".into();
        session.attempted_uci = None;
        return;
    };
    let entry = session.queue.remove(index);
    let Some(decision) = projection
        .decisions
        .iter()
        .find(|decision| decision.id == entry.decision_id)
    else {
        return;
    };
    session.current = Some(entry.decision_id);
    session.task = entry.task;
    session.step = next_step;
    session.visit_assisted = session.task == "learn";
    session.feedback = "awaiting".into();
    session.attempted_uci = None;
    session.initial_fen = decision.occurrence.initial_fen.clone();
    session.path = decision.occurrence.path.clone();
}
fn refresh_adaptive(
    conn: &Connection,
    session: &mut Session,
    projection: &Projection,
) -> Result<(), AppError> {
    let state = schedules(conn, &session.scope.side)?;
    if session.strategy_version == 0 {
        // Existing active runs keep their exact membership and completed work,
        // even if they exceed the new ten-position limit.
        session.strategy_version = STRATEGY_VERSION;
        session.scored.extend(session.completed.iter().cloned());
        session.scored.extend(session.failed.iter().cloned());
        for id in &session.items {
            if !state.contains_key(id) {
                session.new_items.insert(id.clone());
            }
            if Some(id) != session.current.as_ref() && !session.completed.contains(id) {
                session.queue.push(QueuedTask {
                    decision_id: id.clone(),
                    task: if session.failed.contains(id) {
                        "relearn"
                    } else if state.contains_key(id) {
                        "review"
                    } else {
                        "learn"
                    }
                    .into(),
                    ready_at_step: 0,
                });
            }
        }
        session.task = if session
            .failed
            .contains(session.current.as_deref().unwrap_or_default())
        {
            "relearn"
        } else if session
            .new_items
            .contains(session.current.as_deref().unwrap_or_default())
        {
            "learn"
        } else {
            "review"
        }
        .into();
        session.visit_assisted = session.feedback == "retry" || session.task == "learn";
        session.step = session.step.max(1);
    }
    let content_changed = session.fingerprint != projection.fingerprint;
    let valid = projection
        .decisions
        .iter()
        .map(|decision| decision.id.as_str())
        .collect::<HashSet<_>>();
    session.items.retain(|id| {
        valid.contains(id.as_str())
            && (session.completed.contains(id)
                || session.scored.contains(id)
                || state
                    .get(id)
                    .is_none_or(|schedule| schedule.due_at_ms <= now_ms()))
    });
    let included = session.items.iter().cloned().collect::<HashSet<_>>();
    for values in [
        &mut session.completed,
        &mut session.failed,
        &mut session.scored,
        &mut session.new_items,
        &mut session.introduced,
        &mut session.first_try_correct,
        &mut session.recovered,
    ] {
        values.retain(|id| included.contains(id));
    }
    session
        .queue
        .retain(|entry| included.contains(&entry.decision_id));
    if content_changed && session.items.len() < SESSION_LIMIT {
        for (id, fresh) in planned_items(
            projection,
            &state,
            now_ms(),
            session.new_limit.saturating_sub(session.new_items.len()),
            &included,
        ) {
            if session.items.len() == SESSION_LIMIT {
                break;
            }
            if !session.items.contains(&id) {
                session.items.push(id.clone());
                if fresh {
                    session.new_items.insert(id.clone());
                }
                session.queue.push(QueuedTask {
                    decision_id: id,
                    task: if fresh { "learn" } else { "review" }.into(),
                    ready_at_step: 0,
                });
            }
        }
    }
    if let Some(current) = session
        .current
        .as_ref()
        .filter(|id| session.items.contains(id))
        .and_then(|id| {
            projection
                .decisions
                .iter()
                .find(|decision| &decision.id == id)
        })
    {
        if content_changed {
            session.initial_fen = current.occurrence.initial_fen.clone();
            session.path = current.occurrence.path.clone();
        }
    } else {
        choose_adaptive(session, projection);
    }
    session.fingerprint = projection.fingerprint.clone();
    Ok(())
}
fn refresh(
    conn: &Connection,
    session: &mut Session,
    projection: &Projection,
) -> Result<(), AppError> {
    if session.mode == "srs" {
        return refresh_adaptive(conn, session, projection);
    }
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
fn view(
    conn: &Connection,
    session: &Session,
    projection: &Projection,
) -> Result<SessionView, AppError> {
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
        let revealed = session.feedback != "awaiting" || session.task == "learn";
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
    let summary = session.summary();
    let next_review_at_ms = if session.mode == "srs" {
        let state = schedules(conn, &session.scope.side)?;
        session
            .current
            .as_ref()
            .and_then(|id| state.get(id).map(|schedule| schedule.due_at_ms))
            .or_else(|| {
                session
                    .items
                    .iter()
                    .filter_map(|id| state.get(id))
                    .map(|schedule| schedule.due_at_ms)
                    .filter(|at| *at > now_ms())
                    .min()
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
        task: session.current.as_ref().map(|_| {
            if session.task.is_empty() {
                "review".into()
            } else {
                session.task.clone()
            }
        }),
        step: session.step,
        new_count: summary.new_count,
        review_count: summary.review_count,
        first_try_correct: summary.first_try_correct,
        recovered: summary.recovered,
        pending_retests: summary.pending_retests,
        next_review_at_ms,
    })
}

pub fn overview(conn: &Connection, scope: Scope) -> Result<Overview, AppError> {
    let tx = conn.unchecked_transaction()?;
    let projection = project_scope(&tx, &scope)?;
    let state = schedules(&tx, &scope.side)?;
    let now = now_ms();
    let mut due = 0;
    let mut new_positions = 0;
    let mut learning = 0;
    let mut scheduled = 0;
    let mut next_due_at_ms: Option<i64> = None;
    for decision in &projection.decisions {
        match state.get(&decision.id) {
            Some(schedule) => {
                scheduled += 1;
                learning += usize::from(schedule.repetitions == 0);
                if schedule.due_at_ms <= now {
                    due += 1;
                } else {
                    next_due_at_ms = Some(next_due_at_ms.map_or(schedule.due_at_ms, |existing| {
                        existing.min(schedule.due_at_ms)
                    }));
                }
            }
            None => new_positions += 1,
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
        new_positions,
        learning,
        scheduled,
        next_due_at_ms,
        active_srs: active("srs")?,
        active_full_drill: active("full_drill")?,
    };
    tx.commit()?;
    Ok(result)
}
pub fn start(conn: &Connection, request: StartRequest) -> Result<SessionView, AppError> {
    if request.new_limit > SESSION_LIMIT {
        return Err(error("Число новых позиций должно быть от 0 до 10"));
    }
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
            strategy_version: STRATEGY_VERSION,
            new_limit: request.new_limit,
            queue: vec![],
            task: String::new(),
            step: 0,
            visit_assisted: false,
            scored: BTreeSet::new(),
            new_items: BTreeSet::new(),
            introduced: BTreeSet::new(),
            first_try_correct: BTreeSet::new(),
            recovered: BTreeSet::new(),
        };
        if session.mode == "srs" {
            let state = schedules(&tx, &session.scope.side)?;
            for (id, fresh) in planned_items(
                &projection,
                &state,
                now_ms(),
                session.new_limit,
                &HashSet::new(),
            ) {
                session.items.push(id.clone());
                if fresh {
                    session.new_items.insert(id.clone());
                }
                session.queue.push(QueuedTask {
                    decision_id: id,
                    task: if fresh { "learn" } else { "review" }.into(),
                    ready_at_step: 0,
                });
            }
            choose_adaptive(&mut session, &projection);
        } else {
            session.items = eligible(&tx, &projection, &session.mode, now_ms())?;
            let first = projection
                .decisions
                .iter()
                .find(|item| session.items.contains(&item.id));
            set_current(&mut session, first);
        }
        session
    };
    save(&tx, &session)?;
    let result = view(&tx, &session, &projection)?;
    tx.commit()?;
    Ok(result)
}

/// FSRS-6 at 90% requested retention. Historical grades remain 4/1 in the
/// event log; these map to FSRS Good/Again. Guided work is never a new review.
fn schedule(
    conn: &Connection,
    session: &Session,
    decision: &Decision,
    grade: u8,
    timestamp: i64,
) -> Result<(), AppError> {
    let state = schedules(conn, &session.scope.side)?;
    let previous = state.get(&decision.id);
    let fsrs = FSRS::default();
    let memory = previous
        .map(|previous| {
            previous.memory().map(Ok).unwrap_or_else(|| {
                fsrs.memory_state_from_sm2(
                    previous.ease as f32,
                    previous.interval_days.max(1) as f32,
                    DESIRED_RETENTION,
                )
            })
        })
        .transpose()
        .map_err(error)?;
    let elapsed = previous.map_or(0, |previous| {
        (timestamp.saturating_sub(previous.last_review_at_ms).max(0) / DAY_MS).min(u32::MAX as i64)
            as u32
    });
    let next = fsrs
        .next_states(memory, DESIRED_RETENTION, elapsed)
        .map_err(error)?;
    let next = if grade < 3 { next.again } else { next.good };
    let interval = next.interval.clamp(1.0 / 1440.0, 36_500.0);
    let due = timestamp.saturating_add((interval as f64 * DAY_MS as f64).round() as i64);
    let repetitions = if grade < 3 {
        0
    } else {
        previous
            .map_or(0, |previous| previous.repetitions)
            .saturating_add(1)
    };
    let ease = previous.map_or(2.5, |previous| previous.ease);
    conn.execute("INSERT INTO repertoire_training_srs(decision_id,side,position_key,solution_key,repetitions,interval_days,ease,due_at_ms,last_review_at_ms,stability,difficulty,scheduler_version) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,6) ON CONFLICT(decision_id) DO UPDATE SET repetitions=excluded.repetitions,interval_days=excluded.interval_days,due_at_ms=excluded.due_at_ms,last_review_at_ms=excluded.last_review_at_ms,stability=excluded.stability,difficulty=excluded.difficulty,scheduler_version=6",params![decision.id,session.scope.side,decision.key,decision.solution_key,repetitions,interval.round().max(1.0) as i64,ease,due,timestamp,next.memory.stability,next.memory.difficulty])?;
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
        let result = view(&tx, &session, &projection)?;
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
    if session.completed.contains(current)
        || matches!(session.feedback.as_str(), "correct" | "assisted")
    {
        return view(&tx, &session, &projection);
    }
    let (pos, _) = replay(&session.initial_fen, &session.path)?;
    uci.parse::<UciMove>()
        .map_err(error)?
        .to_move(&pos)
        .map_err(|_| error("Недопустимый ход"))?;
    let correct = decision.answers.iter().any(|edge| edge.uci == uci);
    if session.mode == "srs" {
        if correct {
            if session.task == "learn" {
                record(&tx, &session, decision, "learn", Some(uci), None)?;
                session.introduced.insert(current.into());
                session.feedback = "assisted".into();
            } else if session.visit_assisted {
                record(&tx, &session, decision, "retry", Some(uci), None)?;
                session.feedback = "assisted".into();
            } else {
                let first = session.scored.insert(current.into());
                record(
                    &tx,
                    &session,
                    decision,
                    if first { "correct" } else { "retest" },
                    Some(uci),
                    first.then_some(4),
                )?;
                session.completed.insert(current.into());
                if session.failed.contains(current) {
                    session.recovered.insert(current.into());
                } else {
                    session.first_try_correct.insert(current.into());
                }
                session.feedback = "correct".into();
            }
        } else {
            let grade = if session.task == "learn" {
                None
            } else {
                session.failed.insert(current.into());
                session.scored.insert(current.into()).then_some(1)
            };
            record(
                &tx,
                &session,
                decision,
                if session.task == "learn" {
                    "learn_error"
                } else {
                    "error"
                },
                Some(uci),
                grade,
            )?;
            session.visit_assisted = true;
            session.feedback = "retry".into();
        }
        session.attempted_uci = Some(uci.into());
        save(&tx, &session)?;
        let result = view(&tx, &session, &projection)?;
        tx.commit()?;
        return Ok(result);
    }
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
        if assisted {
            session.recovered.insert(current.into());
        } else {
            session.first_try_correct.insert(current.into());
        }
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
    let result = view(&tx, &session, &projection)?;
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
        let result = view(&tx, &session, &projection)?;
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
    if session.mode == "srs" {
        if !session.visit_assisted && !matches!(session.feedback.as_str(), "correct" | "assisted") {
            let first = session.scored.insert(current.into());
            record(&tx, &session, decision, "hint", None, first.then_some(1))?;
            session.failed.insert(current.into());
            session.visit_assisted = true;
            session.feedback = "retry".into();
            session.attempted_uci = None;
            save(&tx, &session)?;
        }
        let result = view(&tx, &session, &projection)?;
        tx.commit()?;
        return Ok(result);
    }
    if !session.completed.contains(current) && !session.failed.contains(current) {
        record(&tx, &session, decision, "hint", None, Some(1))?;
        session.failed.insert(current.into());
        session.feedback = "retry".into();
        session.attempted_uci = None;
        save(&tx, &session)?;
    }
    let result = view(&tx, &session, &projection)?;
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
        let result = view(&tx, &session, &projection)?;
        tx.commit()?;
        return Ok(result);
    }
    if session.current.is_none() {
        return view(&tx, &session, &projection);
    }
    if session.mode == "srs" {
        if !matches!(session.feedback.as_str(), "correct" | "assisted") {
            return Err(error("Сначала воспроизведите подготовленный ход"));
        }
        let current = session
            .current
            .as_deref()
            .ok_or_else(|| error("Тренировка завершена"))?;
        if !session.completed.contains(current) {
            session.queue.push(QueuedTask {
                decision_id: current.into(),
                task: if session.task == "learn" {
                    "recall"
                } else {
                    "relearn"
                }
                .into(),
                ready_at_step: session.step.saturating_add(RETEST_GAP + 1),
            });
        }
        choose_adaptive(&mut session, &projection);
        save(&tx, &session)?;
        let result = view(&tx, &session, &projection)?;
        tx.commit()?;
        return Ok(result);
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
    let result = view(&tx, &session, &projection)?;
    tx.commit()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;
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
        conn.execute_batch(include_str!(
            "../../migrations/009_adaptive_repertoire_training.sql"
        ))
        .unwrap();
        conn
    }
    fn attached(conn: &Connection, pgn: &str, side: &str) -> KnowledgeDocument {
        let document = super::super::import(
            conn,
            ImportRequest {
                kind: "repertoire".into(),
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
                new_limit: 10,
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

    const LONG_LINE: &str = "1.e4 e5 2.Nf3 Nc6 3.Bb5 a6 4.Ba4 Nf6 5.O-O Be7 6.Re1 b5 7.Bb3 d6 8.c3 O-O 9.h3 Nb8 10.d4 Nbd7 11.c4 c6 12.Nc3 Bb7 *";

    #[test]
    fn new_positions_are_shown_before_interleaved_independent_recall() {
        let conn = db();
        attached(&conn, LONG_LINE, "white");
        let request: StartRequest =
            serde_json::from_str(r#"{"scope":{"side":"white"},"mode":"srs"}"#).unwrap();
        assert_eq!(request.new_limit, 5);
        let mut current = start(&conn, request).unwrap();
        assert_eq!(
            (current.total, current.new_count, current.review_count),
            (5, 5, 0)
        );
        let root_id = current.prompt.as_ref().unwrap().decision_id.clone();
        let root_step = current.step;
        for index in 0..5 {
            assert_eq!(current.task.as_deref(), Some("learn"));
            assert!(!current.prompt.as_ref().unwrap().accepted_moves.is_empty());
            let answer_move = choose(&conn, &current);
            current = answer(&conn, &current.id, &answer_move).unwrap();
            assert_eq!(
                (
                    current.feedback.as_str(),
                    current.completed,
                    current.mistakes
                ),
                ("assisted", 0, 0)
            );
            assert_eq!(current.pending_retests, index + 1);
            let before = count(&conn, "repertoire_training_reviews");
            answer(&conn, &current.id, &answer_move).unwrap();
            assert_eq!(count(&conn, "repertoire_training_reviews"), before);
            assert_eq!(count(&conn, "repertoire_training_srs"), 0);
            current = next(&conn, &current.id).unwrap();
        }
        assert_eq!(current.task.as_deref(), Some("recall"));
        assert_eq!(current.prompt.as_ref().unwrap().decision_id, root_id);
        assert!(current.step >= root_step + RETEST_GAP + 1);
        assert!(current.prompt.as_ref().unwrap().accepted_moves.is_empty());
        let recalled = answer(&conn, &current.id, &choose(&conn, &current)).unwrap();
        assert_eq!(
            (
                recalled.completed,
                recalled.first_try_correct,
                recalled.recovered
            ),
            (1, 1, 0)
        );
        assert!(recalled.next_review_at_ms.is_some_and(|at| at > now_ms()));
        assert_eq!(count(&conn, "repertoire_training_srs"), 1);
        let result = finish(&conn, recalled);
        assert_eq!(
            (result.completed, result.total, result.first_try_correct),
            (5, 5, 5)
        );
        let overview = overview(&conn, scope("white")).unwrap();
        assert_eq!(
            (overview.due, overview.scheduled, overview.new_positions),
            (0, 5, 7)
        );
    }

    #[test]
    fn failed_recall_is_retested_without_overwriting_the_first_again_schedule() {
        let conn = db();
        attached(&conn, "1.e4 e5 2.Nf3 Nc6 3.Bb5 a6 4.Ba4 *", "white");
        let mut current = begin(&conn, "srs");
        while current.task.as_deref() == Some("learn") {
            answer(&conn, &current.id, &choose(&conn, &current)).unwrap();
            current = next(&conn, &current.id).unwrap();
        }
        let failed_id = current.prompt.as_ref().unwrap().decision_id.clone();
        let failed_step = current.step;
        let failed = answer(&conn, &current.id, "d2d4").unwrap();
        assert_eq!(
            (failed.feedback.as_str(), failed.completed, failed.mistakes),
            ("retry", 0, 1)
        );
        let before: (i64,i64,f64,f64) = conn.query_row("SELECT due_at_ms,last_review_at_ms,stability,difficulty FROM repertoire_training_srs WHERE decision_id=?1", [&failed_id], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
        hint(&conn, &current.id).unwrap();
        let corrected = answer(&conn, &current.id, &choose(&conn, &failed)).unwrap();
        assert_eq!(
            (corrected.feedback.as_str(), corrected.completed),
            ("assisted", 0)
        );
        current = next(&conn, &current.id).unwrap();
        for _ in 0..3 {
            assert_ne!(current.prompt.as_ref().unwrap().decision_id, failed_id);
            let corrected = answer(&conn, &current.id, &choose(&conn, &current)).unwrap();
            current = next(&conn, &corrected.id).unwrap();
        }
        assert_eq!(current.task.as_deref(), Some("relearn"));
        assert_eq!(current.feedback, "awaiting");
        assert_eq!(current.prompt.as_ref().unwrap().decision_id, failed_id);
        assert!(current.prompt.as_ref().unwrap().accepted_moves.is_empty());
        assert_eq!(current.step, failed_step + 4);
        let recovered = answer(&conn, &current.id, &choose(&conn, &current)).unwrap();
        assert_eq!(
            (
                recovered.completed,
                recovered.recovered,
                recovered.first_try_correct
            ),
            (4, 1, 3)
        );
        let after: (i64,i64,f64,f64) = conn.query_row("SELECT due_at_ms,last_review_at_ms,stability,difficulty FROM repertoire_training_srs WHERE decision_id=?1", [&failed_id], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
        assert_eq!(before, after);
        let grades: String = conn.query_row("SELECT group_concat(grade) FROM repertoire_training_reviews WHERE decision_id=?1 AND grade IS NOT NULL", [&failed_id], |row|row.get(0)).unwrap();
        assert_eq!(grades, "1");
        assert!(next(&conn, &current.id).unwrap().finished);
    }

    #[test]
    fn due_cards_precede_capped_new_material_and_overview_keeps_counts_distinct() {
        let conn = db();
        attached(&conn, LONG_LINE, "white");
        let projection = projection(&conn);
        for (index, decision) in projection.decisions.iter().take(8).enumerate() {
            conn.execute("INSERT INTO repertoire_training_srs(decision_id,side,position_key,solution_key,repetitions,interval_days,ease,due_at_ms,last_review_at_ms) VALUES(?1,'white',?2,?3,3,6,2.5,?4,?5)", params![decision.id,decision.key,decision.solution_key,now_ms()-(index as i64+1)*DAY_MS,now_ms()-15*DAY_MS]).unwrap();
        }
        let overview = overview(&conn, scope("white")).unwrap();
        assert_eq!(
            (
                overview.total,
                overview.due,
                overview.new_positions,
                overview.scheduled
            ),
            (12, 8, 4, 8)
        );
        let session = start(
            &conn,
            StartRequest {
                scope: scope("white"),
                mode: "srs".into(),
                restart: true,
                new_limit: 5,
            },
        )
        .unwrap();
        assert_eq!(
            (session.total, session.review_count, session.new_count),
            (10, 8, 2)
        );
        assert_eq!(session.task.as_deref(), Some("review"));
        assert_eq!(
            session.prompt.as_ref().unwrap().decision_id,
            projection.decisions[7].id
        );
        let reviews_only = start(
            &conn,
            StartRequest {
                scope: scope("white"),
                mode: "srs".into(),
                restart: true,
                new_limit: 0,
            },
        )
        .unwrap();
        assert_eq!((reviews_only.total, reviews_only.new_count), (8, 0));
        assert!(
            start(
                &conn,
                StartRequest {
                    scope: scope("white"),
                    mode: "srs".into(),
                    restart: true,
                    new_limit: 11
                }
            )
            .is_err()
        );
    }

    #[test]
    fn fsrs_migrates_sm2_only_on_the_next_scored_review() {
        let conn = db();
        attached(&conn, "1.e4 *", "white");
        let projection = projection(&conn);
        let decision = &projection.decisions[0];
        let previous_review = now_ms() - 15 * DAY_MS;
        let old_due = now_ms() - DAY_MS;
        conn.execute("INSERT INTO repertoire_training_srs(decision_id,side,position_key,solution_key,repetitions,interval_days,ease,due_at_ms,last_review_at_ms) VALUES(?1,'white',?2,?3,3,15,1.8,?4,?5)",params![decision.id,decision.key,decision.solution_key,old_due,previous_review]).unwrap();
        overview(&conn, scope("white")).unwrap();
        let current = begin(&conn, "srs");
        assert_eq!(current.task.as_deref(), Some("review"));
        let before = schedules(&conn, "white").unwrap();
        assert_eq!(
            (
                before[&decision.id].due_at_ms,
                before[&decision.id].memory()
            ),
            (old_due, None)
        );
        let fsrs = FSRS::default();
        let migrated = fsrs
            .memory_state_from_sm2(1.8, 15.0, DESIRED_RETENTION)
            .unwrap();
        let expected = fsrs
            .next_states(Some(migrated), DESIRED_RETENTION, 15)
            .unwrap()
            .good;
        answer(&conn, &current.id, "e2e4").unwrap();
        let state = schedules(&conn, "white").unwrap();
        assert_eq!(state[&decision.id].memory(), Some(expected.memory));
        assert!(state[&decision.id].due_at_ms > now_ms());
        assert_eq!(state[&decision.id].ease, 1.8);
    }

    #[test]
    fn pgn_expansion_fills_available_new_slots_after_excluding_existing_items() {
        let conn = db();
        let document = attached(&conn, "1.e4 e5 2.Nf3 *", "white");
        let original = start(
            &conn,
            StartRequest {
                scope: scope("white"),
                mode: "srs".into(),
                restart: false,
                new_limit: 5,
            },
        )
        .unwrap();
        assert_eq!(original.total, 2);
        let leaf = &document.chapters[0].root.children[0].children[0].children[0];
        let with_reply = super::super::apply(
            &conn,
            &document.id,
            document.revision,
            EditOperation::AddMove {
                chapter_id: document.chapters[0].id.clone(),
                parent_id: leaf.id.clone(),
                uci: "b8c6".into(),
            },
        )
        .unwrap();
        let reply = &with_reply.chapters[0].root.children[0].children[0].children[0].children[0];
        super::super::apply(
            &conn,
            &document.id,
            with_reply.revision,
            EditOperation::AddMove {
                chapter_id: document.chapters[0].id.clone(),
                parent_id: reply.id.clone(),
                uci: "f1b5".into(),
            },
        )
        .unwrap();
        let resumed = begin(&conn, "srs");
        assert_eq!(resumed.id, original.id);
        assert_eq!((resumed.total, resumed.new_count), (3, 3));
        assert_eq!(
            resumed.prompt.as_ref().unwrap().decision_id,
            original.prompt.as_ref().unwrap().decision_id
        );
    }

    #[test]
    fn adaptive_retest_remains_hidden_and_resumable_after_database_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("adaptive.sqlite3");
        let (id, step, due);
        {
            let conn = storage::open(&path).unwrap();
            attached(&conn, "1.e4 *", "white");
            let initial = begin(&conn, "srs");
            id = initial.id;
            answer(&conn, &id, "e2e4").unwrap();
            let recall = next(&conn, &id).unwrap();
            assert_eq!(recall.task.as_deref(), Some("recall"));
            hint(&conn, &id).unwrap();
            answer(&conn, &id, "e2e4").unwrap();
            let retest = next(&conn, &id).unwrap();
            step = retest.step;
            due = retest.next_review_at_ms;
            assert_eq!(retest.task.as_deref(), Some("relearn"));
        }
        let conn = storage::open(&path).unwrap();
        let resumed = begin(&conn, "srs");
        assert_eq!(
            (resumed.id.as_str(), resumed.step, resumed.next_review_at_ms),
            (id.as_str(), step, due)
        );
        assert_eq!(resumed.feedback, "awaiting");
        assert!(resumed.prompt.as_ref().unwrap().accepted_moves.is_empty());
        let recovered = answer(&conn, &id, "e2e4").unwrap();
        assert_eq!(
            (recovered.completed, recovered.mistakes, recovered.recovered),
            (1, 1, 1)
        );
        assert_eq!(recovered.next_review_at_ms, due);
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM repertoire_training_reviews WHERE grade IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn legacy_active_session_keeps_membership_completed_work_and_failed_schedule() {
        let conn = db();
        attached(&conn, LONG_LINE, "white");
        let initial = begin(&conn, "srs");
        let projected = projection(&conn);
        let mut legacy = load_session(&conn, &initial.id).unwrap();
        legacy.items = projected
            .decisions
            .iter()
            .map(|decision| decision.id.clone())
            .collect();
        legacy.completed.insert(projected.decisions[0].id.clone());
        legacy.failed.insert(projected.decisions[1].id.clone());
        legacy.feedback = "correct".into();
        legacy.attempted_uci = Some("e2e4".into());
        schedule(&conn, &legacy, &projected.decisions[0], 4, now_ms()).unwrap();
        schedule(&conn, &legacy, &projected.decisions[1], 1, now_ms()).unwrap();
        let due = due_at(&conn, &projected.decisions[1].id).unwrap();
        let mut json = serde_json::to_value(&legacy).unwrap();
        for key in [
            "strategy_version",
            "new_limit",
            "queue",
            "task",
            "step",
            "visit_assisted",
            "scored",
            "new_items",
            "introduced",
            "first_try_correct",
            "recovered",
        ] {
            json.as_object_mut().unwrap().remove(key);
        }
        conn.execute(
            "UPDATE repertoire_training_sessions SET state_json=?1 WHERE id=?2",
            params![json.to_string(), initial.id],
        )
        .unwrap();
        let resumed = begin(&conn, "srs");
        assert_eq!(
            (
                resumed.id.as_str(),
                resumed.total,
                resumed.completed,
                resumed.feedback.as_str()
            ),
            (initial.id.as_str(), 12, 1, "correct")
        );
        let pending = next(&conn, &initial.id).unwrap();
        assert_eq!(
            pending.prompt.as_ref().unwrap().decision_id,
            projected.decisions[1].id
        );
        assert_eq!(pending.task.as_deref(), Some("relearn"));
        assert_eq!(pending.feedback, "awaiting");
        assert!(pending.prompt.as_ref().unwrap().accepted_moves.is_empty());
        let recalled = answer(&conn, &initial.id, &choose(&conn, &pending)).unwrap();
        assert_eq!((recalled.completed, recalled.recovered), (2, 1));
        assert_eq!(due_at(&conn, &projected.decisions[1].id).unwrap(), due);
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM repertoire_training_reviews WHERE grade IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
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
        let mut original = begin(&conn, "srs");
        while original.task.as_deref() == Some("learn") {
            answer(&conn, &original.id, &choose(&conn, &original)).unwrap();
            original = next(&conn, &original.id).unwrap();
        }
        let introductions = count(&conn, "repertoire_training_reviews");
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
        assert_eq!(rows.2, 2.5);
        hint(&conn, &id).unwrap();
        hint(&conn, &id).unwrap();
        assert_eq!(
            count(&conn, "repertoire_training_reviews"),
            introductions + 1
        );
        let resumed = begin(&conn, "srs");
        assert_eq!(resumed.id, id);
        assert_eq!(resumed.feedback, "retry");
        let correction = choose(&conn, &resumed);
        let corrected = answer(&conn, &id, &correction).unwrap();
        assert_eq!(corrected.feedback, "assisted");
        assert_eq!(corrected.completed, 0);
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
    fn fsrs_uses_official_memory_states_and_again_interval_is_shorter_than_good() {
        let conn = db();
        attached(&conn, "1.e4 *", "white");
        let view = begin(&conn, "srs");
        let session = load_session(&conn, &view.id).unwrap();
        let projected = projection(&conn);
        let decision = &projected.decisions[0];
        let fsrs = FSRS::default();
        let expected = fsrs.next_states(None, DESIRED_RETENTION, 0).unwrap().good;
        schedule(&conn, &session, decision, 4, 1000).unwrap();
        let state = schedules(&conn, "white").unwrap();
        let first = &state[&decision.id];
        assert_eq!(first.memory(), Some(expected.memory));
        assert!(
            (first.due_at_ms - 1000 - (expected.interval as f64 * DAY_MS as f64).round() as i64)
                .abs()
                <= 1
        );
        let elapsed = 2;
        let expected = fsrs
            .next_states(first.memory(), DESIRED_RETENTION, elapsed)
            .unwrap();
        let timestamp = 1000 + elapsed as i64 * DAY_MS;
        schedule(&conn, &session, decision, 1, timestamp).unwrap();
        let after = schedules(&conn, "white").unwrap();
        let after = &after[&decision.id];
        assert_eq!(after.memory(), Some(expected.again.memory));
        assert!(expected.again.interval < expected.good.interval);
        assert_eq!(after.repetitions, 0);
        assert!(after.due_at_ms > timestamp);
    }

    #[test]
    fn changed_pgn_keeps_unchanged_solution_history_and_refreshes_coverage() {
        let conn = db();
        let document = attached(&conn, REPERTOIRE, "white");
        let session = begin(&conn, "full_drill");
        let old_root = session.prompt.as_ref().unwrap().decision_id.clone();
        answer(&conn, &session.id, "d2d4").unwrap();
        let old_projection = projection(&conn);
        schedule(
            &conn,
            &load_session(&conn, &session.id).unwrap(),
            &old_projection.decisions[0],
            4,
            now_ms(),
        )
        .unwrap();
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
        let resumed = begin(&conn, "full_drill");
        assert_eq!(resumed.id, session.id);
        assert_eq!(resumed.completed, 0);
        assert_eq!(resumed.prompt.as_ref().unwrap().decision_id, new_root.id);
        // A comment edit cannot invalidate an unchanged decision or its schedule.
        answer(&conn, &resumed.id, "d2d4").unwrap();
        schedule(
            &conn,
            &load_session(&conn, &resumed.id).unwrap(),
            new_root,
            4,
            now_ms(),
        )
        .unwrap();
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
        assert_eq!(begin(&conn, "full_drill").completed, 1);
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
                new_limit: 10,
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
                new_limit: 10,
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
        let learned = begin(&conn, "srs");
        let learned = finish(&conn, learned);
        let baseline = learned.total as i64;
        conn.execute("UPDATE repertoire_training_srs SET due_at_ms=0,last_review_at_ms=0,stability=1,difficulty=5,scheduler_version=6", [])
            .unwrap();
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
                new_limit: 10,
            },
        )
        .unwrap();
        answer(&conn, &scoped.id, &choose(&conn, &scoped)).unwrap();
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
        assert_eq!(grade_count, baseline + 1);
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
                new_limit: 10,
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
        assert_eq!(count_after, baseline + 2);
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
                new_limit: 10,
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
        let mut common = source.clone();
        while common.completed < common.total {
            common = answer(&conn, &common.id, &choose(&conn, &common)).unwrap();
            if common.completed < common.total {
                common = next(&conn, &common.id).unwrap();
            }
        }
        let common_id = common.prompt.as_ref().unwrap().decision_id.clone();
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
        let transposed_overview = overview(&conn, source_scope(&document, 3)).unwrap();
        assert_eq!(transposed_overview.due, 0);
        assert_eq!(transposed_overview.new_positions, 2);
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
