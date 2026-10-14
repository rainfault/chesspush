//! Lichess account access and temporary OAuth PKCE callback. No token crosses the IPC boundary.
use crate::{credentials::Credentials, domain::AppError, storage};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{StatusCode, blocking::Client};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use url::Url;
use zeroize::Zeroizing;

const CLIENT_ID: &str = "io.github.rainfault.chesspush";
const AUTH_URL: &str = "https://lichess.org/oauth";
const ACCOUNT_URL: &str = "https://lichess.org/api/account";
const TOKEN_URL: &str = "https://lichess.org/api/token";
const AUTH_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub status: String,
    pub username: Option<String>,
    pub blitz: Option<i64>,
    pub rapid: Option<i64>,
    pub message: Option<String>,
    pub has_token: bool,
}
impl AccountStatus {
    fn empty(status: &str, username: Option<String>, has_token: bool) -> Self {
        Self {
            status: status.into(),
            username,
            blitz: None,
            rapid: None,
            message: None,
            has_token,
        }
    }
}
struct Session {
    generation: u64,
    active: Option<Arc<AtomicBool>>,
    status: AccountStatus,
    retry_after: Option<Instant>,
    last_refresh: Option<Instant>,
}
// Mutexes protect real cross-thread operations: OAuth cancellation/state and API serialization.
pub struct LichessService {
    client: Client,
    credentials: Credentials,
    database: PathBuf,
    session: Mutex<Session>,
    requests: Mutex<()>,
    account_url: String,
    token_url: String,
}
pub struct AuthFlow {
    pub url: String,
    generation: u64,
    listener: TcpListener,
    redirect_uri: String,
    state: String,
    verifier: Zeroizing<String>,
    cancel: Arc<AtomicBool>,
}
#[derive(Deserialize)]
struct Account {
    username: String,
    #[serde(default)]
    perfs: Perfs,
}
#[derive(Default, Deserialize)]
struct Perfs {
    blitz: Option<Perf>,
    rapid: Option<Perf>,
}
#[derive(Deserialize)]
struct Perf {
    rating: i64,
    #[serde(default)]
    games: u64,
}
fn rating(perf: Option<Perf>) -> Option<i64> {
    perf.filter(|p| p.games > 0 && p.rating > 0)
        .map(|p| p.rating)
}
fn state_error() -> AppError {
    AppError::new("state", "Подключение Lichess недоступно")
}
fn cancelled() -> AppError {
    AppError::new("cancelled", "Авторизация отменена")
}
fn validate_token(token: &str) -> Result<(), AppError> {
    if token.is_empty()
        || token.len() > 2048
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(AppError::new("validation", "Некорректный токен Lichess"));
    }
    Ok(())
}
fn transport_error(_: reqwest::Error) -> AppError {
    // Never expose request bodies, authorization headers, response bodies or callback URLs.
    AppError::new("offline", "Нет соединения с Lichess")
}
fn http_error(status: StatusCode) -> AppError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            AppError::new("invalid_token", "Токен Lichess недействителен или отозван")
        }
        StatusCode::TOO_MANY_REQUESTS => AppError::new(
            "rate_limit",
            "Слишком много запросов. Повторите через минуту",
        ),
        _ => AppError::new("lichess", "Lichess временно недоступен"),
    }
}
impl LichessService {
    pub fn new(directory: &Path) -> Result<Self, AppError> {
        let credentials = Credentials::new(directory);
        let database = directory.join("chesspush.sqlite3");
        let conn = storage::open(&database)?;
        let username = storage::setting(&conn, "lichess_username")?.filter(|s| !s.is_empty());
        let mut initial = AccountStatus::empty("disconnected", username, false);
        if let Err(error) = migrate_plaintext(&conn, &credentials) {
            initial.status = "error".into();
            initial.message = Some(error.message);
        }
        match credentials.read() {
            Ok(token) => initial.has_token = token.is_some(),
            Err(error) => {
                initial.status = "error".into();
                initial.message = Some(error.message);
            }
        }
        Self::with_store(credentials, database, initial, ACCOUNT_URL, TOKEN_URL)
    }
    fn with_store(
        credentials: Credentials,
        database: PathBuf,
        status: AccountStatus,
        account_url: &str,
        token_url: &str,
    ) -> Result<Self, AppError> {
        let client = Client::builder()
            .user_agent("ChessPush/0.2 (personal chess utility)")
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            // An unexpected redirect must never forward credentials or an OAuth verifier.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(transport_error)?;
        Ok(Self {
            client,
            credentials,
            database,
            session: Mutex::new(Session {
                generation: 0,
                active: None,
                status,
                retry_after: None,
                last_refresh: None,
            }),
            requests: Mutex::new(()),
            account_url: account_url.into(),
            token_url: token_url.into(),
        })
    }
    pub fn current(&self) -> Result<AccountStatus, AppError> {
        Ok(self
            .session
            .lock()
            .map_err(|_| state_error())?
            .status
            .clone())
    }
    pub fn refresh(&self) -> Result<AccountStatus, AppError> {
        let (generation, token) = {
            let mut session = self.session.lock().map_err(|_| state_error())?;
            if session.active.is_some()
                || session.retry_after.is_some_and(|t| Instant::now() < t)
                || session
                    .last_refresh
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(15))
            {
                return Ok(session.status.clone());
            }
            let token = self.credentials.read()?;
            if token.is_some() {
                session.last_refresh = Some(Instant::now());
            }
            (session.generation, token)
        };
        let Some(token) = token else {
            return self.current();
        };
        let response = self.fetch_account(&token);
        let mut session = self.session.lock().map_err(|_| state_error())?;
        if session.generation != generation || session.active.is_some() {
            return Ok(session.status.clone());
        }
        match response {
            Ok(account) => session.status = account_status(account),
            Err(error) => set_failure(&mut session, error),
        }
        Ok(session.status.clone())
    }
    pub fn connect_token(&self, token: String) -> Result<AccountStatus, AppError> {
        let token = Zeroizing::new(token);
        let token = token.trim();
        validate_token(token)?;
        let generation = self.begin_operation()?;
        let result = self.fetch_account(token);
        match result {
            Ok(account) => self.commit_account(generation, token, account),
            Err(error) => self.finish_error(generation, error),
        }
    }
    fn begin_operation(&self) -> Result<u64, AppError> {
        let mut session = self.session.lock().map_err(|_| state_error())?;
        if let Some(active) = session.active.take() {
            active.store(true, Ordering::Relaxed);
        }
        session.generation += 1;
        session.retry_after = None;
        Ok(session.generation)
    }
    pub fn begin_auth(&self) -> Result<AuthFlow, AppError> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|_| AppError::new("oauth", "Не удалось открыть окно авторизации"))?;
        listener.set_nonblocking(true).map_err(|_| state_error())?;
        let redirect_uri = format!(
            "http://127.0.0.1:{}/oauth/callback",
            listener.local_addr().map_err(|_| state_error())?.port()
        );
        let verifier = Zeroizing::new(random_urlsafe()?);
        let state = random_urlsafe()?;
        let url = authorization_url(&redirect_uri, &state, &verifier);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut session = self.session.lock().map_err(|_| state_error())?;
        if let Some(active) = session.active.take() {
            active.store(true, Ordering::Relaxed);
        }
        session.generation += 1;
        session.retry_after = None;
        session.active = Some(cancel.clone());
        session.status = AccountStatus::empty(
            "authenticating",
            session.status.username.clone(),
            session.status.has_token,
        );
        let generation = session.generation;
        Ok(AuthFlow {
            url,
            generation,
            listener,
            redirect_uri,
            state,
            verifier,
            cancel,
        })
    }
    pub fn finish_browser_failure(&self, flow: &AuthFlow) -> Result<AccountStatus, AppError> {
        flow.cancel.store(true, Ordering::Relaxed);
        self.finish_error(
            flow.generation,
            AppError::new("browser", "Не удалось открыть браузер"),
        )
    }
    pub fn complete_auth(&self, flow: AuthFlow) -> Result<AccountStatus, AppError> {
        let result = (|| {
            let code = wait_for_callback(&flow.listener, &flow.state, &flow.cancel, AUTH_TIMEOUT)?;
            if flow.cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            let token = self.exchange_code(&code, &flow.redirect_uri, &flow.verifier)?;
            if flow.cancel.load(Ordering::Relaxed) {
                return Err(cancelled());
            }
            let account = self.fetch_account(&token)?;
            self.commit_account(flow.generation, &token, account)
        })();
        match result {
            Ok(status) => Ok(status),
            Err(error) => self.finish_error(flow.generation, error),
        }
    }
    pub fn cancel_auth(&self) -> Result<AccountStatus, AppError> {
        let mut session = self.session.lock().map_err(|_| state_error())?;
        if let Some(active) = session.active.take() {
            active.store(true, Ordering::Relaxed);
        }
        session.generation += 1;
        session.status = AccountStatus::empty(
            "disconnected",
            session.status.username.clone(),
            session.status.has_token,
        );
        Ok(session.status.clone())
    }
    pub fn disconnect(&self) -> Result<AccountStatus, AppError> {
        let mut session = self.session.lock().map_err(|_| state_error())?;
        if let Some(active) = session.active.take() {
            active.store(true, Ordering::Relaxed);
        }
        session.generation += 1;
        self.credentials.delete()?;
        let conn = storage::open(&self.database)?;
        conn.execute("DELETE FROM settings WHERE key IN ('lichess_username','lichess_api_token','lichess_token')", [])?;
        session.status = AccountStatus::empty("disconnected", None, false);
        session.retry_after = None;
        Ok(session.status.clone())
    }
    fn commit_account(
        &self,
        generation: u64,
        token: &str,
        account: Account,
    ) -> Result<AccountStatus, AppError> {
        let mut session = self.session.lock().map_err(|_| state_error())?;
        // An old callback/HTTP response cannot reconnect after cancellation or disconnect.
        if session.generation != generation
            || session
                .active
                .as_ref()
                .is_some_and(|c| c.load(Ordering::Relaxed))
        {
            return Ok(session.status.clone());
        }
        self.credentials.save(token)?;
        storage::set_setting(
            &storage::open(&self.database)?,
            "lichess_username",
            &account.username,
        )?;
        // A refresh may have read the previous credential while this login was being validated.
        // Invalidate that snapshot before publishing the newly committed account.
        session.generation += 1;
        session.active = None;
        session.status = account_status(account);
        Ok(session.status.clone())
    }
    fn finish_error(&self, generation: u64, error: AppError) -> Result<AccountStatus, AppError> {
        let mut session = self.session.lock().map_err(|_| state_error())?;
        if session.generation != generation {
            return Ok(session.status.clone());
        }
        session.active = None;
        // Credential-store failure must still end the pending flow and notify the UI.
        // Keep the last known presence flag when Windows cannot answer; never guess that it was deleted.
        match self.credentials.read() {
            Ok(token) => {
                session.status.has_token = token.is_some();
                set_failure(&mut session, error);
            }
            Err(credential_error) => set_failure(&mut session, credential_error),
        }
        Ok(session.status.clone())
    }
    fn response_error(&self, response: &reqwest::blocking::Response) -> AppError {
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            let seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60)
                .clamp(60, 86_400);
            if let Ok(mut session) = self.session.lock() {
                session.retry_after = Some(Instant::now() + Duration::from_secs(seconds));
            }
        }
        http_error(response.status())
    }
    fn fetch_account(&self, token: &str) -> Result<Account, AppError> {
        let _request = self.requests.lock().map_err(|_| state_error())?;
        let response = self
            .client
            .get(&self.account_url)
            .bearer_auth(token)
            .send()
            .map_err(transport_error)?;
        if !response.status().is_success() {
            return Err(self.response_error(&response));
        }
        let account: Account = response
            .json()
            .map_err(|_| AppError::new("response", "Некорректный ответ Lichess"))?;
        if account.username.is_empty() || account.username.len() > 64 {
            return Err(AppError::new("response", "Некорректный ответ Lichess"));
        }
        Ok(account)
    }
    fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
        verifier: &str,
    ) -> Result<Zeroizing<String>, AppError> {
        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            token_type: String,
        }
        let _request = self.requests.lock().map_err(|_| state_error())?;
        let response = self
            .client
            .post(&self.token_url)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("code_verifier", verifier),
                ("client_id", CLIENT_ID),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .map_err(transport_error)?;
        if !response.status().is_success() {
            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                return Err(self.response_error(&response));
            }
            return Err(AppError::new(
                "oauth",
                "Не удалось завершить авторизацию Lichess",
            ));
        }
        let response: TokenResponse = response
            .json()
            .map_err(|_| AppError::new("oauth", "Некорректный ответ авторизации"))?;
        let token = Zeroizing::new(response.access_token);
        validate_token(&token)?;
        if !response.token_type.eq_ignore_ascii_case("bearer") {
            return Err(AppError::new("oauth", "Некорректный ответ авторизации"));
        }
        Ok(token)
    }
}
fn account_status(account: Account) -> AccountStatus {
    AccountStatus {
        status: "connected".into(),
        username: Some(account.username),
        blitz: rating(account.perfs.blitz),
        rapid: rating(account.perfs.rapid),
        message: None,
        has_token: true,
    }
}
fn set_failure(session: &mut Session, error: AppError) {
    let state = match error.code.as_str() {
        "offline" => "offline",
        "invalid_token" => "invalid_token",
        "cancelled" => "disconnected",
        _ => "error",
    };
    session.status = AccountStatus::empty(
        state,
        session.status.username.clone(),
        session.status.has_token,
    );
    session.status.message = if error.code == "cancelled" {
        None
    } else {
        Some(error.message)
    };
    if error.code == "rate_limit" {
        session.retry_after = Some(
            session
                .retry_after
                .unwrap_or(Instant::now() + Duration::from_secs(60)),
        );
    }
}
fn random_urlsafe() -> Result<String, AppError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| AppError::new("oauth", "Не удалось начать авторизацию"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn authorization_url(redirect_uri: &str, state: &str, verifier: &str) -> String {
    let mut url = Url::parse(AUTH_URL).expect("fixed valid Lichess URL");
    url.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("code_challenge_method", "S256"),
        (
            "code_challenge",
            &URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        ),
        ("state", state),
    ]);
    // Reading one's public account and ratings requires no OAuth scope.
    url.into()
}
fn callback_code(target: &str, expected_state: &str) -> Result<Zeroizing<String>, AppError> {
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| AppError::new("callback", "Некорректный ответ авторизации"))?;
    if url.path() != "/oauth/callback" {
        return Err(AppError::new("callback", "Некорректный ответ авторизации"));
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    let values = |key: &str| {
        pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_ref())
            .collect::<Vec<_>>()
    };
    let state = values("state");
    if state.len() != 1 || state[0] != expected_state {
        return Err(AppError::new(
            "state_mismatch",
            "Состояние авторизации не совпало",
        ));
    }
    if !values("error").is_empty() {
        return Err(cancelled());
    }
    let code = values("code");
    if code.len() != 1
        || code[0].is_empty()
        || code[0].len() > 2048
        || !code[0]
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(AppError::new("callback", "Некорректный ответ авторизации"));
    }
    Ok(Zeroizing::new(code[0].to_owned()))
}
fn wait_for_callback(
    listener: &TcpListener,
    state: &str,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Zeroizing<String>, AppError> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cancel.load(Ordering::Relaxed) {
            return Err(cancelled());
        }
        match listener.accept() {
            Ok((mut stream, peer)) if peer.ip().is_loopback() => {
                let target = read_callback_request(&mut stream, cancel);
                let result = target.as_deref().map(|t| callback_code(t, state));
                let accepted = matches!(&result, Some(Ok(_)));
                let denied = matches!(&result, Some(Err(e)) if e.code == "cancelled");
                let text = if accepted {
                    "Авторизация получена. Вернитесь в ChessPush."
                } else {
                    "Авторизация не завершена. Вернитесь в ChessPush."
                };
                let body = format!(
                    "<!doctype html><html lang=ru><meta charset=utf-8><meta name=referrer content=no-referrer><title>ChessPush</title><p>{text}</p></html>"
                );
                let _ = write!(
                    stream,
                    "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\n\r\n{}",
                    if accepted || denied {
                        "200 OK"
                    } else {
                        "400 Bad Request"
                    },
                    body.len(),
                    body
                );
                // Ignore unrelated/forged requests, so they cannot consume the legitimate callback.
                if accepted || denied {
                    return result.expect("present callback result");
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(40))
            }
            Err(_) => return Err(AppError::new("oauth", "Не удалось получить ответ браузера")),
        }
    }
    Err(AppError::new("oauth_timeout", "Время авторизации истекло"))
}
fn read_callback_request(stream: &mut TcpStream, cancel: &AtomicBool) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .ok()?;
    stream
        .set_write_timeout(Some(Duration::from_millis(300)))
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut bytes = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    while bytes.len() < 8192 {
        if cancel.load(Ordering::Relaxed) || Instant::now() > deadline {
            return None;
        }
        let n = stream.read(&mut buffer).ok()?;
        if n == 0 {
            return None;
        }
        bytes.extend_from_slice(&buffer[..n]);
        if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    let request = std::str::from_utf8(&bytes).ok()?;
    let mut parts = request.lines().next()?.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let target = parts.next()?;
    if !target.starts_with('/') || target.starts_with("//") || parts.next()? != "HTTP/1.1" {
        return None;
    }
    Some(target.to_owned())
}
fn migrate_plaintext(
    conn: &rusqlite::Connection,
    credentials: &Credentials,
) -> Result<(), AppError> {
    use rusqlite::OptionalExtension;
    let archived: Option<String> = conn.query_row(
        "SELECT coalesce(json_extract(payload,'$.lichess_api_token'),json_extract(payload,'$.lichess_token')) FROM legacy_rows WHERE table_name='app_settings' AND (json_type(payload,'$.lichess_api_token')='text' OR json_type(payload,'$.lichess_token')='text') LIMIT 1",
        [], |r| r.get(0)).optional()?;
    let archived = archived.map(Zeroizing::new);
    let old = storage::setting(conn, "lichess_api_token")?
        .filter(|t| !t.trim().is_empty())
        .or(storage::setting(conn, "lichess_token")?)
        .map(Zeroizing::new)
        .or(archived);
    if credentials.read()?.is_none()
        && let Some(token) = old.as_deref().filter(|t| !t.trim().is_empty())
    {
        validate_token(token.trim())?;
        credentials.save(token.trim())?;
    }
    // Erase only current app DB copies after secure storage succeeds. Original migration backups stay intact.
    conn.execute_batch("PRAGMA secure_delete=ON;")?;
    let removed = conn.execute(
        "DELETE FROM settings WHERE key IN ('lichess_api_token','lichess_token')",
        [],
    )?;
    let archived = conn.execute("UPDATE legacy_rows SET payload=json_remove(payload,'$.lichess_api_token','$.lichess_token') WHERE table_name='app_settings' AND (json_type(payload,'$.lichess_api_token') IS NOT NULL OR json_type(payload,'$.lichess_token') IS NOT NULL)", [])?;
    if removed + archived > 0 {
        conn.execute_batch(
            "PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA wal_checkpoint(TRUNCATE);",
        )?;
    }
    Ok(())
}

