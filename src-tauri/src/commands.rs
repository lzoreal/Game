//! Commands that own every write to the sources directory.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::mpsc;
use std::sync::Arc;

use serde_json::Value;

use crate::catalog::extract_repacks;
use crate::net::{Downloaded, Downloader, FetchOutcome};
use crate::storage::{self, FileStateStore, StateStore};

#[derive(Clone, Debug)]
pub enum CommandError {
    Message(String),
    Exists(String),
    Cloudflare { message: String, url: String },
}

impl CommandError {
    pub fn message(&self) -> &str {
        match self {
            CommandError::Message(text) => text,
            CommandError::Cloudflare { message, .. } => message,
            CommandError::Exists(_) => "源文件已存在",
        }
    }

    pub fn cloudflare_url(&self) -> Option<&str> {
        match self {
            CommandError::Cloudflare { url, .. } => Some(url),
            _ => None,
        }
    }

    pub fn exists_name(&self) -> Option<&str> {
        match self {
            CommandError::Exists(name) => Some(name),
            _ => None,
        }
    }

    /// Prefixes the message with the source file name, keeping the Cloudflare
    /// URL so the caller can still offer to open it.
    pub fn named(self, name: &str) -> Self {
        match self {
            CommandError::Cloudflare { message, url } => CommandError::Cloudflare {
                message: format!("{name}: {message}"),
                url,
            },
            CommandError::Message(text) => CommandError::Message(format!("{name}: {text}")),
            other => other,
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::Message(text) => f.write_str(text),
            CommandError::Cloudflare { message, .. } => f.write_str(message),
            CommandError::Exists(name) => write!(f, "源文件 {name} 已存在"),
        }
    }
}

impl std::error::Error for CommandError {}

#[derive(Clone, Debug)]
pub struct FetchedSource {
    pub url: String,
    pub data: Value,
    pub count: usize,
    pub default_name: String,
    /// SHA-256 of the raw body, so a later fetch can tell "same content" from
    /// "new content" without parsing or touching the disk.
    pub hash: String,
    /// Validators for the next conditional request.
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    /// True when the body came through the Cloudflare browser channel; those
    /// responses carry no validators.
    pub via_cloudflare: bool,
}

#[derive(Clone, Debug)]
pub struct SourceChange {
    pub name: String,
    pub count: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct ImportResult {
    pub added: Vec<SourceChange>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RefreshResult {
    /// Sources whose file was actually rewritten.
    pub updated: Vec<SourceChange>,
    /// Sources the server confirmed as current (304, or identical content).
    pub unchanged: Vec<String>,
    pub failed: Vec<(String, String)>,
}

/// Whether a fetch may reuse the stored validators and content hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FetchMode {
    /// Same URL as last time: send `If-None-Match` / `If-Modified-Since` and
    /// skip the write when the body hashes the same.
    Incremental,
    /// A different URL: the stored validators and hash describe other content,
    /// so fetch and write unconditionally.
    Fresh,
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn to_command_error(error: crate::net::NetError) -> CommandError {
    match error.cloudflare_url.clone() {
        Some(url) => CommandError::Cloudflare {
            message: error.message,
            url,
        },
        None => CommandError::Message(error.message),
    }
}

pub fn url_to_filename(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let path = match rest.find('/') {
        Some(index) => &rest[index..],
        None => "/",
    };
    let path = path.split(['?', '#']).next().unwrap_or("");
    let base = path.rsplit('/').find(|part| !part.is_empty()).unwrap_or("source");
    let stem = match base.rfind('.') {
        Some(index) if index > 0 => &base[..index],
        _ => base,
    };
    let cleaned: String = stem
        .chars()
        .map(|ch| if "\\/:*?\"<>|".contains(ch) { '_' } else { ch })
        .collect();
    let name: String = cleaned.chars().take(50).collect();
    if name.is_empty() {
        "source".to_string()
    } else {
        name
    }
}

pub fn safe_source_filename(text: &str) -> String {
    let mut name = text.trim().to_string();
    let lower = name.to_lowercase();
    if lower.ends_with(".json.gz") {
        name.truncate(name.len() - 8);
    } else if lower.ends_with(".json") {
        name.truncate(name.len() - 5);
    }
    let cleaned: String = name
        .chars()
        .map(|ch| if "\\/:*?\"<>|".contains(ch) { '_' } else { ch })
        .collect();
    let trimmed = cleaned.trim();
    let limited: String = trimmed.chars().take(80).collect();
    let stem = if limited.is_empty() { "source" } else { &limited };
    format!("{stem}.json.gz")
}

fn format_keys(data: &Value) -> String {
    match data {
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().map(|key| format!("'{key}'")).collect();
            format!("[{}]", keys.join(", "))
        }
        Value::Array(_) => "list".to_string(),
        Value::String(_) => "str".to_string(),
        Value::Number(_) => "int".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Null => "NoneType".to_string(),
    }
}

fn validate_data(data: Value) -> Result<(Value, usize), CommandError> {
    let count = extract_repacks(&data).map(|(items, _)| items.len()).unwrap_or(0);
    if count == 0 {
        return Err(CommandError::Message(format!(
            "不是有效的目录 JSON 格式。JSON 顶层字段: {}",
            format_keys(&data)
        )));
    }
    Ok((data, count))
}

fn parse_content(content: &str) -> Result<(Value, usize), CommandError> {
    let data: Value = serde_json::from_str(content).map_err(|error| {
        CommandError::Message(format!("返回内容不是有效 JSON: {error}"))
    })?;
    validate_data(data)
}

fn read_source_file(path: &Path) -> Result<(Value, usize), CommandError> {
    let data = storage::read_source_file(path)
        .map_err(|error| CommandError::Message(format!("不是有效 JSON: {error}")))?;
    validate_data(data)
}

pub struct SourceCommands {
    pub sources_dir: PathBuf,
    downloader: Arc<Downloader>,
    store: FileStateStore,
}

impl SourceCommands {
    pub fn new(sources_dir: PathBuf, downloader: Arc<Downloader>) -> Self {
        let store = FileStateStore::new(&sources_dir);
        Self {
            sources_dir,
            downloader,
            store,
        }
    }

