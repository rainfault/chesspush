//! Bounded start-page projection, independent of the full analytics report.
use crate::{
    domain::AppError,
    statistics::{store, types::StoredGame},
};
use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct RecentGame {
    pub id: String,
    pub account_id: String,
    pub timestamp_ms: Option<i64>,
    pub opponent: String,
    pub opponent_rating: Option<i32>,
    pub color: String,
    pub result: String,
    pub opening: Option<String>,
    pub analysed: bool,
}

pub fn list(conn: &Connection) -> Result<Vec<RecentGame>, AppError> {
    let selected = store::active_account(conn)?.or(store::accounts(conn)?.into_iter().next());
    let Some(account) = selected else {
        return Ok(Vec::new());
    };
    let mut statement = conn.prepare(
        "SELECT sg.data, ag.user_color, EXISTS(SELECT 1 FROM statistics_analysis_runs r WHERE r.game_id=sg.id)
         FROM statistics_games sg JOIN statistics_account_games ag ON ag.game_id=sg.game_id
         WHERE ag.account_id=?1 AND ag.eligibility='eligible'
         AND json_extract(sg.data,'$.result') IN ('1-0','0-1','1/2-1/2')
         ORDER BY coalesce(json_extract(sg.data,'$.finished_at_ms'),json_extract(sg.data,'$.last_move_at_ms'),json_extract(sg.data,'$.started_at_ms')) DESC,sg.id LIMIT 5"
    )?;
    let rows = statement
        .query_map([&account.id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(data, color, analysed)| {
            let game: StoredGame = serde_json::from_str(&data)?;
            let opponent = if color == "white" {
                &game.black
            } else {
                &game.white
            };
            Ok(RecentGame {
                timestamp_ms: super::statistics::query::game_time(&game),
                opponent: opponent.name.clone(),
                opponent_rating: opponent.rating,
                opening: game.replay.opening.map(|o| o.name),
                id: game.id,
                account_id: account.id.clone(),
                color,
                result: game.result,
                analysed,
            })
        })
        .collect()
}
pub fn controls(conn: &Connection, account_id: Option<&str>) -> Result<Vec<String>, AppError> {
    let fallback = store::active_account(conn)?.or(store::accounts(conn)?.into_iter().next());
    let selected = account_id.or(fallback.as_ref().map(|a| a.id.as_str()));
    let mut statement=conn.prepare("SELECT DISTINCT json_extract(sg.data,'$.initial_seconds'),json_extract(sg.data,'$.increment_seconds') FROM statistics_games sg JOIN statistics_account_games ag ON ag.game_id=sg.game_id WHERE ag.account_id=?1 AND ag.eligibility='eligible' AND json_extract(sg.data,'$.result') IN ('1-0','0-1','1/2-1/2') AND json_extract(sg.data,'$.initial_seconds') IS NOT NULL AND json_extract(sg.data,'$.increment_seconds') IS NOT NULL ORDER BY 1,2")?;
    Ok(statement
        .query_map([selected], |r| {
            Ok(format!("{}+{}", r.get::<_, f64>(0)?, r.get::<_, f64>(1)?))
        })?
        .collect::<Result<Vec<String>, _>>()?)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::statistics::types::{Participant, Replay, ReplayNode};
    use rusqlite::params;
    #[test]
    fn bounded_home_projection_respects_membership_color_and_completed_cohort() {
        let temp = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(&temp.path().join("chesspush.sqlite3")).unwrap();
        conn.execute("INSERT INTO statistics_accounts(id,platform,external_id,username,t0_ms) VALUES('a','lichess','a','A',0),('b','lichess','b','B',0)",[]).unwrap();
        for i in 0..9 {
            let id = format!("g{i}");
            let g = StoredGame {
                id: id.clone(),
                result: if i == 8 { "*" } else { "1-0" }.into(),
                finished_at_ms: Some(i),
                initial_seconds: Some(600.),
                increment_seconds: Some(2.),
                white: Participant {
                    name: "White".into(),
                    ..Default::default()
                },
                black: Participant {
                    name: "Black".into(),
                    ..Default::default()
                },
                replay: Replay {
                    nodes: vec![ReplayNode::default(); 2],
                    ..Default::default()
                },
                ..Default::default()
            };
            conn.execute("INSERT INTO games(id,identity,metadata,tags,pgn,source,source_kind) VALUES(?1,?2,'{}','{}','','','local')",params![i+1,id]).unwrap();
            conn.execute("INSERT INTO statistics_games(game_id,id,source,external_id,data,movetext_hash,updated_at_ms) VALUES(?1,?2,'lichess',?2,?3,'hash',?1)",params![i+1,id,serde_json::to_string(&g).unwrap()]).unwrap();
            conn.execute("INSERT INTO statistics_account_games(account_id,game_id,user_color,eligibility,evidence) VALUES('a',?1,'black',?2,'{}')",params![i+1,if i==7 {"boundary_uncertain"}else{"eligible"}]).unwrap();
            conn.execute("INSERT INTO statistics_account_games(account_id,game_id,user_color,eligibility,evidence) VALUES('b',?1,'white','eligible','{}')",[i+1]).unwrap();
        }
        let rows = list(&conn).unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].id, "g6");
        assert!(rows.iter().all(|r| r.account_id == "a"
            && r.color == "black"
            && r.opponent == "White"
            && !r.analysed));
        assert_eq!(controls(&conn, Some("a")).unwrap(), vec!["600+2"]);
        assert_eq!(
            controls(&conn, Some("missing")).unwrap(),
            Vec::<String>::new()
        );
    }
}
