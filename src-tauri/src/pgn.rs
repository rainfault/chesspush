use crate::domain::{Game, Record, Tags};
use regex::Regex;
use std::sync::LazyLock;

static CLEANERS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"\{[^}]*\}",
        r"\([^)]*\)",
        r";[^\n]*",
        r"\$\d+",
        r"\b\d+\.(?:\.\.)?",
    ]
    .into_iter()
    .map(|s| Regex::new(s).unwrap())
    .collect()
});
pub fn clean_moves(moves: &str) -> String {
    CLEANERS.iter().fold(moves.to_string(), |s, re| {
        re.replace_all(&s, " ").into_owned()
    })
}
pub fn is_result(s: &str) -> bool {
    matches!(s, "1-0" | "0-1" | "1/2-1/2" | "*")
}
pub fn tokens(moves: &str) -> Vec<String> {
    clean_moves(moves)
        .split_whitespace()
        .filter(|s| !is_result(s))
        .map(|s| s.trim_end_matches(['!', '?', '+', '#']).to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}
fn move_count(moves: &str) -> usize {
    // Keep the reference's space-based counting (including its tab semantics).
    clean_moves(moves)
        .replace('\n', " ")
        .split(' ')
        .map(|s| s.trim().trim_end_matches(['!', '?', '+', '#']))
        .filter(|s| !s.is_empty() && !is_result(s))
        .count()
        / 2
}
pub fn integer(s: &str) -> i64 {
    s.trim().parse().unwrap_or(0)
}
pub fn perf_type(event: &str, tc: &str) -> String {
    let e = event.to_lowercase();
    for (needle, name) in [
        ("ultrabullet", "ultraBullet"),
        ("bullet", "bullet"),
        ("blitz", "blitz"),
        ("rapid", "rapid"),
        ("classical", "classical"),
    ] {
        if e.contains(needle) {
            return name.into();
        }
    }
    let (seconds, inc) = tc
        .split_once('+')
        .map(|(a, b)| {
            (
                integer(a),
                if a.trim().parse::<i64>().is_ok() {
                    integer(b)
                } else {
                    0
                },
            )
        })
        .unwrap_or_default();
    if seconds != 0 && seconds < 180 {
        "bullet"
    } else if seconds != 0 && seconds < 480 {
        "blitz"
    } else if seconds != 0 && seconds < 1500 {
        "rapid"
    } else if seconds != 0 || inc != 0 {
        "classical"
    } else {
        "unknown"
    }
    .into()
}
pub fn tags(raw: &str) -> Tags {
    let header = raw.split_once("\n\n").map_or(raw, |(h, _)| h);
    let mut tags = Tags::new();
    for line in header.lines().filter(|s| s.starts_with('[')) {
        if let Some(space) = line.find(' ')
            && space > 1
            && let Some(first) = line[space..].find('"').map(|i| i + space)
            && let Some(last) = line.rfind('"')
            && last > first
        {
            tags.insert(line[1..space].into(), line[first + 1..last].into());
        }
    }
    tags
}
pub fn value<'a>(t: &'a Tags, key: &str) -> &'a str {
    t.get(key).map(String::as_str).unwrap_or("")
}
fn game_id(site: &str) -> String {
    let Ok(url) = url::Url::parse(site.trim()) else {
        return String::new();
    };
    let host = url.host_str().unwrap_or("").to_lowercase();
    if host != "lichess.org" && !host.ends_with(".lichess.org") {
        return String::new();
    }
    let candidate = url.path().split('/').find(|s| !s.is_empty()).unwrap_or("");
    if (8..=12).contains(&candidate.len()) && candidate.bytes().all(|b| b.is_ascii_alphanumeric()) {
        candidate.into()
    } else {
        String::new()
    }
}
pub fn record(raw: &str, tags: Tags) -> Record {
    let normalized = raw.replace("\r\n", "\n");
    let mut in_moves = false;
    let mut lines = Vec::new();
    for line in normalized.split('\n') {
        if !in_moves && line.starts_with('[') {
            continue;
        }
        if !in_moves && line.trim().is_empty() {
            in_moves = true;
            continue;
        }
        if in_moves {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        lines = normalized
            .split('\n')
            .filter(|l| !l.starts_with('['))
            .collect();
    }
    let moves = lines.join("\n").trim().to_string();
    let get = |k| value(&tags, k).to_string();
    let mut id = game_id(value(&tags, "Site"));
    if id.is_empty() {
        id = game_id(value(&tags, "LichessURL"));
    }
    Record {
        metadata: Game {
            game_id: id,
            event: get("Event"),
            site: get("Site"),
            date: if tags.contains_key("Date") {
                get("Date")
            } else {
                get("UTCDate")
            },
            white: get("White"),
            black: get("Black"),
            result: tags.get("Result").cloned().unwrap_or("*".into()),
            white_elo: integer(value(&tags, "WhiteElo")),
            black_elo: integer(value(&tags, "BlackElo")),
            eco: get("ECO"),
            opening: get("Opening"),
            time_control: get("TimeControl"),
            perf_type: perf_type(value(&tags, "Event"), value(&tags, "TimeControl")),
            termination: get("Termination"),
            move_count: move_count(&moves),
            moves,
        },
        tags,
        raw_pgn: format!("{}\n", raw.trim()),
    }
}
