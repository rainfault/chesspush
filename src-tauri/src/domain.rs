use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Tags = BTreeMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Game {
    pub game_id: String,
    pub event: String,
    pub site: String,
    pub date: String,
    pub white: String,
    pub black: String,
    pub result: String,
    pub white_elo: i64,
    pub black_elo: i64,
    pub eco: String,
    pub opening: String,
    pub time_control: String,
    pub perf_type: String,
    pub termination: String,
    pub moves: String,
    pub move_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub metadata: Game,
    pub tags: Tags,
    pub raw_pgn: String,
}

// One game model. Purpose and provenance belong to the import, not subclasses.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    MyGames,
    LichessCorpus,
    Master,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Filters {
    pub color: String,
    pub result_mode: String,
    pub min_white_elo: i64,
    pub max_white_elo: i64,
    pub min_black_elo: i64,
    pub max_black_elo: i64,
    pub min_both_elo: i64,
    pub max_rating_diff: i64,
    pub eco: String,
    pub opening_contains: String,
    pub move_prefix: String,
    pub perf_type: String,
    pub min_moves: usize,
    pub exclude_bullet: bool,
    pub include_timeout: bool,
    pub username: String,
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            color: "any".into(),
            result_mode: "all".into(),
            min_white_elo: 0,
            max_white_elo: 4000,
            min_black_elo: 0,
            max_black_elo: 4000,
            min_both_elo: 0,
            max_rating_diff: 4000,
            eco: String::new(),
            opening_contains: String::new(),
            move_prefix: String::new(),
            perf_type: "any".into(),
            min_moves: 0,
            exclude_bullet: true,
            include_timeout: false,
            username: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    pub code: String,
    pub message: String,
}
impl AppError {
    pub fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
        }
    }
}
impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::new("io", e)
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("database", e)
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("data", e)
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AppError {}