    /// Lets tests point the state file at their own temp directory instead of
    /// the process-wide `GAMEMAGNET_STATE_DIR`.
    #[cfg(test)]
    fn with_store(
        sources_dir: PathBuf,
        downloader: Arc<Downloader>,
        store: FileStateStore,
    ) -> Self {
        Self {
            sources_dir,
            downloader,
            store,
        }
    }

    pub fn fetch(&self, url: &str) -> Result<FetchedSource, CommandError> {
        let downloaded = self
            .downloader
            .download_json_with_meta(url)
            .map_err(to_command_error)?;
        let hash = sha256_hex(downloaded.body.as_bytes());
        self.build_fetched(url, hash, downloaded)
    }

    fn build_fetched(
        &self,
        url: &str,
        hash: String,
        downloaded: Downloaded,
    ) -> Result<FetchedSource, CommandError> {
        let (data, count) = parse_content(&downloaded.body)?;
        Ok(FetchedSource {
            url: url.to_string(),
            data,
            count,
            default_name: url_to_filename(url),
            hash,
            etag: downloaded.etag,
            last_modified: downloaded.last_modified,
            via_cloudflare: downloaded.via_cloudflare,
        })
    }

    pub fn add_fetched(
        &self,
        fetched: &FetchedSource,
        filename: &str,
        overwrite: bool,
    ) -> Result<SourceChange, CommandError> {
        let name = safe_source_filename(filename);
        let path = self.sources_dir.join(&name);
        if path.exists() && !overwrite {
            return Err(CommandError::Exists(name));
        }
        self.write_fetched(&name, fetched)?;
        Ok(SourceChange {
            name,
            count: Some(fetched.count),
        })
    }

    /// Re-downloads one source. `Ok(None)` means the stored file is already
    /// current, which happens on a `304` or on identical content.
    pub fn update(
        &self,
        name: &str,
        url: Option<&str>,
    ) -> Result<Option<SourceChange>, CommandError> {
        let name = self.checked_name(name)?;
        let path = self.sources_dir.join(&name);
        let stored = storage::read_source_url(&path);
        let resolved = url
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .or_else(|| stored.clone())
            .ok_or_else(|| CommandError::Message(format!("{name}: 没有记录源地址")))?;
        let mode = if stored.as_deref() == Some(resolved.as_str()) {
            FetchMode::Incremental
        } else {
            FetchMode::Fresh
        };
        self.fetch_one(&name, &resolved, mode, true)
            .map_err(|error| error.named(&name))
    }

