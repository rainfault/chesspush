use crate::{
    domain::{AppError, Filters, Record, SourceKind},
    parser::{self, Progress, ScanOptions},
    storage,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Deserialize)]
pub struct ParseRequest {
    pub source: PathBuf,
    pub filters: Filters,
    pub start_index: u64,
    pub limit: u64,
    #[serde(default)]
    pub import_all: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct ParseResult {
    pub progress: Progress,
    pub output: Option<String>,
    pub error: Option<AppError>,
    pub warning: Option<String>,
    pub elapsed_ms: u128,
}

pub fn run(
    request: ParseRequest,
    directory: &Path,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&Progress),
) -> Result<ParseResult, AppError> {
    crate::filters::validate(&request.filters)?;
    if !request.source.is_file()
        || !(1..=1_000_000).contains(&request.limit)
        || request.start_index > i64::MAX as u64
    {
        return Err(AppError::new(
            "validation",
            "Проверьте файл, лимит выборки и стартовый индекс",
        ));
    }
    let extension = request
        .source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !["pgn", "zst"].contains(&extension.as_str()) {
        return Err(AppError::new("validation", "Выберите PGN или PGN.ZST"));
    }
    let started = Instant::now();
    let mut conn = storage::open(&directory.join("chesspush.sqlite3"))?;
    let source_id = storage::remember(&conn, &request.source, "source")?;
    let exports = directory.join("exports");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut suffix = 0;
    let created = (|| -> Result<_, std::io::Error> {
        fs::create_dir_all(&exports)?;
        loop {
            let path = exports.join(format!(
                "{}_{}_{suffix}.pgn",
                if request.import_all {
                    "import"
                } else {
                    "model_games"
                },
                stamp
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(f) => return Ok((path, f)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => suffix += 1,
                Err(e) => return Err(e),
            }
        }
    })();
    let mut warning = None;
    let mut file_ids = vec![source_id];
    let (output, mut writer) = match created {
        Ok((output, file)) => {
            file_ids.push(storage::remember(
                &conn,
                &output,
                if request.import_all {
                    "import"
                } else {
                    "export"
                },
            )?);
            (Some(output), Some(BufWriter::new(file)))
        }
        Err(e) => {
            warning = Some(format!("Экспорт PGN: {e}"));
            (None, None)
        }
    };
    let mut batch: Vec<Record> = Vec::with_capacity(250);
    let mut batch_bytes = 0;
    let kind = if request.import_all {
        SourceKind::MyGames
    } else if extension == "zst" {
        SourceKind::LichessCorpus
    } else {
        SourceKind::Local
    };
    let mut progress = Progress::default();
    let mut last_emit = Instant::now();
    emit(&progress);
    let outcome = parser::scan(
        &request.source,
        ScanOptions {
            filters: if request.import_all {
                None
            } else {
                Some(&request.filters)
            },
            start_index: if request.import_all {
                0
            } else {
                request.start_index
            },
            scan_limit: None,
            match_limit: if request.import_all {
                u64::MAX
            } else {
                request.limit
            },
        },
        cancel,
        &mut progress,
        |p| {
            if last_emit.elapsed().as_millis() >= 100 {
                emit(p);
                last_emit = Instant::now();
            }
        },
        |_, record| {
            if let Some(w) = writer.as_mut()
                && let Err(e) = writeln!(w, "{}\n", record.raw_pgn.trim())
            {
                warning = Some(format!("Экспорт PGN: {e}"));
                writer = None;
            }
            batch.push(record.clone());
            batch_bytes += record.raw_pgn.len();
            if batch.len() == 250 || batch_bytes >= 2 * 1024 * 1024 {
                storage::save_batch(&mut conn, &batch, &file_ids, &request.source, kind)?;
                batch.clear();
                batch_bytes = 0;
            }
            Ok(())
        },
    );
    // Commit complete accepted records even after cancellation or corrupt input.
    let persisted = storage::save_batch(&mut conn, &batch, &file_ids, &request.source, kind);
    if let Some(w) = writer.as_mut()
        && let Err(e) = w.flush()
    {
        warning = Some(format!("Экспорт PGN: {e}"));
    }
    emit(&progress);
    Ok(ParseResult {
        progress,
        output: output.map(|p| p.to_string_lossy().into()),
        error: outcome.err().or_else(|| persisted.err()),
        warning,
        elapsed_ms: started.elapsed().as_millis(),
    })
}
