pub mod application;
pub mod chess_domain;
#[cfg(feature = "desktop")]
pub mod desktop;
pub mod domain;
pub mod filters;
pub mod legacy_import;
pub mod parser;
pub mod pgn;
pub mod storage;

mod credentials;
pub mod lichess;
pub mod statistics;

pub mod game_graph;
#[cfg(feature = "desktop")]
pub mod game_graph_desktop;
pub mod knowledge;
pub mod recent;

pub mod library;
