//! Portable profile snapshots. The versioned SQLite database is authoritative;
//! managed PGNs and browser preferences travel with it, never OS credentials.
use crate::{domain::AppError, storage};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File, OpenOptions as FileOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAGIC: &[u8] = b"CHESSPUSH-PROFILE\0\x01";
const DATABASE: &str = "chesspush.sqlite3";
const FORMAT: u32 = 1;
const MAX_MANIFEST: u64 = 40 * 1024 * 1024;
const MAX_UI_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL: u64 = 64 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 100_000;
const RESTORED_UI: &str = ".profile-restored-ui.json";
const NOTICE: &str = ".profile-restore-notice.json";
const LAST_BACKUP: &str = ".profile-last-backup.json";
pub type UiState = BTreeMap<String, String>;

/// Held before opening this profile. The file lives beside its directory so
/// restoring that directory never depends on renaming an open lock file.
pub struct ProfileLock {
    _file: File,
    owner_path: PathBuf,
}
pub fn lock_profile(directory: &Path) -> Result<ProfileLock, AppError> {
    let (parent, name, _) = root_paths(directory)?;
    let path = parent.join(format!(".{name}.profile-lock"));
    if path.exists() {
        ensure_regular(&path)?;
    }
    let file = FileOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().map_err(|_| {
        AppError::new(
            "profile_in_use",
            "Этот профиль уже открыт в другом окне ChessPush",
        )
    })?;
    let owner_path = parent.join(format!(".{name}.profile-owner.json"));
    write_json(&owner_path, &std::process::id())?;
    Ok(ProfileLock {
        _file: file,
        owner_path,
    })
}
impl Drop for ProfileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.owner_path);
    }
}
pub fn owner_pid(directory: &Path) -> Result<Option<u32>, AppError> {
    let (parent, name, _) = root_paths(directory)?;
    let path = parent.join(format!(".{name}.profile-owner.json"));
    if !path.exists() {
        return Ok(None);
    }
    ensure_regular(&path)?;
    if fs::metadata(&path)?.len() > 32 {
        return Err(error("Повреждены сведения о процессе профиля"));
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSummary {
    pub format_version: u32,
    pub created_at_ms: i64,
    pub app_version: String,
    pub schema_version: i64,
    pub files: usize,
    pub bytes: u64,
    pub external_files: u64,
    pub counts: BTreeMap<String, u64>,
    pub sha256: String,
    pub path: String,
    pub credentials_included: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreparedRestore {
    pub summary: BackupSummary,
    pub safety_backup: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProfileStatus {
    pub last_backup: Option<BackupSummary>,
    pub restore_notice: Option<String>,
    pub pending_restore: bool,
}
#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    path: String,
    bytes: u64,
    sha256: String,
}
#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format_version: u32,
    app_version: String,
    created_at_ms: i64,
    schema_version: i64,
    source_root: String,
    external_files: u64,
    counts: BTreeMap<String, u64>,
    ui_state: UiState,
    entries: Vec<Entry>,
}
#[derive(Debug, Serialize, Deserialize)]
struct Pending {
    stage_name: String,
    archive_sha256: String,
    safety_backup: String,
}

fn error(message: impl ToString) -> AppError {
    AppError::new("profile", message)
}
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
fn portable_path(value: &str) -> String {
    // canonicalize() returns extended-length Windows paths; stored file-dialog
    // references normally do not. Both names must identify the same managed PGN.
    let value = if let Some(unc) = value.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{unc}")
    } else {
        value.strip_prefix("\\\\?\\").unwrap_or(value).to_owned()
    };
    value.replace('\\', "/")
}
fn path_key(value: &str) -> String {
    let normalized = portable_path(value);
    if cfg!(windows) || normalized.as_bytes().get(1) == Some(&b':') {
        normalized.to_lowercase()
    } else {
        normalized
    }
}
fn nonce() -> Result<String, AppError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| error("Не удалось создать временный файл"))?;
    Ok(bytes.iter().map(|value| format!("{value:02x}")).collect())
}
fn root_paths(directory: &Path) -> Result<(PathBuf, String, PathBuf), AppError> {
    let parent = directory
        .parent()
        .ok_or_else(|| error("Некорректный каталог профиля"))?;
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let name = directory
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| error("Некорректное имя профиля"))?
        .to_owned();
    let journal = parent.join(format!(".{name}.profile-restore.json"));
    Ok((parent, name, journal))
}
fn validate_ui(state: UiState) -> Result<UiState, AppError> {
    let mut bytes = 0_usize;
    for (key, value) in &state {
        let allowed = matches!(
            key.as_str(),
            "chesspush.workspace.v1"
                | "chesspush.repertoire.side"
                | "chesspush.repertoire.context"
                | "chesspush.practice.filters.v1"
        ) || key.starts_with("chesspush.knowledge.draft.")
            || key.starts_with("chesspush.knowledge.orphan.")
            || key.starts_with("chesspush.training.");
        if !allowed || key.len() > 512 || key.chars().any(char::is_control) {
            return Err(error("Недопустимый ключ настроек интерфейса"));
        }
        bytes = bytes
            .checked_add(key.len() + value.len())
            .ok_or_else(|| error("Слишком большой профиль интерфейса"))?;
        if bytes > MAX_UI_BYTES {
            return Err(error("Черновики интерфейса превышают 32 MiB"));
        }
    }
    Ok(state)
}
fn validate_entry(path: &str) -> Result<(), AppError> {
    if path.is_empty()
        || path.len() > 2048
        || path.split('/').count() > 32
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
        || !Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(error("Недопустимый путь в резервной копии"));
    }
    for part in path.split('/') {
        let name = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if part.is_empty()
            || part.ends_with(['.', ' '])
            || matches!(name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (name.len() == 4
                && (name.starts_with("COM") || name.starts_with("LPT"))
                && name.as_bytes()[3].is_ascii_digit())
        {
            return Err(error("Недопустимое имя файла в резервной копии"));
        }
    }
    if path != DATABASE
        && !(path.starts_with("exports/") && path.to_ascii_lowercase().ends_with(".pgn"))
    {
        return Err(error("Резервная копия содержит неподдерживаемый файл"));
    }
    Ok(())
}
fn ensure_regular(path: &Path) -> Result<(), AppError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(error("Ожидается обычный файл без символических ссылок"));
    }
    Ok(())
}
fn hash_file(path: &Path) -> Result<(u64, String), AppError> {
    ensure_regular(path)?;
    let mut input = File::open(path)?;
    let mut digest = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or_else(|| error("Слишком большой файл"))?;
        if total > MAX_TOTAL {
            return Err(error("Резервная копия превышает 64 GiB"));
        }
        digest.update(&buffer[..count]);
    }
    Ok((total, format!("{:x}", digest.finalize())))
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), AppError> {
    let temporary = path.with_extension(format!("{}.tmp", nonce()?));
    let result = (|| {
        let mut file = FileOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
struct Scratch {
    path: PathBuf,
    parent: PathBuf,
}
impl Scratch {
    fn create(parent: &Path, prefix: &str) -> Result<Self, AppError> {
        fs::create_dir_all(parent)?;
        let parent = parent.canonicalize()?;
        let path = parent.join(format!(".{prefix}.profile-work-{}", nonce()?));
        fs::create_dir(&path)?;
        Ok(Self { path, parent })
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Only the directory we created, under the canonical expected parent.
        if self.path.parent() == Some(self.parent.as_path())
            && fs::symlink_metadata(&self.path)
                .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
            && self
                .path
                .canonicalize()
                .is_ok_and(|path| path.parent() == Some(self.parent.as_path()))
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn read_only_database(path: &Path) -> Result<Connection, AppError> {
    ensure_regular(path)?;
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA query_only=ON;")?;
    Ok(conn)
}
fn validate_database(path: &Path, schema: i64) -> Result<(), AppError> {
    if schema < 1 || schema > storage::SCHEMA_VERSION {
        return Err(error(
            "Резервная копия создана неподдерживаемой версией ChessPush",
        ));
    }
    let conn = read_only_database(path)?;
    let actual: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if actual != schema {
        return Err(error("Версия базы не совпадает с описанием копии"));
    }
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(error("Резервная копия базы повреждена"));
    }
    if conn.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(error("Нарушена целостность связей в резервной копии"));
    }
    for table in ["games", "files", "settings"] {
        if !conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(error("Это не профиль ChessPush"));
        }
    }
    Ok(())
}
fn count(conn: &Connection, table: &str, condition: &str) -> Result<u64, AppError> {
    if !conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get::<_, bool>(0),
    )? {
        return Ok(0);
    }
    Ok(conn.query_row(
        &format!("SELECT count(*) FROM {table} {condition}"),
        [],
        |row| row.get(0),
    )?)
}
fn counts(conn: &Connection) -> Result<BTreeMap<String, u64>, AppError> {
    let mut result = BTreeMap::new();
    for (name, table, condition) in [
        ("games", "games", ""),
        ("knowledge", "knowledge_documents", "WHERE kind='study'"),
        (
            "repertoire",
            "knowledge_documents",
            "WHERE kind='repertoire' OR json_extract(data_json,'$.header.metadata.repertoire_attachment.active')=1",
        ),
        ("reviews", "repertoire_training_reviews", ""),
        ("sessions", "repertoire_training_sessions", ""),
    ] {
        result.insert(name.into(), count(conn, table, condition)?);
    }
    Ok(result)
}
fn copy_managed(
    directory: &Path,
    target: &Path,
    relative: &str,
    paths: &mut Vec<String>,
    copied_bytes: &mut u64,
) -> Result<(), AppError> {
    if relative.split('/').count() > 32 {
        return Err(error("Слишком глубокий каталог управляемых PGN"));
    }
    let from = directory.join(relative);
    if !from.exists() {
        return Ok(());
    }
    if fs::symlink_metadata(&from)?.file_type().is_symlink() {
        return Err(error(
            "Каталог управляемых PGN содержит символическую ссылку",
        ));
    }
    for item in fs::read_dir(from)? {
        let item = item?;
        let kind = item.file_type()?;
        if kind.is_symlink() {
            return Err(error(
                "Каталог управляемых PGN содержит символическую ссылку",
            ));
        }
        let name = item
            .file_name()
            .to_str()
            .ok_or_else(|| error("Некорректное имя файла PGN"))?
            .to_owned();
        let relative = format!("{relative}/{name}");
        if kind.is_dir() {
            copy_managed(directory, target, &relative, paths, copied_bytes)?;
        } else if kind.is_file() && relative.to_ascii_lowercase().ends_with(".pgn") {
            validate_entry(&relative)?;
            if paths.len() >= MAX_FILES {
                return Err(error("Слишком много файлов в профиле"));
            }
            *copied_bytes = copied_bytes
                .checked_add(item.metadata()?.len())
                .filter(|value| *value <= MAX_TOTAL)
                .ok_or_else(|| error("Профиль превышает 64 GiB"))?;
            let to = target.join(&relative);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(item.path(), to)?;
            paths.push(relative);
        }
    }
    Ok(())
}
fn summary(manifest: &Manifest, path: &Path, digest: String) -> BackupSummary {
    BackupSummary {
        format_version: manifest.format_version,
        created_at_ms: manifest.created_at_ms,
        app_version: manifest.app_version.clone(),
        schema_version: manifest.schema_version,
        files: manifest.entries.len(),
        bytes: manifest.entries.iter().map(|entry| entry.bytes).sum(),
        external_files: manifest.external_files,
        counts: manifest.counts.clone(),
        sha256: digest,
        path: path.to_string_lossy().into_owned(),
        credentials_included: false,
    }
}
pub fn backup(
    directory: &Path,
    destination: &Path,
    ui_state: UiState,
) -> Result<BackupSummary, AppError> {
    let ui_state = validate_ui(ui_state)?;
    if destination
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|value| !value.eq_ignore_ascii_case("chesspush"))
    {
        return Err(error("Выберите файл с расширением .chesspush"));
    }
    let directory = directory.canonicalize()?;
    let parent = destination
        .parent()
        .ok_or_else(|| error("Выберите каталог резервной копии"))?
        .canonicalize()?;
    let name = destination
        .file_name()
        .ok_or_else(|| error("Некорректное имя резервной копии"))?;
    let destination = parent.join(name);
    if destination.starts_with(&directory) {
        return Err(error("Сохраните резервную копию вне каталога профиля"));
    }
    if destination.exists() {
        ensure_regular(&destination)?;
    }
    let (profile_parent, name, _) = root_paths(&directory)?;
    let work = Scratch::create(&profile_parent, &name)?;
    let database = work.path.join(DATABASE);
    let conn = storage::open(&directory.join(DATABASE))?;
    conn.backup("main", &database, None)?;
    drop(conn);
    let snapshot = Connection::open(&database)?;
    // Legacy plaintext credentials are never copied into a portable archive.
    snapshot.execute_batch("PRAGMA secure_delete=ON; DELETE FROM settings WHERE key IN ('lichess_api_token','lichess_token');")?;
    if count(&snapshot, "legacy_rows", "")? > 0 {
        snapshot.execute("UPDATE legacy_rows SET payload=json_remove(payload,'$.lichess_api_token','$.lichess_token') WHERE table_name='app_settings'", [])?;
    }
    snapshot.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
    let schema: i64 = snapshot.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let counts = counts(&snapshot)?;
    let references = snapshot
        .prepare("SELECT path FROM files")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(snapshot);
    let mut paths = vec![DATABASE.to_owned()];
    let mut copied_bytes = fs::metadata(&database)?.len();
    copy_managed(
        &directory,
        &work.path,
        "exports",
        &mut paths,
        &mut copied_bytes,
    )?;
    paths.sort();
    let mut total = 0_u64;
    let mut entries = Vec::with_capacity(paths.len());
    for path in paths {
        let (bytes, sha256) = hash_file(&work.path.join(&path))?;
        total = total
            .checked_add(bytes)
            .filter(|value| *value <= MAX_TOTAL)
            .ok_or_else(|| error("Профиль превышает 64 GiB"))?;
        entries.push(Entry {
            path,
            bytes,
            sha256,
        });
    }
    let managed: HashSet<_> = entries
        .iter()
        .map(|entry| path_key(&directory.join(&entry.path).to_string_lossy()))
        .collect();
    let external_files = references
        .iter()
        .filter(|path| !managed.contains(&path_key(path)))
        .count() as u64;
    let manifest = Manifest {
        format_version: FORMAT,
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at_ms: now_ms(),
        schema_version: schema,
        source_root: portable_path(&directory.to_string_lossy()),
        external_files,
        counts,
        ui_state,
        entries,
    };
    validate_database(&database, schema)?;
    let temporary = parent.join(format!(".chesspush-backup-{}.tmp", nonce()?));
    let written = (|| -> Result<(), AppError> {
        let file = FileOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut encoder = zstd::stream::write::Encoder::new(file, 3)?;
        let encoded = serde_json::to_vec(&manifest)?;
        if encoded.len() as u64 > MAX_MANIFEST {
            return Err(error("Слишком большое описание профиля"));
        }
        encoder.write_all(MAGIC)?;
        encoder.write_all(&(encoded.len() as u64).to_le_bytes())?;
        encoder.write_all(&encoded)?;
        for entry in &manifest.entries {
            std::io::copy(&mut File::open(work.path.join(&entry.path))?, &mut encoder)?;
        }
        encoder.finish()?.sync_all()?;
        fs::rename(&temporary, &destination)?;
        Ok(())
    })();
    if let Err(reason) = written {
        let _ = fs::remove_file(&temporary);
        return Err(reason);
    }
    let (_, digest) = hash_file(&destination)?;
    let result = summary(&manifest, &destination, digest);
    write_json(&directory.join(LAST_BACKUP), &result)?;
    Ok(result)
}
fn extract(archive: &Path, target: &Path) -> Result<Manifest, AppError> {
    ensure_regular(archive)?;
    let mut input = zstd::stream::read::Decoder::new(File::open(archive)?)?;
    input.window_log_max(27)?;
    let mut magic = vec![0_u8; MAGIC.len()];
    input
        .read_exact(&mut magic)
        .map_err(|_| error("Это не резервная копия ChessPush"))?;
    if magic != MAGIC {
        return Err(error("Это не резервная копия ChessPush"));
    }
    let mut length = [0_u8; 8];
    input.read_exact(&mut length)?;
    let length = u64::from_le_bytes(length);
    if length == 0 || length > MAX_MANIFEST {
        return Err(error("Некорректное описание резервной копии"));
    }
    let mut encoded = vec![0_u8; length as usize];
    input.read_exact(&mut encoded)?;
    let manifest: Manifest = serde_json::from_slice(&encoded)?;
    if manifest.format_version != FORMAT
        || manifest.entries.is_empty()
        || manifest.entries.len() > MAX_FILES
        || manifest.app_version.len() > 128
        || manifest.source_root.len() > 32_768
        || manifest.source_root.chars().any(char::is_control)
    {
        return Err(error("Неподдерживаемый формат резервной копии"));
    }
    validate_ui(manifest.ui_state.clone())?;
    let mut names = HashSet::new();
    let mut total = 0_u64;
    for entry in &manifest.entries {
        validate_entry(&entry.path)?;
        total = total
            .checked_add(entry.bytes)
            .filter(|value| *value <= MAX_TOTAL)
            .ok_or_else(|| error("Резервная копия превышает 64 GiB"))?;
        if !names.insert(entry.path.to_lowercase())
            || entry.sha256.len() != 64
            || !entry.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(error(
                "Повторяющийся или повреждённый файл в резервной копии",
            ));
        }
    }
    if !names.contains(DATABASE) {
        return Err(error("В резервной копии нет базы данных"));
    }
    fs::create_dir_all(target)?;
    let mut buffer = [0_u8; 128 * 1024];
    for entry in &manifest.entries {
        let path = target.join(&entry.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = FileOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut remaining = entry.bytes;
        let mut digest = Sha256::new();
        while remaining > 0 {
            let length = (remaining.min(buffer.len() as u64)) as usize;
            input
                .read_exact(&mut buffer[..length])
                .map_err(|_| error("Резервная копия обрезана"))?;
            output.write_all(&buffer[..length])?;
            digest.update(&buffer[..length]);
            remaining -= length as u64;
        }
        output.sync_all()?;
        if format!("{:x}", digest.finalize()) != entry.sha256 {
            return Err(error("Контрольная сумма файла не совпадает"));
        }
    }
    if input.read(&mut [0_u8; 1])? != 0 {
        return Err(error("В резервной копии есть лишние данные"));
    }
    validate_database(&target.join(DATABASE), manifest.schema_version)?;
    let actual = counts(&read_only_database(&target.join(DATABASE))?)?;
    if actual != manifest.counts {
        return Err(error("Состав базы не совпадает с описанием копии"));
    }
    Ok(manifest)
}
pub fn preview(directory: &Path, archive: &Path) -> Result<BackupSummary, AppError> {
    let (parent, name, _) = root_paths(directory)?;
    let work = Scratch::create(&parent, &name)?;
    let before = hash_file(archive)?.1;
    let manifest = extract(archive, &work.path.join("candidate"))?;
    if before != hash_file(archive)?.1 {
        return Err(error("Файл резервной копии изменился во время проверки"));
    }
    Ok(summary(&manifest, archive, before))
}
fn stage_path(parent: &Path, profile_name: &str, pending: &Pending) -> Result<PathBuf, AppError> {
    let prefix = format!(".{profile_name}.profile-work-");
    if !pending.stage_name.starts_with(&prefix)
        || pending.stage_name.len() != prefix.len() + 32
        || !pending.stage_name[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(error("Некорректный журнал восстановления"));
    }
    let path = parent.join(&pending.stage_name);
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || path.canonicalize()?.parent() != Some(parent)
    {
        return Err(error("Некорректный каталог восстановления"));
    }
    Ok(path)
}
pub fn prepare_restore(
    directory: &Path,
    archive: &Path,
    expected: &str,
    current_ui_state: UiState,
) -> Result<PreparedRestore, AppError> {
    let (parent, name, journal) = root_paths(directory)?;
    if journal.exists() {
        return Err(error(
            "Восстановление уже подготовлено. Перезапустите ChessPush",
        ));
    }
    let digest = hash_file(archive)?.1;
    if digest != expected {
        return Err(error("Резервная копия изменилась. Выберите её заново"));
    }
    let work = Scratch::create(&parent, &name)?;
    let manifest = extract(archive, &work.path.join("candidate"))?;
    if digest != hash_file(archive)?.1 {
        return Err(error("Резервная копия изменилась во время проверки"));
    }
    let backups = parent.join(format!("{name}.backups"));
    fs::create_dir_all(&backups)?;
    let safety = backups.join(format!(
        "before-restore-{}-{}.chesspush",
        now_ms(),
        nonce()?
    ));
    backup(directory, &safety, current_ui_state)?;
    write_json(&work.path.join("manifest.json"), &manifest)?;
    let pending = Pending {
        stage_name: work
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| error("Некорректный каталог восстановления"))?
            .into(),
        archive_sha256: digest.clone(),
        safety_backup: safety.to_string_lossy().into_owned(),
    };
    write_json(&journal, &pending)?;
    let result = PreparedRestore {
        summary: summary(&manifest, archive, digest),
        safety_backup: pending.safety_backup,
    };
    // The durable journal now owns this staging directory until startup/cancel.
    std::mem::forget(work);
    Ok(result)
}
pub fn cancel_restore(directory: &Path) -> Result<(), AppError> {
    let (parent, name, journal) = root_paths(directory)?;
    if !journal.exists() {
        return Ok(());
    }
    let pending: Pending = serde_json::from_slice(&fs::read(&journal)?)?;
    let stage = stage_path(&parent, &name, &pending)?;
    if stage.join("previous").exists() {
        return Err(error("Замена профиля уже началась"));
    }
    fs::remove_file(journal)?;
    drop(Scratch {
        path: stage,
        parent,
    });
    Ok(())
}
pub fn has_pending(directory: &Path) -> Result<bool, AppError> {
    let (parent, name, journal) = root_paths(directory)?;
    if !journal.exists() {
        return Ok(false);
    }
    ensure_regular(&journal)?;
    if fs::metadata(&journal)?.len() > 16 * 1024 {
        return Err(error("Повреждён журнал восстановления"));
    }
    let pending: Pending = serde_json::from_slice(&fs::read(journal)?)?;
    let stage = stage_path(&parent, &name, &pending)?;
    if !stage.join("candidate").is_dir() {
        return Err(error("Подготовленный профиль недоступен"));
    }
    Ok(true)
}
fn remapped(value: &str, source: &str, destination: &Path, files: &HashSet<String>) -> String {
    let value_normalized = portable_path(value);
    let source = portable_path(source).trim_end_matches('/').to_owned();
    if value_normalized.len() <= source.len()
        || !value_normalized
            .get(..source.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(&source))
        || !value_normalized
            .get(source.len()..)
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        return value.into();
    }
    let relative = &value_normalized[source.len() + 1..];
    if !files.contains(relative) {
        return value.into();
    }
    destination.join(relative).to_string_lossy().into_owned()
}
fn remap_metadata(
    value: &mut serde_json::Value,
    source: &str,
    destination: &Path,
    files: &HashSet<String>,
) {
    if let Some(metadata) = value.pointer_mut("/header/metadata") {
        for key in ["source_path", "source_file", "path"] {
            if let Some(path) = metadata
                .get_mut(key)
                .and_then(|value| value.as_str().map(str::to_owned))
            {
                metadata[key] = remapped(&path, source, destination, files).into();
            }
        }
        if let Some(attachment) = metadata.get_mut("repertoire_attachment") {
            if let Some(path) = attachment.get("path").and_then(serde_json::Value::as_str) {
                attachment["path"] = remapped(path, source, destination, files).into();
            }
        }
    }
}
fn migrate_candidate(
    candidate: &Path,
    manifest: &Manifest,
    directory: &Path,
) -> Result<(), AppError> {
    let conn = storage::open(&candidate.join(DATABASE))?;
    let files: HashSet<_> = manifest
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect();
    let tx = conn.unchecked_transaction()?;
    let rows = tx
        .prepare("SELECT id,path FROM files")?
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, path) in rows {
        let updated = remapped(&path, &manifest.source_root, directory, &files);
        if updated != path {
            tx.execute("UPDATE files SET path=?1 WHERE id=?2", params![updated, id])?;
        }
    }
    let rows = tx
        .prepare("SELECT id,source FROM games")?
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, path) in rows {
        let updated = remapped(&path, &manifest.source_root, directory, &files);
        if updated != path {
            tx.execute(
                "UPDATE games SET source=?1 WHERE id=?2",
                params![updated, id],
            )?;
        }
    }
    for table in ["knowledge_documents", "knowledge_document_versions"] {
        let rows = tx
            .prepare(&format!("SELECT rowid,data_json FROM {table}"))?
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (rowid, json) in rows {
            let mut value: serde_json::Value = serde_json::from_str(&json)?;
            remap_metadata(&mut value, &manifest.source_root, directory, &files);
            if value != serde_json::from_str::<serde_json::Value>(&json)? {
                tx.execute(
                    &format!("UPDATE {table} SET data_json=?1 WHERE rowid=?2"),
                    params![serde_json::to_string(&value)?, rowid],
                )?;
            }
        }
    }
    tx.execute("UPDATE statistics_engine_state SET paused=1 WHERE id=1", [])?;
    tx.commit()?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(conn);
    validate_database(&candidate.join(DATABASE), storage::SCHEMA_VERSION)?;
    Ok(())
}
fn apply(
    directory: &Path,
    parent: &Path,
    name: &str,
    journal: &Path,
    pending: &Pending,
) -> Result<(), AppError> {
    let stage = stage_path(parent, name, pending)?;
    let candidate = stage.join("candidate");
    let previous = stage.join("previous");
    if !candidate.exists() && previous.exists() && directory.exists() {
        // A prior startup completed the atomic move but crashed before cleanup.
        if !directory.join(RESTORED_UI).is_file() {
            return Err(error("Не удалось подтвердить замену профиля"));
        }
        validate_database(&directory.join(DATABASE), storage::SCHEMA_VERSION)?;
    } else {
        let candidate_metadata = fs::symlink_metadata(&candidate)?;
        if !candidate_metadata.is_dir()
            || candidate_metadata.file_type().is_symlink()
            || candidate.canonicalize()?.parent() != Some(stage.as_path())
        {
            return Err(error("Некорректный каталог подготовленного профиля"));
        }
        let manifest: Manifest = serde_json::from_slice(&fs::read(stage.join("manifest.json"))?)?;
        // Verify the staged copy again before trusting a persisted restore request.
        for entry in &manifest.entries {
            validate_entry(&entry.path)?;
            let entry_path = candidate.join(&entry.path);
            if !entry_path
                .canonicalize()?
                .starts_with(candidate.canonicalize()?)
            {
                return Err(error(
                    "Файл подготовленного профиля находится за его пределами",
                ));
            }
            let (bytes, digest) = hash_file(&entry_path)?;
            if bytes != entry.bytes || digest != entry.sha256 {
                return Err(error("Подготовленная копия профиля повреждена"));
            }
        }
        validate_database(&candidate.join(DATABASE), manifest.schema_version)?;
        migrate_candidate(&candidate, &manifest, directory)?;
        write_json(
            &candidate.join(RESTORED_UI),
            &validate_ui(manifest.ui_state)?,
        )?;
        write_json(
            &candidate.join(NOTICE),
            &format!(
                "Профиль восстановлен. Страховочная копия: {}",
                pending.safety_backup
            ),
        )?;
        if !previous.exists() {
            fs::rename(directory, &previous)?;
        }
        if let Err(reason) = fs::rename(&candidate, directory) {
            if !directory.exists() {
                fs::rename(&previous, directory).map_err(|rollback| {
                    error(format!("Не удалось вернуть предыдущий профиль: {rollback}"))
                })?;
            }
            return Err(reason.into());
        }
    }
    fs::remove_file(journal)?;
    drop(Scratch {
        path: stage,
        parent: parent.to_path_buf(),
    });
    Ok(())
}
/// Called before opening any application connection or starting workers.
/// Recoverable rejected requests are quarantined; the existing profile still opens.
pub fn apply_pending(directory: &Path) -> Result<(), AppError> {
    let (parent, name, journal) = root_paths(directory)?;
    if !journal.exists() {
        return Ok(());
    }
    let result = (|| {
        ensure_regular(&journal)?;
        if fs::metadata(&journal)?.len() > 16 * 1024 {
            return Err(error("Повреждён журнал восстановления"));
        }
        let bytes = fs::read(&journal)?;
        let pending: Pending = serde_json::from_slice(&bytes)?;
        apply(directory, &parent, &name, &journal, &pending)
    })();
    if let Err(reason) = result {
        // Recover the previous directory even if a failed new directory was
        // already installed. Keep the rejected material for manual recovery.
        let staged = serde_json::from_slice::<Pending>(&fs::read(&journal)?)
            .ok()
            .and_then(|pending| stage_path(&parent, &name, &pending).ok());
        if let Some(stage) = staged.filter(|stage| stage.join("previous").is_dir()) {
            let previous = stage.join("previous");
            if fs::symlink_metadata(&previous)?.file_type().is_symlink()
                || previous.canonicalize()?.parent() != Some(stage.as_path())
            {
                return Err(error("Предыдущий профиль требует ручного восстановления"));
            }
            let rejected = stage.join(format!("rejected-{}", nonce()?));
            let had_directory = directory.exists();
            if had_directory {
                fs::rename(directory, &rejected)?;
            }
            if let Err(rollback) = fs::rename(&previous, directory) {
                if had_directory {
                    let _ = fs::rename(&rejected, directory);
                }
                return Err(error(format!(
                    "Не удалось вернуть предыдущий профиль: {rollback}"
                )));
            }
        } else if !directory.exists() {
            return Err(error(format!(
                "Профиль требует восстановления: {}",
                reason.message
            )));
        }
        let failed = parent.join(format!(".{name}.profile-restore-failed-{}.json", now_ms()));
        fs::rename(&journal, failed)?;
        write_json(
            &directory.join(NOTICE),
            &format!(
                "Восстановление отменено: {}. Предыдущий профиль сохранён.",
                reason.message
            ),
        )?;
    }
    Ok(())
}
pub fn restored_ui(directory: &Path) -> Result<Option<UiState>, AppError> {
    let path = directory.join(RESTORED_UI);
    if !path.exists() {
        return Ok(None);
    }
    ensure_regular(&path)?;
    let state = validate_ui(serde_json::from_slice(&fs::read(&path)?)?)?;
    Ok(Some(state))
}
pub fn acknowledge_restored_ui(directory: &Path) -> Result<(), AppError> {
    let path = directory.join(RESTORED_UI);
    if path.exists() {
        ensure_regular(&path)?;
        fs::remove_file(path)?;
    }
    Ok(())
}
pub fn status(directory: &Path) -> Result<ProfileStatus, AppError> {
    Ok(ProfileStatus {
        last_backup: fs::read(directory.join(LAST_BACKUP))
            .ok()
            .map(|bytes| serde_json::from_slice(&bytes))
            .transpose()?,
        restore_notice: fs::read(directory.join(NOTICE))
            .ok()
            .map(|bytes| serde_json::from_slice(&bytes))
            .transpose()?,
        pending_restore: has_pending(directory)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{self, EditOperation, ImportRequest, fragments, training};
    use serde_json::json;

    fn ui(value: &str) -> UiState {
        BTreeMap::from([
            ("chesspush.knowledge.draft.synthetic".into(), value.into()),
            ("chesspush.repertoire.side".into(), "white".into()),
        ])
    }
    fn seed(directory: &Path, title: &str) -> (Connection, knowledge::KnowledgeDocument) {
        fs::create_dir_all(directory.join("exports")).unwrap();
        let conn = storage::open(&directory.join(DATABASE)).unwrap();
        storage::set_setting(&conn, "theme", title).unwrap();
        let pgn = "[Event \"Fixture\"]\n[Result \"*\"]\n\n1. e4 {Plan. [%cal Ge2e4]} e5 2. Nf3 Nc6 3. Bb5 a6 *";
        let managed = directory.join("exports/model.pgn");
        fs::write(&managed, pgn).unwrap();
        let id = storage::remember(&conn, &managed, "export").unwrap();
        storage::remember(
            &conn,
            &directory.parent().unwrap().join("external-source.pgn.zst"),
            "source",
        )
        .unwrap();
        conn.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES('fixture','{}','{}',?1,?2,'\"Local\"')", params![pgn,managed.to_string_lossy()]).unwrap();
        conn.execute("INSERT INTO file_games(file_id,game_id) VALUES(?1,1)", [id])
            .unwrap();
        let note = knowledge::import(
            &conn,
            ImportRequest {
                title: title.into(),
                kind: "study".into(),
                side: "both".into(),
                pgn: pgn.into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        let note = knowledge::apply(
            &conn,
            &note.id,
            note.revision,
            EditOperation::SetTags {
                tags: vec!["План".into(), "Структура".into()],
            },
        )
        .unwrap();
        fragments::attach_file(&conn, &managed, "white").unwrap();
        let mut session = training::start(
            &conn,
            serde_json::from_value(
                json!({"scope":{"side":"white"},"mode":"srs","new_limit":5,"restart":true}),
            )
            .unwrap(),
        )
        .unwrap();
        for _ in 0..30 {
            if session.finished {
                break;
            }
            // Independent recall intentionally hides accepted answers in the
            // public prompt. Read the fixture's prepared shared tree instead.
            fn fixture_answer(node: &knowledge::VariationNode, key: &str) -> Option<String> {
                if node.position_key == key {
                    return node.children.first().and_then(|child| child.uci.clone());
                }
                node.children
                    .iter()
                    .find_map(|child| fixture_answer(child, key))
            }
            let answer = fixture_answer(
                &note.chapters[0].root,
                &session.prompt.as_ref().unwrap().position_key,
            )
            .unwrap();
            training::answer(&conn, &session.id, &answer).unwrap();
            session = training::next(&conn, &session.id).unwrap();
        }
        assert!(session.finished);
        assert!(count(&conn, "repertoire_training_srs", "").unwrap() > 0);
        (conn, note)
    }
    fn rows(conn: &Connection, table: &str) -> Vec<Vec<String>> {
        let mut query = conn
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = query.column_count();
        query
            .query_map([], |row| {
                (0..columns)
                    .map(|index| Ok(format!("{:?}", row.get_ref(index)?)))
                    .collect()
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
    #[test]
    fn wal_snapshot_roundtrip_preserves_chess_links_fsrs_ui_and_managed_files() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let target = temp.path().join("target");
        let (source_conn, note) = seed(&source, "Source");
        // The writer stays open in WAL mode while SQLite Backup snapshots it.
        let canonical = serde_json::to_value(&note.chapters).unwrap();
        let expected_srs = rows(&source_conn, "repertoire_training_srs");
        let expected_reviews = rows(&source_conn, "repertoire_training_reviews");
        let expected_sessions = rows(&source_conn, "repertoire_training_sessions");
        source_conn
            .execute(
                "INSERT INTO settings(key,value) VALUES('lichess_api_token','synthetic_secret')",
                [],
            )
            .unwrap();
        source_conn.execute("INSERT INTO legacy_rows(source_path,table_name,row_id,payload) VALUES('old','app_settings','1',?1)", [json!({"lichess_token":"synthetic_secret","theme":"Source"}).to_string()]).unwrap();
        let archive = temp.path().join("profile.chesspush");
        let created = backup(&source, &archive, ui("draft source")).unwrap();
        assert_eq!(created.external_files, 1);
        assert_eq!(created.counts["knowledge"], 1);
        assert_eq!(created.files, 2);
        assert!(!created.credentials_included);
        assert_eq!(
            storage::setting(&source_conn, "lichess_api_token")
                .unwrap()
                .as_deref(),
            Some("synthetic_secret")
        );
        let checked = preview(&source, &archive).unwrap();
        assert_eq!(checked.sha256, created.sha256);
        let (old_conn, _) = seed(&target, "Old");
        drop(old_conn);
        let prepared =
            prepare_restore(&target, &archive, &created.sha256, ui("old draft")).unwrap();
        assert!(Path::new(&prepared.safety_backup).is_file());
        assert!(has_pending(&target).unwrap());
        apply_pending(&target).unwrap();
        assert!(!has_pending(&target).unwrap());
        let restored = storage::open(&target.join(DATABASE)).unwrap();
        assert_eq!(
            storage::setting(&restored, "theme").unwrap().as_deref(),
            Some("Source")
        );
        assert_eq!(
            storage::setting(&restored, "lichess_api_token").unwrap(),
            None
        );
        assert!(
            !restored
                .query_row("SELECT payload FROM legacy_rows LIMIT 1", [], |row| row
                    .get::<_, String>(
                    0
                ))
                .unwrap()
                .contains("synthetic_secret")
        );
        assert_eq!(
            serde_json::to_value(&knowledge::load(&restored, &note.id, None).unwrap().chapters)
                .unwrap(),
            canonical
        );
        assert_eq!(rows(&restored, "repertoire_training_srs"), expected_srs);
        assert_eq!(
            rows(&restored, "repertoire_training_reviews"),
            expected_reviews
        );
        assert_eq!(
            rows(&restored, "repertoire_training_sessions"),
            expected_sessions
        );
        let managed: String = restored
            .query_row("SELECT path FROM files WHERE kind='export'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(Path::new(&managed), target.join("exports/model.pgn"));
        let external: String = restored
            .query_row("SELECT path FROM files WHERE kind='source'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            Path::new(&external),
            temp.path().join("external-source.pgn.zst")
        );
        assert_eq!(
            fs::read(source.join("exports/model.pgn")).unwrap(),
            fs::read(target.join("exports/model.pgn")).unwrap()
        );
        assert_eq!(
            restored
                .query_row("SELECT paused FROM statistics_engine_state", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            1
        );
        assert_eq!(restored_ui(&target).unwrap(), Some(ui("draft source")));
        // Read is idempotent until the browser acknowledges a successful write.
        assert_eq!(restored_ui(&target).unwrap(), Some(ui("draft source")));
        acknowledge_restored_ui(&target).unwrap();
        assert_eq!(restored_ui(&target).unwrap(), None);
        assert!(
            status(&target)
                .unwrap()
                .restore_notice
                .unwrap()
                .contains("Профиль восстановлен")
        );
        drop(restored);
        // The automatically saved previous profile is itself a valid portable snapshot.
        let prior = preview(&target, Path::new(&prepared.safety_backup)).unwrap();
        assert_eq!(prior.counts["games"], 1);
        drop(source_conn);
    }
    fn archive_bytes(path: &Path, manifest: &Manifest, contents: &[&[u8]]) {
        let mut output = zstd::stream::write::Encoder::new(File::create(path).unwrap(), 1).unwrap();
        let json = serde_json::to_vec(manifest).unwrap();
        output.write_all(MAGIC).unwrap();
        output
            .write_all(&(json.len() as u64).to_le_bytes())
            .unwrap();
        output.write_all(&json).unwrap();
        for bytes in contents {
            output.write_all(bytes).unwrap();
        }
        output.finish().unwrap();
    }
    #[test]
    fn corrupt_truncated_future_and_traversal_archives_never_modify_current_profile() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("current");
        let (conn, _) = seed(&root, "Keep");
        let stable = rows(&conn, "knowledge_documents");
        let archive = temp.path().join("valid.chesspush");
        let valid = backup(&root, &archive, ui("keep")).unwrap();
        let expanded = zstd::stream::decode_all(File::open(&archive).unwrap()).unwrap();
        let length =
            u64::from_le_bytes(expanded[MAGIC.len()..MAGIC.len() + 8].try_into().unwrap()) as usize;
        let manifest_start = MAGIC.len() + 8;
        let encoded = &expanded[manifest_start..manifest_start + length];
        let payload = &expanded[manifest_start + length..];
        let mut manifest: Manifest = serde_json::from_slice(encoded).unwrap();
        manifest.entries[0].path = "../outside.sqlite3".into();
        let hostile = temp.path().join("traversal.chesspush");
        archive_bytes(&hostile, &manifest, &[payload]);
        assert!(preview(&root, &hostile).is_err());
        assert!(!temp.path().join("outside.sqlite3").exists());
        let mut manifest: Manifest = serde_json::from_slice(encoded).unwrap();
        manifest.schema_version = storage::SCHEMA_VERSION + 1;
        let future = temp.path().join("future.chesspush");
        archive_bytes(&future, &manifest, &[payload]);
        assert!(preview(&root, &future).is_err());
        let truncated = temp.path().join("truncated.chesspush");
        let compressed = fs::read(&archive).unwrap();
        fs::write(&truncated, &compressed[..compressed.len() / 2]).unwrap();
        assert!(preview(&root, &truncated).is_err());
        let mut manifest: Manifest = serde_json::from_slice(encoded).unwrap();
        manifest.entries[0].sha256 = "0".repeat(64);
        let corrupt = temp.path().join("corrupt.chesspush");
        archive_bytes(&corrupt, &manifest, &[payload]);
        assert!(
            prepare_restore(&root, &corrupt, &hash_file(&corrupt).unwrap().1, ui("keep")).is_err()
        );
        assert!(prepare_restore(&root, &archive, &"0".repeat(64), ui("keep")).is_err());
        assert!(!has_pending(&root).unwrap());
        assert_eq!(rows(&conn, "knowledge_documents"), stable);
        assert_eq!(preview(&root, &archive).unwrap().sha256, valid.sha256);
    }
    #[test]
    fn cancelled_restore_and_invalid_pending_keep_current_profile_bootable() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("current");
        let (conn, _) = seed(&root, "Keep");
        let archive = temp.path().join("valid.chesspush");
        let snapshot = backup(&root, &archive, ui("keep")).unwrap();
        drop(conn);
        prepare_restore(&root, &archive, &snapshot.sha256, ui("keep")).unwrap();
        cancel_restore(&root).unwrap();
        assert!(!has_pending(&root).unwrap());
        let (_, _, journal) = root_paths(&root).unwrap();
        fs::write(&journal, b"not json").unwrap();
        apply_pending(&root).unwrap();
        assert!(!journal.exists());
        let conn = storage::open(&root.join(DATABASE)).unwrap();
        assert_eq!(
            storage::setting(&conn, "theme").unwrap().as_deref(),
            Some("Keep")
        );
        assert!(
            status(&root)
                .unwrap()
                .restore_notice
                .unwrap()
                .contains("Предыдущий профиль сохранён")
        );
    }
    #[test]
    fn interrupted_directory_swap_recovers_previous_profile_and_failed_stage_is_quarantined() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("current");
        let (conn, _) = seed(&root, "Keep");
        let archive = temp.path().join("valid.chesspush");
        let snapshot = backup(&root, &archive, ui("keep")).unwrap();
        drop(conn);
        prepare_restore(&root, &archive, &snapshot.sha256, ui("keep")).unwrap();
        let (parent, name, journal) = root_paths(&root).unwrap();
        let pending: Pending = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
        let stage = stage_path(&parent, &name, &pending).unwrap();
        fs::rename(&root, stage.join("previous")).unwrap();
        fs::write(
            stage.join("candidate").join(DATABASE),
            b"corrupt after old moved",
        )
        .unwrap();
        apply_pending(&root).unwrap();
        let conn = storage::open(&root.join(DATABASE)).unwrap();
        assert_eq!(
            storage::setting(&conn, "theme").unwrap().as_deref(),
            Some("Keep")
        );
        assert!(!journal.exists());
        assert!(
            status(&root)
                .unwrap()
                .restore_notice
                .unwrap()
                .contains("Предыдущий профиль сохранён")
        );
    }
    #[test]
    fn completed_directory_swap_cleanup_is_idempotent_after_crash() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("current");
        let (conn, _) = seed(&root, "Keep");
        let archive = temp.path().join("valid.chesspush");
        let snapshot = backup(&root, &archive, ui("restore")).unwrap();
        drop(conn);
        prepare_restore(&root, &archive, &snapshot.sha256, ui("keep")).unwrap();
        let (parent, name, journal) = root_paths(&root).unwrap();
        let pending: Pending = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
        let stage = stage_path(&parent, &name, &pending).unwrap();
        fs::rename(&root, stage.join("previous")).unwrap();
        write_json(&stage.join("candidate").join(RESTORED_UI), &ui("restore")).unwrap();
        fs::rename(stage.join("candidate"), &root).unwrap();
        apply_pending(&root).unwrap();
        apply_pending(&root).unwrap();
        assert!(!stage.exists());
        assert!(!journal.exists());
        assert_eq!(restored_ui(&root).unwrap(), Some(ui("restore")));
        assert_eq!(
            storage::setting(&storage::open(&root.join(DATABASE)).unwrap(), "theme")
                .unwrap()
                .as_deref(),
            Some("Keep")
        );
    }
    #[test]
    fn corrupt_new_directory_after_swap_rolls_back_before_opening_the_app() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("current");
        let (conn, _) = seed(&root, "Keep");
        let archive = temp.path().join("valid.chesspush");
        let snapshot = backup(&root, &archive, ui("restore")).unwrap();
        drop(conn);
        prepare_restore(&root, &archive, &snapshot.sha256, ui("keep")).unwrap();
        let (parent, name, journal) = root_paths(&root).unwrap();
        let pending: Pending = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
        let stage = stage_path(&parent, &name, &pending).unwrap();
        fs::rename(&root, stage.join("previous")).unwrap();
        write_json(&stage.join("candidate").join(RESTORED_UI), &ui("restore")).unwrap();
        fs::rename(stage.join("candidate"), &root).unwrap();
        fs::write(root.join(DATABASE), b"corrupt after directory swap").unwrap();
        apply_pending(&root).unwrap();
        let conn = storage::open(&root.join(DATABASE)).unwrap();
        assert_eq!(
            storage::setting(&conn, "theme").unwrap().as_deref(),
            Some("Keep")
        );
        assert!(
            status(&root)
                .unwrap()
                .restore_notice
                .unwrap()
                .contains("Предыдущий профиль сохранён")
        );
        assert!(!journal.exists());
    }
    #[test]
    fn profile_lock_excludes_second_process_handle_and_is_released() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("profile");
        let first = lock_profile(&root).unwrap();
        assert_eq!(owner_pid(&root).unwrap(), Some(std::process::id()));
        assert!(lock_profile(&root).is_err());
        drop(first);
        let second = lock_profile(&root).unwrap();
        drop(second);
        assert_eq!(owner_pid(&root).unwrap(), None);
    }
    #[test]
    fn ui_whitelist_and_utf8_path_remapping_reject_secrets_without_panicking() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("current");
        let (conn, _) = seed(&root, "Keep");
        let archive = temp.path().join("secret.chesspush");
        assert!(
            backup(
                &root,
                &archive,
                BTreeMap::from([("chesspush.token".into(), "do not archive".into())])
            )
            .is_err()
        );
        assert!(!archive.exists());
        let inside_profile = root.join("exports/unsafe.chesspush");
        assert!(backup(&root, &inside_profile, ui("keep")).is_err());
        assert!(!inside_profile.exists());
        assert_eq!(
            remapped("/a/ёжик", "/a/аб", &root, &HashSet::new()),
            "/a/ёжик"
        );
        assert_eq!(
            storage::setting(&conn, "theme").unwrap().as_deref(),
            Some("Keep")
        );
    }
}
