//! Source file storage: gzip JSON, URL sidecars and atomic writes.
//! Plus application state storage: per-source snapshots, health records, delta tracking.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use flate2::{Compression, GzBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SOURCE_URL_SUFFIX: &str = ".source-url";
const STATE_DIR: &str = ".gm";
const STATE_FILE: &str = "state.json";
const STATE_BACKUP: &str = "state.json.bak";

/// Current schema version of the state file. Bump when making breaking changes.
pub const STATE_SCHEMA_VERSION: u8 = 1;

/// Application state snapshot stored alongside sources/
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStateSnapshot {
    pub version: u8,
    pub source_snapshots: HashMap<String, SourceSnapshot>,
    pub source_health: HashMap<String, SourceHealth>,
}

impl Default for AppStateSnapshot {
    fn default() -> Self {
        Self {
            version: STATE_SCHEMA_VERSION,
            source_snapshots: HashMap::new(),
            source_health: HashMap::new(),
        }
    }
}

/// Per-source fetch record: what we downloaded, when, and the validators that
/// let the next refresh skip the body entirely.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceSnapshot {
    /// SHA-256 of the downloaded body; equal hashes mean "unchanged".
    pub hash: String,
    /// `ETag` from the last response, replayed as `If-None-Match`.
    pub etag: Option<String>,
    /// `Last-Modified` from the last response, replayed as `If-Modified-Since`.
    pub last_modified: Option<String>,
    pub last_success: Option<DateTime<Utc>>,
    pub consecutive_failures: usize,
    pub last_error: Option<String>,
    /// True when the last successful fetch needed the Cloudflare browser
    /// channel. Those sources never take part in automatic refresh.
    pub via_cloudflare: bool,
}

/// Health metrics collected over time
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceHealth {
    pub success_count: usize,
    pub failure_count: usize,
    pub last_success_time: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
    pub consecutive_failures: usize,
}

/// Trait to load/save application state (sidecar file)
pub trait StateStore: Send + Sync {
    fn load(&self) -> Result<AppStateSnapshot, String>;
    fn save(&self, snap: &AppStateSnapshot) -> Result<(), String>;

    /// Fetch record for one source; missing means "never fetched".
    fn snapshot(&self, name: &str) -> SourceSnapshot {
        self.load()
            .ok()
            .and_then(|snap| snap.source_snapshots.get(name).cloned())
            .unwrap_or_default()
    }

    /// Records a successful fetch. Failure to persist only costs the next
    /// refresh its shortcut, so it must never fail the refresh itself.
    fn record_success(
        &self,
        name: &str,
        hash: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        via_cloudflare: bool,
    ) {
        let Ok(mut snap) = self.load() else {
            return;
        };
        apply_success(&mut snap, name, hash, etag, last_modified, via_cloudflare);
        if let Err(error) = self.save(&snap) {
            log::warn!("保存源状态失败: {error}");
        }
    }

    fn record_failure(&self, name: &str, error: &str, via_cloudflare: bool) {
        let Ok(mut snap) = self.load() else {
            return;
        };
        apply_failure(&mut snap, name, error, via_cloudflare);
        if let Err(error) = self.save(&snap) {
            log::warn!("保存源状态失败: {error}");
        }
    }
}

fn apply_success(
    snap: &mut AppStateSnapshot,
    name: &str,
    hash: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
    via_cloudflare: bool,
) {
    let now = Utc::now();
    let entry = snap.source_snapshots.entry(name.to_string()).or_default();
    entry.hash = hash.to_string();
    entry.etag = etag.map(str::to_string);
    entry.last_modified = last_modified.map(str::to_string);
    entry.via_cloudflare = via_cloudflare;
    entry.last_success = Some(now);
    entry.consecutive_failures = 0;
    entry.last_error = None;

    let health = snap.source_health.entry(name.to_string()).or_default();
    health.success_count += 1;
    health.consecutive_failures = 0;
    health.last_success_time = Some(now);
    health.last_error_code = None;
}

fn apply_failure(snap: &mut AppStateSnapshot, name: &str, error: &str, via_cloudflare: bool) {
    let entry = snap.source_snapshots.entry(name.to_string()).or_default();
    entry.consecutive_failures += 1;
    entry.last_error = Some(error.to_string());
    // Remember that this URL needs the browser channel so automatic refresh
    // stops trying it (and stops opening browser windows).
    if via_cloudflare {
        entry.via_cloudflare = true;
    }

    let health = snap.source_health.entry(name.to_string()).or_default();
    health.failure_count += 1;
    health.consecutive_failures += 1;
    health.last_error_code = Some(error.to_string());
}

