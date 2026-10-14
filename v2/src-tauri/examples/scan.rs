use chesspush_lib::{
    domain::Filters,
    parser::{Progress, ScanOptions, scan},
};
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::args().nth(1).ok_or("Usage: scan path.pgn.zst")?);
    let start = Instant::now();
    let mut p = Progress::default();
    scan(
        &path,
        ScanOptions {
            filters: Some(&Filters::default()),
            start_index: 0,
            scan_limit: None,
            match_limit: u64::MAX,
        },
        &AtomicBool::new(false),
        &mut p,
        |_| {},
        |_, _| Ok(()),
    )?;
    println!(
        "{}",
        serde_json::json!({"scanned":p.scanned,"matched":p.matched,"seconds":start.elapsed().as_secs_f64(),"games_per_second":p.scanned as f64/start.elapsed().as_secs_f64()})
    );
    Ok(())
}
