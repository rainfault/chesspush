//! Shared chess identity: a position is distinct from its historical occurrence.
use serde::{Deserialize, Serialize};
use shakmaty::{Chess, EnPassantMode, fen::Fen};
pub const INITIAL_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OccurrenceRef {
    pub game_id: String,
    pub revision: i64,
    pub ply: u32,
}
pub fn position_key(pos: &Chess) -> String {
    let fen = Fen::from_position(pos, EnPassantMode::Legal).to_string();
    format!(
        "standard|{}",
        fen.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
    )
}
