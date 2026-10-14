use crate::{
    domain::{AppError, Filters, Game, Tags},
    pgn::{integer, perf_type, tokens, value},
};

pub fn validate(f: &Filters) -> Result<(), AppError> {
    if ![
        f.min_white_elo,
        f.max_white_elo,
        f.min_black_elo,
        f.max_black_elo,
        f.min_both_elo,
        f.max_rating_diff,
    ]
    .iter()
    .all(|v| (0..=4000).contains(v))
        || f.min_white_elo > f.max_white_elo
        || f.min_black_elo > f.max_black_elo
        || f.min_moves > 1000
    {
        return Err(AppError::new(
            "validation",
            "Некорректный диапазон рейтинга или длина партии",
        ));
    }
    if !["any", "white", "black"].contains(&f.color.as_str())
        || ![
            "all",
            "selected_wins",
            "selected_losses",
            "wins_draws",
            "no_quick_losses",
        ]
        .contains(&f.result_mode.as_str())
        || ![
            "any",
            "",
            "rapid",
            "blitz",
            "classical",
            "bullet",
            "ultraBullet",
            "unknown",
        ]
        .contains(&f.perf_type.as_str())
    {
        return Err(AppError::new("validation", "Неизвестный фильтр"));
    }
    Ok(())
}
pub fn eco_matches(eco: &str, requested: &str) -> bool {
    let request = requested.trim().to_uppercase().replace(';', ",");
    let values: Vec<_> = request
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    values.is_empty()
        || values.iter().any(|v| {
            if v.len() >= 3
                && let Some((a, b)) = v.split_once('-')
            {
                if a.chars().next() != b.chars().next()
                    || a.chars().next() != eco.chars().next()
                    || a.is_empty()
                {
                    return false;
                }
                return match (
                    a.get(1..).unwrap_or("").parse::<i64>(),
                    b.get(1..).unwrap_or("").parse::<i64>(),
                    eco.get(1..).unwrap_or("").parse::<i64>(),
                ) {
                    (Ok(a), Ok(b), Ok(n)) => (a..=b).contains(&n),
                    _ => false,
                };
            }
            eco.to_uppercase().starts_with(v)
        })
}
fn score(result: &str, side: &str) -> f64 {
    match result {
        "1/2-1/2" => 0.5,
        "1-0" if side == "white" => 1.0,
        "0-1" if side == "black" => 1.0,
        _ => 0.0,
    }
}
fn side<'a>(white: &str, black: &str, f: &'a Filters) -> &'a str {
    if f.color == "any" && !f.username.is_empty() {
        if white.to_lowercase() == f.username.to_lowercase() {
            "white"
        } else if black.to_lowercase() == f.username.to_lowercase() {
            "black"
        } else {
            "unknown"
        }
    } else {
        &f.color
    }
}
fn result_matches(s: Option<f64>, mode: &str) -> bool {
    match mode {
        "selected_wins" => s == Some(1.0),
        "selected_losses" => s == Some(0.0),
        "wins_draws" => s == Some(1.0) || s == Some(0.5),
        _ => true,
    }
}
pub fn tags_match(t: &Tags, f: &Filters) -> bool {
    let perf = perf_type(value(t, "Event"), value(t, "TimeControl"));
    let w = integer(value(t, "WhiteElo"));
    let b = integer(value(t, "BlackElo"));
    if (f.exclude_bullet && ["bullet", "ultraBullet"].contains(&perf.as_str()))
        || (!f.perf_type.is_empty() && f.perf_type != "any" && f.perf_type != perf)
        || !(f.min_white_elo..=f.max_white_elo).contains(&w)
        || !(f.min_black_elo..=f.max_black_elo).contains(&b)
        || w.min(b) < f.min_both_elo
        || w.abs_diff(b) > f.max_rating_diff as u64
        || (!f.include_timeout
            && ["time forfeit", "timeout", "abandoned"]
                .contains(&value(t, "Termination").to_lowercase().as_str()))
        || !eco_matches(value(t, "ECO"), &f.eco)
        || !value(t, "Opening")
            .to_lowercase()
            .contains(&f.opening_contains.to_lowercase())
    {
        return false;
    }
    let s = side(value(t, "White"), value(t, "Black"), f);
    !["white", "black"].contains(&s)
        || result_matches(Some(score(value(t, "Result"), s)), &f.result_mode)
}
pub fn game_matches(g: &Game, f: &Filters, prefix: &[String]) -> bool {
    if g.move_count < f.min_moves || (!prefix.is_empty() && !tokens(&g.moves).starts_with(prefix)) {
        return false;
    }
    let s = side(&g.white, &g.black, f);
    if !["white", "black"].contains(&s) {
        return true;
    }
    let player = if s == "white" { &g.white } else { &g.black };
    let score = if player.is_empty() {
        None
    } else {
        Some(score(
            &g.result,
            if player.to_lowercase() == g.white.to_lowercase() {
                "white"
            } else {
                "black"
            },
        ))
    };
    result_matches(score, &f.result_mode)
        && !(f.result_mode == "no_quick_losses"
            && score == Some(0.0)
            && g.move_count < 25.max(f.min_moves))
}
