use chesspush_lib::legacy_import;
use std::path::PathBuf;

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 4 {
        eprintln!(
            "Usage: cargo run --no-default-features --example migrate_legacy -- <legacy.sqlite3> <settings.json> <target.sqlite3> <backup-directory>"
        );
        std::process::exit(2);
    }
    match legacy_import::migrate(&args[0], &args[1], &args[2], &args[3]) {
        Ok(report) => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        Err(error) => {
            eprintln!("Migration failed: {error}");
            std::process::exit(1);
        }
    }
}