    /// One source, one fetch. Records the content hash and the HTTP validators
    /// so the next refresh can skip the body entirely.
    fn fetch_one(
        &self,
        name: &str,
        url: &str,
        mode: FetchMode,
        use_solver: bool,
    ) -> Result<Option<SourceChange>, CommandError> {
        let snap = self.store.snapshot(name);
        // Cloudflare sources never send validators: the browser channel does
        // not give us a response with headers to compare against.
        let (etag, last_modified) = if mode == FetchMode::Incremental && !snap.via_cloudflare {
            (snap.etag.as_deref(), snap.last_modified.as_deref())
        } else {
            (None, None)
        };

        let outcome = if use_solver {
            self.downloader.conditional_json(url, etag, last_modified)
        } else {
            self.downloader
                .conditional_json_no_solver(url, etag, last_modified)
        }
        .map_err(|error| {
            let cloudflare = error.is_cloudflare();
            let command = to_command_error(error);
            self.store
                .record_failure(name, &command.to_string(), cloudflare);
            command
        })?;

        let fetched = match outcome {
            FetchOutcome::NotModified => {
                // Nothing changed on the server. Refresh the timestamp so the
                // source is not immediately stale again.
                self.store.record_success(
                    name,
                    &snap.hash,
                    snap.etag.as_deref(),
                    snap.last_modified.as_deref(),
                    snap.via_cloudflare,
                );
                return Ok(None);
            }
            FetchOutcome::Fetched(downloaded) => {
                let hash = sha256_hex(downloaded.body.as_bytes());
                if mode == FetchMode::Incremental && !snap.hash.is_empty() && snap.hash == hash {
                    // Same bytes: keep the file (and its mtime) untouched, but
                    // store the validators in case the server rotated them.
                    // The body is not parsed, which is the expensive part.
                    self.store.record_success(
                        name,
                        &hash,
                        downloaded.etag.as_deref(),
                        downloaded.last_modified.as_deref(),
                        downloaded.via_cloudflare,
                    );
                    return Ok(None);
                }
                self.build_fetched(url, hash, downloaded).map_err(|error| {
                    self.store.record_failure(name, &error.to_string(), false);
                    error
                })?
            }
        };

        self.write_fetched(name, &fetched)?;
        Ok(Some(SourceChange {
            name: name.to_string(),
            count: Some(fetched.count),
        }))
    }

    /// Source files that are worth refreshing in the background: they have a
    /// recorded URL, they are not behind Cloudflare, and their last successful
    /// fetch is older than `stale_secs`.
    pub fn stale_targets(&self, stale_secs: i64) -> Vec<String> {
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(stale_secs.max(0));
        let Ok(snap) = self.store.load() else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for raw in storage::source_filenames(&self.sources_dir) {
            let Ok(name) = self.checked_name(&raw) else {
                continue;
            };
            let path = self.sources_dir.join(&name);
            if storage::read_source_url(&path).is_none() {
                continue;
            }
            let Some(entry) = snap.source_snapshots.get(&name) else {
                // Never fetched through us: nothing to lose by trying.
                names.push(name);
                continue;
            };
            if entry.via_cloudflare {
                continue;
            }
            let stale = entry
                .last_success
                .map(|last| last < cutoff)
                .unwrap_or(true);
            if stale {
                names.push(name);
            }
        }
        names
    }

    pub fn import_files(&self, paths: &[String], overwrite: &[String]) -> ImportResult {
        let mut result = ImportResult::default();
        for path in paths {
            let base = Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(path)
                .to_string();
            let lower = base.to_lowercase();
            if !lower.ends_with(".json") && !lower.ends_with(".json.gz") {
                result.errors.push(format!("{base}: 不是 .json 或 .json.gz 文件"));
                continue;
            }
            let name = safe_source_filename(&base);
            let (data, count) = match read_source_file(Path::new(path)) {
                Ok(value) => value,
                Err(error) => {
                    result.errors.push(format!("{name}: {error}"));
                    continue;
                }
            };
            let dest = self.sources_dir.join(&name);
            if dest.exists() && !overwrite.contains(&name) {
                result.errors.push(format!("{name}: 已存在，未覆盖"));
                continue;
            }
            if let Err(error) = storage::write_source_file(&dest, &data) {
                result.errors.push(format!("{name}: {error}"));
                continue;
            }
            result.added.push(SourceChange {
                name,
                count: Some(count),
            });
        }
        result
    }

    pub fn delete(&self, name: &str) -> Result<SourceChange, CommandError> {
        let name = self.checked_name(name)?;
        let path = self.sources_dir.join(&name);
        let legacy = if name.to_lowercase().ends_with(".json.gz") {
            PathBuf::from(path.to_string_lossy().trim_end_matches(".gz"))
        } else {
            PathBuf::from(format!("{}.gz", path.to_string_lossy()))
        };
        for target in [&path, &legacy] {
            let _ = std::fs::remove_file(target);
            let mut sidecar = target.as_os_str().to_owned();
            sidecar.push(storage::SOURCE_URL_SUFFIX);
            let _ = std::fs::remove_file(PathBuf::from(sidecar));
        }
        Ok(SourceChange { name, count: None })
    }

    pub fn refresh(
        &self,
        names: Option<&[String]>,
        report: &mut dyn FnMut(usize, usize, &str),
    ) -> RefreshResult {
        let workers = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .clamp(1, 4);
        self.refresh_with_workers(names, report, workers, true)
    }

    /// Background refresh: same pipeline, but a Cloudflare challenge is a plain
    /// failure instead of a reason to open a browser window. The failure is
    /// recorded as `via_cloudflare`, so later automatic passes skip the source.
    pub fn refresh_passive(
        &self,
        names: Option<&[String]>,
        report: &mut dyn FnMut(usize, usize, &str),
    ) -> RefreshResult {
        let workers = std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            .clamp(1, 4);
        self.refresh_with_workers(names, report, workers, false)
    }

