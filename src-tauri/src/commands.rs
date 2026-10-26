//! Commands that own every write to the sources directory.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::catalog::extract_repacks;
use crate::net::Downloader;
use crate::storage;

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
    pub updated: Vec<SourceChange>,
    pub failed: Vec<(String, String)>,
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
}

impl SourceCommands {
    pub fn new(sources_dir: PathBuf, downloader: Arc<Downloader>) -> Self {
        Self {
            sources_dir,
            downloader,
        }
    }

    pub fn fetch(&self, url: &str) -> Result<FetchedSource, CommandError> {
        let content = self.downloader.download_json(url).map_err(|error| {
            match error.cloudflare_url.clone() {
                Some(cloudflare_url) => CommandError::Cloudflare {
                    message: error.message,
                    url: cloudflare_url,
                },
                None => CommandError::Message(error.message),
            }
        })?;
        let default_name = url_to_filename(url);
        let (data, count) = parse_content(&content)?;
        Ok(FetchedSource {
            url: url.to_string(),
            data,
            count,
            default_name,
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
        self.write(&name, &fetched.url, &fetched.data)?;
        Ok(SourceChange {
            name,
            count: Some(fetched.count),
        })
    }

    pub fn update(&self, name: &str, url: Option<&str>) -> Result<SourceChange, CommandError> {
        let name = self.checked_name(name)?;
        let path = self.sources_dir.join(&name);
        let url = url
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
            .or_else(|| storage::read_source_url(&path))
            .ok_or_else(|| CommandError::Message(format!("{name}: 没有记录源地址")))?;
        let fetched = self
            .fetch(&url)
            .map_err(|error| CommandError::Message(format!("{name}: {error}")))?;
        self.write(&name, &url, &fetched.data)?;
        Ok(SourceChange {
            name,
            count: Some(fetched.count),
        })
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
        let mut result = RefreshResult::default();
        for (index, (name, url)) in targets.into_iter().enumerate() {
            report(index + 1, total, &name);
            match self.fetch(&url) {
                Ok(fetched) => match self.write(&name, &url, &fetched.data) {
                    Ok(()) => result.updated.push(SourceChange {
                        name,
                        count: Some(fetched.count),
                    }),
                    Err(error) => result.failed.push((name, error.to_string())),
                },
                Err(error) => result.failed.push((name, error.to_string())),
            }
        }
        result
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
    }

    impl FakeClient {
        fn new(entries: Vec<(&str, Result<String, String>)>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(
                    entries
                        .into_iter()
                        .map(|(url, result)| (url.to_string(), result))
                        .collect(),
                ),
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

    fn commands(dir: &Path, entries: Vec<(&str, Result<String, String>)>) -> SourceCommands {
        SourceCommands::new(dir.to_path_buf(), Arc::new(Downloader::new(FakeClient::new(entries))))
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
        let change = update_commands.update("a.json.gz", None).unwrap();
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
        assert_eq!(
            events,
            vec![(1, 2, "a.json.gz".to_string()), (2, 2, "b.json.gz".to_string())]
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
