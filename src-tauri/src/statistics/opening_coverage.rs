//! Observed own-decision coverage, never a percentage of all chess opening theory.
//! Prepared choices use the existing current-revision KnowledgeDocument index;
//! observed choices remain occurrences, including repeated visits in one game.
use crate::domain::AppError;
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

pub const VERSION: &str = "observed-own-decisions-v1";
pub const SOURCE: &str = "active-attached-repertoire";

#[derive(Debug, Default, Clone, Copy)]
pub struct Counts {
    pub covered: usize,
    pub total: usize,
}
#[derive(Debug, Serialize)]
pub struct OpeningCoverage {
    pub covered_decisions: usize,
    pub total_decisions: usize,
    pub percent: Option<f64>,
    /// Games with at least one eligible observed own decision within the horizon.
    pub games: usize,
    /// Include decisions on moves 1..=depth_moves (both sides' absolute move number).
    pub depth_moves: u32,
    pub version: &'static str,
    pub source: &'static str,
    pub repertoire_documents: usize,
}
impl OpeningCoverage {
    pub fn summarize(
        game_ids: &[String],
        counts: &HashMap<String, Counts>,
        depth_moves: u32,
        repertoire_documents: usize,
    ) -> Self {
        let mut covered_decisions = 0;
        let mut total_decisions = 0;
        let mut games = 0;
        for count in game_ids.iter().filter_map(|id| counts.get(id)) {
            covered_decisions += count.covered;
            total_decisions += count.total;
            games += usize::from(count.total > 0);
        }
        Self {
            covered_decisions,
            total_decisions,
            percent: (total_decisions > 0)
                .then(|| 100. * covered_decisions as f64 / total_decisions as f64),
            games,
            depth_moves,
            version: VERSION,
            source: SOURCE,
            repertoire_documents,
        }
    }
}