    fn refresh_with_workers(
        &self,
        names: Option<&[String]>,
        report: &mut dyn FnMut(usize, usize, &str),
        max_workers: usize,
        use_solver: bool,
    ) -> RefreshResult {
        let names = names
            .map(|list| list.to_vec())
            .unwrap_or_else(|| storage::source_filenames(&self.sources_dir));
        let mut targets = Vec::new();
        for raw in names {
            let Ok(name) = self.checked_name(&raw) else {
                continue;
            };
            let path = self.sources_dir.join(&name);
            if let Some(url) = storage::read_source_url(&path) {
                targets.push((name, url));
            }
        }

        let total = targets.len();
        if total == 0 {
            return RefreshResult::default();
        }

        let workers = max_workers.clamp(1, 4).min(total);
        let next = AtomicUsize::new(0);
        // (index, name, Ok(Some(change)) = rewritten, Ok(None) = unchanged)
        let (sender, receiver) =
            mpsc::channel::<(usize, String, Result<Option<SourceChange>, String>)>();
        let mut slots: Vec<Option<(String, Result<Option<SourceChange>, String>)>> =
            (0..total).map(|_| None).collect();

        std::thread::scope(|scope| {
            for _ in 0..workers {
                let sender = sender.clone();
                let targets = &targets;
                let next = &next;
                scope.spawn(move || loop {
                    let index = next.fetch_add(1, AtomicOrdering::Relaxed);
                    if index >= targets.len() {
                        break;
                    }
                    let (name, url) = &targets[index];
                    let outcome = self
                        .fetch_one(name, url, FetchMode::Incremental, use_solver)
                        .map_err(|error| error.to_string());
                    if sender.send((index, name.clone(), outcome)).is_err() {
                        break;
                    }
                });
            }
            drop(sender);

            let mut completed = 0usize;
            for (index, name, outcome) in receiver {
                completed += 1;
                report(completed, total, &name);
                slots[index] = Some((name, outcome));
            }
        });

        let mut result = RefreshResult::default();
        for (name, outcome) in slots.into_iter().flatten() {
            match outcome {
                Ok(Some(change)) => result.updated.push(change),
                Ok(None) => result.unchanged.push(name),
                Err(error) => result.failed.push((name, error)),
            }
        }
        result
    }

    fn write_fetched(&self, name: &str, fetched: &FetchedSource) -> Result<(), CommandError> {
        self.write(name, &fetched.url, &fetched.data)?;
        self.store.record_success(
            name,
            &fetched.hash,
            fetched.etag.as_deref(),
            fetched.last_modified.as_deref(),
            fetched.via_cloudflare,
        );
        Ok(())
    }

    fn write(&self, name: &str, url: &str, data: &Value) -> Result<(), CommandError> {
        let path = self.sources_dir.join(name);
        storage::write_source_file(&path, data).map_err(|error| {
            CommandError::Message(format!("{name}: {error}"))
        })?;
        storage::write_url_file(&path, url).map_err(|error| {
            CommandError::Message(format!("{name}: {error}"))
        })?;
        Ok(())
    }

    fn checked_name(&self, name: &str) -> Result<String, CommandError> {
        let path = Path::new(name);
        let base = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let lower = base.to_lowercase();
        let valid = base == name
            && (lower.ends_with(".json") || lower.ends_with(".json.gz"));
        if !valid {
            return Err(CommandError::Message(format!("无效的源文件名: '{name}'")));
        }
        Ok(base.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{HttpClient, HttpResponse};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::time::Duration;

    struct FakeClient {
        responses: Mutex<HashMap<String, Result<String, String>>>,
        delay: Duration,
    }

    impl FakeClient {
        fn new(entries: Vec<(&str, Result<String, String>)>) -> Arc<Self> {
            Self::with_delay(entries, Duration::ZERO)
        }

        fn with_delay(
            entries: Vec<(&str, Result<String, String>)>,
            delay: Duration,
        ) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(
                    entries
                        .into_iter()
                        .map(|(url, result)| (url.to_string(), result))
                        .collect(),
                ),
                delay,
            })
        }
    }

    impl HttpClient for FakeClient {
        fn get(
            &self,
            url: &str,
            _headers: &[(&str, &str)],
            _timeout: Duration,
            _profile: Option<&str>,
        ) -> Result<HttpResponse, String> {
            if !self.delay.is_zero() {
                std::thread::sleep(self.delay);
            }
            match self.responses.lock().unwrap().get(url) {
                Some(Ok(body)) => Ok(HttpResponse {
                    status: 200,
                    reason: "OK".to_string(),
                    headers: HashMap::new(),
                    body: body.as_bytes().to_vec(),
                }),
                Some(Err(error)) => Err(error.clone()),
                None => Err("unexpected url".to_string()),
            }
        }
    }