/// File-based implementation storing JSON in sources/.gm/state.json
pub struct FileStateStore {
    base_dir: PathBuf,
    /// Refresh workers record results concurrently; without this, two
    /// read-modify-write cycles can overwrite each other's validators.
    write_lock: Mutex<()>,
}

impl FileStateStore {
    /// Uses `<sources>/.gm`, or the per-user fallback when that is read-only.
    pub fn new<P: AsRef<Path>>(sources_dir: P) -> Self {
        Self {
            base_dir: state_dir(sources_dir.as_ref()),
            write_lock: Mutex::new(()),
        }
    }

    /// Uses exactly `dir`, no probing. Only meant for tests.
    pub fn with_dir<P: AsRef<Path>>(dir: P) -> Self {
        Self {
            base_dir: dir.as_ref().to_path_buf(),
            write_lock: Mutex::new(()),
        }
    }

    fn guard(&self) -> std::sync::MutexGuard<'_, ()> {
        // A panic elsewhere must not disable state persistence.
        self.write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn state_file(&self) -> PathBuf {
        self.base_dir.join(STATE_FILE)
    }

    fn backup_file(&self) -> PathBuf {
        self.base_dir.join(STATE_BACKUP)
    }

    fn atomic_save(&self, snap: &AppStateSnapshot) -> Result<(), String> {
        fs::create_dir_all(&self.base_dir)
            .map_err(|e| format!("Create state dir {}: {e}", self.base_dir.display()))?;
        let tmp = self.state_file().with_extension("tmp");
        let serialized = serde_json::to_string_pretty(snap)
            .map_err(|e| format!("Serialize error: {}", e))?;

        // Keep the previous file around so a half-written state (or a manual
        // edit) can be recovered by hand; it is optional, never fatal.
        if let Err(back_err) = fs::copy(self.state_file(), self.backup_file()) {
            log::warn!("Could not backup old state file (this is optional): {back_err}");
        }

        // Write temp file then rename atomically
        fs::write(&tmp, &serialized).map_err(|e| format!("Write temp file error: {}", e))?;
        fs::rename(&tmp, self.state_file()).map_err(|e| format!("Rename to final file error: {}", e))?;
        Ok(())
    }
}

impl StateStore for FileStateStore {
    fn load(&self) -> Result<AppStateSnapshot, String> {
        match fs::read_to_string(self.state_file()) {
            Ok(contents) => match serde_json::from_str::<AppStateSnapshot>(&contents) {
                Ok(snap) => {
                    if snap.version != STATE_SCHEMA_VERSION {
                        log::warn!(
                            "State version mismatch: expected {}, got {}",
                            STATE_SCHEMA_VERSION,
                            snap.version
                        );
                    }
                    Ok(snap)
                }
                // A corrupt file must not cost the user their validators
                // forever: read the copy taken before the last write.
                Err(error) => fs::read_to_string(self.backup_file())
                    .ok()
                    .and_then(|backup| serde_json::from_str::<AppStateSnapshot>(&backup).ok())
                    .ok_or_else(|| format!("Parse state JSON: {error}")),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(AppStateSnapshot::default())
            }
            Err(error) => Err(format!("Read state file: {error}")),
        }
    }

    fn save(&self, snap: &AppStateSnapshot) -> Result<(), String> {
        self.atomic_save(snap)
    }

    /// Same as the default, but the whole read-modify-write happens under the
    /// store's lock so parallel refresh workers cannot clobber each other.
    fn record_success(
        &self,
        name: &str,
        hash: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        via_cloudflare: bool,
    ) {
        let _guard = self.guard();
        let Ok(mut snap) = self.load() else {
            return;
        };
        apply_success(&mut snap, name, hash, etag, last_modified, via_cloudflare);
        if let Err(error) = self.save(&snap) {
            log::warn!("保存源状态失败: {error}");
        }
    }