/// Current source attachments of the selected side only. Duplicated files,
/// alternate paths and multiple identical RAV edges do not multiply coverage.
pub struct PreparedChoices {
    choices: HashSet<(String, String)>,
    pub documents: usize,
}
impl PreparedChoices {
    pub fn load(conn: &Connection, color: &str) -> Result<Self, AppError> {
        let documents = conn.query_row(
            "SELECT count(*) FROM knowledge_documents d
             WHERE d.kind='repertoire' AND d.side IN (?1,'both')
               AND json_extract(d.data_json,'$.header.metadata.repertoire_attachment.active')=1",
            [color],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT DISTINCT n.parent_position_key,n.uci
             FROM knowledge_node_index n JOIN knowledge_documents d ON d.id=n.document_id
             WHERE d.kind='repertoire' AND d.side IN (?1,'both') AND n.document_revision=d.revision
               AND json_extract(d.data_json,'$.header.metadata.repertoire_attachment.active')=1
               AND n.parent_position_key IS NOT NULL AND n.uci IS NOT NULL",
        )?;
        let choices = statement
            .query_map([color], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(Self { choices, documents })
    }

    /// Only compact facts from already legally replayed stored games are read.
    /// No full PGN/tree or engine result is copied across IPC, and this is read-only.
    pub fn by_game(
        &self,
        conn: &Connection,
        game_ids: &[String],
        color: &str,
        depth_moves: u32,
    ) -> Result<HashMap<String, Counts>, AppError> {
        if !(1..=40).contains(&depth_moves) || !matches!(color, "white" | "black") {
            return Err(AppError::new(
                "validation",
                "Некорректная глубина или сторона покрытия",
            ));
        }
        let ids =
            serde_json::to_string(game_ids).map_err(|error| AppError::new("statistics", error))?;
        let mut statement = conn.prepare(
            "SELECT sg.id,n.from_key,n.uci
             FROM json_each(?1) selected
             JOIN statistics_games sg ON sg.id=selected.value
             JOIN game_graph_games g ON g.game_id=sg.game_id AND g.revision=sg.movetext_revision
             JOIN game_occurrences n ON n.game_id=g.game_id AND n.revision=g.revision
             WHERE n.ply>0
               AND n.ply<=?3-json_extract(sg.data,'$.replay.nodes[0].absolute_ply')
               AND instr(n.from_key,CASE ?2 WHEN 'white' THEN ' w ' ELSE ' b ' END)>0
               AND n.uci IS NOT NULL
             UNION ALL
             SELECT sg.id,
                    json_extract(sg.data,'$.replay.nodes['||(cast(n.key AS INTEGER)-1)||'].position_key'),
                    json_extract(n.value,'$.uci')
             FROM json_each(?1) selected
             JOIN statistics_games sg ON sg.id=selected.value
             JOIN json_each(sg.data,'$.replay.nodes') n
             WHERE NOT EXISTS(SELECT 1 FROM game_graph_games g
                    WHERE g.game_id=sg.game_id AND g.revision=sg.movetext_revision)
               AND cast(n.key AS INTEGER)>0 AND json_extract(n.value,'$.actor')=?2
               AND json_extract(sg.data,'$.replay.nodes['||(cast(n.key AS INTEGER)-1)||'].absolute_ply')<?3
               AND json_extract(sg.data,'$.replay.nodes['||(cast(n.key AS INTEGER)-1)||'].position_key') LIKE 'standard|%'
               AND json_extract(n.value,'$.uci') IS NOT NULL",
        )?;
        let mut result: HashMap<String, Counts> = HashMap::new();
        for choice in statement.query_map(params![ids, color, depth_moves * 2], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (game, position, uci) = choice?;
            let count = result.entry(game).or_default();
            count.total += 1;
            count.covered += usize::from(self.choices.contains(&(position, uci)));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        knowledge,
        statistics::{chess, types::StoredGame},
    };
    use rusqlite::params;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(&dir.path().join("coverage.sqlite3")).unwrap();
        conn.execute("INSERT INTO statistics_accounts(id,platform,external_id,username,t0_ms) VALUES('a','lichess','a','A',0)", []).unwrap();
        (dir, conn)
    }
    fn game(conn: &Connection, id: &str, pgn: &str, color: &str) -> String {
        let replay = chess::replay_pgn(pgn).unwrap();
        let game = StoredGame {
            id: id.into(),
            own_color: color.into(),
            variant: "standard".into(),
            result: "1-0".into(),
            replay,
            ..Default::default()
        };
        conn.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES(?1,'{}','{}',?2,'','local')",params![id,pgn]).unwrap();
        let game_id = conn.last_insert_rowid();
        conn.execute("INSERT INTO statistics_games(game_id,id,source,external_id,data,movetext_hash,updated_at_ms) VALUES(?1,?2,'lichess',?2,?3,'hash',1)",params![game_id,id,serde_json::to_string(&game).unwrap()]).unwrap();
        conn.execute("INSERT INTO statistics_account_games(account_id,game_id,user_color,eligibility,evidence) VALUES('a',?1,?2,'eligible','{}')",params![game_id,color]).unwrap();
        id.into()
    }
    fn repertoire(
        conn: &Connection,
        side: &str,
        pgn: &str,
        active: bool,
    ) -> knowledge::KnowledgeDocument {
        let document = knowledge::import(
            conn,
            knowledge::ImportRequest {
                title: "Fixture".into(),
                side: side.into(),
                pgn: pgn.into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        if active {
            knowledge::fragments::set_attachment(conn, &document.id, document.revision, true)
                .unwrap()
        } else {
            document
        }
    }
    fn count(conn: &Connection, ids: &[String], color: &str, depth: u32) -> OpeningCoverage {
        let prepared = PreparedChoices::load(conn, color).unwrap();
        let before = conn.total_changes();
        let counts = prepared.by_game(conn, ids, color, depth).unwrap();
        let result = OpeningCoverage::summarize(ids, &counts, depth, prepared.documents);
        assert_eq!(conn.total_changes(), before);
        result
    }
    #[test]
    fn only_active_own_side_documents_cover_exact_position_and_played_move() {
        let (_dir, conn) = db();
        let ids = [game(
            &conn,
            "sicilian",
            "1.e4 c5 2.Nf3 d6 3.d4 cxd4 1-0",
            "white",
        )];
        assert_eq!(count(&conn, &ids, "white", 15).percent, Some(0.));
        let manual = repertoire(&conn, "white", "1.e4 e5 2.Nf3 Nc6 *", false);
        repertoire(&conn, "black", "1.e4 c5 2.Nf3 d6 3.d4 cxd4 *", true);
        assert_eq!(count(&conn, &ids, "white", 15).covered_decisions, 0);
        let attached =
            knowledge::fragments::set_attachment(&conn, &manual.id, manual.revision, true).unwrap();
        let coverage = count(&conn, &ids, "white", 15);
        // Nf3 exists in the white source, but after e5, not after c5.
        assert_eq!(
            (
                coverage.covered_decisions,
                coverage.total_decisions,
                coverage.repertoire_documents
            ),
            (1, 3, 1)
        );
        knowledge::fragments::set_attachment(&conn, &attached.id, attached.revision, false)
            .unwrap();
        assert_eq!(count(&conn, &ids, "white", 15).percent, Some(0.));
        assert_eq!(count(&conn, &[], "black", 15).percent, None);
    }
    #[test]
    fn weighted_occurrences_and_repeated_visits_are_not_unique_positions_or_game_average() {
        let (_dir, conn) = db();
        repertoire(&conn, "white", "1.e4 *", true);
        let ids = [
            game(&conn, "short", "1.e4 e5 1-0", "white"),
            game(&conn, "long", "1.e4 c5 2.Nf3 d6 3.d4 cxd4 1-0", "white"),
        ];
        let coverage = count(&conn, &ids, "white", 15);
        assert_eq!(
            (
                coverage.covered_decisions,
                coverage.total_decisions,
                coverage.games,
                coverage.percent
            ),
            (2, 4, 2, Some(50.))
        );
        repertoire(&conn, "white", "1.Nf3 *", true);
        let repeated = [game(
            &conn,
            "repeat",
            "1.Nf3 Nf6 2.Ng1 Ng8 3.Nf3 Nf6 4.e4 e5 1-0",
            "white",
        )];
        let coverage = count(&conn, &repeated, "white", 15);
        assert_eq!(
            (coverage.covered_decisions, coverage.total_decisions),
            (2, 4)
        );
        let short = count(&conn, &repeated, "white", 1);
        assert_eq!(
            (
                short.covered_decisions,
                short.total_decisions,
                short.percent
            ),
            (1, 1, Some(100.))
        );
    }
    #[test]
    fn transpositions_match_and_current_revision_replaces_old_choices() {
        let (_dir, conn) = db();
        let source = repertoire(&conn, "white", "1.d4 Nf6 2.Nf3 d5 3.c4 e6 *", true);
        let ids = [game(
            &conn,
            "transposed",
            "1.Nf3 d5 2.d4 Nf6 3.c4 e6 1-0",
            "white",
        )];
        let coverage = count(&conn, &ids, "white", 15);
        assert_eq!(
            (coverage.covered_decisions, coverage.total_decisions),
            (1, 3)
        );
        // Same piece placement with a different move counter is the same opening position.
        let fen = "rnbqkb1r/ppp1pppp/5n2/3p4/3P4/5N2/PPP1PPPP/RNBQKB1R w KQkq - 0 19";
        let fen_pgn = format!("[SetUp \"1\"]\n[FEN \"{fen}\"]\n\n19.c4 e6 1-0");
        let late = [game(&conn, "late-fen", &fen_pgn, "white")];
        assert_eq!(count(&conn, &late, "white", 15).percent, None);
        assert_eq!(count(&conn, &late, "white", 19).percent, Some(100.));
        knowledge::apply(
            &conn,
            &source.id,
            source.revision,
            knowledge::EditOperation::RemoveVariation {
                chapter_id: source.chapters[0].id.clone(),
                node_id: source.chapters[0].root.children[0].id.clone(),
            },
        )
        .unwrap();
        assert_eq!(count(&conn, &ids, "white", 15).covered_decisions, 0);
        assert!(
            PreparedChoices::load(&conn, "white")
                .unwrap()
                .by_game(&conn, &ids, "white", 0)
                .is_err()
        );
    }

    #[test]
    fn indexed_current_revision_and_unindexed_projection_have_identical_horizon_counts() {
        let (_dir, conn) = db();
        repertoire(&conn, "white", "1.e4 c5 2.Nf3 d6 *", true);
        let ids = [game(
            &conn,
            "indexed",
            "1.e4 c5 2.Nf3 d6 3.d4 cxd4 1-0",
            "white",
        )];
        let fallback = count(&conn, &ids, "white", 2);
        let (game_id, json): (i64, String) = conn
            .query_row(
                "SELECT game_id,data FROM statistics_games WHERE id='indexed'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let game: StoredGame = serde_json::from_str(&json).unwrap();
        crate::game_graph::index_game(&conn, game_id, &game, 1, 1, 1).unwrap();
        let indexed = count(&conn, &ids, "white", 2);
        assert_eq!(
            (
                fallback.covered_decisions,
                fallback.total_decisions,
                fallback.percent
            ),
            (
                indexed.covered_decisions,
                indexed.total_decisions,
                indexed.percent
            )
        );
        assert_eq!((indexed.covered_decisions, indexed.total_decisions), (2, 2));
        // A stale index must not suppress current-revision observed choices.
        conn.execute(
            "UPDATE statistics_games SET movetext_revision=2 WHERE game_id=?1",
            [game_id],
        )
        .unwrap();
        let stale = count(&conn, &ids, "white", 2);
        assert_eq!((stale.covered_decisions, stale.total_decisions), (2, 2));
    }
    #[test]
    fn black_choices_use_black_turn_and_duplicate_sources_do_not_multiply_numerator() {
        let (_dir, conn) = db();
        repertoire(&conn, "black", "1.e4 c5 2.Nf3 d6 *", true);
        repertoire(&conn, "black", "1.e4 c5 2.Nf3 d6 *", true);
        let ids = [game(
            &conn,
            "black",
            "1.e4 c5 2.Nf3 d6 3.d4 cxd4 1-0",
            "black",
        )];
        let coverage = count(&conn, &ids, "black", 15);
        assert_eq!(
            (
                coverage.covered_decisions,
                coverage.total_decisions,
                coverage.repertoire_documents
            ),
            (2, 3, 2)
        );
        assert_eq!(count(&conn, &ids, "black", 1).percent, Some(100.));
    }
    #[test]
    fn explicitly_attached_both_side_document_covers_each_side_without_mixing_outcomes() {
        let (_dir, conn) = db();
        repertoire(&conn, "both", "1.e4 c5 *", true);
        let white = [game(&conn, "white-both", "1.e4 c5 1-0", "white")];
        let black = [game(&conn, "black-both", "1.e4 c5 1-0", "black")];
        assert_eq!(count(&conn, &white, "white", 15).percent, Some(100.));
        assert_eq!(count(&conn, &black, "black", 15).percent, Some(100.));
    }
}
