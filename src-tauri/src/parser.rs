use crate::{
    domain::{AppError, Filters, Record},
    filters, pgn,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Progress {
    pub scanned: u64,
    pub matched: u64,
    pub cancelled: bool,
}

pub struct ScanOptions<'a> {
    pub filters: Option<&'a Filters>,
    pub start_index: u64,
    pub scan_limit: Option<u64>,
    pub match_limit: u64,
}

// Only a decompression buffer, one PGN record, and the current line are retained.
// Check cancellation between lines, including while skipping a long record.
pub struct PgnStream {
    reader: Box<dyn BufRead>,
    compressed: bool,
    pending: Option<String>,
    first_line: bool,
}
impl PgnStream {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let file = File::open(path)?;
        let compressed = path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("zst"));
        let reader: Box<dyn BufRead> = if compressed {
            let mut decoder = zstd::stream::read::Decoder::new(file)?;
            decoder.window_log_max(31)?;
            Box::new(BufReader::with_capacity(128 * 1024, decoder))
        } else {
            Box::new(BufReader::with_capacity(128 * 1024, file))
        };
        Ok(Self {
            reader,
            compressed,
            pending: None,
            first_line: true,
        })
    }
    pub fn next(&mut self, cancel: &AtomicBool) -> Result<Option<String>, AppError> {
        let mut raw = self.pending.take().unwrap_or_default();
        let mut blank = false;
        let mut bytes = Vec::new();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            bytes.clear();
            let read = self
                .reader
                .by_ref()
                .take((MAX_RECORD_BYTES + 1) as u64)
                .read_until(b'\n', &mut bytes)
                .map_err(|e| AppError::new("decode", e))?;
            if read == 0 {
                return Ok(raw.trim().starts_with('[').then(|| raw.trim().to_string()));
            }
            if bytes.len() > MAX_RECORD_BYTES {
                return Err(AppError::new(
                    "record_too_large",
                    "Строка PGN превышает 16 MiB",
                ));
            }
            let mut line = if self.compressed {
                String::from_utf8(bytes.clone()).map_err(|e| AppError::new("utf8", e))?
            } else {
                String::from_utf8_lossy(&bytes).into_owned()
            };
            if self.first_line {
                line = line.trim_start_matches('\u{feff}').to_string();
                self.first_line = false;
            }
            if line.ends_with("\r\n") {
                line.truncate(line.len() - 2);
                line.push('\n');
            }
            let boundary = if self.compressed {
                blank && line.starts_with('[')
            } else {
                line.starts_with("[Event ")
            };
            if boundary && !raw.trim().is_empty() {
                self.pending = Some(line);
                if raw.trim().starts_with('[') {
                    return Ok(Some(raw.trim().to_string()));
                }
                raw = self.pending.take().unwrap_or_default();
                blank = false;
                continue;
            }
            blank = line.trim().is_empty();
            if raw.len() + line.len() > MAX_RECORD_BYTES {
                return Err(AppError::new(
                    "record_too_large",
                    "Запись PGN превышает 16 MiB",
                ));
            }
            raw.push_str(&line);
        }
    }
}

pub fn scan(
    path: &Path,
    options: ScanOptions<'_>,
    cancel: &AtomicBool,
    progress: &mut Progress,
    mut on_progress: impl FnMut(&Progress),
    mut on_match: impl FnMut(u64, &Record) -> Result<(), AppError>,
) -> Result<(), AppError> {
    if let Some(f) = options.filters {
        filters::validate(f)?;
    }
    let prefix = options
        .filters
        .map(|f| pgn::tokens(&f.move_prefix))
        .unwrap_or_default();
    let source_limit = options
        .scan_limit
        .map(|n| options.start_index.saturating_add(n));
    if options.scan_limit == Some(0) || options.match_limit == 0 {
        return Ok(());
    }
    let mut stream = PgnStream::open(path)?;
    let result = (|| {
        while source_limit.is_none_or(|n| progress.scanned < n)
            && progress.matched < options.match_limit
        {
            let Some(raw) = stream.next(cancel)? else {
                break;
            };
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            progress.scanned += 1;
            // Reference reports before filtering the current record. Consumers throttle UI events.
            on_progress(progress);
            if progress.scanned <= options.start_index {
                continue;
            }
            let tags = pgn::tags(&raw);
            if options
                .filters
                .is_some_and(|f| !filters::tags_match(&tags, f))
            {
                continue;
            }
            let game = pgn::record(&raw, tags);
            if options
                .filters
                .is_some_and(|f| !filters::game_matches(&game.metadata, f, &prefix))
            {
                continue;
            }
            on_match(progress.scanned, &game)?;
            progress.matched += 1;
            if cancel.load(Ordering::Relaxed) {
                break;
            }
        }
        Ok(())
    })();
    progress.cancelled = cancel.load(Ordering::Relaxed);
    on_progress(progress);
    result
}