    fn record_failure(&self, name: &str, error: &str, via_cloudflare: bool) {
        let _guard = self.guard();
        let Ok(mut snap) = self.load() else {
            return;
        };
        apply_failure(&mut snap, name, error, via_cloudflare);
        if let Err(error) = self.save(&snap) {
            log::warn!("保存源状态失败: {error}");
        }
    }
}

/// Directory that holds user-owned state files (favorites, handled markers).
///
/// Prefers `<sources>/.gm` so a portable copy carries its state along, and
/// falls back to `%APPDATA%\GameMagNet` when the executable sits in a read-only
/// install directory. `GAMEMAGNET_STATE_DIR` overrides both, for tests.
pub fn state_dir(sources_dir: &Path) -> PathBuf {
    if let Ok(path) = std::env::var("GAMEMAGNET_STATE_DIR") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    let preferred = sources_dir.join(STATE_DIR);
    if ensure_writable(&preferred) {
        return preferred;
    }
    log::warn!(
        "状态目录 {} 不可写，改用用户目录",
        preferred.display()
    );
    let fallback = fallback_state_dir();
    let _ = fs::create_dir_all(&fallback);
    fallback
}

fn ensure_writable(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(".write-probe");
    match fs::write(&probe, b"1") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn fallback_state_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("GameMagNet");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/share/gamemagnet");
    }
    std::env::temp_dir().join("gamemagnet")
}

pub fn sources_dir() -> PathBuf {
    if let Ok(path) = std::env::var("GAMEMAGNET_SOURCES") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sources");
        if dev.is_dir() {
            return dev;
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sources")
}

pub fn source_filenames(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    let packed: Vec<&String> = names
        .iter()
        .filter(|name| name.to_lowercase().ends_with(".json.gz"))
        .collect();
    let plain: Vec<&String> = names
        .iter()
        .filter(|name| name.to_lowercase().ends_with(".json"))
        .collect();
    let shadowed: Vec<String> = packed
        .iter()
        .map(|name| name[..name.len() - 3].to_string())
        .collect();

    let mut result: Vec<String> = packed
        .iter()
        .map(|name| (*name).clone())
        .chain(
            plain
                .iter()
                .filter(|name| !shadowed.contains(name))
                .map(|name| (*name).clone()),
        )
        .collect();
    result.sort();
    result
}

pub fn read_source_url(path: &Path) -> Option<String> {
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(SOURCE_URL_SUFFIX);
    let text = fs::read_to_string(PathBuf::from(sidecar)).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn write_url_file(path: &Path, url: &str) -> std::io::Result<()> {
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(SOURCE_URL_SUFFIX);
    fs::write(PathBuf::from(sidecar), url)
}

pub fn read_source_file(path: &Path) -> Result<Value, String> {
    let is_gzip = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("gz"))
        .unwrap_or(false);
    let mut bytes = Vec::new();
    if is_gzip {
        let file = fs::File::open(path).map_err(|err| err.to_string())?;
        GzDecoder::new(file)
            .read_to_end(&mut bytes)
            .map_err(|err| err.to_string())?;
    } else {
        bytes = fs::read(path).map_err(|err| err.to_string())?;
    }
    serde_json::from_slice(&bytes).map_err(|err| err.to_string())
}

pub fn write_source_file(path: &Path, data: &Value) -> std::io::Result<()> {
    // Unique temp names: a manual refresh and a background refresh can target
    // the same source, and a shared `x.json.tmp` lets the two writes interleave.
    static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut payload = serde_json::to_vec(data)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let is_gzip = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("gz"))
        .unwrap_or(false);
    if is_gzip {
        let mut encoder = GzBuilder::new()
            .mtime(0)
            .write(Vec::new(), Compression::new(6));
        encoder.write_all(&payload)?;
        payload = encoder.finish()?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, &payload)?;
    fs::rename(&tmp, path)
}