pub fn open_browser(url: &str) -> Result<(), AppError> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let url: Vec<u16> = url.encode_utf16().chain(Some(0)).collect();
        if unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                url.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        } as isize
            <= 32
        {
            return Err(AppError::new("browser", "Не удалось открыть браузер"));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let command = "open";
        #[cfg(not(target_os = "macos"))]
        let command = "xdg-open";
        std::process::Command::new(command)
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|_| AppError::new("browser", "Не удалось открыть браузер"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, thread};

    fn mock_http(
        responses: Vec<(u16, &'static str)>,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0_u8; 1024];
                loop {
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..n]);
                    if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]);
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|s| s.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                    assert!(bytes.len() < 16_384);
                }
                tx.send(String::from_utf8(bytes).unwrap()).unwrap();
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nRetry-After: 120\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (url, rx, handle)
    }
    const ACCOUNT: &str = r#"{"username":"SyntheticPlayer","perfs":{"blitz":{"rating":1957,"games":12},"rapid":{"rating":2010,"games":4}}}"#;
    const TOKEN: &str = "synthetic_fixture_token_no_real_account";
    fn service(directory: &Path, endpoint: &str) -> LichessService {
        let database = directory.join("chesspush.sqlite3");
        storage::open(&database).unwrap();
        LichessService::with_store(
            Credentials::memory(),
            database,
            AccountStatus::empty("disconnected", None, false),
            &format!("{endpoint}/api/account"),
            &format!("{endpoint}/api/token"),
        )
        .unwrap()
    }
    fn parsed_account() -> Account {
        serde_json::from_str(ACCOUNT).unwrap()
    }

    #[test]
    fn pkce_matches_rfc7636_and_requests_no_scopes() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let url = Url::parse(&authorization_url(
            "http://127.0.0.1:12345/oauth/callback",
            "synthetic_state",
            verifier,
        ))
        .unwrap();
        let parameters: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(url.host_str(), Some("lichess.org"));
        assert_eq!(
            parameters["code_challenge"],
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        assert_eq!(parameters["code_challenge_method"], "S256");
        assert!(!parameters.contains_key("scope"));
        assert!(!parameters.contains_key("client_secret"));
        assert!(!url.as_str().contains(verifier));
        let random = random_urlsafe().unwrap();
        assert_eq!(random.len(), 43);
        assert_ne!(random, random_urlsafe().unwrap());
    }
    #[test]
    fn callback_checks_state_path_duplicates_and_denial() {
        assert_eq!(
            &**callback_code("/oauth/callback?state=valid&code=synthetic_code", "valid").unwrap(),
            "synthetic_code"
        );
        assert_eq!(
            callback_code("/oauth/callback?state=wrong&code=synthetic_code", "valid")
                .unwrap_err()
                .code,
            "state_mismatch"
        );
        assert!(callback_code("/unrelated?state=valid&code=synthetic_code", "valid").is_err());
        assert!(
            callback_code(
                "/oauth/callback?state=valid&state=valid&code=synthetic_code",
                "valid"
            )
            .is_err()
        );
        assert!(callback_code("/oauth/callback?state=valid&code=one&code=two", "valid").is_err());
        assert_eq!(
            callback_code("/oauth/callback?state=valid&error=access_denied", "valid")
                .unwrap_err()
                .code,
            "cancelled"
        );
        assert!(
            callback_code(
                "/oauth/callback?state=valid&code=contains%0Anewline",
                "valid"
            )
            .is_err()
        );
    }
    #[test]
    fn manual_token_reads_current_ratings_and_never_returns_secret() {
        let temp = tempfile::tempdir().unwrap();
        let (url, requests, server) = mock_http(vec![
            (200, ACCOUNT),
            (401, r#"{"error":"secret response must not escape"}"#),
        ]);
        let service = service(temp.path(), &url);
        let result = service.connect_token(TOKEN.into()).unwrap();
        assert_eq!(result.status, "connected");
        assert_eq!(result.blitz, Some(1957));
        assert_eq!(result.rapid, Some(2010));
        assert_eq!(result.username.as_deref(), Some("SyntheticPlayer"));
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains(TOKEN));
        assert!(!serialized.contains("access_token"));
        assert_eq!(&**service.credentials.read().unwrap().unwrap(), TOKEN);
        let invalid = service
            .connect_token("invalid_synthetic_fixture".into())
            .unwrap();
        assert_eq!(invalid.status, "invalid_token");
        assert!(invalid.blitz.is_none() && invalid.rapid.is_none());
        assert!(!invalid.message.unwrap().contains("secret response"));
        assert_eq!(&**service.credentials.read().unwrap().unwrap(), TOKEN);
        assert!(
            requests
                .recv()
                .unwrap()
                .to_lowercase()
                .contains(&format!("authorization: bearer {TOKEN}"))
        );
        requests.recv().unwrap();
        server.join().unwrap();
    }
    #[test]
    fn offline_hides_ratings_and_preserves_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let service = service(temp.path(), &url);
        service.commit_account(0, TOKEN, parsed_account()).unwrap();
        let result = service.refresh().unwrap();
        assert_eq!(result.status, "offline");
        assert!(result.has_token);
        assert!(result.blitz.is_none() && result.rapid.is_none());
        assert_eq!(&**service.credentials.read().unwrap().unwrap(), TOKEN);
        service.disconnect().unwrap();
        assert!(service.credentials.read().unwrap().is_none());
    }
    #[test]
    fn oauth_uses_loopback_and_completes_exchange_with_correct_verifier() {
        let temp = tempfile::tempdir().unwrap();
        let (url, requests, server) = mock_http(vec![
            (
                200,
                r#"{"access_token":"synthetic_fixture_token_no_real_account","token_type":"Bearer","expires_in":1000}"#,
            ),
            (200, ACCOUNT),
        ]);
        let service = service(temp.path(), &url);
        let flow = service.begin_auth().unwrap();
        let callback = flow.redirect_uri.clone();
        let state = flow.state.clone();
        let verifier = flow.verifier.to_string();
        let callback_client = thread::spawn(move || {
            let client = Client::new();
            assert_eq!(
                client
                    .get(format!("{callback}?state=wrong&code=synthetic_code"))
                    .send()
                    .unwrap()
                    .status(),
                400
            );
            assert_eq!(
                client
                    .get(format!("{callback}?state={state}&code=synthetic_code"))
                    .send()
                    .unwrap()
                    .status(),
                200
            );
        });
        let status = service.complete_auth(flow).unwrap();
        assert_eq!(status.status, "connected");
        callback_client.join().unwrap();
        let exchange = requests.recv().unwrap();
        let body = exchange.split("\r\n\r\n").nth(1).unwrap();
        let form: std::collections::HashMap<_, _> = url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(form["code_verifier"], verifier);
        assert_eq!(form["grant_type"], "authorization_code");
        assert!(!form.contains_key("client_secret"));
        requests.recv().unwrap();
        server.join().unwrap();
    }
    #[test]
    fn cancellation_and_stale_callback_cannot_reconnect() {
        let temp = tempfile::tempdir().unwrap();
        let service = service(temp.path(), "http://127.0.0.1:1");
        let flow = service.begin_auth().unwrap();
        let generation = flow.generation;
        let address = flow.listener.local_addr().unwrap();
        assert_eq!(service.current().unwrap().status, "authenticating");
        assert_eq!(service.cancel_auth().unwrap().status, "disconnected");
        let started = Instant::now();
        assert_eq!(service.complete_auth(flow).unwrap().status, "disconnected");
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(TcpStream::connect(address).is_err());
        assert_eq!(
            service
                .commit_account(generation, TOKEN, parsed_account())
                .unwrap()
                .status,
            "disconnected"
        );
        assert!(service.credentials.read().unwrap().is_none());
    }
    #[test]
    fn credential_store_failure_ends_authentication_with_an_error_status() {
        let temp = tempfile::tempdir().unwrap();
        let service = service(temp.path(), "http://127.0.0.1:1");
        service.commit_account(0, TOKEN, parsed_account()).unwrap();
        let flow = service.begin_auth().unwrap();
        service.credentials.poison_for_test();
        let status = service.finish_browser_failure(&flow).unwrap();
        assert_eq!(status.status, "error");
        assert!(status.has_token);
        assert!(service.session.lock().unwrap().active.is_none());
        assert!(!serde_json::to_string(&status).unwrap().contains(TOKEN));
    }
    #[test]
    fn callback_timeout_and_zero_games_are_explicit() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        assert_eq!(
            wait_for_callback(
                &listener,
                "state",
                &AtomicBool::new(false),
                Duration::from_millis(1)
            )
            .unwrap_err()
            .code,
            "oauth_timeout"
        );
        let account: Account = serde_json::from_str(
            r#"{"username":"SyntheticPlayer","perfs":{"rapid":{"rating":1500,"games":0}}}"#,
        )
        .unwrap();
        let status = account_status(account);
        assert!(status.rapid.is_none() && status.blitz.is_none());
    }
    #[test]
    fn plaintext_migration_removes_sqlite_settings_and_archive_copies() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("chesspush.sqlite3");
        let conn = storage::open(&path).unwrap();
        storage::set_setting(&conn, "lichess_api_token", TOKEN).unwrap();
        storage::set_setting(&conn, "lichess_username", "SyntheticPlayer").unwrap();
        conn.execute("INSERT INTO legacy_rows(source_path,table_name,row_id,payload) VALUES('synthetic','app_settings','1',?1)", [serde_json::json!({"lichess_api_token":TOKEN,"theme":"light"}).to_string()]).unwrap();
        let credentials = Credentials::memory();
        migrate_plaintext(&conn, &credentials).unwrap();
        assert_eq!(&**credentials.read().unwrap().unwrap(), TOKEN);
        assert!(
            storage::setting(&conn, "lichess_api_token")
                .unwrap()
                .is_none()
        );
        assert_eq!(
            storage::setting(&conn, "lichess_username")
                .unwrap()
                .as_deref(),
            Some("SyntheticPlayer")
        );
        let archive: String = conn
            .query_row("SELECT payload FROM legacy_rows", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&archive).unwrap(),
            serde_json::json!({"theme":"light"})
        );
        migrate_plaintext(&conn, &credentials).unwrap();
        drop(conn);
        assert!(
            !std::fs::read(path)
                .unwrap()
                .windows(TOKEN.len())
                .any(|w| w == TOKEN.as_bytes())
        );
    }
    #[test]
    fn refresh_of_previous_token_cannot_overwrite_a_newly_connected_account() {
        let temp = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (seen_request, first_seen) = mpsc::channel();
        let (allow_new_response, new_response) = mpsc::channel();
        let (allow_old_response, old_response) = mpsc::channel();
        let server = thread::spawn(move || {
            let new_account =
                r#"{"username":"SyntheticNewPlayer","perfs":{"blitz":{"rating":1999,"games":2}}}"#;
            for (index, (body, gate, expected_token)) in [
                (new_account, new_response, "synthetic_new_token"),
                (ACCOUNT, old_response, TOKEN),
            ]
            .into_iter()
            .enumerate()
            {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut buffer).unwrap();
                    assert!(n > 0 && request.len() < 8192);
                    request.extend_from_slice(&buffer[..n]);
                }
                assert!(
                    String::from_utf8(request)
                        .unwrap()
                        .to_lowercase()
                        .contains(&format!("authorization: bearer {expected_token}"))
                );
                seen_request.send(index).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let service = Arc::new(service(temp.path(), &url));
        service.commit_account(0, TOKEN, parsed_account()).unwrap();
        let login_service = service.clone();
        let login = thread::spawn(move || {
            login_service
                .connect_token("synthetic_new_token".into())
                .unwrap()
        });
        assert_eq!(first_seen.recv_timeout(Duration::from_secs(5)).unwrap(), 0);
        let refresh_service = service.clone();
        let refresh = thread::spawn(move || refresh_service.refresh().unwrap());
        // The refresh captures the old token/generation before queuing behind the login's HTTP request.
        let deadline = Instant::now() + Duration::from_secs(5);
        while service.session.lock().unwrap().last_refresh.is_none() {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(1));
        }
        allow_new_response.send(()).unwrap();
        assert_eq!(
            login.join().unwrap().username.as_deref(),
            Some("SyntheticNewPlayer")
        );
        // Only let the old response arrive after the new account has been committed.
        assert_eq!(first_seen.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
        allow_old_response.send(()).unwrap();
        assert_eq!(
            refresh.join().unwrap().username.as_deref(),
            Some("SyntheticNewPlayer")
        );
        assert_eq!(service.current().unwrap().blitz, Some(1999));
        assert_eq!(
            &**service.credentials.read().unwrap().unwrap(),
            "synthetic_new_token"
        );
        server.join().unwrap();
    }
    #[test]
    fn focus_refreshes_are_throttled() {
        let temp = tempfile::tempdir().unwrap();
        let (url, requests, server) = mock_http(vec![(200, ACCOUNT)]);
        let service = service(temp.path(), &url);
        service.commit_account(0, TOKEN, parsed_account()).unwrap();
        assert_eq!(service.refresh().unwrap().status, "connected");
        for _ in 0..4 {
            assert_eq!(service.refresh().unwrap().status, "connected");
        }
        requests.recv().unwrap();
        server.join().unwrap();
    }
    #[test]
    fn rate_limit_waits_before_retry_and_does_not_remove_token() {
        let temp = tempfile::tempdir().unwrap();
        let (url, requests, server) = mock_http(vec![(429, "{}")]);
        let service = service(temp.path(), &url);
        service.commit_account(0, TOKEN, parsed_account()).unwrap();
        assert_eq!(service.refresh().unwrap().status, "error");
        assert!(
            service
                .session
                .lock()
                .unwrap()
                .retry_after
                .unwrap()
                .duration_since(Instant::now())
                > Duration::from_secs(100)
        );
        assert_eq!(service.refresh().unwrap().status, "error");
        assert!(service.credentials.read().unwrap().is_some());
        requests.recv().unwrap();
        server.join().unwrap();
    }
}
