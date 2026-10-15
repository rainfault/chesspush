pub mod application;
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