pub fn file_updated(path: &Path) -> f64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn gzip_roundtrip_and_sidecar() {
        let dir = temp_dir("storage");
        let path = dir.join("demo.json.gz");
        let data = serde_json::json!({"repacks": [{"title": "游戏"}]});
        write_source_file(&path, &data).unwrap();
        write_url_file(&path, "https://example.com/demo.json").unwrap();
        assert_eq!(read_source_file(&path).unwrap(), data);
        assert_eq!(
            read_source_url(&path).as_deref(),
            Some("https://example.com/demo.json")
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn state_dir_prefers_the_directory_next_to_the_sources() {
        let dir = temp_dir("state-dir");
        assert_eq!(state_dir(&dir), dir.join(STATE_DIR));
        assert!(dir.join(STATE_DIR).is_dir());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn packed_sources_shadow_plain_ones() {
        let dir = temp_dir("shadow");
        for name in ["a.json", "a.json.gz", "b.json", "c.json.gz"] {
            fs::write(dir.join(name), b"{}").unwrap();
        }
        let names = source_filenames(&dir);
        assert_eq!(names, vec!["a.json.gz", "b.json", "c.json.gz"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_source_that_never_loaded_reports_an_empty_snapshot() {
        let dir = temp_dir("state-empty");
        let store = FileStateStore::with_dir(&dir);
        let snap = store.snapshot("missing.json");
        assert!(snap.hash.is_empty());
        assert!(snap.last_success.is_none());
        assert!(!snap.via_cloudflare);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn success_stores_hash_and_validators_and_clears_failures() {
        let dir = temp_dir("state-success");
        let store = FileStateStore::with_dir(&dir);
        store.record_failure("a.json", "connection reset", false);
        store.record_success(
            "a.json",
            "abc123",
            Some("\"v1\""),
            Some("Wed, 21 Oct 2026 07:28:00 GMT"),
            false,
        );

        let snap = store.snapshot("a.json");
        assert_eq!(snap.hash, "abc123");
        assert_eq!(snap.etag.as_deref(), Some("\"v1\""));
        assert_eq!(
            snap.last_modified.as_deref(),
            Some("Wed, 21 Oct 2026 07:28:00 GMT")
        );
        assert!(snap.last_success.is_some());
        assert_eq!(snap.consecutive_failures, 0);
        assert!(snap.last_error.is_none());

        let health = store.load().unwrap().source_health["a.json"].clone();
        assert_eq!(health.success_count, 1);
        assert_eq!(health.failure_count, 1);
        assert_eq!(health.consecutive_failures, 0);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failure_keeps_the_previous_hash_so_the_next_refresh_can_still_compare() {
        let dir = temp_dir("state-failure");
        let store = FileStateStore::with_dir(&dir);
        store.record_success("a.json", "abc123", None, None, false);
        store.record_failure("a.json", "timed out", false);
        store.record_failure("a.json", "timed out", false);

        let snap = store.snapshot("a.json");
        assert_eq!(snap.hash, "abc123");
        assert_eq!(snap.consecutive_failures, 2);
        assert_eq!(snap.last_error.as_deref(), Some("timed out"));
        assert!(snap.last_success.is_some());
        assert!(!snap.via_cloudflare);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_cloudflare_failure_is_remembered_so_auto_refresh_skips_it() {
        let dir = temp_dir("state-cf-failure");
        let store = FileStateStore::with_dir(&dir);
        store.record_failure("cf.json", "Cloudflare 人机验证", true);
        assert!(store.snapshot("cf.json").via_cloudflare);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cloudflare_sources_are_remembered_as_such() {
        let dir = temp_dir("state-cf");
        let store = FileStateStore::with_dir(&dir);
        store.record_success("cf.json", "h", None, None, true);
        assert!(store.snapshot("cf.json").via_cloudflare);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn concurrent_records_keep_every_entry() {
        let dir = temp_dir("state-parallel");
        let store = std::sync::Arc::new(FileStateStore::with_dir(&dir));
        let mut handles = Vec::new();
        for worker in 0..4 {
            let store = std::sync::Arc::clone(&store);
            handles.push(std::thread::spawn(move || {
                for index in 0..10 {
                    store.record_success(
                        &format!("{worker}-{index}.json"),
                        "hash",
                        None,
                        None,
                        false,
                    );
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(store.load().unwrap().source_snapshots.len(), 40);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_corrupt_state_file_falls_back_to_the_backup_copy() {        let dir = temp_dir("state-corrupt");
        let store = FileStateStore::with_dir(&dir);
        store.record_success("a.json", "abc123", None, None, false);
        // Second save copies the good file to .bak before overwriting.
        store.record_success("a.json", "def456", None, None, false);
        fs::write(dir.join(STATE_FILE), b"{ this is not json").unwrap();

        let snap = store.snapshot("a.json");
        assert_eq!(snap.hash, "abc123", "the backup predates the bad write");
        fs::remove_dir_all(&dir).ok();
    }
}