    /// Serves a queue of scripted responses per URL and remembers the request
    /// headers, so tests can assert on `If-None-Match` and friends.
    struct ScriptedServer {
        responses: Mutex<HashMap<String, std::collections::VecDeque<HttpResponse>>>,
        seen: Mutex<HashMap<String, Vec<Vec<(String, String)>>>>,
    }

    impl ScriptedServer {
        fn new(entries: Vec<(&str, Vec<HttpResponse>)>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(
                    entries
                        .into_iter()
                        .map(|(url, list)| (url.to_string(), list.into_iter().collect()))
                        .collect(),
                ),
                seen: Mutex::new(HashMap::new()),
            })
        }

        fn calls(&self, url: &str) -> usize {
            self.seen
                .lock()
                .unwrap()
                .get(url)
                .map(Vec::len)
                .unwrap_or(0)
        }

        fn last_headers(&self, url: &str) -> Vec<(String, String)> {
            self.seen
                .lock()
                .unwrap()
                .get(url)
                .and_then(|list| list.last().cloned())
                .unwrap_or_default()
        }
    }

    impl HttpClient for ScriptedServer {
        fn get(
            &self,
            url: &str,
            headers: &[(&str, &str)],
            _timeout: Duration,
            _profile: Option<&str>,
        ) -> Result<HttpResponse, String> {
            self.seen
                .lock()
                .unwrap()
                .entry(url.to_string())
                .or_default()
                .push(
                    headers
                        .iter()
                        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                        .collect(),
                );
            let mut responses = self.responses.lock().unwrap();
            let queue = responses
                .get_mut(url)
                .ok_or_else(|| format!("unexpected url {url}"))?;
            queue
                .pop_front()
                .ok_or_else(|| format!("no scripted response left for {url}"))
        }
    }

    fn http_ok(body: &str, etag: Option<&str>) -> HttpResponse {
        let mut headers = HashMap::new();
        if let Some(etag) = etag {
            headers.insert("etag".to_string(), etag.to_string());
        }
        HttpResponse {
            status: 200,
            reason: "OK".to_string(),
            headers,
            body: body.as_bytes().to_vec(),
        }
    }

    fn http_not_modified() -> HttpResponse {
        HttpResponse {
            status: 304,
            reason: "Not Modified".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }

    fn http_challenge() -> HttpResponse {
        let mut headers = HashMap::new();
        headers.insert("cf-mitigated".to_string(), "challenge".to_string());
        HttpResponse {
            status: 403,
            reason: "Forbidden".to_string(),
            headers,
            body: b"<title>Just a moment...</title>".to_vec(),
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-cmd-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn payload(title: &str) -> String {
        format!(r#"{{"downloads": [{{"title": "{title}", "uris": ["magnet:a"]}}]}}"#)
    }

    /// State file kept inside the test's temp dir, never the real `.gm`.
    fn state(dir: &Path) -> FileStateStore {
        FileStateStore::with_dir(dir.join(".gm"))
    }

    fn commands(dir: &Path, entries: Vec<(&str, Result<String, String>)>) -> SourceCommands {
        commands_with_client(dir, FakeClient::new(entries))
    }

    fn commands_with_client(dir: &Path, client: Arc<dyn HttpClient>) -> SourceCommands {
        SourceCommands::with_store(
            dir.to_path_buf(),
            Arc::new(Downloader::new(client)),
            state(dir),
        )
    }

    fn scripted_commands(dir: &Path, server: Arc<ScriptedServer>) -> SourceCommands {
        commands_with_client(dir, server)
    }

    /// A source file with an old payload plus its URL sidecar.
    fn seeded_source(dir: &Path, name: &str, url: &str) -> PathBuf {
        let path = dir.join(name);
        let stale = serde_json::json!({"downloads": [{"title": "Old", "uris": ["magnet:old"]}]});
        storage::write_source_file(&path, &stale).unwrap();
        storage::write_url_file(&path, url).unwrap();
        path
    }

    fn titles(path: &Path) -> Vec<String> {
        let stored = storage::read_source_file(path).unwrap();
        stored["downloads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["title"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn naming_rules() {
        assert_eq!(
            url_to_filename("https://example.com/sources/catalog.json"),
            "catalog"
        );
        assert_eq!(safe_source_filename(" My:Game "), "My_Game.json.gz");
        assert_eq!(safe_source_filename("already.json"), "already.json.gz");
        assert_eq!(safe_source_filename("already.json.gz"), "already.json.gz");
    }

    #[test]
    fn fetch_validates_payloads() {
        let dir = temp_dir("fetch");
        let url = "https://example.com/a.json";
        let ok = commands(&dir, vec![(url, Ok(payload("Alpha")))]);
        let fetched = ok.fetch(url).unwrap();
        assert_eq!(fetched.count, 1);
        assert_eq!(fetched.default_name, "a");

        let bad = commands(
            &dir,
            vec![
                ("https://example.com/bad.json", Ok("{not json".to_string())),
                ("https://example.com/empty.json", Ok("{\"foo\": []}".to_string())),
            ],
        );
        assert!(bad.fetch("https://example.com/bad.json").is_err());
        let error = bad.fetch("https://example.com/empty.json").unwrap_err();
        assert!(error.to_string().contains("目录 JSON"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn add_writes_gzip_sidecar_and_refuses_overwrite() {
        let dir = temp_dir("add");
        let url = "https://example.com/a.json";
        let commands = commands(&dir, vec![(url, Ok(payload("Alpha")))]);
        let fetched = commands.fetch(url).unwrap();
        let change = commands.add_fetched(&fetched, "a", false).unwrap();
        assert_eq!(change.name, "a.json.gz");

        let stored = storage::read_source_file(&dir.join("a.json.gz")).unwrap();
        assert_eq!(stored["downloads"][0]["title"], "Alpha");
        assert_eq!(
            storage::read_source_url(&dir.join("a.json.gz")).as_deref(),
            Some(url)
        );

        let error = commands.add_fetched(&fetched, "a", false).unwrap_err();
        assert_eq!(error.exists_name(), Some("a.json.gz"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_uses_sidecar_and_explicit_url() {
        let dir = temp_dir("update");
        let path = dir.join("a.json.gz");
        let stale = serde_json::json!({"downloads": [{"title": "Old", "uris": ["magnet:old"]}]});
        storage::write_source_file(&path, &stale).unwrap();
        storage::write_url_file(&path, "https://example.com/a.json").unwrap();
        let update_commands = commands(
            &dir,
            vec![("https://example.com/a.json", Ok(payload("New")))],
        );
        let change = update_commands
            .update("a.json.gz", None)
            .unwrap()
            .expect("first fetch writes");
        assert_eq!(change.name, "a.json.gz");
        let stored = storage::read_source_file(&path).unwrap();
        assert_eq!(stored["downloads"][0]["title"], "New");

        let overwrite_commands = commands(
            &dir,
            vec![("https://example.com/b.json", Ok(payload("Newer")))],
        );
        overwrite_commands
            .update("a.json.gz", Some("https://example.com/b.json"))
            .unwrap();
        assert_eq!(
            storage::read_source_url(&path).as_deref(),
            Some("https://example.com/b.json")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_stores_validators_and_a_304_reports_no_change() {
        let dir = temp_dir("update-conditional");
        let url = "https://example.com/a.json";
        let path = seeded_source(&dir, "a.json.gz", url);
        let server = ScriptedServer::new(vec![(
            url,
            vec![http_ok(&payload("New"), Some("\"v1\"")), http_not_modified()],
        )]);
        let commands = scripted_commands(&dir, server.clone());

        let change = commands.update("a.json.gz", None).unwrap().unwrap();
        assert_eq!(change.count, Some(1));
        let snap = state(&dir).snapshot("a.json.gz");
        assert_eq!(snap.etag.as_deref(), Some("\"v1\""));
        assert_eq!(snap.hash.len(), 64, "sha256 hex");
        assert!(snap.last_success.is_some());
        assert_eq!(titles(&path), vec!["New"]);

        assert!(
            commands.update("a.json.gz", None).unwrap().is_none(),
            "a 304 means the stored file is current"
        );
        assert_eq!(server.calls(url), 2);
        assert!(server
            .last_headers(url)
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("if-none-match") && value == "\"v1\""));
        assert_eq!(titles(&path), vec!["New"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn identical_content_is_not_rewritten_but_its_new_etag_is_kept() {
        let dir = temp_dir("update-same");
        let url = "https://example.com/a.json";
        let path = seeded_source(&dir, "a.json.gz", url);
        let server = ScriptedServer::new(vec![(
            url,
            vec![
                http_ok(&payload("Same"), Some("\"v1\"")),
                http_ok(&payload("Same"), Some("\"v2\"")),
            ],
        )]);
        let commands = scripted_commands(&dir, server);

        let first = commands.refresh(None, &mut |_, _, _| {});
        assert_eq!(first.updated.len(), 1);
        assert!(first.unchanged.is_empty());

        let second = commands.refresh(None, &mut |_, _, _| {});
        assert!(
            second.updated.is_empty(),
            "same bytes must not be written again"
        );
        assert_eq!(second.unchanged, vec!["a.json.gz"]);
        assert_eq!(state(&dir).snapshot("a.json.gz").etag.as_deref(), Some("\"v2\""));
        assert_eq!(titles(&path), vec!["Same"]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cloudflare_sources_never_send_conditional_headers() {
        let dir = temp_dir("update-cloudflare");
        let url = "https://example.com/cf.json";
        seeded_source(&dir, "cf.json.gz", url);
        state(&dir).record_success("cf.json.gz", "oldhash", Some("\"v1\""), None, true);
        let server = ScriptedServer::new(vec![(url, vec![http_ok(&payload("New"), Some("\"v2\""))])]);
        let commands = scripted_commands(&dir, server.clone());

        commands.update("cf.json.gz", None).unwrap().unwrap();
        assert!(
            !server
                .last_headers(url)
                .iter()
                .any(|(key, _)| key.to_lowercase().starts_with("if-")),
            "the browser channel cannot replay validators"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_changed_url_ignores_the_old_validators() {
        let dir = temp_dir("update-new-url");
        let old = "https://example.com/a.json";
        let new = "https://example.com/b.json";
        let path = seeded_source(&dir, "a.json.gz", old);
        state(&dir).record_success("a.json.gz", "oldhash", Some("\"a1\""), None, false);
        let server = ScriptedServer::new(vec![(new, vec![http_ok(&payload("New"), None)])]);
        let commands = scripted_commands(&dir, server.clone());

        commands.update("a.json.gz", Some(new)).unwrap().unwrap();
        assert!(server.calls(old) == 0, "must not hit the old URL");
        assert!(!server
            .last_headers(new)
            .iter()
            .any(|(key, _)| key.to_lowercase().starts_with("if-")));
        assert_eq!(
            storage::read_source_url(&path).as_deref(),
            Some(new),
            "the sidecar follows the new URL"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn passive_refresh_records_cloudflare_and_leaves_the_file_intact() {
        let dir = temp_dir("refresh-passive");
        let url = "https://example.com/cf.json";
        let path = seeded_source(&dir, "cf.json.gz", url);
        let server = ScriptedServer::new(vec![(
            url,
            vec![http_challenge(), http_challenge(), http_challenge()],
        )]);
        let commands = scripted_commands(&dir, server);

        let result = commands.refresh_passive(None, &mut |_, _, _| {});
        assert!(result.updated.is_empty());
        assert_eq!(result.failed.len(), 1);
        assert_eq!(titles(&path), vec!["Old"], "the old file must survive");
        assert!(state(&dir).snapshot("cf.json.gz").via_cloudflare);
        assert!(
            commands.stale_targets(0).is_empty(),
            "a blocked source is not retried automatically"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stale_targets_skip_fresh_cloudflare_and_urlless_sources() {        let dir = temp_dir("stale");
        seeded_source(&dir, "fresh.json.gz", "https://example.com/fresh.json");
        seeded_source(&dir, "old.json.gz", "https://example.com/old.json");
        seeded_source(&dir, "cf.json.gz", "https://example.com/cf.json");
        seeded_source(&dir, "never.json.gz", "https://example.com/never.json");
        let local = dir.join("local.json.gz");
        storage::write_source_file(
            &local,
            &serde_json::json!({"downloads": [{"title": "Local", "uris": ["magnet:x"]}]}),
        )
        .unwrap();

        let store = state(&dir);
        store.record_success("fresh.json.gz", "h", None, None, false);
        store.record_success("old.json.gz", "h", None, None, false);
        store.record_success("cf.json.gz", "h", None, None, true);
        let mut snapshot = store.load().unwrap();
        let two_days = chrono::Utc::now() - chrono::Duration::days(2);
        let ten_days = chrono::Utc::now() - chrono::Duration::days(10);
        snapshot
            .source_snapshots
            .get_mut("old.json.gz")
            .unwrap()
            .last_success = Some(two_days);
        snapshot
            .source_snapshots
            .get_mut("cf.json.gz")
            .unwrap()
            .last_success = Some(ten_days);
        store.save(&snapshot).unwrap();

        let commands = scripted_commands(&dir, ScriptedServer::new(vec![]));
        assert_eq!(
            commands.stale_targets(24 * 60 * 60),
            vec!["never.json.gz", "old.json.gz"]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_failure_names_the_file() {
        let dir = temp_dir("update-fail");
        let path = dir.join("a.json.gz");
        let stale = serde_json::json!({"downloads": [{"title": "Old", "uris": ["magnet:old"]}]});
        storage::write_source_file(&path, &stale).unwrap();
        storage::write_url_file(&path, "https://example.com/bad.json").unwrap();
        let commands = commands(
            &dir,
            vec![("https://example.com/bad.json", Err("boom".to_string()))],
        );
        let error = commands.update("a.json.gz", None).unwrap_err();
        assert!(error.to_string().contains("a.json.gz"));
        assert!(error.to_string().contains("boom"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_validates_and_requires_approved_overwrite() {
        let dir = temp_dir("import");
        let sources = dir.join("sources");
        std::fs::create_dir_all(&sources).unwrap();
        let valid = dir.join("catalog-import.json");
        std::fs::write(&valid, payload("Imported")).unwrap();
        let invalid = dir.join("bad.json");
        std::fs::write(&invalid, "{not json").unwrap();
        let not_json = dir.join("note.txt");
        std::fs::write(&not_json, "x").unwrap();

        let commands = commands(&sources, vec![]);
        let result = commands.import_files(
            &[
                valid.to_string_lossy().to_string(),
                invalid.to_string_lossy().to_string(),
                not_json.to_string_lossy().to_string(),
            ],
            &[],
        );
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].name, "catalog-import.json.gz");
        assert_eq!(result.errors.len(), 2);
        assert!(!sources.join("catalog-import.json.gz.source-url").exists());

        let duplicate = dir.join("catalog-import.json");
        std::fs::write(&duplicate, payload("New")).unwrap();
        let blocked = commands.import_files(&[duplicate.to_string_lossy().to_string()], &[]);
        assert!(blocked.added.is_empty());
        assert_eq!(blocked.errors.len(), 1);
        let allowed = commands.import_files(
            &[dir.join("catalog-import.json").to_string_lossy().to_string()],
            &["catalog-import.json.gz".to_string()],
        );
        assert_eq!(allowed.added.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_removes_gzip_and_legacy_plain() {
        let dir = temp_dir("delete");
        let path = dir.join("a.json.gz");
        storage::write_source_file(
            &path,
            &serde_json::json!({"downloads": [{"title": "x", "uris": ["magnet:x"]}]}),
        )
        .unwrap();
        storage::write_url_file(&path, "https://example.com/a.json").unwrap();
        let legacy = dir.join("a.json");
        std::fs::write(&legacy, payload("x")).unwrap();

        commands(&dir, vec![]).delete("a.json.gz").unwrap();
        assert!(!path.exists());
        assert!(!dir.join("a.json.gz.source-url").exists());
        assert!(!legacy.exists());
        assert!(commands(&dir, vec![])
            .delete("../a.json.gz")
            .is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refresh_reports_progress_and_keeps_failures() {
        let dir = temp_dir("refresh");
        let good = dir.join("a.json.gz");
        let bad = dir.join("b.json.gz");
        let no_url = dir.join("c.json.gz");
        let stale = serde_json::json!({"downloads": [{"title": "Old", "uris": ["magnet:old"]}]});
        for target in [&good, &bad, &no_url] {
            storage::write_source_file(target, &stale).unwrap();
        }
        storage::write_url_file(&good, "https://example.com/a.json").unwrap();
        storage::write_url_file(&bad, "https://example.com/b.json").unwrap();
        let commands = commands(
            &dir,
            vec![
                ("https://example.com/a.json", Ok(payload("New"))),
                ("https://example.com/b.json", Err("boom".to_string())),
            ],
        );
        let mut events = Vec::new();
        let result = commands.refresh(None, &mut |index, total, name| {
            events.push((index, total, name.to_string()))
        });
        assert_eq!(result.updated.len(), 1);
        assert_eq!(result.updated[0].name, "a.json.gz");
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].0, "b.json.gz");
        assert!(result.failed[0].1.contains("boom"));
        assert_eq!(events.len(), 2);
        let mut names: Vec<&str> = events.iter().map(|event| event.2.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["a.json.gz", "b.json.gz"]);
        let mut indices: Vec<usize> = events.iter().map(|event| event.0).collect();
        indices.sort();
        assert_eq!(indices, vec![1, 2]);
        assert!(events.iter().all(|event| event.1 == 2));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn refresh_runs_sources_in_parallel_and_keeps_order() {
        let dir = temp_dir("refresh-parallel");
        let stale = serde_json::json!({"downloads": [{"title": "Old", "uris": ["magnet:old"]}]});
        let urls = [
            "https://example.com/a.json",
            "https://example.com/b.json",
            "https://example.com/c.json",
        ];
        for (index, url) in urls.iter().enumerate() {
            let path = dir.join(format!("{index}.json.gz"));
            storage::write_source_file(&path, &stale).unwrap();
            storage::write_url_file(&path, url).unwrap();
        }
        let client = FakeClient::with_delay(
            urls.iter()
                .map(|url| (*url, Ok(payload("New"))))
                .collect(),
            Duration::from_millis(300),
        );
        let commands = commands_with_client(&dir, client);

        let started = std::time::Instant::now();
        let result = commands.refresh_with_workers(None, &mut |_, _, _| {}, 3, true);
        let elapsed = started.elapsed();

        let names: Vec<&str> = result
            .updated
            .iter()
            .map(|change| change.name.as_str())
            .collect();
        assert_eq!(names, vec!["0.json.gz", "1.json.gz", "2.json.gz"]);
        assert!(result.failed.is_empty());
        assert!(
            elapsed < Duration::from_millis(750),
            "parallel refresh took {elapsed:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
