//! Local Stockfish UCI worker. No engine input is accepted from a current online game.
use super::{
    chess, store,
    types::{EngineRun, MoveComparison, PositionEvaluation, StoredGame},
};
use crate::{domain::AppError, storage};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CONFIG_VERSION: &str = "stockfish-same-root-v1";
const MAX_UCI_LINE: u64 = 65536;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    pub path: String,
    pub threads: u32,
    pub hash_mb: u32,
    pub profile: String,
    pub custom_nodes: Option<u64>,
    pub custom_movetime_ms: Option<u64>,
    pub syzygy_path: Option<String>,
}
impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            path: String::new(),
            threads: 1,
            hash_mb: 128,
            profile: "standard".into(),
            custom_nodes: None,
            custom_movetime_ms: None,
            syzygy_path: None,
        }
    }
}
impl EngineConfig {
    fn budgets(&self) -> Result<(u64, u64), AppError> {
        let (nodes, time) = match self.profile.as_str() {
            "quick" => (250_000, 1_000),
            "standard" => (2_000_000, 5_000),
            "deep" => (20_000_000, 30_000),
            _ => return Err(error("Неизвестный профиль анализа")),
        };
        let nodes = self.custom_nodes.unwrap_or(nodes);
        let time = self.custom_movetime_ms.unwrap_or(time);
        if !(100..=2_000_000_000).contains(&nodes) || !(10..=300_000).contains(&time) {
            return Err(error("Недопустимый бюджет анализа"));
        }
        let cores = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1);
        if self.threads == 0
            || self.threads > cores.min(128)
            || !(16..=4096).contains(&self.hash_mb)
        {
            return Err(error(
                "Threads превышает число процессоров или Hash вне диапазона 16–4096 MiB",
            ));
        }
        Ok((nodes, time))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineNetwork {
    pub name: String,
    pub sha256: Option<String>,
    pub embedded: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineIdentity {
    pub name: String,
    pub author: String,
    pub binary_hash: String,
    pub networks: Vec<EngineNetwork>,
    pub options: BTreeMap<String, String>,
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tablebase_manifest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobStatus {
    pub id: i64,
    pub game_id: String,
    pub profile_id: String,
    pub state: String,
    pub progress: u32,
    pub total: u32,
    pub attempts: u32,
    pub last_error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineQueueStatus {
    pub configured: bool,
    pub paused: bool,
    pub identity: Option<EngineIdentity>,
    pub jobs: Vec<EngineJobStatus>,
}
fn error(message: impl ToString) -> AppError {
    AppError::new("engine", message)
}
fn interrupted() -> AppError {
    AppError::new("engine_interrupted", "Анализ приостановлен или отменён")
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hash_file(path: &Path) -> Result<String, AppError> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn configuration(conn: &Connection) -> Result<EngineConfig, AppError> {
    let value: Option<String> = conn.query_row(
        "SELECT config_json FROM statistics_engine_state WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    Ok(value
        .map(|v| serde_json::from_str(&v))
        .transpose()?
        .unwrap_or_default())
}
pub fn validate_config(config: &EngineConfig) -> Result<EngineIdentity, AppError> {
    config.budgets()?;
    let path = Path::new(&config.path);
    if !path.is_absolute() || !path.is_file() {
        return Err(error(
            "Укажите абсолютный путь к исполняемому файлу Stockfish",
        ));
    }
    let tablebase_manifest = config
        .syzygy_path
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(super::engine_metrics::tablebase_manifest)
        .transpose()
        .map_err(error)?;
    let binary_hash = hash_file(path)?;
    let mut engine = UciProcess::start(path)?;
    let (name, author, available) = engine.handshake()?;
    if !name.to_ascii_lowercase().contains("stockfish") {
        return Err(error("Для статистики требуется Stockfish с UCI_ShowWDL"));
    }
    for key in ["Threads", "Hash", "MultiPV", "UCI_ShowWDL"] {
        if !available.contains_key(key) {
            return Err(error(format!("Движок не поддерживает {key}")));
        }
    }
    let mut options = BTreeMap::new();
    for (key, value) in [
        ("Threads", config.threads.to_string()),
        ("Hash", config.hash_mb.to_string()),
        ("MultiPV", "1".into()),
        ("UCI_ShowWDL", "true".into()),
        ("UCI_Chess960", "false".into()),
        ("UCI_LimitStrength", "false".into()),
        ("Skill Level", "20".into()),
        ("Ponder", "false".into()),
        ("SyzygyPath", String::new()),
    ] {
        if available.contains_key(key) {
            options.insert(key.into(), value);
        }
    }
    let mut networks = Vec::new();
    for key in ["EvalFile", "EvalFileSmall"] {
        if let Some(line) = available.get(key) {
            let default = line
                .split(" default ")
                .nth(1)
                .unwrap_or("")
                .split(" min ")
                .next()
                .unwrap_or("")
                .trim();
            if !default.is_empty() && default != "<empty>" {
                let external = path.parent().unwrap_or(Path::new(".")).join(default);
                // Stockfish defaults embed its networks. Resolve a neighbouring override explicitly,
                // making the effective network independent of the application's working directory.
                let sha256 = if external.is_file() {
                    options.insert(key.into(), external.to_string_lossy().into());
                    Some(hash_file(&external)?)
                } else {
                    None
                };
                networks.push(EngineNetwork {
                    name: default.into(),
                    embedded: sha256.is_none(),
                    sha256,
                });
            }
        }
    }
    engine.configure(&options)?;
    let (probe, _) = engine.search(chess::INITIAL_FEN, &[], "white", 0, None, 2000, 250, || {
        false
    })?;
    if !probe.complete || probe.wdl.is_none() || probe.cp.is_none() {
        return Err(error(
            "Stockfish не возвращает пригодные CP/WDL: проверьте версию и NNUE",
        ));
    }
    let mut identity = EngineIdentity {
        name,
        author,
        binary_hash,
        networks,
        options,
        profile_id: String::new(),
        tablebase_manifest,
    };
    let (nodes, movetime) = config.budgets()?;
    identity.profile_id = digest(
        serde_json::to_string(&(
            CONFIG_VERSION,
            &identity,
            config.profile.as_str(),
            nodes,
            movetime,
            "clear-hash-per-query",
            "syzygy-disabled",
        ))?
        .as_bytes(),
    );
    Ok(identity)
}
pub fn save_configuration(
    conn: &Connection,
    config: &EngineConfig,
) -> Result<EngineIdentity, AppError> {
    let identity = validate_config(config)?;
    conn.execute(
        "UPDATE statistics_engine_state SET config_json=?1,identity_json=?2 WHERE id=1",
        params![
            serde_json::to_string(config)?,
            serde_json::to_string(&identity)?
        ],
    )?;
    Ok(identity)
}
pub fn queue_status(conn: &Connection) -> Result<EngineQueueStatus, AppError> {
    let (paused, raw): (bool, Option<String>) = conn.query_row(
        "SELECT paused,identity_json FROM statistics_engine_state WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let jobs=conn.prepare("SELECT id,game_id,profile_id,state,progress,total,attempts,last_error FROM statistics_analysis_jobs ORDER BY CASE state WHEN 'running' THEN 0 WHEN 'queued' THEN 1 WHEN 'paused' THEN 2 ELSE 3 END,id DESC LIMIT 250")?.query_map([],|r|Ok(EngineJobStatus{id:r.get(0)?,game_id:r.get(1)?,profile_id:r.get(2)?,state:r.get(3)?,progress:r.get(4)?,total:r.get(5)?,attempts:r.get(6)?,last_error:r.get(7)?}))?.collect::<Result<Vec<_>,_>>()?;
    let identity = raw.map(|s| serde_json::from_str(&s)).transpose()?;
    Ok(EngineQueueStatus {
        configured: identity.is_some(),
        paused,
        identity,
        jobs,
    })
}
pub fn set_paused(conn: &Connection, paused: bool) -> Result<(), AppError> {
    conn.execute(
        "UPDATE statistics_engine_state SET paused=?1 WHERE id=1",
        [paused],
    )?;
    if !paused {
        conn.execute("UPDATE statistics_analysis_jobs SET state='queued',not_before_ms=0 WHERE state='paused'",[])?;
    }
    Ok(())
}
pub fn cancel_job(conn: &Connection, id: i64) -> Result<(), AppError> {
    conn.execute("UPDATE statistics_analysis_jobs SET state='cancelled' WHERE id=?1 AND state IN ('queued','running','paused','retryable_failed')",[id])?;
    Ok(())
}
pub fn recover_jobs(conn: &Connection) -> Result<(), AppError> {
    conn.execute("UPDATE statistics_analysis_jobs SET state='queued',heartbeat_ms=NULL WHERE state='running'",[])?;
    Ok(())
}
fn final_game(game: &StoredGame) -> bool {
    matches!(game.result.as_str(), "1-0" | "0-1" | "1/2-1/2")
        && matches!(
            game.status.as_str(),
            "mate" | "resign" | "stalemate" | "timeout" | "outoftime" | "draw"
        )
        && matches!(game.variant.as_str(), "standard" | "fromPosition")
        && game.replay.nodes.len() > 1
        && game.eligibility == "eligible"
}
pub fn enqueue(conn: &Connection, game_ids: &[String]) -> Result<usize, AppError> {
    enqueue_inner(conn, game_ids, false)
}
pub fn enqueue_force(conn: &Connection, game_ids: &[String]) -> Result<usize, AppError> {
    enqueue_inner(conn, game_ids, true)
}
fn enqueue_inner(conn: &Connection, game_ids: &[String], manual: bool) -> Result<usize, AppError> {
    let config = configuration(conn)?;
    let raw: Option<String> = conn.query_row(
        "SELECT identity_json FROM statistics_engine_state WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    let identity: EngineIdentity =
        serde_json::from_str(&raw.ok_or_else(|| error("Сначала настройте Stockfish"))?)?;
    let mut added = 0;
    for id in game_ids {
        let game = store::game(conn, id)?.ok_or_else(|| error("Партия не найдена"))?;
        if !final_game(&game) {
            continue;
        }
        let revision: i64 = conn.query_row(
            "SELECT movetext_revision FROM statistics_games WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        let run_id = digest(format!("{id}:{revision}:{}", identity.profile_id).as_bytes());
        added+=conn.execute("INSERT INTO statistics_analysis_jobs(game_id,movetext_revision,profile_id,config_json,identity_json,total,run_id,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(game_id,movetext_revision,profile_id) DO UPDATE SET state='queued',not_before_ms=0,last_error=NULL,attempts=0 WHERE ?9 AND statistics_analysis_jobs.state IN ('cancelled','permanent_failed','retryable_failed','succeeded')",params![id,revision,identity.profile_id,serde_json::to_string(&config)?,serde_json::to_string(&identity)?,game.replay.nodes.len().saturating_mul(2).saturating_sub(1),run_id,now(),manual])?;
    }
    Ok(added)
}
pub fn get_run(
    conn: &Connection,
    game_id: &str,
    profile_id: Option<&str>,
) -> Result<Option<EngineRun>, AppError> {
    let raw:Option<String>=conn.query_row("SELECT r.data_json FROM statistics_analysis_runs r JOIN statistics_games g ON g.id=r.game_id AND g.movetext_revision=r.movetext_revision WHERE r.game_id=?1 AND r.state='succeeded' AND (?2 IS NULL OR r.profile_id=?2) ORDER BY r.completed_at_ms DESC LIMIT 1",params![game_id,profile_id],|r|r.get(0)).optional()?;
    Ok(raw.map(|s| serde_json::from_str(&s)).transpose()?)
}
pub fn get_evaluations(
    conn: &Connection,
    game_id: &str,
    profile_id: Option<&str>,
) -> Result<Vec<PositionEvaluation>, AppError> {
    Ok(get_run(conn, game_id, profile_id)?
        .map(|r| r.evaluations)
        .unwrap_or_default())
}
pub fn runs(conn: &Connection) -> Result<Vec<EngineRun>, AppError> {
    let raws=conn.prepare("SELECT r.data_json FROM statistics_analysis_runs r JOIN statistics_games g ON g.id=r.game_id AND g.movetext_revision=r.movetext_revision WHERE r.state='succeeded' ORDER BY r.completed_at_ms DESC")?.query_map([],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
    raws.into_iter()
        .map(|r| serde_json::from_str(&r).map_err(Into::into))
        .collect()
}
#[derive(Debug, Clone)]
struct Job {
    id: i64,
    game_id: String,
    revision: i64,
    config: EngineConfig,
    identity: EngineIdentity,
    run_id: String,
}
/// Called by exactly one dedicated worker thread. All UCI waits are bounded and interruptible.
pub fn worker_tick(db_path: &Path, shutdown: &AtomicBool) -> Result<bool, AppError> {
    if shutdown.load(Ordering::Relaxed) {
        return Ok(false);
    }
    let conn = storage::open(db_path)?;
    if conn.query_row(
        "SELECT paused FROM statistics_engine_state WHERE id=1",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(false);
    }
    let row:Option<(i64,String,i64,String,String,String)>=conn.query_row("SELECT id,game_id,movetext_revision,config_json,identity_json,run_id FROM statistics_analysis_jobs WHERE state IN ('queued','retryable_failed') AND not_before_ms<=?1 ORDER BY id LIMIT 1",[now()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let Some((id, game_id, revision, config, identity, run_id)) = row else {
        return Ok(false);
    };
    let job = Job {
        id,
        game_id,
        revision,
        config: serde_json::from_str(&config)?,
        identity: serde_json::from_str(&identity)?,
        run_id,
    };
    if conn.execute("UPDATE statistics_analysis_jobs SET state='running',attempts=attempts+1,heartbeat_ms=?2 WHERE id=?1 AND state IN ('queued','retryable_failed')",params![id,now()])?==0 {return Ok(false)}
    let result = process_job(&conn, &job, shutdown);
    if let Err(e) = result {
        let state: String = conn.query_row(
            "SELECT state FROM statistics_analysis_jobs WHERE id=?1",
            [id],
            |r| r.get(0),
        )?;
        if state == "cancelled" {
            return Ok(true);
        }
        if e.code == "engine_interrupted" {
            let paused: bool = conn.query_row(
                "SELECT paused FROM statistics_engine_state WHERE id=1",
                [],
                |r| r.get(0),
            )?;
            conn.execute(
                "UPDATE statistics_analysis_jobs SET state=?2,heartbeat_ms=NULL,attempts=max(0,attempts-1) WHERE id=?1",
                params![id, if paused { "paused" } else { "queued" }],
            )?;
        } else {
            let attempts: u32 = conn.query_row(
                "SELECT attempts FROM statistics_analysis_jobs WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            conn.execute("UPDATE statistics_analysis_jobs SET state=?2,last_error=?3,not_before_ms=?4,heartbeat_ms=NULL WHERE id=?1",params![id,if attempts>=3 {"permanent_failed"} else {"retryable_failed"},e.message,now()+30_000*(1i64<<attempts.min(8))])?;
        }
    }
    Ok(true)
}
fn should_interrupt(conn: &Connection, id: i64, shutdown: &AtomicBool) -> bool {
    shutdown.load(Ordering::Relaxed) || conn.query_row("SELECT s.paused OR j.state='cancelled' FROM statistics_engine_state s,statistics_analysis_jobs j WHERE s.id=1 AND j.id=?1",[id],|r|r.get::<_,bool>(0)).unwrap_or(true)
}
fn checkpoint(conn: &Connection, job: &Job, run: &EngineRun, state: &str) -> Result<(), AppError> {
    conn.execute("INSERT INTO statistics_analysis_runs(id,game_id,movetext_revision,profile_id,state,data_json,completed_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET state=excluded.state,data_json=excluded.data_json,completed_at_ms=excluded.completed_at_ms",params![run.id,run.game_id,job.revision,run.profile_id,state,serde_json::to_string(run)?,if run.completed_at_ms>0 {Some(run.completed_at_ms)} else {None}])?;
    let base: i64 = conn.query_row(
        "SELECT json_array_length(data,'$.replay.nodes')*2-1 FROM statistics_games WHERE id=?1",
        [&job.game_id],
        |r| r.get(0),
    )?;
    let extra = run
        .confirmed_moves
        .iter()
        .filter(|c| {
            let sign = if c.best.root_pov == "black" { -1 } else { 1 };
            c.complete
                && c.best.mate.is_some_and(|m| m * sign > 0)
                && c.played.mate.is_none_or(|m| m * sign <= 0)
        })
        .count()
        * 2;
    conn.execute(
        "UPDATE statistics_analysis_jobs SET progress=?2,heartbeat_ms=?3,total=?4 WHERE id=?1",
        params![
            job.id,
            run.evaluations.len() + run.confirmed_moves.len() + 2 * run.mate_rechecks.len(),
            now(),
            base + extra as i64
        ],
    )?;
    Ok(())
}
fn process_job(conn: &Connection, job: &Job, shutdown: &AtomicBool) -> Result<(), AppError> {
    let game = store::game(conn, &job.game_id)?.ok_or_else(|| error("Партия удалена"))?;
    let revision: i64 = conn.query_row(
        "SELECT movetext_revision FROM statistics_games WHERE id=?1",
        [&job.game_id],
        |r| r.get(0),
    )?;
    if !final_game(&game) || revision != job.revision {
        return Err(error(
            "Партия не завершена, не прошла проверку или её ходы изменены",
        ));
    }
    // Re-check the persisted rule context rather than trusting a serialized DTO.
    let legal = chess::replay_pgn(&game.raw_pgn)
        .map_err(|e| error(format!("PGN не прошёл проверку: {e}")))?;
    if legal.initial_fen != game.replay.initial_fen
        || legal.nodes.len() != game.replay.nodes.len()
        || legal
            .nodes
            .iter()
            .zip(&game.replay.nodes)
            .any(|(a, b)| a.fen != b.fen || a.uci != b.uci)
    {
        return Err(error("Сохранённые позиции не соответствуют исходному PGN"));
    }
    if hash_file(Path::new(&job.config.path))? != job.identity.binary_hash {
        return Err(error(
            "Файл Stockfish изменился. Сохраните настройки и запустите новый профиль",
        ));
    }
    for network in &job.identity.networks {
        if let Some(expected) = &network.sha256 {
            let path = job
                .identity
                .options
                .get(
                    if job.identity.networks.first().map(|n| n.name.as_str())
                        == Some(network.name.as_str())
                    {
                        "EvalFile"
                    } else {
                        "EvalFileSmall"
                    },
                )
                .ok_or_else(|| error("Параметры NNUE изменились"))?;
            if &hash_file(Path::new(path))? != expected {
                return Err(error("NNUE изменился. Создайте новый профиль анализа"));
            }
        }
    }
    let current_manifest = job
        .config
        .syzygy_path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .map(super::engine_metrics::tablebase_manifest)
        .transpose()
        .map_err(error)?;
    if current_manifest != job.identity.tablebase_manifest {
        return Err(error(
            "Набор Syzygy изменился. Сохраните настройки нового профиля",
        ));
    }
    let mut engine = UciProcess::start(Path::new(&job.config.path))?;
    let (engine_name, _, _) = engine.handshake()?;
    if engine_name != job.identity.name {
        return Err(error("Идентификатор движка изменился"));
    }
    engine.configure(&job.identity.options)?;
    let raw: Option<String> = conn
        .query_row(
            "SELECT data_json FROM statistics_analysis_runs WHERE id=?1",
            [&job.run_id],
            |r| r.get(0),
        )
        .optional()?;
    let mut run = raw
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_else(|| EngineRun {
            id: job.run_id.clone(),
            game_id: job.game_id.clone(),
            profile: job.config.profile.clone(),
            profile_id: job.identity.profile_id.clone(),
            engine_version: job.identity.name.clone(),
            engine_binary_hash: job.identity.binary_hash.clone(),
            revision: job.revision.to_string(),
            settings_json: serde_json::to_string(&(&job.config, &job.identity, CONFIG_VERSION))
                .unwrap_or_default(),
            ..Default::default()
        });
    checkpoint(conn, job, &run, "running")?;
    let mut history = Vec::<String>::new();
    for (index, node) in game.replay.nodes.iter().enumerate() {
        if should_interrupt(conn, job.id, shutdown) {
            return Err(interrupted());
        }
        let eval = if let Some(eval) = run.evaluations.iter().find(|e| e.ply == node.ply) {
            eval.clone()
        } else {
            let (eval, _) = observation(
                conn,
                &mut engine,
                job,
                &game.replay.initial_fen,
                &history,
                &node.fen,
                node.ply,
                None,
                shutdown,
            )?;
            run.evaluations.push(eval.clone());
            checkpoint(conn, job, &run, "running")?;
            eval
        };
        if let Some(next) = game.replay.nodes.get(index + 1) {
            let played = next
                .uci
                .as_ref()
                .ok_or_else(|| error("Нет UCI-хода в проверенной партии"))?;
            if !run.confirmed_moves.iter().any(|m| m.ply == next.ply) {
                let (actual, played_key) = if eval.best_move.as_ref() == Some(played) {
                    (
                        eval.clone(),
                        cache_key(&job.identity, &game.replay.initial_fen, &history, None)?.0,
                    )
                } else {
                    observation(
                        conn,
                        &mut engine,
                        job,
                        &game.replay.initial_fen,
                        &history,
                        &node.fen,
                        node.ply,
                        Some(played),
                        shutdown,
                    )?
                };
                let mut complete = eval.complete && actual.complete;
                let sign = if node.fen.split_whitespace().nth(1) == Some("b") {
                    -1
                } else {
                    1
                };
                // A deeper restricted search can refute the unrestricted search. Keep the evidence,
                // but do not turn a materially negative raw loss into an artificial zero.
                if let (Some(best), Some(played_cp)) = (eval.cp, actual.cp)
                    && sign * (best - played_cp) < -15
                {
                    complete = false;
                }
                if let (Some(best), Some(played_wdl)) = (eval.wdl, actual.wdl) {
                    let q = |w: [u32; 3]| (w[0] as f64 + 0.5 * w[1] as f64) / 1000.;
                    if (sign as f64) * super::metrics::q_delta(q(best), q(played_wdl)) < -0.02 {
                        complete = false;
                    }
                }
                let comparison = MoveComparison {
                    ply: next.ply,
                    best: eval.clone(),
                    played: actual,
                    complete,
                    method: "same_root_searchmoves".into(),
                };
                run.confirmed_moves.push(comparison);
                let best_key =
                    cache_key(&job.identity, &game.replay.initial_fen, &history, None)?.0;
                conn.execute("INSERT OR REPLACE INTO statistics_move_evaluations(run_id,ply,best_cache_key,played_cache_key,status) VALUES(?1,?2,?3,?4,?5)",params![run.id,next.ply,best_key,played_key,if complete {"available"} else {"unstable"}])?;
                checkpoint(conn, job, &run, "running")?;
            }
            // Existing completed runs can acquire newly required observations without
            // rerunning their cached primary searches or changing the profile.
            if let Some(comparison) = run
                .confirmed_moves
                .iter()
                .find(|m| m.ply == next.ply)
                .cloned()
            {
                let before = run.mate_rechecks.len();
                let sign = if next.actor.as_deref() == Some("black") {
                    -1
                } else {
                    1
                };
                let mate_candidate = comparison.complete
                    && comparison.best.mate.is_some_and(|m| m * sign > 0)
                    && comparison.played.mate.is_none_or(|m| m * sign <= 0);
                if mate_candidate && !run.mate_rechecks.iter().any(|m| m.ply == next.ply) {
                    let mut check_job = job.clone();
                    let (nodes, time) = job.config.budgets()?;
                    check_job.config.custom_nodes =
                        Some(nodes.saturating_mul(2).min(2_000_000_000));
                    check_job.config.custom_movetime_ms = Some(time.saturating_mul(2).min(300_000));
                    check_job.identity.profile_id = digest(
                        serde_json::to_string(&(
                            &job.identity.profile_id,
                            "mate-loss-recheck-v1",
                            check_job.config.custom_nodes,
                            check_job.config.custom_movetime_ms,
                        ))?
                        .as_bytes(),
                    );
                    let (best, _) = observation(
                        conn,
                        &mut engine,
                        &check_job,
                        &game.replay.initial_fen,
                        &history,
                        &node.fen,
                        node.ply,
                        None,
                        shutdown,
                    )?;
                    let (played, _) = observation(
                        conn,
                        &mut engine,
                        &check_job,
                        &game.replay.initial_fen,
                        &history,
                        &node.fen,
                        node.ply,
                        Some(played),
                        shutdown,
                    )?;
                    run.mate_rechecks.push(MoveComparison {
                        ply: next.ply,
                        complete: best.complete && played.complete,
                        best,
                        played,
                        method: "same_root_mate_recheck_double_budget_v1".into(),
                    });
                }

                if run.mate_rechecks.len() != before {
                    checkpoint(conn, job, &run, "running")?;
                }
            }
            history.push(played.clone());
        }
    }
    if should_interrupt(conn, job.id, shutdown) {
        return Err(interrupted());
    }
    run.tablebases =
        super::engine_metrics::probe_tablebases(&game, job.config.syzygy_path.as_deref());
    if should_interrupt(conn, job.id, shutdown) {
        return Err(interrupted());
    }
    run.completed_at_ms = now();
    checkpoint(conn, job, &run, "succeeded")?;
    conn.execute("UPDATE statistics_analysis_jobs SET state='succeeded',progress=total,last_error=NULL,heartbeat_ms=NULL WHERE id=?1 AND state='running'",[job.id])?;
    Ok(())
}
fn cache_key(
    identity: &EngineIdentity,
    initial: &str,
    history: &[String],
    searchmove: Option<&str>,
) -> Result<(String, String), AppError> {
    let context = serde_json::to_string(&(
        CONFIG_VERSION,
        "standard",
        initial,
        history,
        searchmove,
        &identity.profile_id,
    ))?;
    Ok((digest(context.as_bytes()), context))
}
#[allow(clippy::too_many_arguments)]
fn observation(
    conn: &Connection,
    engine: &mut UciProcess,
    job: &Job,
    initial: &str,
    history: &[String],
    fen: &str,
    ply: u32,
    searchmove: Option<&str>,
    shutdown: &AtomicBool,
) -> Result<(PositionEvaluation, String), AppError> {
    let (key, context) = cache_key(&job.identity, initial, history, searchmove)?;
    let cached:Option<(String,String)>=conn.query_row("SELECT context_json,evaluation_json FROM statistics_engine_observations WHERE cache_key=?1",[&key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((canonical, raw)) = cached
        && canonical == context
    {
        let mut eval: PositionEvaluation = serde_json::from_str(&raw)?;
        eval.ply = ply;
        return Ok((eval, key));
    }
    let root_pov = if fen.split_whitespace().nth(1) == Some("b") {
        "black"
    } else {
        "white"
    };
    let (evaluation, raw) = if let Some(outcome) = chess::terminal_outcome(fen) {
        let wdl = match outcome.as_str() {
            "white" => [1000, 0, 0],
            "black" => [0, 0, 1000],
            _ => [0, 1000, 0],
        };
        (
            PositionEvaluation {
                ply,
                wdl: Some(wdl),
                score_kind: "terminal".into(),
                stop_reason: "terminal_rules".into(),
                root_pov: root_pov.into(),
                terminal_outcome: Some(outcome),
                complete: true,
                ..Default::default()
            },
            "terminal-rules".into(),
        )
    } else {
        let (nodes, time) = job.config.budgets()?;
        engine.search(
            initial,
            history,
            root_pov,
            ply,
            searchmove,
            nodes,
            time,
            || should_interrupt(conn, job.id, shutdown),
        )?
    };
    conn.execute("INSERT INTO statistics_engine_observations(cache_key,context_json,evaluation_json,raw_info,created_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(cache_key) DO UPDATE SET context_json=excluded.context_json,evaluation_json=excluded.evaluation_json,raw_info=excluded.raw_info,created_at_ms=excluded.created_at_ms",params![key,context,serde_json::to_string(&evaluation)?,raw,now()])?;
    Ok((evaluation, key))
}

struct UciProcess {
    child: Child,
    input: SyncSender<(String, SyncSender<Result<(), String>>)>,
    output: Receiver<Result<String, String>>,
}
impl UciProcess {
    fn start(path: &Path) -> Result<Self, AppError> {
        let mut command = Command::new(path);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(parent) = path.parent() {
            command.current_dir(parent);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| error(format!("Stockfish не запускается: {e}")))?;
        let mut input = child.stdin.take().ok_or_else(|| error("Нет UCI stdin"))?;
        let (writer_tx, writer_rx) =
            mpsc::sync_channel::<(String, SyncSender<Result<(), String>>)>(8);
        std::thread::spawn(move || {
            while let Ok((line, ack)) = writer_rx.recv() {
                let result = input
                    .write_all(line.as_bytes())
                    .and_then(|_| input.write_all(b"\n"))
                    .and_then(|_| input.flush())
                    .map_err(|e| e.to_string());
                let failed = result.is_err();
                let _ = ack.send(result);
                if failed {
                    break;
                }
            }
        });
        let output = child.stdout.take().ok_or_else(|| error("Нет UCI stdout"))?;
        let (tx, rx) = mpsc::sync_channel(128);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut bytes = Vec::new();
                let read = Read::by_ref(&mut reader)
                    .take(MAX_UCI_LINE + 1)
                    .read_until(b'\n', &mut bytes);
                match read {
                    Ok(0) => break,
                    Ok(_) if bytes.len() as u64 > MAX_UCI_LINE => {
                        let _ = tx.send(Err("Слишком длинная строка UCI".into()));
                        break;
                    }
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&bytes).trim().to_string();
                        if tx.send(Ok(line)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input: writer_tx,
            output: rx,
        })
    }
    fn send(&mut self, line: &str) -> Result<(), AppError> {
        self.send_with_timeout(line, HANDSHAKE_TIMEOUT)
    }
    fn send_with_timeout(&mut self, line: &str, timeout: Duration) -> Result<(), AppError> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.input
            .try_send((line.into(), tx))
            .map_err(|_| error("UCI stdin недоступен"))?;
        rx.recv_timeout(timeout)
            .map_err(|_| error("Истекло время записи UCI"))?
            .map_err(error)
    }
    fn line(&mut self, deadline: Instant) -> Result<Option<String>, AppError> {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(error("Истекло время ожидания UCI"));
        }
        match self
            .output
            .recv_timeout(left.min(Duration::from_millis(50)))
        {
            Ok(Ok(line)) => Ok(Some(line)),
            Ok(Err(e)) => Err(error(e)),
            Err(RecvTimeoutError::Timeout) => {
                if let Some(status) = self.child.try_wait()? {
                    return Err(error(format!("Stockfish завершился: {status}")));
                }
                Ok(None)
            }
            Err(RecvTimeoutError::Disconnected) => Err(error("Stockfish закрыл UCI")),
        }
    }
    fn until(&mut self, token: &str, timeout: Duration) -> Result<(), AppError> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(line) = self.line(deadline)?
                && line == token
            {
                return Ok(());
            }
        }
    }
    fn handshake(&mut self) -> Result<(String, String, BTreeMap<String, String>), AppError> {
        self.send("uci")?;
        let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        let mut name = String::new();
        let mut author = String::new();
        let mut options = BTreeMap::new();
        loop {
            if let Some(line) = self.line(deadline)? {
                if line == "uciok" {
                    if name.is_empty() {
                        return Err(error("UCI не вернул имя движка"));
                    }
                    return Ok((name, author, options));
                }
                if let Some(value) = line.strip_prefix("id name ") {
                    name = value.into()
                }
                if let Some(value) = line.strip_prefix("id author ") {
                    author = value.into()
                }
                if let Some(value) = line.strip_prefix("option name ")
                    && let Some((key, _)) = value.split_once(" type ")
                {
                    options.insert(key.into(), line);
                }
            }
        }
    }
    fn configure(&mut self, options: &BTreeMap<String, String>) -> Result<(), AppError> {
        for (key, value) in options {
            if key.contains(['\r', '\n']) || value.contains(['\r', '\n']) {
                return Err(error("Некорректная UCI-настройка"));
            }
            self.send(&format!("setoption name {key} value {value}"))?;
        }
        self.send("isready")?;
        self.until("readyok", HANDSHAKE_TIMEOUT)?;
        self.send("ucinewgame")?;
        self.send("isready")?;
        self.until("readyok", HANDSHAKE_TIMEOUT)
    }
    #[allow(clippy::too_many_arguments)]
    fn search(
        &mut self,
        initial: &str,
        history: &[String],
        pov: &str,
        ply: u32,
        searchmove: Option<&str>,
        nodes: u64,
        movetime: u64,
        mut cancel: impl FnMut() -> bool,
    ) -> Result<(PositionEvaluation, String), AppError> {
        if initial.contains(['\n', '\r'])
            || history.iter().any(|m| !valid_uci_move(m))
            || searchmove.is_some_and(|m| !valid_uci_move(m))
        {
            return Err(error("Некорректный контекст UCI"));
        }
        self.send("setoption name Clear Hash")?;
        self.send("isready")?;
        self.until("readyok", HANDSHAKE_TIMEOUT)?;
        self.send(&format!(
            "position fen {initial}{}",
            if history.is_empty() {
                String::new()
            } else {
                format!(" moves {}", history.join(" "))
            }
        ))?;
        let restriction = searchmove
            .map(|m| format!(" searchmoves {m}"))
            .unwrap_or_default();
        self.send(&format!(
            "go nodes {nodes} movetime {movetime}{restriction}"
        ))?;
        let deadline = Instant::now() + Duration::from_millis(movetime + 10_000);
        let mut last = None;
        let mut exact_by_move = BTreeMap::<String, PositionEvaluation>::new();
        let mut raw = String::new();
        loop {
            if cancel() {
                let _ = self.send("stop");
                let _ = self.until_bestmove(Duration::from_millis(700));
                return Err(interrupted());
            }
            let Some(line) = self.line(deadline)? else {
                continue;
            };
            if line.starts_with("info ") {
                if raw.len() + line.len() < 262144 {
                    raw.push_str(&line);
                    raw.push('\n');
                }
                if let Some(eval) = parse_info(&line, pov, ply) {
                    if eval.complete
                        && let Some(first) = eval.pv.first()
                    {
                        exact_by_move.insert(first.clone(), eval.clone());
                    }
                    if last
                        .as_ref()
                        .is_none_or(|previous: &PositionEvaluation| eval.depth >= previous.depth)
                    {
                        last = Some(eval)
                    }
                }
            }
            if let Some(bestmove) = line.strip_prefix("bestmove ") {
                let best = bestmove.split_whitespace().next().unwrap_or("");
                let mut eval = last.ok_or_else(|| error("Stockfish не вернул оценку позиции"))?;
                // A node/time cap can interrupt an aspiration search and leave a bound.
                // Use a completed exact iteration only when it chose the same final move.
                // The interrupted bound remains in raw_info; its score is never treated as exact.
                if eval.bound.is_some()
                    && let Some(exact) = exact_by_move.get(best)
                {
                    eval = exact.clone();
                    eval.stop_reason = "completed_iteration_before_bound".into();
                }
                if valid_uci_move(best) {
                    eval.best_move = Some(best.into());
                } else {
                    eval.complete = false;
                }
                if searchmove.is_some_and(|requested| Some(requested) != eval.best_move.as_deref())
                {
                    return Err(error("Stockfish нарушил ограничение searchmoves"));
                }
                return Ok((eval, raw));
            }
        }
    }
    fn until_bestmove(&mut self, timeout: Duration) -> Result<(), AppError> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(line) = self.line(deadline)?
                && line.starts_with("bestmove ")
            {
                return Ok(());
            }
        }
    }
}
impl Drop for UciProcess {
    fn drop(&mut self) {
        let _ = self.send_with_timeout("quit", Duration::from_millis(100));
        for _ in 0..10 {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn valid_uci_move(value: &str) -> bool {
    let b = value.as_bytes();
    (b.len() == 4 || b.len() == 5)
        && (b'a'..=b'h').contains(&b[0])
        && (b'1'..=b'8').contains(&b[1])
        && (b'a'..=b'h').contains(&b[2])
        && (b'1'..=b'8').contains(&b[3])
        && (b.len() == 4 || b"nbrq".contains(&b[4]))
}
fn parse_info(line: &str, pov: &str, ply: u32) -> Option<PositionEvaluation> {
    let tokens: Vec<_> = line.split_whitespace().collect();
    let value = |name: &str| {
        tokens
            .iter()
            .position(|&t| t == name)
            .and_then(|i| tokens.get(i + 1))
            .copied()
    };
    if value("multipv").is_some_and(|v| v != "1") {
        return None;
    }
    let score = tokens.iter().position(|&t| t == "score")?;
    let kind = *tokens.get(score + 1)?;
    let raw: i32 = tokens.get(score + 2)?.parse().ok()?;
    let sign = if pov == "black" { -1 } else { 1 };
    let mut eval = PositionEvaluation {
        ply,
        depth: value("depth").and_then(|v| v.parse().ok()).unwrap_or(0),
        nodes: value("nodes").and_then(|v| v.parse().ok()).unwrap_or(0),
        seldepth: value("seldepth").and_then(|v| v.parse().ok()),
        score_kind: kind.into(),
        time_ms: value("time").and_then(|v| v.parse().ok()).unwrap_or(0),
        stop_reason: "limits_or_engine".into(),
        root_pov: pov.into(),
        complete: true,
        ..Default::default()
    };
    match kind {
        "cp" => eval.cp = raw.checked_mul(sign),
        "mate" => {
            eval.mate = raw.checked_mul(sign);
            if raw == 0 {
                eval.stop_reason = "mate0_unresolved".into();
                eval.complete = false;
            }
        }
        _ => return None,
    }
    if tokens.contains(&"lowerbound") {
        eval.bound = Some(
            if sign == 1 {
                "lowerbound"
            } else {
                "upperbound"
            }
            .into(),
        );
    }
    if tokens.contains(&"upperbound") {
        eval.bound = Some(
            if sign == 1 {
                "upperbound"
            } else {
                "lowerbound"
            }
            .into(),
        );
    }
    if eval.bound.is_some() {
        eval.stop_reason = "bound".into();
        eval.complete = false;
    }
    if let Some(i) = tokens.iter().position(|&t| t == "wdl")
        && let (Some(w), Some(d), Some(l)) = (
            tokens.get(i + 1).and_then(|v| v.parse::<u32>().ok()),
            tokens.get(i + 2).and_then(|v| v.parse::<u32>().ok()),
            tokens.get(i + 3).and_then(|v| v.parse::<u32>().ok()),
        )
        && w <= 1000
        && d <= 1000
        && l <= 1000
        && w + d + l == 1000
    {
        eval.wdl = Some(if sign == 1 { [w, d, l] } else { [l, d, w] });
    }
    if let Some(i) = tokens.iter().position(|&t| t == "pv") {
        eval.pv = tokens[i + 1..]
            .iter()
            .take_while(|m| valid_uci_move(m))
            .map(|s| s.to_string())
            .collect();
    }
    Some(eval)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn info_converts_black_and_bounds_without_fabricated_mate_zero() {
        let e=parse_info("info depth 18 seldepth 24 multipv 1 score cp 125 lowerbound wdl 800 150 50 nodes 12345 pv e7e5 g1f3","black",4).unwrap();
        assert_eq!(e.cp, Some(-125));
        assert_eq!(e.wdl, Some([50, 150, 800]));
        assert_eq!(e.bound.as_deref(), Some("upperbound"));
        assert!(!e.complete);
        assert_eq!(e.pv, vec!["e7e5", "g1f3"]);
        let mate = parse_info("info depth 0 score mate 0 nodes 0", "black", 9).unwrap();
        assert_eq!(mate.mate, Some(0));
        assert!(!mate.complete);
        assert!(mate.terminal_outcome.is_none());
        assert!(parse_info("info depth 8 multipv 2 score cp 23", "white", 0).is_none());
    }
    #[test]
    fn cache_distinguishes_history_profile_and_restriction() {
        let a = EngineIdentity {
            name: "Stockfish".into(),
            author: String::new(),
            binary_hash: "abc".into(),
            networks: vec![],
            options: BTreeMap::new(),
            profile_id: "one".into(),
            tablebase_manifest: None,
        };
        let initial = "same FEN";
        let base = cache_key(&a, initial, &[], None).unwrap().0;
        assert_ne!(
            base,
            cache_key(
                &a,
                initial,
                &["g1f3".into(), "g8f6".into(), "f3g1".into(), "f6g8".into()],
                None
            )
            .unwrap()
            .0
        );
        assert_ne!(base, cache_key(&a, initial, &[], Some("e2e4")).unwrap().0);
        let mut b = a;
        b.profile_id = "two".into();
        assert_ne!(base, cache_key(&b, initial, &[], None).unwrap().0);
    }
    #[test]
    fn rejects_unfinished_unknown_cheat_aborted_and_empty_games() {
        let base = StoredGame {
            result: "1-0".into(),
            status: "resign".into(),
            variant: "standard".into(),
            eligibility: "eligible".into(),
            replay: super::super::types::Replay {
                nodes: vec![Default::default(), Default::default()],
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(final_game(&base));
        for status in [
            "started",
            "created",
            "aborted",
            "noStart",
            "cheat",
            "unknownFinish",
            "finished",
            "variantEnd",
        ] {
            assert!(
                !final_game(&StoredGame {
                    status: status.into(),
                    ..base.clone()
                }),
                "{status}"
            );
        }
        assert!(!final_game(&StoredGame {
            result: "*".into(),
            ..base.clone()
        }));
        assert!(!final_game(&StoredGame {
            replay: Default::default(),
            ..base.clone()
        }));
        assert!(!final_game(&StoredGame {
            variant: "chess960".into(),
            ..base.clone()
        }));
        assert!(!final_game(&StoredGame {
            eligibility: "boundary_uncertain".into(),
            ..base
        }));
    }
    #[test]
    fn resources_and_uci_inputs_are_bounded() {
        assert!(
            EngineConfig {
                threads: 0,
                ..Default::default()
            }
            .budgets()
            .is_err()
        );
        assert!(
            EngineConfig {
                hash_mb: 100_000,
                ..Default::default()
            }
            .budgets()
            .is_err()
        );
        assert!(!valid_uci_move("e2e4\nquit"));
        assert!(valid_uci_move("a7a8q"));
    }
    #[test]
    #[ignore = "Set CHESSPUSH_STOCKFISH to run the actual official engine"]
    fn real_stockfish_handshake_search_and_searchmoves() {
        let path = std::env::var("CHESSPUSH_STOCKFISH").expect("Stockfish test path");
        let config = EngineConfig {
            path,
            profile: "quick".into(),
            custom_nodes: Some(5000),
            custom_movetime_ms: Some(500),
            ..Default::default()
        };
        let identity = validate_config(&config).unwrap();
        assert!(identity.name.contains("Stockfish"));
        assert!(!identity.binary_hash.is_empty());
        let mut engine = UciProcess::start(Path::new(&config.path)).unwrap();
        engine.handshake().unwrap();
        engine.configure(&identity.options).unwrap();
        let initial = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
        let (best, _) = engine
            .search(initial, &[], "white", 0, None, 5000, 500, || false)
            .unwrap();
        assert!(best.complete, "{best:?}");
        assert!(best.cp.is_some());
        assert!(best.wdl.is_some());
        assert!(best.best_move.is_some());
        let (played, _) = engine
            .search(initial, &[], "white", 0, Some("a2a3"), 5000, 500, || false)
            .unwrap();
        assert_eq!(played.best_move.as_deref(), Some("a2a3"));
        assert!(played.complete, "{played:?}");
        let (black, _) = engine
            .search(
                initial,
                &["e2e4".into()],
                "black",
                1,
                None,
                5000,
                500,
                || false,
            )
            .unwrap();
        assert_eq!(black.root_pov, "black");
        assert!(black.wdl.is_some());
        assert_eq!(
            engine
                .search(initial, &[], "white", 0, None, 20_000_000, 30000, || true)
                .unwrap_err()
                .code,
            "engine_interrupted"
        );
    }
}

#[cfg(test)]
mod supervisor_tests {
    use super::*;
    use std::sync::OnceLock;
    fn fake_engine() -> &'static Path {
        static EXE: OnceLock<std::path::PathBuf> = OnceLock::new();
        EXE.get_or_init(|| {
            let dir = tempfile::tempdir().unwrap().keep();
            let exe = dir.join(if cfg!(windows) {
                "fake-uci.exe"
            } else {
                "fake-uci"
            });
            let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_uci.rs");
            let mut command = Command::new("rustc");
            command
                .arg("--edition=2024")
                .arg(source)
                .arg("-o")
                .arg(&exe);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }
            assert!(command.status().unwrap().success());
            exe
        })
    }
    fn fixture() -> UciProcess {
        let mut p = UciProcess::start(fake_engine()).unwrap();
        p.handshake().unwrap();
        p
    }
    #[test]
    fn deterministic_process_covers_cancel_crash_invalid_score_and_output_cap() {
        let mut p = fixture();
        p.send("setoption name TestMode value hang").unwrap();
        let began = Instant::now();
        let e = p
            .search(
                chess::INITIAL_FEN,
                &[],
                "white",
                0,
                None,
                1000,
                1000,
                || true,
            )
            .unwrap_err();
        assert_eq!(e.code, "engine_interrupted");
        assert!(began.elapsed() < Duration::from_secs(2));
        drop(p);
        let mut p = fixture();
        p.send("setoption name TestMode value crash").unwrap();
        assert!(
            p.search(
                chess::INITIAL_FEN,
                &[],
                "white",
                0,
                None,
                1000,
                1000,
                || false
            )
            .is_err()
        );
        drop(p);
        let mut p = fixture();
        p.send("setoption name TestMode value no-score").unwrap();
        assert!(
            p.search(
                chess::INITIAL_FEN,
                &[],
                "white",
                0,
                None,
                1000,
                1000,
                || false
            )
            .unwrap_err()
            .message
            .contains("оценку")
        );
        drop(p);
        let mut p = fixture();
        p.send("stopread").unwrap();
        let began = Instant::now();
        assert!(
            p.send_with_timeout(&"x".repeat(1_000_000), Duration::from_millis(100))
                .is_err()
        );
        drop(p);
        assert!(began.elapsed() < Duration::from_secs(3));
        let mut p = fixture();
        p.send("emitlong").unwrap();
        assert!(
            p.until("never", Duration::from_secs(2))
                .unwrap_err()
                .message
                .contains("длинная")
        );
    }
    pub(super) fn saved_game(conn: &Connection) -> StoredGame {
        conn.execute("INSERT INTO statistics_accounts(id,platform,external_id,username,t0_ms) VALUES('lichess:owner','lichess','owner','Owner',1)",[]).unwrap();
        let pgn = "[Event \"fixture\"]\n[Site \"https://lichess.org/fixture1\"]\n[White \"Owner\"]\n[Black \"Opponent\"]\n[Result \"0-1\"]\n\n1. f3 e5 2. g4 Qh4# 0-1\n";
        let game = StoredGame {
            id: "lichess:fixture1".into(),
            external_id: "fixture1".into(),
            account_id: "lichess:owner".into(),
            source: "lichess".into(),
            own_color: "white".into(),
            result: "0-1".into(),
            status: "mate".into(),
            variant: "standard".into(),
            eligibility: "eligible".into(),
            raw_pgn: pgn.into(),
            replay: chess::replay_pgn(pgn).unwrap(),
            ..Default::default()
        };
        store::save_game(conn, "lichess:owner", &game, "{}", "eligible").unwrap();
        game
    }
    #[test]
    fn durable_queue_resumes_cache_separates_profiles_and_preserves_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("queue.sqlite");
        let conn = storage::open(&path).unwrap();
        let game = saved_game(&conn);
        let config = EngineConfig {
            path: fake_engine().to_string_lossy().into(),
            profile: "quick".into(),
            custom_nodes: Some(1000),
            custom_movetime_ms: Some(100),
            ..Default::default()
        };
        let first = save_configuration(&conn, &config).unwrap();
        assert_eq!(enqueue(&conn, std::slice::from_ref(&game.id)).unwrap(), 1);
        assert_eq!(enqueue(&conn, std::slice::from_ref(&game.id)).unwrap(), 0);
        let job = queue_status(&conn).unwrap().jobs[0].id;
        set_paused(&conn, true).unwrap();
        assert!(!worker_tick(&path, &AtomicBool::new(false)).unwrap());
        set_paused(&conn, false).unwrap();
        cancel_job(&conn, job).unwrap();
        assert_eq!(enqueue(&conn, std::slice::from_ref(&game.id)).unwrap(), 0);
        assert_eq!(
            enqueue_force(&conn, std::slice::from_ref(&game.id)).unwrap(),
            1
        );
        // Simulate a crash after the lease was acquired. Startup recovers the durable row.
        conn.execute(
            "UPDATE statistics_analysis_jobs SET state='running',progress=2",
            [],
        )
        .unwrap();
        recover_jobs(&conn).unwrap();
        assert_eq!(queue_status(&conn).unwrap().jobs[0].state, "queued");
        // Pause after a committed root, while the child is searching the next request.
        let worker_path = path.clone();
        let worker = std::thread::spawn(move || worker_tick(&worker_path, &AtomicBool::new(false)));
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let progress: i64 = conn
                .query_row(
                    "SELECT progress FROM statistics_analysis_jobs WHERE id=?1",
                    [job],
                    |r| r.get(0),
                )
                .unwrap();
            if progress >= 3 {
                break;
            }
            assert!(Instant::now() < deadline, "worker failed to make progress");
            std::thread::sleep(Duration::from_millis(5));
        }
        set_paused(&conn, true).unwrap();
        assert!(worker.join().unwrap().unwrap());
        assert_eq!(queue_status(&conn).unwrap().jobs[0].state, "paused");
        assert!(get_run(&conn, &game.id, None).unwrap().is_none());
        let partial: i64 = conn
            .query_row(
                "SELECT count(*) FROM statistics_engine_observations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(partial > 0);
        set_paused(&conn, false).unwrap();
        assert!(worker_tick(&path, &AtomicBool::new(false)).unwrap());
        let run = get_run(&conn, &game.id, Some(&first.profile_id))
            .unwrap()
            .unwrap();
        assert_eq!(run.evaluations.len(), 5);
        assert_eq!(run.confirmed_moves.len(), 4);
        assert_eq!(
            run.evaluations.last().unwrap().terminal_outcome.as_deref(),
            Some("black")
        );
        assert_eq!(run.confirmed_moves[0].method, "same_root_searchmoves");
        assert_eq!(queue_status(&conn).unwrap().jobs[0].state, "succeeded");
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM statistics_engine_observations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(n >= 5);
        // Replaying the job uses committed observations, including completed restricted searches.
        conn.execute(
            "UPDATE statistics_analysis_jobs SET state='queued' WHERE id=?1",
            [job],
        )
        .unwrap();
        assert!(worker_tick(&path, &AtomicBool::new(false)).unwrap());
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM statistics_engine_observations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, count);
        let second = save_configuration(
            &conn,
            &EngineConfig {
                custom_nodes: Some(2000),
                ..config
            },
        )
        .unwrap();
        assert_ne!(first.profile_id, second.profile_id);
        assert_eq!(enqueue(&conn, std::slice::from_ref(&game.id)).unwrap(), 1);
        assert!(
            get_run(&conn, &game.id, Some(&second.profile_id))
                .unwrap()
                .is_none()
        );
        let mut active = game.clone();
        active.id = "lichess:active01".into();
        active.external_id = "active01".into();
        active.status = "started".into();
        active.result = "*".into();
        store::save_game(&conn, "lichess:owner", &active, "{}", "eligible").unwrap();
        assert_eq!(enqueue(&conn, &[active.id]).unwrap(), 0);
    }
}

#[cfg(test)]
mod real_pipeline_tests {
    use super::*;
    #[test]
    #[ignore = "Set CHESSPUSH_STOCKFISH to run the actual official engine"]
    fn real_stockfish_full_game_persists_and_feeds_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("real-engine.sqlite");
        let conn = storage::open(&path).unwrap();
        let game = super::supervisor_tests::saved_game(&conn);
        let config = EngineConfig {
            path: std::env::var("CHESSPUSH_STOCKFISH").unwrap(),
            profile: "quick".into(),
            custom_nodes: Some(25000),
            custom_movetime_ms: Some(500),
            ..Default::default()
        };
        let identity = save_configuration(&conn, &config).unwrap();
        assert_eq!(enqueue(&conn, std::slice::from_ref(&game.id)).unwrap(), 1);
        assert!(worker_tick(&path, &AtomicBool::new(false)).unwrap());
        let status = queue_status(&conn).unwrap();
        assert_eq!(status.jobs[0].state, "succeeded", "{status:?}");
        let run = get_run(&conn, &game.id, Some(&identity.profile_id))
            .unwrap()
            .unwrap();
        assert_eq!(run.evaluations.len(), 5);
        assert_eq!(run.confirmed_moves.len(), 4);
        assert_eq!(
            run.evaluations.last().unwrap().terminal_outcome.as_deref(),
            Some("black")
        );
        let metrics = super::super::metrics::game_metrics(&game, Some(&run));
        assert!(!metrics.decisions.is_empty(), "{run:?}");
        assert!(!metrics.loss_q.is_empty());
        assert!(!metrics.cp_losses.is_empty());
        assert!(metrics.accuracy.is_some());
        assert!(metrics.loss_q.iter().any(|l| *l > 0.));
        drop(conn);
        let reopened = storage::open(&path).unwrap();
        assert_eq!(
            get_run(&reopened, &game.id, Some(&identity.profile_id))
                .unwrap()
                .unwrap()
                .id,
            run.id
        );
        let raw_count:i64=reopened.query_row("SELECT count(*) FROM statistics_engine_observations WHERE raw_info LIKE '%score %'",[],|r|r.get(0)).unwrap();
        assert!(raw_count >= 4);
    }
}

#[cfg(test)]
mod real_mate_recheck {
    use super::*;
    #[test]
    #[ignore = "Set CHESSPUSH_STOCKFISH to run the actual official engine"]
    fn real_stockfish_rechecks_a_lost_mate_without_mixing_primary_profile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mate.sqlite");
        let conn = storage::open(&path).unwrap();
        conn.execute("INSERT INTO statistics_accounts(id,platform,external_id,username,t0_ms) VALUES('lichess:owner','lichess','owner','Owner',1)",[]).unwrap();
        let pgn = "[SetUp \"1\"]\n[FEN \"7k/8/5KQ1/8/8/8/8/8 w - - 0 1\"]\n[Result \"1/2-1/2\"]\n\n1. Qh7+ Kxh7 1/2-1/2";
        let game = StoredGame {
            id: "lichess:matecase".into(),
            external_id: "matecase".into(),
            account_id: "lichess:owner".into(),
            source: "lichess".into(),
            own_color: "white".into(),
            result: "1/2-1/2".into(),
            status: "draw".into(),
            variant: "fromPosition".into(),
            eligibility: "eligible".into(),
            raw_pgn: pgn.into(),
            replay: chess::replay_pgn(pgn).unwrap(),
            ..Default::default()
        };
        store::save_game(&conn, "lichess:owner", &game, "{}", "eligible").unwrap();
        let config = EngineConfig {
            path: std::env::var("CHESSPUSH_STOCKFISH").unwrap(),
            profile: "quick".into(),
            custom_nodes: Some(25000),
            custom_movetime_ms: Some(500),
            ..Default::default()
        };
        let identity = save_configuration(&conn, &config).unwrap();
        enqueue(&conn, std::slice::from_ref(&game.id)).unwrap();
        worker_tick(&path, &AtomicBool::new(false)).unwrap();
        let status = queue_status(&conn).unwrap();
        assert_eq!(status.jobs[0].state, "succeeded", "{status:?}");
        let run = get_run(&conn, &game.id, Some(&identity.profile_id))
            .unwrap()
            .unwrap();
        assert_eq!(run.confirmed_moves.len(), 2);
        assert_eq!(run.mate_rechecks.len(), 1, "{run:?}");
        assert!(run.mate_rechecks[0].complete);
        assert_eq!(run.profile_id, identity.profile_id);
        assert!(run.mate_rechecks[0].best.mate.is_some_and(|m| m > 0));
        assert!(run.mate_rechecks[0].played.mate.is_none());
        let mut report = super::super::types::StatisticsReport::default();
        super::super::engine_metrics::append(
            &mut report,
            &[&game],
            &std::collections::HashMap::from([(game.id.clone(), &run)]),
            &Default::default(),
        );
        assert_eq!(
            report
                .metrics
                .iter()
                .find(|m| m.id == "E05_mate")
                .unwrap()
                .value,
            Some(1.)
        );
        let mut old_run = run.clone();
        old_run.mate_rechecks.clear();
        conn.execute(
            "UPDATE statistics_analysis_runs SET data_json=?2 WHERE id=?1",
            params![run.id, serde_json::to_string(&old_run).unwrap()],
        )
        .unwrap();
        assert_eq!(enqueue(&conn, std::slice::from_ref(&game.id)).unwrap(), 0);
        assert_eq!(
            enqueue_force(&conn, std::slice::from_ref(&game.id)).unwrap(),
            1
        );
        assert!(worker_tick(&path, &AtomicBool::new(false)).unwrap());
        let upgraded = get_run(&conn, &game.id, Some(&identity.profile_id))
            .unwrap()
            .unwrap();
        assert_eq!(upgraded.mate_rechecks.len(), 1);
        assert_eq!(upgraded.evaluations[0].nodes, run.evaluations[0].nodes);
        let status = queue_status(&conn).unwrap();
        assert_eq!(status.jobs[0].progress, status.jobs[0].total);
    }
}
