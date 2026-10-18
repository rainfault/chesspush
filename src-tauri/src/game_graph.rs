//! Rebuildable revisioned factual graph. Knowledge overlays never enter factual indexes.
pub use crate::chess_domain::OccurrenceRef;
use crate::{
    domain::AppError,
    statistics::{chess, query as statistics_query, types::*},
};
use rusqlite::{Connection, OptionalExtension, params, types::Value};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GraphRequest {
    pub filter: StatisticsFilter,
    pub position_key: Option<String>,
    pub initial_fen: Option<String>,
    pub line: Option<Vec<String>>,
    pub depth: Option<u32>,
    pub top_k: Option<u32>,
    pub preview_limit: Option<u32>,
    pub preview_top_k: Option<u32>,
    pub max_nodes: Option<u32>,
    pub repertoire_ids: Vec<String>,
    /// Exact observed prefix by default; overview explicitly opts into all paths.
    pub all_paths: bool,
    pub review_refs: Vec<GraphReviewReference>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphNode {
    pub id: String,
    pub fen: String,
    pub games: usize,
    pub visits: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
    pub score: Option<f64>,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
    pub hidden_continuations: usize,
    pub engine_cp: Option<i32>,
    pub engine_mate: Option<i32>,
    pub engine_profile: Option<String>,
    pub engine_depth: Option<u32>,
    pub engine_coverage: usize,
    pub engine_reference: Option<OccurrenceRef>,
    pub engine_run_id: Option<String>,
    pub example: Option<OccurrenceRef>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PreparedLink {
    pub document_id: String,
    pub chapter_id: String,
    pub node_id: String,
    pub document_revision: i64,
    pub side: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphEdge {
    pub branch_games: usize,
    pub id: String,
    pub from: String,
    pub to: String,
    pub uci: String,
    pub san: String,
    pub games: usize,
    pub visits: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
    pub score: Option<f64>,
    pub frequency: f64,
    pub prepared: Vec<PreparedLink>,
    pub coverage: String,
    pub example: Option<OccurrenceRef>,
    pub first_ms: Option<i64>,
    pub last_ms: Option<i64>,
    pub spent_mean: Option<f64>,
    pub spent_sample: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GraphReviewReference {
    pub game_id: String,
    pub ply: u32,
    pub run_id: String,
    pub position_key: Option<String>,
    pub played_uci: Option<String>,
    pub revision: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphProjection {
    pub root_key: String,
    pub selected_key: String,
    pub selected_fen: String,
    pub path_nodes: Vec<ReplayNode>,
    pub initial_fen: String,
    pub line: Vec<String>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub continuations: Vec<GraphEdge>,
    pub previews: Vec<GraphPreview>,
    pub alternative_paths: usize,
    pub branch_games: usize,
    pub all_paths: bool,
    pub repertoire_ends_here: bool,
    pub review_refs: Vec<GraphReviewReference>,
    pub review_analysed_games: usize,
    pub selected: GraphNode,
    pub hidden_nodes: usize,
    pub indexed_games: usize,
    pub eligible_games: usize,
    pub total_positions: usize,
    pub total_visits: usize,
    pub index_revision: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphPreview {
    pub uci: String,
    pub replies: Vec<GraphEdge>,
    pub hidden: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GraphEvidenceRequest {
    pub filter: StatisticsFilter,
    pub position_key: String,
    pub uci: Option<String>,
    pub offset: usize,
    pub limit: usize,
    pub initial_fen: Option<String>,
    pub line: Option<Vec<String>>,
    pub all_paths: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEvidence {
    pub reference: OccurrenceRef,
    pub account_id: String,
    pub own_color: String,
    pub white: String,
    pub black: String,
    pub result: String,
    pub outcome: String,
    pub timestamp_ms: Option<i64>,
    pub fen: String,
    pub san: Option<String>,
    pub uci: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEvidencePage {
    pub rows: Vec<GraphEvidence>,
    pub total: usize,
    pub offset: usize,
    pub has_more: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OccurrenceContext {
    pub reference: OccurrenceRef,
    pub initial_fen: String,
    pub fen: String,
    pub position_key: String,
    pub line: Vec<String>,
    pub raw_pgn: String,
    pub current_revision: i64,
    pub white: String,
    pub black: String,
}

/// Called inside the source-save transaction, using the same validated legal replay.
pub fn index_game(
    conn: &Connection,
    game_id: i64,
    game: &StoredGame,
    revision: i64,
    payload_revision: i64,
    updated_at_ms: i64,
) -> Result<(), AppError> {
    let opening = game.replay.opening.as_ref();
    conn.execute("INSERT INTO game_graph_games(game_id,revision,payload_revision,updated_at_ms,external_ref,source,variant,result,status,speed,rated,control,timestamp_ms,white,black,white_rating,black_rating,opening_name,opening_eco,opening_family,plies,initial_fen) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22) ON CONFLICT(game_id) DO UPDATE SET revision=excluded.revision,payload_revision=excluded.payload_revision,updated_at_ms=excluded.updated_at_ms,source=excluded.source,variant=excluded.variant,result=excluded.result,status=excluded.status,speed=excluded.speed,rated=excluded.rated,control=excluded.control,timestamp_ms=excluded.timestamp_ms,white=excluded.white,black=excluded.black,white_rating=excluded.white_rating,black_rating=excluded.black_rating,opening_name=excluded.opening_name,opening_eco=excluded.opening_eco,opening_family=excluded.opening_family,plies=excluded.plies,initial_fen=excluded.initial_fen",
        params![game_id,revision,payload_revision,updated_at_ms,game.id,game.source,game.variant,game.result,game.status,game.speed,game.rated,statistics_query::control(game),statistics_query::game_time(game),game.white.name,game.black.name,game.white.rating,game.black.rating,opening.map(|o|&o.name),opening.map(|o|&o.eco),opening.map(|o|&o.family),game.replay.nodes.len().saturating_sub(1),game.replay.initial_fen])?;
    conn.execute(
        "INSERT OR IGNORE INTO game_graph_revisions(game_id,revision,initial_fen) VALUES(?1,?2,?3)",
        params![game_id, revision, game.replay.initial_fen],
    )?;
    let mut previous: Option<&str> = None;
    let mut positions=conn.prepare_cached("INSERT OR IGNORE INTO chess_positions(position_key,variant,representative_fen) VALUES(?1,?2,?3)")?;
    let mut occurrences=conn.prepare_cached("INSERT INTO game_occurrences(game_id,revision,ply,position_key,fen,from_key,uci,san,phase,clock_seconds,spent_seconds) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11) ON CONFLICT(game_id,revision,ply) DO UPDATE SET clock_seconds=excluded.clock_seconds,spent_seconds=excluded.spent_seconds,phase=excluded.phase")?;
    for n in &game.replay.nodes {
        positions.execute(params![n.position_key, game.variant, n.fen])?;
        occurrences.execute(params![
            game_id,
            revision,
            n.ply,
            n.position_key,
            n.fen,
            previous,
            n.uci,
            n.san,
            n.phase,
            n.clock_seconds,
            n.spent_seconds
        ])?;
        previous = Some(&n.position_key);
    }
    conn.execute(
        "DELETE FROM game_graph_opening_labels WHERE game_id=?1",
        [game_id],
    )?;
    let path = game
        .replay
        .opening
        .as_ref()
        .map(|o| chess::opening_name_path(&o.name))
        .unwrap_or_else(|| vec!["Не классифицировано".into()]);
    for color in ["white", "black"] {
        for n in 1..=path.len() {
            conn.execute("INSERT OR IGNORE INTO game_graph_opening_labels(game_id,color,label) VALUES(?1,?2,?3)",params![game_id,color,chess::opening_label_id(color,&path[..n])])?;
        }
    }
    Ok(())
}

/// Backfills only new or changed source games. Repeated interaction reads SQL indexes.
pub fn ensure_index(conn: &Connection) -> Result<(), AppError> {
    const MISSING: &str = " FROM statistics_games s LEFT JOIN game_graph_games g ON g.game_id=s.game_id WHERE g.game_id IS NULL OR g.revision<>s.movetext_revision OR g.payload_revision<>s.payload_revision OR g.updated_at_ms<>s.updated_at_ms";
    let needed: bool = conn.query_row(&format!("SELECT EXISTS(SELECT 1{MISSING})"), [], |r| {
        r.get(0)
    })?;
    if !needed {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE game_graph_games SET revision=revision WHERE 0", [])?;
    let rows=tx.prepare(&format!("SELECT s.game_id,s.data,s.movetext_revision,s.payload_revision,s.updated_at_ms{MISSING}"))?
        .query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?)))?.collect::<Result<Vec<_>,_>>()?;
    for (id, data, revision, payload_revision, updated) in rows {
        index_game(
            &tx,
            id,
            &serde_json::from_str(&data)?,
            revision,
            payload_revision,
            updated,
        )?;
    }
    tx.commit()?;
    Ok(())
}

struct Selection {
    cte: String,
    values: Vec<Value>,
    ply: Option<usize>,
}
fn selection(filter: &StatisticsFilter) -> Selection {
    let mut conditions = vec![
        "g.plies>0".to_owned(),
        "g.result IN ('1-0','0-1','1/2-1/2')".into(),
        "g.status IN ('mate','resign','stalemate','timeout','outoftime','draw')".into(),
        "g.variant IN ('standard','fromPosition')".into(),
    ];
    let mut values = Vec::new();
    {
        let mut add = |expression: &str, value: Value| {
            values.push(value);
            conditions.push(format!("{expression}?{}", values.len()));
        };
        if let Some(id) = &filter.account_id {
            add("a.account_id=", Value::Text(id.clone()));
        }
        if let Some(source) = &filter.source {
            add("g.source=", Value::Text(source.clone()));
        }
        if let Some(color) = &filter.color {
            add("a.user_color=", Value::Text(color.clone()));
        }
        if let Some(speed) = &filter.speed {
            add("g.speed=", Value::Text(speed.clone()));
        }
        if let Some(variant) = &filter.variant {
            add("g.variant=", Value::Text(variant.clone()));
        }
        if let Some(control) = &filter.control {
            add("g.control=", Value::Text(control.clone()));
        }
        if let Some(rated) = filter.rated {
            add("g.rated=", Value::Integer(i64::from(rated)));
        }
        if let Some(since) = filter.since_ms {
            add("g.timestamp_ms>=", Value::Integer(since));
        }
        if let Some(until) = filter.until_ms {
            add("g.timestamp_ms<", Value::Integer(until));
        }
        let rating = "CASE a.user_color WHEN 'white' THEN g.black_rating ELSE g.white_rating END";
        if let Some(min) = filter.opponent_rating_min {
            add(&format!("({rating})>="), Value::Integer(min.into()));
        }
        if let Some(max) = filter.opponent_rating_max {
            add(&format!("({rating})<="), Value::Integer(max.into()));
        }
    }
    if let Some(id) = &filter.opening_label_id {
        values.push(Value::Text(id.clone()));
        conditions.push(format!("EXISTS(SELECT 1 FROM game_graph_opening_labels l WHERE l.game_id=g.game_id AND l.color=a.user_color AND l.label=?{})",values.len()));
    }
    if let Some(opening) = &filter.opening {
        values.push(Value::Text(opening.to_lowercase()));
        let i = values.len();
        conditions.push(format!("(instr(lower(g.opening_name),?{i})>0 OR lower(g.opening_eco)=?{i} OR lower(g.opening_family)=?{i})"));
    }
    if let Some(ids) = &filter.game_ids {
        let placeholders = ids
            .iter()
            .map(|id| {
                values.push(Value::Text(id.clone()));
                format!("?{}", values.len())
            })
            .collect::<Vec<_>>()
            .join(",");
        conditions.push(if ids.is_empty() {
            "0".into()
        } else {
            format!("g.external_ref IN ({placeholders})")
        });
    }
    conditions.push(if filter.include_uncertain.unwrap_or(false) {
        "a.eligibility IN ('eligible','boundary_uncertain')".into()
    } else {
        "a.eligibility='eligible'".into()
    });
    Selection {
        cte: format!(
            "WITH candidates AS (SELECT g.*,a.account_id,a.user_color,CASE WHEN g.result='1/2-1/2' THEN 'draw' WHEN (g.result='1-0' AND a.user_color='white') OR (g.result='0-1' AND a.user_color='black') THEN 'win' ELSE 'loss' END AS outcome,row_number() OVER (PARTITION BY g.game_id ORDER BY CASE a.eligibility WHEN 'eligible' THEN 0 ELSE 1 END,o.enabled DESC,a.account_id) AS chosen FROM game_graph_games g JOIN statistics_account_games a ON a.game_id=g.game_id JOIN statistics_accounts o ON o.id=a.account_id WHERE {}), selected AS (SELECT * FROM candidates WHERE chosen=1)",
            conditions.join(" AND ")
        ),
        values,
        ply: None,
    }
}
fn score(w: usize, d: usize, n: usize) -> Option<f64> {
    (n > 0).then(|| 100.0 * (w as f64 + 0.5 * d as f64) / n as f64)
}
fn phase_condition(filter: &StatisticsFilter, values: &mut Vec<Value>) -> String {
    if let Some(phase) = &filter.phase {
        values.push(Value::Text(phase.clone()));
        format!(" AND n.phase=?{}", values.len())
    } else {
        String::new()
    }
}
fn node(
    conn: &Connection,
    selection: &Selection,
    key: &str,
    filter: &StatisticsFilter,
) -> Result<GraphNode, AppError> {
    let mut values = selection.values.clone();
    values.push(Value::Text(key.into()));
    let k = values.len();
    let mut phase = phase_condition(filter, &mut values);
    if let Some(ply) = selection.ply {
        phase.push_str(&format!(" AND n.ply={ply}"));
    }
    let sql = format!(
        "{} SELECT count(DISTINCT n.game_id),count(*),count(DISTINCT CASE s.outcome WHEN 'win' THEN n.game_id END),count(DISTINCT CASE s.outcome WHEN 'draw' THEN n.game_id END),count(DISTINCT CASE s.outcome WHEN 'loss' THEN n.game_id END),min(s.timestamp_ms),max(s.timestamp_ms) FROM game_occurrences n JOIN selected s ON s.game_id=n.game_id AND s.revision=n.revision WHERE n.position_key=?{k}{phase}",
        selection.cte
    );
    let mut n = conn.query_row(&sql, rusqlite::params_from_iter(values.iter()), |r| {
        Ok(GraphNode {
            id: key.into(),
            games: r.get(0)?,
            visits: r.get(1)?,
            wins: r.get(2)?,
            draws: r.get(3)?,
            losses: r.get(4)?,
            first_ms: r.get(5)?,
            last_ms: r.get(6)?,
            ..Default::default()
        })
    })?;
    n.score = score(n.wins, n.draws, n.games);
    n.fen = conn
        .query_row(
            "SELECT representative_fen FROM chess_positions WHERE position_key=?1",
            [key],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_default();
    let example_sql = format!(
        "{} SELECT s.external_ref,n.revision,n.ply,n.fen FROM game_occurrences n JOIN selected s ON s.game_id=n.game_id AND s.revision=n.revision WHERE n.position_key=?{k}{phase} ORDER BY s.timestamp_ms DESC,s.external_ref,n.ply LIMIT 1",
        selection.cte
    );
    if let Some((reference, fen)) = conn
        .query_row(
            &example_sql,
            rusqlite::params_from_iter(values.iter()),
            |r| {
                Ok((
                    OccurrenceRef {
                        game_id: r.get(0)?,
                        revision: r.get(1)?,
                        ply: r.get(2)?,
                    },
                    r.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?
    {
        n.example = Some(reference);
        n.fen = fen;
    }
    Ok(n)
}
fn edges(
    conn: &Connection,
    selection: &Selection,
    key: &str,
    filter: &StatisticsFilter,
    limit: usize,
) -> Result<(Vec<GraphEdge>, usize), AppError> {
    let mut values = selection.values.clone();
    values.push(Value::Text(key.into()));
    let k = values.len();
    let mut phase = phase_condition(filter, &mut values);
    if let Some(ply) = selection.ply {
        phase.push_str(&format!(" AND n.ply={}", ply + 1));
    }
    let base = format!(
        "FROM game_occurrences n JOIN selected s ON s.game_id=n.game_id AND s.revision=n.revision WHERE n.from_key=?{k}{phase}"
    );
    let sql = format!(
        "{} SELECT n.uci,n.san,n.position_key,count(DISTINCT n.game_id),count(*),count(DISTINCT CASE s.outcome WHEN 'win' THEN n.game_id END),count(DISTINCT CASE s.outcome WHEN 'draw' THEN n.game_id END),count(DISTINCT CASE s.outcome WHEN 'loss' THEN n.game_id END),min(s.timestamp_ms),max(s.timestamp_ms),avg(n.spent_seconds),count(n.spent_seconds) {base} GROUP BY n.uci,n.position_key ORDER BY count(*) DESC,n.uci LIMIT {limit}",
        selection.cte
    );
    let mut result = conn
        .prepare(&sql)?
        .query_map(rusqlite::params_from_iter(values.iter()), |r| {
            Ok(GraphEdge {
                id: format!("{key}|{}", r.get::<_, String>(0)?),
                from: key.into(),
                to: r.get(2)?,
                uci: r.get(0)?,
                san: r.get(1)?,
                games: r.get(3)?,
                visits: r.get(4)?,
                wins: r.get(5)?,
                draws: r.get(6)?,
                losses: r.get(7)?,
                first_ms: r.get(8)?,
                last_ms: r.get(9)?,
                spent_mean: r.get(10)?,
                spent_sample: r.get(11)?,
                coverage: "observed".into(),
                ..Default::default()
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let (all, total): (usize, usize) = conn.query_row(
        &format!(
            "{} SELECT count(DISTINCT n.uci),count(*) {base}",
            selection.cte
        ),
        rusqlite::params_from_iter(values.iter()),
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    for edge in &mut result {
        edge.score = score(edge.wins, edge.draws, edge.games);
        edge.frequency = if total > 0 {
            100.0 * edge.visits as f64 / total as f64
        } else {
            0.0
        };
        let mut ev = values.clone();
        ev.push(Value::Text(edge.uci.clone()));
        edge.example=conn.query_row(&format!("{} SELECT s.external_ref,n.revision,n.ply {base} AND n.uci=?{} ORDER BY s.timestamp_ms DESC,s.external_ref,n.ply LIMIT 1",selection.cte,ev.len()),rusqlite::params_from_iter(ev.iter()),|r|Ok(OccurrenceRef{game_id:r.get(0)?,revision:r.get(1)?,ply:r.get(2)?})).optional()?;
    }
    Ok((result, all))
}

/// A branch is a prefix of a factual revision, never a path assembled from
/// unrelated position edges. Ply is kept separately so cycles are visits.
fn branch_selection(initial: &str, line: &[String]) -> Result<Selection, AppError> {
    let root = chess::line_position_from(initial, &[])
        .map_err(|e| AppError::new("position", e))?
        .position_key;
    Ok(Selection {
        cte: "WITH selected AS (SELECT s.* FROM graph_selected s WHERE EXISTS(SELECT 1 FROM game_occurrences r WHERE r.game_id=s.game_id AND r.revision=s.revision AND r.ply=0 AND r.position_key=?1) AND NOT EXISTS(SELECT 1 FROM json_each(?2) p WHERE NOT EXISTS(SELECT 1 FROM game_occurrences h WHERE h.game_id=s.game_id AND h.revision=s.revision AND h.ply=cast(p.key AS INTEGER)+1 AND h.uci=p.value)))".into(),
        values: vec![Value::Text(root), Value::Text(serde_json::to_string(line).map_err(|e| AppError::new("position",e.to_string()))?)],
        ply: Some(line.len()),
    })
}
fn alternative_path_count(
    conn: &Connection,
    selection: &Selection,
    key: &str,
    initial: &str,
    line: &[String],
) -> Result<usize, AppError> {
    let root = chess::line_position_from(initial, &[])
        .map_err(|e| AppError::new("position", e))?
        .position_key;
    let current = format!("{root}|{}", line.join(" "));
    let mut values = selection.values.clone();
    values.push(Value::Text(key.into()));
    let sql = format!(
        "{} SELECT DISTINCT (SELECT r.position_key FROM game_occurrences r WHERE r.game_id=n.game_id AND r.revision=n.revision AND r.ply=0)||'|'||coalesce((SELECT group_concat(uci,' ') FROM (SELECT h.uci FROM game_occurrences h WHERE h.game_id=n.game_id AND h.revision=n.revision AND h.ply>0 AND h.ply<=n.ply ORDER BY h.ply)),'') FROM game_occurrences n JOIN selected s ON s.game_id=n.game_id AND s.revision=n.revision WHERE n.position_key=?{}",
        selection.cte,
        values.len()
    );
    let paths = conn
        .prepare(&sql)?
        .query_map(rusqlite::params_from_iter(values.iter()), |r| {
            r.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(paths.iter().filter(|p| **p != current).count())
}

pub fn query(conn: &Connection, request: &GraphRequest) -> Result<GraphProjection, AppError> {
    ensure_index(conn)?;
    let mut initial = request
        .initial_fen
        .clone()
        .unwrap_or_else(|| chess::INITIAL_FEN.into());
    let mut line = request.line.clone().unwrap_or_default();
    let path_position =
        chess::line_position_from(&initial, &line).map_err(|e| AppError::new("position", e))?;
    let selected_key = request
        .position_key
        .clone()
        .unwrap_or_else(|| path_position.position_key.clone());
    if request.line.is_some() && selected_key != path_position.position_key {
        return Err(AppError::new(
            "position",
            "Путь не соответствует выбранной позиции",
        ));
    }
    let selection = materialize_selection(conn, &request.filter)?;
    let depth = request.depth.unwrap_or(2).clamp(1, 5) as usize;
    let top_k = request.top_k.unwrap_or(6).clamp(1, 30) as usize;
    let max_nodes = request.max_nodes.unwrap_or(120).clamp(2, 300) as usize;
    let prepared = crate::knowledge::overlay(conn, &request.repertoire_ids)?;
    let mut queue = VecDeque::from([(selected_key.clone(), 0usize)]);
    let mut scheduled = HashSet::from([selected_key.clone()]);
    let mut visited = HashSet::new();
    let mut nodes = BTreeMap::new();
    let mut result_edges = BTreeMap::new();
    let mut hidden = 0;
    while let Some((key, level)) = queue.pop_front() {
        if !visited.insert(key.clone()) {
            continue;
        }
        let mut current = node(conn, &selection, &key, &request.filter)?;
        if level < depth {
            let all_outgoing = project_edges(conn, &selection, &key, &request.filter, &prepared)?;
            let mut outgoing = all_outgoing.iter().take(top_k).cloned().collect::<Vec<_>>();
            for edge in &all_outgoing {
                if !edge.prepared.is_empty()
                    && !outgoing.iter().any(|shown| shown.id == edge.id)
                    && outgoing.len() < top_k * 2
                {
                    outgoing.push(edge.clone());
                }
            }
            current.hidden_continuations = all_outgoing.len().saturating_sub(outgoing.len());
            hidden += current.hidden_continuations;
            for edge in outgoing {
                if !scheduled.contains(&edge.to) && scheduled.len() >= max_nodes {
                    current.hidden_continuations += 1;
                    hidden += 1;
                    continue;
                }
                if scheduled.insert(edge.to.clone()) {
                    queue.push_back((edge.to.clone(), level + 1));
                }
                result_edges.insert(edge.id.clone(), edge);
            }
        } else {
            let (_, all) = edges(conn, &selection, &key, &request.filter, 0)?;
            current.hidden_continuations = all;
            hidden += all;
        }
        if current.fen.is_empty() {
            current.fen = prepared
                .iter()
                .find(|p| p.to_key == key)
                .map(|p| p.fen.clone())
                .unwrap_or_default();
        }
        nodes.insert(key, current);
    }
    let mut selected = nodes.get(&selected_key).cloned().unwrap_or_default();
    if selected_key == path_position.position_key {
        selected.fen = path_position.fen.clone();
    }
    if selected.fen.is_empty() {
        return Err(AppError::new(
            "position",
            "Позиция не найдена в выборке или репертуаре",
        ));
    }
    if request.line.is_none()
        && selected_key != path_position.position_key
        && let Some(reference) = &selected.example
    {
        let context = resolve_occurrence(conn, reference)?;
        initial = context.initial_fen;
        line = context.line;
        selected.fen = context.fen;
    }
    let root_key = chess::line_position_from(&initial, &[])
        .map_err(|e| AppError::new("position", e))?
        .position_key;
    let branch = branch_selection(&initial, &line)?;
    let branch_node = node(conn, &branch, &selected_key, &request.filter)?;
    let branch_games = branch_node.games;
    let alternative_paths =
        alternative_path_count(conn, &selection, &selected_key, &initial, &line)?;
    let local_selection = if request.all_paths {
        &selection
    } else {
        &branch
    };
    if !request.all_paths {
        let fen = selected.fen.clone();
        let hidden_continuations = selected.hidden_continuations;
        selected = branch_node;
        selected.fen = fen;
        selected.hidden_continuations = hidden_continuations;
    }
    populate_engine(conn, local_selection, &request.filter, &mut selected)?;
    // Overview nodes are position aggregates. The separately returned selected
    // node belongs to the explicit local prefix and may have a different N.
    if request.all_paths {
        nodes.insert(selected_key.clone(), selected.clone());
    }
    let eligible_games: usize = conn.query_row(
        &format!("{} SELECT count(*) FROM selected", selection.cte),
        rusqlite::params_from_iter(selection.values.iter()),
        |r| r.get(0),
    )?;
    let (total_positions, total_visits) = cohort_counts(conn, &selection, &request.filter)?;
    let (indexed_games,index_revision):(usize,String)=conn.query_row("SELECT count(*),cast(coalesce(sum(revision+payload_revision+updated_at_ms),0) AS TEXT) FROM game_graph_games",[],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let mut continuations = project_edges(
        conn,
        local_selection,
        &selected_key,
        &request.filter,
        &prepared,
    )?;
    let branch_edges = project_edges(conn, &branch, &selected_key, &request.filter, &[])?;
    for edge in &mut continuations {
        edge.branch_games = branch_edges
            .iter()
            .find(|b| b.uci == edge.uci)
            .map_or(0, |b| b.games);
    }
    let mut previews = Vec::new();
    let preview_limit = request.preview_limit.unwrap_or(5).clamp(1, 30) as usize;
    let preview_top_k = request.preview_top_k.unwrap_or(3).clamp(1, 5) as usize;
    for edge in continuations.iter().take(preview_limit) {
        let mut next_line = line.clone();
        next_line.push(edge.uci.clone());
        let next_branch = branch_selection(&initial, &next_line)?;
        let next_selection = if request.all_paths {
            &selection
        } else {
            &next_branch
        };
        let replies = project_edges(conn, next_selection, &edge.to, &request.filter, &prepared)?;
        let hidden = replies.len().saturating_sub(preview_top_k);
        previews.push(GraphPreview {
            uci: edge.uci.clone(),
            replies: replies.into_iter().take(preview_top_k).collect(),
            hidden,
        });
    }
    let mut review_values = branch.values.clone();
    let profile_condition = if let Some(id) = &request.filter.profile_id {
        review_values.push(Value::Text(id.clone()));
        format!(" AND r.profile_id=?{}", review_values.len())
    } else {
        String::new()
    };
    let review_analysed_games:usize=conn.query_row(&format!("{} SELECT count(DISTINCT s.game_id) FROM selected s JOIN statistics_analysis_runs r ON r.game_id=s.external_ref AND r.movetext_revision=s.revision WHERE r.state='succeeded'{profile_condition}",branch.cte),rusqlite::params_from_iter(review_values.iter()),|r|r.get(0))?;
    let mut review_refs = Vec::new();
    if !request.all_paths && !request.review_refs.is_empty() {
        review_values.push(Value::Text(serde_json::to_string(&request.review_refs)?));
        let sql = format!(
            "{} SELECT json_set(j.value,'$.position_key',n.from_key,'$.played_uci',n.uci,'$.revision',n.revision) FROM json_each(?{}) j JOIN selected s ON json_extract(j.value,'$.game_id')=s.external_ref JOIN game_occurrences n ON n.game_id=s.game_id AND n.revision=s.revision AND n.ply=json_extract(j.value,'$.ply') JOIN statistics_analysis_runs r ON r.id=json_extract(j.value,'$.run_id') AND r.game_id=s.external_ref AND r.movetext_revision=s.revision WHERE r.state='succeeded' AND json_extract(j.value,'$.ply')>{}{profile_condition}",
            branch.cte,
            review_values.len(),
            line.len()
        );
        for item in conn
            .prepare(&sql)?
            .query_map(rusqlite::params_from_iter(review_values.iter()), |r| {
                r.get::<_, String>(0)
            })?
        {
            review_refs.push(serde_json::from_str(&item?)?);
        }
    }
    let path_nodes =
        chess::line_nodes_from(&initial, &line).map_err(|e| AppError::new("position", e))?;
    let repertoire_ends_here = prepared.iter().any(|p| p.to_key == selected_key)
        && !prepared.iter().any(|p| p.from_key == selected_key);
    Ok(GraphProjection {
        repertoire_ends_here,
        continuations,
        previews,
        alternative_paths,
        branch_games,
        all_paths: request.all_paths,
        review_refs,
        review_analysed_games,
        path_nodes,
        root_key,
        selected_key,
        selected_fen: selected.fen.clone(),
        initial_fen: initial,
        line,
        nodes: nodes.into_values().collect(),
        edges: result_edges.into_values().collect(),
        selected,
        hidden_nodes: hidden,
        indexed_games,
        eligible_games,
        total_positions,
        total_visits,
        index_revision,
    })
}

// Materialize the membership/filter cohort once per command. All bounded local
// aggregates then join a small keyed temp table instead of repeating a window scan.
fn materialize_selection(
    conn: &Connection,
    filter: &StatisticsFilter,
) -> Result<Selection, AppError> {
    let selected = selection(filter);
    conn.execute("CREATE TEMP TABLE IF NOT EXISTS graph_selected AS SELECT g.*,a.account_id,a.user_color,'' AS outcome,0 AS chosen FROM game_graph_games g JOIN statistics_account_games a ON 0",[])?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS temp.graph_selected_game ON graph_selected(game_id)",
        [],
    )?;
    conn.execute("DELETE FROM graph_selected", [])?;
    conn.execute(
        &format!(
            "{} INSERT INTO graph_selected SELECT * FROM selected",
            selected.cte
        ),
        rusqlite::params_from_iter(selected.values.iter()),
    )?;
    Ok(Selection {
        cte: "WITH selected AS (SELECT * FROM graph_selected)".into(),
        values: vec![],
        ply: None,
    })
}

pub fn evidence(
    conn: &Connection,
    request: &GraphEvidenceRequest,
) -> Result<GraphEvidencePage, AppError> {
    ensure_index(conn)?;
    let cohort = materialize_selection(conn, &request.filter)?;
    let branch = if !request.all_paths {
        request
            .line
            .as_ref()
            .map(|line| {
                branch_selection(
                    request.initial_fen.as_deref().unwrap_or(chess::INITIAL_FEN),
                    line,
                )
            })
            .transpose()?
    } else {
        None
    };
    let selected = branch.as_ref().unwrap_or(&cohort);
    let mut values = selected.values.clone();
    values.push(Value::Text(request.position_key.clone()));
    let k = values.len();
    let mut condition = if request.uci.is_some() {
        format!("n.from_key=?{k}")
    } else {
        format!("n.position_key=?{k}")
    };
    if let Some(uci) = &request.uci {
        values.push(Value::Text(uci.clone()));
        condition.push_str(&format!(" AND n.uci=?{}", values.len()));
    }
    condition.push_str(&phase_condition(&request.filter, &mut values));
    if let Some(ply) = selected.ply {
        condition.push_str(&format!(
            " AND n.ply={}",
            ply + usize::from(request.uci.is_some())
        ));
    }
    let from = format!(
        "FROM game_occurrences n JOIN selected s ON s.game_id=n.game_id AND s.revision=n.revision WHERE {condition}"
    );
    let total = conn.query_row(
        &format!("{} SELECT count(*) {from}", selected.cte),
        rusqlite::params_from_iter(values.iter()),
        |r| r.get::<_, usize>(0),
    )?;
    let limit = if request.limit == 0 {
        30
    } else {
        request.limit.min(100)
    };
    let offset = request.offset.min(total);
    let rows=conn.prepare(&format!("{} SELECT s.external_ref,n.revision,n.ply,s.account_id,s.user_color,s.white,s.black,s.result,s.outcome,s.timestamp_ms,n.fen,n.san,n.uci {from} ORDER BY s.timestamp_ms DESC,s.external_ref,n.ply LIMIT {limit} OFFSET {offset}",selected.cte))?
        .query_map(rusqlite::params_from_iter(values.iter()),|r|Ok(GraphEvidence{reference:OccurrenceRef{game_id:r.get(0)?,revision:r.get(1)?,ply:r.get(2)?},account_id:r.get(3)?,own_color:r.get(4)?,white:r.get(5)?,black:r.get(6)?,result:r.get(7)?,outcome:r.get(8)?,timestamp_ms:r.get(9)?,fen:r.get(10)?,san:r.get(11)?,uci:r.get(12)?}))?.collect::<Result<Vec<_>,_>>()?;
    let has_more = offset + rows.len() < total;
    Ok(GraphEvidencePage {
        rows,
        total,
        offset,
        has_more,
    })
}

pub fn resolve_occurrence(
    conn: &Connection,
    reference: &OccurrenceRef,
) -> Result<OccurrenceContext, AppError> {
    ensure_index(conn)?;
    let current: Option<(i64, i64, String, String)> = conn
        .query_row(
            "SELECT game_id,revision,white,black FROM game_graph_games WHERE external_ref=?1",
            [&reference.game_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let (game_id, current_revision, white, black) =
        current.ok_or_else(|| AppError::new("not_found", "Партия не найдена"))?;
    // On upgrade resolve historical links from immutable source payload, never current PGN.
    let present: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM game_occurrences WHERE game_id=?1 AND revision=?2)",
        params![game_id, reference.revision],
        |r| r.get(0),
    )?;
    if !present {
        let raw:Option<String>=conn.query_row("SELECT raw_pgn FROM statistics_payloads WHERE game_id=?1 AND movetext_revision=?2 ORDER BY payload_revision DESC LIMIT 1",params![game_id,reference.revision],|r|r.get(0)).optional()?;
        let raw = raw.ok_or_else(|| {
            AppError::new("revision_not_found", "Исходная ревизия партии недоступна")
        })?;
        let replay = chess::replay_pgn(&raw).map_err(|e| AppError::new("replay", e))?;
        let tx = conn.unchecked_transaction()?;
        tx.execute("INSERT OR IGNORE INTO game_graph_revisions(game_id,revision,initial_fen) VALUES(?1,?2,?3)",params![game_id,reference.revision,replay.initial_fen])?;
        let mut previous: Option<String> = None;
        for n in replay.nodes {
            tx.execute("INSERT OR IGNORE INTO chess_positions(position_key,variant,representative_fen) VALUES(?1,'standard',?2)",params![n.position_key,n.fen])?;
            tx.execute("INSERT OR IGNORE INTO game_occurrences(game_id,revision,ply,position_key,fen,from_key,uci,san,phase,clock_seconds,spent_seconds) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![game_id,reference.revision,n.ply,n.position_key,n.fen,previous,n.uci,n.san,n.phase,n.clock_seconds,n.spent_seconds])?;
            previous = Some(n.position_key);
        }
        tx.commit()?;
    }
    let initial_fen: String = conn.query_row(
        "SELECT initial_fen FROM game_graph_revisions WHERE game_id=?1 AND revision=?2",
        params![game_id, reference.revision],
        |r| r.get(0),
    )?;
    let occurrence:Option<(String,String)>=conn.query_row("SELECT fen,position_key FROM game_occurrences WHERE game_id=?1 AND revision=?2 AND ply=?3",params![game_id,reference.revision,reference.ply],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (fen, position_key) = occurrence
        .ok_or_else(|| AppError::new("not_found", "Полуход не найден в указанной ревизии"))?;
    let line=conn.prepare("SELECT uci FROM game_occurrences WHERE game_id=?1 AND revision=?2 AND ply>0 AND ply<=?3 ORDER BY ply")?.query_map(params![game_id,reference.revision,reference.ply],|r|r.get(0))?.collect::<Result<Vec<String>,_>>()?;
    let raw_pgn = source_pgn_for_indexed_revision(conn, game_id, reference.revision, &initial_fen)?;
    Ok(OccurrenceContext {
        reference: reference.clone(),
        initial_fen,
        fen,
        position_key,
        line,
        raw_pgn,
        current_revision,
        white,
        black,
    })
}
pub fn load_state(
    conn: &Connection,
    account_id: Option<&str>,
) -> Result<Option<serde_json::Value>, AppError> {
    crate::storage::setting(
        conn,
        &format!("game_graph_view:{}", account_id.unwrap_or("all")),
    )?
    .map(|s| serde_json::from_str(&s).map_err(AppError::from))
    .transpose()
}
pub fn save_state(
    conn: &Connection,
    account_id: Option<&str>,
    state: &serde_json::Value,
) -> Result<(), AppError> {
    let raw = serde_json::to_string(state)?;
    if raw.len() > 200_000 {
        return Err(AppError::new(
            "state",
            "Слишком большой размер состояния графа",
        ));
    }
    crate::storage::set_setting(
        conn,
        &format!("game_graph_view:{}", account_id.unwrap_or("all")),
        &raw,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lichess::VerifiedIdentity, statistics::store, storage};
    pub(super) fn setup() -> (tempfile::TempDir, Connection) {
        let temp = tempfile::tempdir().unwrap();
        let conn = storage::open(&temp.path().join("fixture.sqlite3")).unwrap();
        store::observe(
            &conn,
            &VerifiedIdentity {
                id: "me".into(),
                username: "Me".into(),
                generation: 1,
            },
            1_000,
        )
        .unwrap();
        (temp, conn)
    }
    pub(super) fn game(id: &str, moves: &str) -> StoredGame {
        let pgn = format!("[White \"Me\"]\n[Black \"Other\"]\n[Result \"1-0\"]\n\n{moves} 1-0");
        StoredGame {
            id: format!("lichess:{id}"),
            external_id: id.into(),
            account_id: "lichess:me".into(),
            source: "lichess".into(),
            started_at_ms: Some(1_000),
            last_move_at_ms: Some(10_000),
            own_color: "white".into(),
            result: "1-0".into(),
            status: "resign".into(),
            speed: "rapid".into(),
            rated: Some(true),
            variant: "standard".into(),
            initial_seconds: Some(600.),
            increment_seconds: Some(5.),
            white: Participant {
                name: "Me".into(),
                id: Some("me".into()),
                rating: Some(1500),
                ..Default::default()
            },
            black: Participant {
                name: "Other".into(),
                id: Some("other".into()),
                rating: Some(1600),
                ..Default::default()
            },
            replay: chess::replay_pgn(&pgn).unwrap(),
            raw_pgn: pgn,
            eligibility: "eligible".into(),
            ..Default::default()
        }
    }
    pub(super) fn save(conn: &Connection, game: &StoredGame) {
        store::save_game(
            conn,
            "lichess:me",
            game,
            &format!("{{\"id\":\"{}\"}}", game.id),
            "eligible",
        )
        .unwrap();
    }
    pub(super) fn request() -> GraphRequest {
        GraphRequest {
            all_paths: true,
            filter: StatisticsFilter {
                account_id: Some("lichess:me".into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    #[test]
    fn exact_prefix_never_splices_transpositions_or_repeated_visits() {
        let (_temp, conn) = setup();
        let a = game("prefixA", "1. Nf3 d5 2. d4 Nf6 3. c4 e6");
        let b = game("prefixB", "1. d4 Nf6 2. c4 d5 3. Nf3 c6");
        save(&conn, &a);
        save(&conn, &b);
        let line = vec!["g1f3", "d7d5", "d2d4", "g8f6", "c2c4"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut req = request();
        req.all_paths = false;
        req.line = Some(line.clone());
        let branch = query(&conn, &req).unwrap();
        assert_eq!(branch.selected.games, 1);
        assert_eq!(branch.selected.visits, 1);
        assert_eq!(
            branch
                .nodes
                .iter()
                .find(|n| n.id == branch.selected_key)
                .unwrap()
                .games,
            2
        );
        assert_eq!(branch.alternative_paths, 1);
        assert_eq!(branch.continuations.len(), 1);
        assert_eq!(branch.continuations[0].uci, "e7e6");
        assert_eq!(branch.continuations[0].frequency, 100.);
        let evidence_request = GraphEvidenceRequest {
            filter: req.filter.clone(),
            position_key: branch.selected_key.clone(),
            line: Some(line),
            ..Default::default()
        };
        let ev = evidence(&conn, &evidence_request).unwrap();
        assert_eq!(ev.total, 1);
        assert_eq!(ev.rows[0].reference.game_id, a.id);
        assert_eq!(ev.rows[0].reference.ply, 5);
        let after = evidence(
            &conn,
            &GraphEvidenceRequest {
                uci: Some("e7e6".into()),
                ..evidence_request
            },
        )
        .unwrap();
        assert_eq!(after.total, 1);
        assert_eq!(after.rows[0].reference.ply, 6);
        req.all_paths = true;
        let all = query(&conn, &req).unwrap();
        assert_eq!(all.selected.games, 2);
        assert_eq!(all.branch_games, 1);
        assert_eq!(all.continuations.len(), 2);
        assert_eq!(
            all.continuations
                .iter()
                .find(|e| e.uci == "c7c6")
                .unwrap()
                .branch_games,
            0
        );
        let phantom = GraphRequest {
            line: Some(
                vec!["g1f3", "d7d5", "d2d4", "g8f6", "c2c4", "c7c6"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
            all_paths: false,
            ..req
        };
        assert_eq!(query(&conn, &phantom).unwrap().selected.games, 0);
        let cycle = game("prefixCycle", "1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6 4. Ng1 Ng8");
        save(&conn, &cycle);
        let root = query(
            &conn,
            &GraphRequest {
                all_paths: false,
                ..request()
            },
        )
        .unwrap();
        assert_eq!(root.selected.games, 3);
        assert_eq!(root.selected.visits, 3);
        let repeated = query(
            &conn,
            &GraphRequest {
                line: Some(
                    vec!["g1f3", "g8f6", "f3g1", "f6g8"]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                ),
                all_paths: false,
                ..request()
            },
        )
        .unwrap();
        assert_eq!(repeated.selected.games, 1);
        assert_eq!(repeated.selected.visits, 1);
        assert_eq!(repeated.continuations[0].visits, 1);
    }
    #[test]
    fn transpositions_merge_positions_but_repeated_visits_keep_occurrences() {
        let (_temp, conn) = setup();
        let a = game("transposeA", "1. Nf3 d5 2. d4 Nf6 3. c4");
        let b = game("transposeB", "1. d4 Nf6 2. c4 d5 3. Nf3");
        let cycle = game("cycle", "1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6 4. Ng1 Ng8");
        assert_eq!(
            a.replay.nodes[5].position_key,
            b.replay.nodes[5].position_key
        );
        save(&conn, &a);
        save(&conn, &b);
        save(&conn, &cycle);
        let root = query(&conn, &request()).unwrap();
        assert_eq!(root.selected.games, 3);
        assert_eq!(root.selected.visits, 5);
        assert_eq!(
            root.edges
                .iter()
                .find(|e| e.from == root.root_key && e.uci == "g1f3")
                .unwrap()
                .visits,
            3
        );
        let mut req = request();
        req.position_key = Some(a.replay.nodes[5].position_key.clone());
        let merged = query(&conn, &req).unwrap();
        assert_eq!(merged.selected.games, 2);
        assert_eq!(merged.selected.visits, 2);
        let page = evidence(
            &conn,
            &GraphEvidenceRequest {
                filter: req.filter,
                position_key: merged.selected_key,
                limit: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.rows.len(), 1);
        assert!(page.has_more);
        let context = resolve_occurrence(&conn, &page.rows[0].reference).unwrap();
        assert_eq!(context.line.len(), 5);
    }
    #[test]
    fn repeated_payload_and_clock_changes_do_not_duplicate_or_retarget_nodes() {
        let (_temp, conn) = setup();
        let mut a = game("repeat", "1. e4 e5");
        save(&conn, &a);
        save(&conn, &a);
        assert_eq!(query(&conn, &request()).unwrap().total_visits, 3);
        a.replay.nodes[1].clock_seconds = Some(590.);
        a.raw_pgn = a.raw_pgn.replace("e4", "e4 {[%clk 0:09:50]}");
        save(&conn, &a);
        let current = store::game(&conn, &a.id).unwrap().unwrap();
        assert_eq!(current.movetext_revision, 1);
        let count: i64 = conn
            .query_row("SELECT count(*) FROM game_occurrences", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
        assert_eq!(
            resolve_occurrence(
                &conn,
                &OccurrenceRef {
                    game_id: a.id,
                    revision: 1,
                    ply: 1
                }
            )
            .unwrap()
            .fen,
            a.replay.nodes[1].fen
        );
    }
    #[test]
    fn corrected_movetext_keeps_historical_links_and_current_counts_separate() {
        let (_temp, conn) = setup();
        let a = game("corrected", "1. e4 e5");
        save(&conn, &a);
        let old = OccurrenceRef {
            game_id: a.id.clone(),
            revision: 1,
            ply: 1,
        };
        let corrected = game("corrected", "1. d4 d5 2. c4");
        save(&conn, &corrected);
        let context = resolve_occurrence(&conn, &old).unwrap();
        assert_eq!(context.current_revision, 2);
        assert_eq!(context.line, vec!["e2e4"]);
        assert_eq!(context.position_key, a.replay.nodes[1].position_key);
        assert!(context.raw_pgn.contains("e4"));
        let graph = query(&conn, &request()).unwrap();
        assert_eq!(graph.total_visits, 4);
        assert_eq!(
            graph
                .edges
                .iter()
                .filter(|e| e.from == graph.root_key)
                .map(|e| e.uci.as_str())
                .collect::<Vec<_>>(),
            vec!["d2d4"]
        );
        assert_eq!(
            store::game(&conn, &a.id)
                .unwrap()
                .unwrap()
                .movetext_revision,
            2
        );
    }
    #[test]
    fn graph_cohort_matches_statistics_filters_and_excludes_unfinished_unowned() {
        let (_temp, conn) = setup();
        let a = game("rapid", "1. e4 e5");
        save(&conn, &a);
        let mut b = game("blitz", "1. d4 d5");
        b.speed = "blitz".into();
        b.own_color = "black".into();
        b.last_move_at_ms = Some(20_000);
        save(&conn, &b);
        let mut c = game("active", "1. c4 e5");
        c.status = "started".into();
        save(&conn, &c);
        let excluded = game("excluded", "1. g3 d5");
        store::save_game(&conn, "lichess:me", &excluded, "{}", "excluded").unwrap();
        let mut req = request();
        assert_eq!(query(&conn, &req).unwrap().eligible_games, 2);
        for f in [
            StatisticsFilter {
                speed: Some("rapid".into()),
                ..Default::default()
            },
            StatisticsFilter {
                color: Some("black".into()),
                ..Default::default()
            },
            StatisticsFilter {
                since_ms: Some(15_000),
                ..Default::default()
            },
            StatisticsFilter {
                until_ms: Some(15_000),
                ..Default::default()
            },
            StatisticsFilter {
                opponent_rating_min: Some(1550),
                ..Default::default()
            },
            StatisticsFilter {
                rated: Some(false),
                ..Default::default()
            },
            StatisticsFilter {
                control: Some("600+5".into()),
                ..Default::default()
            },
        ] {
            req.filter = f;
            req.filter.account_id = Some("lichess:me".into());
            let expected = [&a, &b]
                .into_iter()
                .filter(|g| statistics_query::eligible(g, &req.filter))
                .count();
            assert_eq!(query(&conn, &req).unwrap().eligible_games, expected);
        }
        req.filter = StatisticsFilter {
            account_id: Some("lichess:foreign".into()),
            ..Default::default()
        };
        assert_eq!(query(&conn, &req).unwrap().eligible_games, 0);
    }
    #[test]
    fn one_canonical_game_counted_once_across_multiple_verified_accounts() {
        let (_temp, conn) = setup();
        let a = game("both", "1. e4 e5");
        save(&conn, &a);
        store::observe(
            &conn,
            &VerifiedIdentity {
                id: "other".into(),
                username: "Other".into(),
                generation: 2,
            },
            1000,
        )
        .unwrap();
        let mut other = a.clone();
        other.own_color = "black".into();
        store::save_game(&conn, "lichess:other", &other, "{}", "eligible").unwrap();
        let graph = query(&conn, &GraphRequest::default()).unwrap();
        assert_eq!(graph.eligible_games, 1);
        assert_eq!(graph.selected.games, 1);
        let mut req = request();
        req.filter.account_id = Some("lichess:other".into());
        let black = query(&conn, &req).unwrap();
        assert_eq!(black.selected.losses, 1);
        assert_eq!(black.selected.wins, 0);
    }
    #[test]
    fn empty_graph_is_truthful_and_state_survives_reopen() {
        let (temp, conn) = setup();
        let root = query(&conn, &request()).unwrap();
        assert_eq!(root.eligible_games, 0);
        assert_eq!(root.selected.score, None);
        assert!(root.edges.is_empty());
        let state = serde_json::json!({"line":["e2e4"],"repertoire_ids":[],"orientation":"black"});
        save_state(&conn, Some("lichess:me"), &state).unwrap();
        drop(conn);
        let reopened = storage::open(&temp.path().join("fixture.sqlite3")).unwrap();
        assert_eq!(
            load_state(&reopened, Some("lichess:me")).unwrap(),
            Some(state)
        );
        assert_eq!(load_state(&reopened, Some("another")).unwrap(), None);
    }
    #[test]
    fn repertoire_overlay_never_changes_factual_counts_and_keeps_unplayed_score_unknown() {
        let (_temp, conn) = setup();
        save(&conn, &game("overlay", "1. e4 e5"));
        let document = crate::knowledge::import(
            &conn,
            crate::knowledge::ImportRequest {
                title: "My opening".into(),
                side: "white".into(),
                pgn: "1. e4 e5 (1... c5) 2. Nf3 *".into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        let baseline = query(&conn, &request()).unwrap();
        let mut req = request();
        req.repertoire_ids = vec![document.id.clone()];
        let graph = query(&conn, &req).unwrap();
        assert_eq!(graph.total_visits, baseline.total_visits);
        assert_eq!(graph.selected.games, baseline.selected.games);
        let unplayed = graph.edges.iter().find(|e| e.uci == "c7c5").unwrap();
        assert_eq!(unplayed.games, 0);
        assert_eq!(unplayed.score, None);
        assert!(!unplayed.prepared.is_empty());
        assert_eq!(
            graph.edges.iter().find(|e| e.uci == "e2e4").unwrap().games,
            1
        );
        let leaf = query(
            &conn,
            &GraphRequest {
                line: Some(vec!["e2e4".into(), "c7c5".into()]),
                all_paths: false,
                ..req.clone()
            },
        )
        .unwrap();
        assert!(leaf.repertoire_ends_here);
        assert_eq!(leaf.selected.games, 0);
        let main = query(
            &conn,
            &GraphRequest {
                line: Some(vec!["e2e4".into(), "e7e5".into()]),
                all_paths: false,
                ..req.clone()
            },
        )
        .unwrap();
        assert!(!main.repertoire_ends_here);
        let without_layer = query(
            &conn,
            &GraphRequest {
                repertoire_ids: vec![],
                line: Some(vec!["e2e4".into(), "c7c5".into()]),
                all_paths: false,
                ..req
            },
        )
        .unwrap();
        assert!(!without_layer.repertoire_ends_here);
    }
    #[test]
    fn local_reply_previews_keep_prepared_moves_and_exact_branch_counts() {
        let (_temp, conn) = setup();
        save(&conn, &game("preview-e4", "1. e4 e5 2. Nf3"));
        save(&conn, &game("preview-d4", "1. d4 d5 2. Nf3"));
        let document = crate::knowledge::import(
            &conn,
            crate::knowledge::ImportRequest {
                title: "Preview repertoire".into(),
                side: "white".into(),
                pgn: "1. e4 e5 (1... c5) 2. Nf3 *".into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        let graph = query(
            &conn,
            &GraphRequest {
                all_paths: false,
                preview_limit: Some(30),
                preview_top_k: Some(2),
                repertoire_ids: vec![document.id],
                ..request()
            },
        )
        .unwrap();
        assert_eq!(graph.previews.len(), 2);
        let e4 = graph.previews.iter().find(|p| p.uci == "e2e4").unwrap();
        let played = e4.replies.iter().find(|e| e.uci == "e7e5").unwrap();
        assert_eq!(played.games, 1);
        assert_eq!(played.frequency, 100.);
        assert!(!played.prepared.is_empty());
        let prepared = e4.replies.iter().find(|e| e.uci == "c7c5").unwrap();
        assert_eq!(prepared.games, 0);
        assert_eq!(prepared.score, None);
        assert!(!prepared.prepared.is_empty());
        let d4 = graph.previews.iter().find(|p| p.uci == "d2d4").unwrap();
        assert_eq!(d4.replies.len(), 1);
        assert_eq!(d4.replies[0].uci, "d7d5");
        assert_eq!(d4.replies[0].games, 1);
    }

    #[test]
    fn bounded_cycles_do_not_expand_forever() {
        let (_temp, conn) = setup();
        save(
            &conn,
            &game("cycle", "1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6 4. Ng1 Ng8"),
        );
        let mut req = request();
        req.depth = Some(5);
        req.max_nodes = Some(3);
        req.top_k = Some(30);
        let graph = query(&conn, &req).unwrap();
        assert!(graph.nodes.len() <= 3);
        assert!(graph.hidden_nodes > 0);
        req.max_nodes = Some(300);
        let graph = query(&conn, &req).unwrap();
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.edges.len(), 4);
        assert!(graph.edges.iter().any(|e| e.to == graph.root_key));
    }
}

fn populate_engine(
    conn: &Connection,
    selection: &Selection,
    filter: &StatisticsFilter,
    node: &mut GraphNode,
) -> Result<(), AppError> {
    let mut values = selection.values.clone();
    values.push(Value::Text(node.id.clone()));
    let k = values.len();
    let mut node_phase = phase_condition(filter, &mut values);
    if let Some(ply) = selection.ply {
        node_phase.push_str(&format!(" AND n.ply={ply}"));
    }
    let mut profile = String::new();
    if let Some(id) = &filter.profile_id {
        values.push(Value::Text(id.clone()));
        profile = format!(" AND r.profile_id=?{}", values.len());
    }
    // This is one exact, saved occurrence, not an average of incomparable contexts/profiles.
    // EngineRun scores are already normalized to White POV by UCI parsing.
    let joins = format!(
        "FROM game_occurrences n JOIN selected s ON s.game_id=n.game_id AND s.revision=n.revision JOIN statistics_analysis_runs r ON r.game_id=s.external_ref AND r.movetext_revision=n.revision JOIN json_each(r.data_json,'$.evaluations') e ON json_extract(e.value,'$.ply')=n.ply WHERE n.position_key=?{k}{node_phase} AND r.state='succeeded' AND json_extract(e.value,'$.complete')=1 AND json_extract(e.value,'$.bound') IS NULL AND (json_extract(e.value,'$.cp') IS NOT NULL OR json_extract(e.value,'$.mate') IS NOT NULL){profile}"
    );
    type SavedObservation = (
        String,
        String,
        u32,
        String,
        i64,
        u32,
        Option<i32>,
        Option<i32>,
    );
    let observation:Option<SavedObservation>=conn.query_row(&format!("{} SELECT r.id,r.profile_id,json_extract(e.value,'$.depth'),s.external_ref,n.revision,n.ply,json_extract(e.value,'$.cp'),json_extract(e.value,'$.mate') {joins} ORDER BY r.completed_at_ms DESC,s.external_ref,n.ply LIMIT 1",selection.cte),rusqlite::params_from_iter(values.iter()),|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional()?;
    if let Some((run_id, profile_id, depth, game_id, revision, ply, cp, mate)) = observation {
        node.engine_cp = cp;
        node.engine_mate = mate;
        node.engine_profile = Some(profile_id.clone());
        node.engine_depth = Some(depth);
        node.engine_run_id = Some(run_id);
        node.engine_reference = Some(OccurrenceRef {
            game_id,
            revision,
            ply,
        });
        values.push(Value::Text(profile_id));
        node.engine_coverage = conn.query_row(
            &format!(
                "{} SELECT count(DISTINCT n.game_id||':'||n.ply) {joins} AND r.profile_id=?{}",
                selection.cte,
                values.len()
            ),
            rusqlite::params_from_iter(values.iter()),
            |r| r.get(0),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod saved_engine_tests {
    use super::*;
    #[test]
    fn saved_evaluation_keeps_profile_history_reference_and_white_pov() {
        let temp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(&temp.path().join("engine-graph.sqlite3")).unwrap();
        crate::statistics::store::observe(
            &conn,
            &crate::lichess::VerifiedIdentity {
                id: "me".into(),
                username: "Me".into(),
                generation: 1,
            },
            1000,
        )
        .unwrap();
        let pgn = "1. e4 e5 1-0";
        let game = StoredGame {
            id: "lichess:engineGraph".into(),
            external_id: "engineGraph".into(),
            source: "lichess".into(),
            own_color: "white".into(),
            result: "1-0".into(),
            status: "resign".into(),
            variant: "standard".into(),
            replay: chess::replay_pgn(pgn).unwrap(),
            raw_pgn: pgn.into(),
            ..Default::default()
        };
        crate::statistics::store::save_game(&conn, "lichess:me", &game, "{}", "eligible").unwrap();
        let run = EngineRun {
            id: "runGraph".into(),
            game_id: game.id.clone(),
            profile_id: "profileGraph".into(),
            evaluations: vec![PositionEvaluation {
                ply: 1,
                cp: Some(37),
                depth: 18,
                complete: true,
                root_pov: "black".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        conn.execute("INSERT INTO statistics_analysis_runs(id,game_id,movetext_revision,profile_id,state,data_json,completed_at_ms) VALUES(?1,?2,1,?3,'succeeded',?4,1000)",params![run.id,run.game_id,run.profile_id,serde_json::to_string(&run).unwrap()]).unwrap();
        let mut request = GraphRequest {
            line: Some(vec!["e2e4".into()]),
            filter: StatisticsFilter {
                account_id: Some("lichess:me".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let review_request = GraphRequest {
            line: Some(vec![]),
            review_refs: vec![
                GraphReviewReference {
                    game_id: game.id.clone(),
                    ply: 1,
                    run_id: run.id.clone(),
                    ..Default::default()
                },
                GraphReviewReference {
                    game_id: game.id.clone(),
                    ply: 2,
                    run_id: "missing".into(),
                    ..Default::default()
                },
            ],
            ..request.clone()
        };
        let reviewed = query(&conn, &review_request).unwrap();
        assert_eq!(reviewed.review_refs.len(), 1);
        assert_eq!(
            reviewed.review_refs[0].position_key.as_deref(),
            Some(game.replay.nodes[0].position_key.as_str())
        );
        assert_eq!(reviewed.review_refs[0].played_uci.as_deref(), Some("e2e4"));
        assert_eq!(reviewed.review_refs[0].revision, Some(1));
        assert_eq!(reviewed.review_analysed_games, 1);
        let wrong_profile = GraphRequest {
            filter: StatisticsFilter {
                profile_id: Some("missing".into()),
                ..review_request.filter.clone()
            },
            ..review_request.clone()
        };
        assert_eq!(query(&conn, &wrong_profile).unwrap().review_refs.len(), 0);
        let before_branch = GraphRequest {
            line: Some(vec!["e2e4".into()]),
            ..review_request
        };
        assert_eq!(query(&conn, &before_branch).unwrap().review_refs.len(), 0);
        let node = query(&conn, &request).unwrap().selected;
        assert_eq!(node.engine_cp, Some(37));
        assert_eq!(node.engine_depth, Some(18));
        assert_eq!(node.engine_coverage, 1);
        assert_eq!(
            node.engine_reference,
            Some(OccurrenceRef {
                game_id: game.id.clone(),
                revision: 1,
                ply: 1
            })
        );
        request.filter.profile_id = Some("differentProfile".into());
        assert_eq!(query(&conn, &request).unwrap().selected.engine_cp, None);
        request.filter.profile_id = None;
        let updated = StoredGame {
            replay: chess::replay_pgn("1. d4 d5 1-0").unwrap(),
            raw_pgn: "1. d4 d5 1-0".into(),
            ..game
        };
        crate::statistics::store::save_game(&conn, "lichess:me", &updated, "{}", "eligible")
            .unwrap();
        assert_eq!(query(&conn, &request).unwrap().selected.engine_cp, None);
    }
}

fn project_edges(
    conn: &Connection,
    selection: &Selection,
    key: &str,
    filter: &StatisticsFilter,
    prepared: &[crate::knowledge::PreparedMove],
) -> Result<Vec<GraphEdge>, AppError> {
    // Standard chess has at most 218 legal moves; 256 is a bounded complete local slice.
    let (mut outgoing, _) = edges(conn, selection, key, filter, 256)?;
    let matching: Vec<_> = prepared.iter().filter(|p| p.from_key == key).collect();
    for p in &matching {
        let link = PreparedLink {
            document_id: p.document_id.clone(),
            chapter_id: p.chapter_id.clone(),
            node_id: p.node_id.clone(),
            document_revision: p.document_revision,
            side: p.side.clone(),
        };
        if let Some(edge) = outgoing.iter_mut().find(|e| e.uci == p.uci) {
            edge.prepared.push(link);
            edge.coverage = "prepared".into();
        } else {
            outgoing.push(GraphEdge {
                id: format!("{key}|{}", p.uci),
                from: key.into(),
                to: p.to_key.clone(),
                uci: p.uci.clone(),
                san: p.san.clone(),
                prepared: vec![link],
                coverage: "prepared".into(),
                ..Default::default()
            });
        }
    }
    if !matching.is_empty() {
        let turn = key
            .split('|')
            .nth(1)
            .and_then(|fen| fen.split_whitespace().nth(1))
            .unwrap_or("w");
        let actor = if turn == "w" { "white" } else { "black" };
        // For a document covering both sides, use the explicitly selected own color.
        // A mixed-color cohort has no single actor role; it must not claim an error.
        let own = matching
            .iter()
            .any(|p| p.side == actor || p.side == "both" && filter.color.as_deref() == Some(actor));
        let mixed = matching.iter().any(|p| p.side == "both") && filter.color.is_none();
        for edge in &mut outgoing {
            if edge.prepared.is_empty() {
                edge.coverage = if mixed {
                    "repertoire_gap"
                } else if own {
                    "own_deviation"
                } else {
                    "uncovered_reply"
                }
                .into();
            }
        }
    }
    Ok(outgoing)
}

#[cfg(test)]
mod complete_continuation_tests {
    use super::*;
    #[test]
    fn all_observed_moves_remain_navigable_beyond_canvas_limit() {
        let temp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(&temp.path().join("wide-graph.sqlite3")).unwrap();
        crate::statistics::store::observe(
            &conn,
            &crate::lichess::VerifiedIdentity {
                id: "me".into(),
                username: "Me".into(),
                generation: 1,
            },
            1000,
        )
        .unwrap();
        let fen = "k7/8/8/8/3Q4/8/4N3/K6R w - - 0 1";
        let legal = chess::legal_continuations(fen).unwrap();
        assert!(legal.len() > 30);
        for (index, mv) in legal.iter().enumerate() {
            let pgn = format!("[FEN \"{fen}\"]\n[SetUp \"1\"]\n\n1. {} 1-0", mv.san);
            let game = StoredGame {
                id: format!("lichess:wide{index}"),
                external_id: format!("wide{index}"),
                source: "lichess".into(),
                own_color: "white".into(),
                result: "1-0".into(),
                status: "resign".into(),
                variant: "standard".into(),
                replay: chess::replay_pgn(&pgn).unwrap(),
                raw_pgn: pgn,
                ..Default::default()
            };
            crate::statistics::store::save_game(&conn, "lichess:me", &game, "{}", "eligible")
                .unwrap();
        }
        let graph = query(
            &conn,
            &GraphRequest {
                initial_fen: Some(fen.into()),
                top_k: Some(1),
                max_nodes: Some(2),
                depth: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(graph.continuations.len(), legal.len());
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.selected.hidden_continuations, legal.len() - 1);
        assert!(graph.continuations.iter().all(|edge| edge.games == 1));
        assert_eq!(graph.path_nodes.len(), 1);
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SummaryCache {
    fingerprint: String,
    positions: usize,
    visits: usize,
}
fn cohort_counts(
    conn: &Connection,
    selection: &Selection,
    filter: &StatisticsFilter,
) -> Result<(usize, usize), AppError> {
    use sha2::{Digest, Sha256};
    let identity:String=conn.query_row(&format!("{} SELECT json_group_array(json_array(game_id,revision,payload_revision,updated_at_ms,user_color,outcome)) FROM (SELECT * FROM selected ORDER BY game_id)",selection.cte),rusqlite::params_from_iter(selection.values.iter()),|r|r.get(0))?;
    let mut hash = Sha256::new();
    hash.update(b"graph-summary-v1");
    hash.update(identity.as_bytes());
    hash.update(serde_json::to_vec(&filter.phase)?);
    let fingerprint = format!("{:x}", hash.finalize());
    let mut cache: Vec<SummaryCache> =
        crate::storage::setting(conn, "game_graph_summary_cache_v1")?
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
    if let Some(entry) = cache.iter().find(|entry| entry.fingerprint == fingerprint) {
        return Ok((entry.positions, entry.visits));
    }
    let mut values = selection.values.clone();
    let phase = phase_condition(filter, &mut values);
    let (positions,visits):(usize,usize)=conn.query_row(&format!("{} SELECT count(DISTINCT n.position_key),count(*) FROM game_occurrences n INDEXED BY game_occurrences_position JOIN selected s ON s.game_id=n.game_id AND s.revision=n.revision WHERE 1{phase}",selection.cte),rusqlite::params_from_iter(values.iter()),|r|Ok((r.get(0)?,r.get(1)?)))?;
    cache.push(SummaryCache {
        fingerprint,
        positions,
        visits,
    });
    if cache.len() > 16 {
        cache.remove(0);
    }
    crate::storage::set_setting(
        conn,
        "game_graph_summary_cache_v1",
        &serde_json::to_string(&cache)?,
    )?;
    Ok((positions, visits))
}

#[cfg(test)]
mod summary_invalidation_tests {
    use super::*;
    #[test]
    fn synchronized_new_game_and_membership_change_invalidate_cached_totals() {
        let (_temp, conn) = tests::setup();
        assert_eq!(query(&conn, &tests::request()).unwrap().total_visits, 0);
        let first = tests::game("first", "1. e4 e5");
        tests::save(&conn, &first);
        assert_eq!(query(&conn, &tests::request()).unwrap().total_visits, 3);
        assert_eq!(query(&conn, &tests::request()).unwrap().total_visits, 3);
        let second = tests::game("second", "1. d4 d5 2. c4");
        tests::save(&conn, &second);
        let graph = query(&conn, &tests::request()).unwrap();
        assert_eq!(graph.total_visits, 7);
        assert_eq!(graph.eligible_games, 2);
        crate::statistics::store::save_game(&conn, "lichess:me", &second, "{}", "excluded")
            .unwrap();
        assert_eq!(query(&conn, &tests::request()).unwrap().total_visits, 3);
    }
}

fn source_pgn_for_indexed_revision(
    conn: &Connection,
    game_id: i64,
    revision: i64,
    initial_fen: &str,
) -> Result<String, AppError> {
    let exact:Option<String>=conn.query_row("SELECT raw_pgn FROM statistics_payloads WHERE game_id=?1 AND movetext_revision=?2 ORDER BY payload_revision DESC LIMIT 1",params![game_id,revision],|r|r.get(0)).optional()?;
    if let Some(raw) = exact {
        return Ok(raw);
    }
    // A source can revert A -> B -> A. Payload dedup retains A once (revision 1),
    // while occurrence revision 3 is distinct. Match the FULL retained line and
    // full starting state; never choose a payload by ply or PositionKey alone.
    let expected=conn.prepare("SELECT uci FROM game_occurrences WHERE game_id=?1 AND revision=?2 AND ply>0 ORDER BY ply")?.query_map(params![game_id,revision],|r|r.get(0))?.collect::<Result<Vec<String>,_>>()?;
    let start = chess::line_position_from(initial_fen, &[])
        .map_err(|e| AppError::new("replay", e))?
        .fen;
    let payloads=conn.prepare("SELECT raw_pgn FROM statistics_payloads WHERE game_id=?1 ORDER BY payload_revision DESC")?.query_map([game_id],|r|r.get(0))?.collect::<Result<Vec<String>,_>>()?;
    for raw in payloads {
        if let Ok(replay) = chess::replay_pgn(&raw) {
            let actual: Vec<_> = replay
                .nodes
                .iter()
                .filter_map(|node| node.uci.clone())
                .collect();
            if actual == expected && replay.nodes.first().is_some_and(|node| node.fen == start) {
                return Ok(raw);
            }
        }
    }
    Err(AppError::new(
        "revision_not_found",
        "Исходный PGN для сохранённой ревизии недоступен",
    ))
}
#[cfg(test)]
mod source_reversion_tests {
    use super::*;
    #[test]
    fn repeated_source_payload_can_resolve_distinct_later_line_revision() {
        let (_temp, conn) = tests::setup();
        let first = tests::game("revert", "1. e4 e5");
        let second = tests::game("revert", "1. d4 d5");
        tests::save(&conn, &first);
        tests::save(&conn, &second);
        tests::save(&conn, &first);
        let count: i64 = conn
            .query_row("SELECT count(*) FROM statistics_payloads", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let ref3 = OccurrenceRef {
            game_id: first.id.clone(),
            revision: 3,
            ply: 1,
        };
        let ref1 = OccurrenceRef {
            revision: 1,
            ..ref3.clone()
        };
        let later = resolve_occurrence(&conn, &ref3).unwrap();
        let earlier = resolve_occurrence(&conn, &ref1).unwrap();
        assert_eq!(later.raw_pgn, first.raw_pgn);
        assert_eq!(earlier.raw_pgn, later.raw_pgn);
        assert_ne!(later.reference, earlier.reference);
        assert_eq!(later.line, vec!["e2e4"]);
        assert_eq!(later.current_revision, 3);
    }
}

#[cfg(test)]
mod canonical_review_tests {
    use super::*;
    #[test]
    fn review_grouping_reuses_position_identity_despite_fen_move_counters() {
        let (_temp, conn) = tests::setup();
        let a = tests::game("clockIdentityA", "1. e4 e5");
        let b = tests::game(
            "clockIdentityB",
            "[FEN \"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 28 7\"]\n[SetUp \"1\"]\n\n7. e4 e5",
        );
        assert_ne!(a.replay.nodes[0].fen, b.replay.nodes[0].fen);
        assert_eq!(
            a.replay.nodes[0].position_key,
            b.replay.nodes[0].position_key
        );
        let mut refs = Vec::new();
        for (i, game) in [a, b].iter().enumerate() {
            tests::save(&conn, game);
            let run = EngineRun {
                id: format!("canonicalReview{i}"),
                game_id: game.id.clone(),
                profile_id: "canonicalProfile".into(),
                evaluations: vec![PositionEvaluation {
                    ply: 1,
                    cp: Some(30),
                    complete: true,
                    ..Default::default()
                }],
                ..Default::default()
            };
            conn.execute("INSERT INTO statistics_analysis_runs(id,game_id,movetext_revision,profile_id,state,data_json,completed_at_ms) VALUES(?1,?2,1,?3,'succeeded',?4,1000)",params![run.id,run.game_id,run.profile_id,serde_json::to_string(&run).unwrap()]).unwrap();
            refs.push(GraphReviewReference {
                game_id: game.id.clone(),
                ply: 1,
                run_id: run.id,
                ..Default::default()
            });
        }
        let graph = query(
            &conn,
            &GraphRequest {
                all_paths: false,
                review_refs: refs,
                ..tests::request()
            },
        )
        .unwrap();
        assert_eq!(graph.review_refs.len(), 2);
        assert_eq!(
            graph.review_refs[0].position_key,
            graph.review_refs[1].position_key
        );
        assert_eq!(
            graph.review_refs[0].played_uci,
            graph.review_refs[1].played_uci
        );
        assert_eq!(graph.review_refs[0].played_uci.as_deref(), Some("e2e4"));
        assert_eq!(graph.review_refs[0].revision, Some(1));
    }
}
