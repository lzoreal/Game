//! Application state and task dispatch, mirroring the previous QML backend.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chrono::Local;
use serde::Serialize;
use serde_json::{json, Value};

use crate::bt::{self, HandoffPlan, SystemProbe, Target};
use crate::catalog::{
    latest_items, search_items, stream_sources, LATEST_LIMIT, MAX_RESULTS,
};
use crate::commands::{CommandError, SourceCommands};
use crate::detail::{build_detail, DetailCard};
use crate::model::{Group, Item, Row, SourceMeta, SourceRow};
use crate::net::Downloader;
use crate::platform::Platform;
use crate::search::{parse_query, sort_by_date};
use crate::storage;
use crate::swarm;
use crate::update::{self, UpdateResult};
use crate::user::{UserState, UserStore};

pub const APP_VERSION: &str = "v1.5.1";
const DEBOUNCE_MS: u64 = 250;
const RAW_LIMIT: usize = 100_000;
/// Auto refresh waits long enough for the first load and the user's first
/// click to finish, then only touches sources that have gone stale.
const AUTO_REFRESH_DELAY: Duration = Duration::from_secs(60);
const AUTO_REFRESH_STALE_SECS: i64 = 24 * 60 * 60;

pub type Emitter = Arc<dyn Fn(&str, Value) + Send + Sync>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BtCatalogRow {
    pub key: &'static str,
    pub name: &'static str,
    pub badge: &'static str,
    pub color: &'static str,
    pub desc: &'static str,
    pub url: &'static str,
    pub recommended: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    All,
    Favorites,
    Unhandled,
}

impl View {
    fn parse(value: &str) -> Self {
        match value {
            "favorites" => Self::Favorites,
            "unhandled" => Self::Unhandled,
            _ => Self::All,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Favorites => "favorites",
            Self::Unhandled => "unhandled",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortMode {
    FavoriteDesc,
    DateDesc,
    DateAsc,
    TitleAsc,
}

impl SortMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::FavoriteDesc => "favoriteDesc",
            Self::DateDesc => "dateDesc",
            Self::DateAsc => "dateAsc",
            Self::TitleAsc => "titleAsc",
        }
    }

    /// The button cycles through the date and title orders; the favorites
    /// order is the entry point of the favorites view, not part of the cycle.
    fn next(self) -> Self {
        match self {
            Self::FavoriteDesc => Self::DateDesc,
            Self::DateDesc => Self::DateAsc,
            Self::DateAsc => Self::TitleAsc,
            Self::TitleAsc => Self::DateDesc,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRow {
    pub source: String,
    pub size: String,
    pub date: String,
    /// True for the entry the favorite remembers.
    pub chosen: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: &'static str,
    pub sources_dir: String,
    pub sources: Vec<SourceRow>,
    pub page_title: String,
    pub page_sub: String,
    pub scope_text: String,
    pub count_text: String,
    pub elapsed_text: String,
    pub status: String,
    pub busy: bool,
    pub latest_loading: bool,
    pub query: String,
    pub fuzzy: bool,
    pub threshold: f64,
    pub active_source: String,
    pub active_source_display: String,
    pub active_source_url: String,
    pub sort_mode: &'static str,
    pub view: &'static str,
    pub show_handled: bool,
    pub blocked: Vec<String>,
    pub block_enabled: bool,
    pub favorite_count: usize,
    pub handled_count: usize,
    pub total_groups: usize,
    pub user_state_path: String,
    #[serde(rename = "selectedResultIndex")]
    pub selected_index: i64,
    pub has_detail: bool,
    pub bt_targets: Vec<Target>,
    pub bt_catalog: Vec<BtCatalogRow>,
    pub update_info: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DetailPayload {
    pub title: String,
    pub key: String,
    pub sources: Vec<String>,
    pub meta: String,
    pub hint: String,
    pub cards: Vec<DetailCard>,
    pub raw: String,
    pub has: bool,
    pub favorite: bool,
    pub handled: bool,
    pub broken: bool,
    pub compare: Vec<CompareRow>,
}

impl Default for DetailPayload {
    fn default() -> Self {
        Self {
            title: String::new(),
            key: String::new(),
            sources: Vec::new(),
            meta: String::new(),
            hint: String::new(),
            cards: Vec::new(),
            raw: String::new(),
            has: false,
            favorite: false,
            handled: false,
            broken: false,
            compare: Vec::new(),
        }
    }
}

struct Inner {
    items: Vec<Item>,
    metas: BTreeMap<String, SourceMeta>,
    group_keys: HashSet<u64>,
    query: String,
    fuzzy: bool,
    threshold: f64,
    active_source: String,
    sort_mode: SortMode,
    view: View,
    show_handled: bool,
    user: UserState,
    selected_index: Option<usize>,
    groups: Vec<Group>,
    status: String,
    count_text: String,
    elapsed_text: String,
    latest_cache: HashMap<String, (Vec<Group>, usize)>,
    latest_inflight: HashSet<String>,
    latest_wanted: Option<String>,
    latest_gen: u64,
    latest_loading: bool,
    search_gen: u64,
    swarm_texts: HashMap<String, String>,
    swarm_pending: HashSet<String>,
    bt_targets: Vec<Target>,
    update_info: Option<Value>,
    detail: DetailPayload,
    detail_raw: Option<(String, String)>,
    active_counts: HashMap<String, usize>,
}

pub struct AppState {
    inner: Mutex<Inner>,
    pub sources_dir: PathBuf,
    downloader: Arc<Downloader>,
    commands: SourceCommands,
    platform: Arc<dyn Platform>,
    emitter: Emitter,
    user_store: UserStore,
}

fn group_digits(value: usize) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn ready_status(inner: &Inner) -> String {
    if inner.metas.is_empty() {
        return "就绪".to_string();
    }
    format!(
        "就绪 · {} 个源 · {} 个游戏",
        inner.metas.len(),
        group_digits(inner.items.len())
    )
}

fn source_updated_text(meta: &SourceMeta) -> String {
    if meta.updated <= 0.0 {
        return "未知".to_string();
    }
    let seconds = meta.updated as i64;
    match chrono::DateTime::from_timestamp(seconds, 0) {
        Some(utc) => utc
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        None => "未知".to_string(),
    }
}

fn scope_text(inner: &Inner) -> String {
    inner
        .metas
        .get(&inner.active_source)
        .map(|meta| meta.display.clone())
        .unwrap_or_else(|| "全部游戏".to_string())
}

/// The link a favorite remembers: the newest item's first link, the same entry
/// the detail pane shows first.
fn remembered_uri_of(group: &Group) -> String {
    group
        .items
        .iter()
        .find_map(|item| item.uris.first().map(|uri| uri.to_string()))
        .unwrap_or_default()
}

fn group_broken(group: &Group, user: &UserState) -> bool {
    let Some(uri) = user.remembered_uri(&group.key) else {
        return false;
    };
    !group.uris.iter().any(|candidate| candidate == uri)
}

fn env_flag_off(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            )
        })
        .unwrap_or(false)
}

/// Filters the view (`all` / `favorites` / `unhandled`) and the blocklist.
/// Phrase/exclusion filtering is *not* done here: `search_items` already
/// applies the grammar to each item's normalized title, and re-testing the
/// group's `key` would drop valid hits because `key` has version numbers
/// stripped (`"v1.1"` matched `alpha game v1 1`, but `alpha game` does not
/// contain it). The latest list, which has no positive term, is filtered against
/// `key` by `arrange_groups`.
fn view_keeps(inner: &Inner, group: &Group) -> bool {
    if inner.user.is_blocked(&group.key) {
        return false;
    }
    let handled = inner.user.is_handled(&group.key);
    match inner.view {
        View::All => inner.show_handled || !handled,
        View::Favorites => inner.user.is_favorite(&group.key) && (inner.show_handled || !handled),
        View::Unhandled => !handled,
    }
}

fn order_groups(groups: &mut Vec<Group>, mode: SortMode, user: &UserState) {
    match mode {
        SortMode::DateDesc | SortMode::DateAsc => {
            let taken = std::mem::take(groups);
            *groups = sort_by_date(taken, mode == SortMode::DateDesc);
        }
        SortMode::TitleAsc => {
            groups.sort_by_cached_key(|group| group.title.to_lowercase());
        }
        SortMode::FavoriteDesc => {
            groups.sort_by(|left, right| {
                user.favorite_time(&right.key)
                    .cmp(&user.favorite_time(&left.key))
            });
        }
    }
}

fn arrange_groups(groups: Vec<Group>, inner: &Inner, filter_query: bool) -> Vec<Group> {
    let parsed = parse_query(&inner.query);
    let mut groups: Vec<Group> = groups
        .into_iter()
        .filter(|group| view_keeps(inner, group))
        .filter(|group| !filter_query || parsed.matches(&group.key))
        .collect();
    order_groups(&mut groups, inner.sort_mode, &inner.user);
    groups
}

/// True when the query has at least one positive term, so an in-flight latest
/// list must not overwrite the search results. A query made only of
/// `-exclusions` still uses the latest list as its base set.
fn query_has_positive(query: &str) -> bool {
    !query.is_empty() && parse_query(query).has_positive()
}

fn build_detail_payload(group: &Group, user: &UserState) -> DetailPayload {
    let expanded = HashSet::new();
    let model = build_detail(group, &expanded, false, 100_000, RAW_LIMIT);
    let remembered = user.remembered_uri(&group.key);
    let compare = group
        .items
        .iter()
        .map(|item| CompareRow {
            source: item.source_display.to_string(),
            size: item.file_size.to_string(),
            date: match item.date_dt {
                Some(dt) => crate::dates::format_date(&dt),
                None => item.upload_date.to_string(),
            },
            chosen: remembered
                .map(|uri| item.uris.iter().any(|candidate| candidate.as_ref() == uri))
                .unwrap_or(false),
        })
        .collect();
    DetailPayload {
        title: model.title,
        key: group.key.clone(),
        sources: model.sources,
        meta: model.meta,
        hint: model.hint,
        cards: model.cards,
        raw: String::new(),
        has: true,
        favorite: user.is_favorite(&group.key),
        handled: user.is_handled(&group.key),
        broken: group_broken(group, user),
        compare,
    }
}

fn normalize_source_urls(lines: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut urls = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            trimmed.to_string()
        } else {
            format!("https://{trimmed}")
        };
        if seen.insert(url.clone()) {
            urls.push(url);
        }
    }
    urls
}

fn download_release_bytes(downloader: &Downloader, url: &str) -> Result<Vec<u8>, String> {
    let headers = [
        ("User-Agent", update::USER_AGENT),
        ("Accept", "application/octet-stream"),
    ];
    let mut errors = Vec::new();
    match downloader.download_bytes(url, &headers, Duration::from_secs(15)) {
        Ok(bytes) if !bytes.is_empty() => return Ok(bytes),
        Ok(_) => errors.push("直连返回空文件".to_string()),
        Err(error) => errors.push(format!("直连: {error}")),
    }

    if url.starts_with("https://github.com/") {
        for mirror in ["https://gh-proxy.com/", "https://ghfast.top/"] {
            let candidate = format!("{mirror}{url}");
            match downloader.download_bytes(&candidate, &headers, Duration::from_secs(60)) {
                Ok(bytes) if !bytes.is_empty() => return Ok(bytes),
                Ok(_) => errors.push(format!("{mirror} 返回空文件")),
                Err(error) => errors.push(format!("{mirror}: {error}")),
            }
        }
    }

    Err(format!("下载更新失败。{}", errors.join("；")))
}

fn replace_running_executable(bytes: &[u8]) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| format!("无法定位当前程序：{error}"))?;
    replace_executable_at(&exe, bytes)?;
    Ok(exe)
}

fn replace_executable_at(exe: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    let dir = exe
        .parent()
        .ok_or_else(|| "无法定位程序所在目录。".to_string())?;
    let name = exe
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "程序文件名无效。".to_string())?;
    let new_path = dir.join(format!("{name}.new"));
    let old_path = dir.join(format!("{name}.old"));

    std::fs::write(&new_path, bytes).map_err(|error| {
        format!("无法写入更新文件（程序目录不可写？请改用发布页中的安装包）：{error}")
    })?;
    if old_path.exists() {
        let _ = std::fs::remove_file(&old_path);
    }
    std::fs::rename(exe, &old_path).map_err(|error| format!("无法备份当前程序：{error}"))?;
    if let Err(error) = std::fs::rename(&new_path, exe) {
        let _ = std::fs::rename(&old_path, exe);
        let _ = std::fs::remove_file(&new_path);
        return Err(format!("无法替换程序文件：{error}"));
    }
    Ok(())
}

fn spawn_restarted_exe(exe: &std::path::Path) -> Result<(), String> {
    let dir = exe
        .parent()
        .ok_or_else(|| "无法定位程序所在目录。".to_string())?;
    let script = dir.join(format!("gamemagnet-restart-{}.cmd", std::process::id()));
    let content = format!(
        "@echo off\r\nping -n 3 127.0.0.1 >nul\r\nstart \"\" \"{}\"\r\ndel \"%~f0\"\r\n",
        exe.display()
    );
    std::fs::write(&script, content)
        .map_err(|error| format!("无法写入重启脚本：{error}"))?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", script.to_string_lossy().as_ref()])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法启动更新后的程序：{error}"))
    }
    #[cfg(not(windows))]
    {
        let _ = script;
        Err("当前平台不支持自动重启".to_string())
    }
}

fn group_signature(group: &Group) -> String {
    format!(
        "{}|{}|{}|{}",
        group.title,
        group.date_display,
        group.link_count,
        group.items.len()
    )
}

fn load_group_raw(dir: &std::path::Path, entries: &[(String, usize)]) -> String {
    let mut wanted: HashMap<&str, std::collections::HashSet<usize>> = HashMap::new();
    for (name, position) in entries {
        wanted.entry(name.as_str()).or_default().insert(*position);
    }

    let mut picked: HashMap<(String, usize), Value> = HashMap::new();
    for (name, positions) in &wanted {
        let Ok(data) = storage::read_source_file(&dir.join(name)) else {
            continue;
        };
        let Some((items, _)) = crate::catalog::extract_repacks(&data) else {
            continue;
        };
        let mut valid = 0usize;
        for raw in items {
            if crate::catalog::is_valid_repack(raw) {
                if positions.contains(&valid) {
                    picked.insert(((*name).to_string(), valid), raw.clone());
                }
                valid += 1;
            }
        }
    }

    let values: Vec<Value> = entries
        .iter()
        .filter_map(|(name, position)| picked.remove(&(name.clone(), *position)))
        .collect();
    let mut raw = serde_json::to_string_pretty(&Value::Array(values)).unwrap_or_default();
    if raw.chars().count() > RAW_LIMIT {
        raw = raw.chars().take(RAW_LIMIT).collect::<String>() + "

…";
    }
    raw
}

impl AppState {
    pub fn new(
        sources_dir: PathBuf,
        downloader: Arc<Downloader>,
        platform: Arc<dyn Platform>,
        emitter: Emitter,
    ) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&sources_dir);
        let commands = SourceCommands::new(sources_dir.clone(), Arc::clone(&downloader));
        let bt_targets = bt::detect(&SystemProbe);
        let user_store = UserStore::new(&sources_dir);
        let user = user_store.load();
        Arc::new(Self {
            inner: Mutex::new(Inner {
                items: Vec::new(),
                metas: BTreeMap::new(),
                group_keys: HashSet::new(),
                query: String::new(),
                fuzzy: true,
                threshold: 0.3,
                active_source: String::new(),
                sort_mode: SortMode::DateDesc,
                view: View::All,
                show_handled: false,
                user,
                selected_index: None,
                groups: Vec::new(),
                status: "正在加载源…".to_string(),
                count_text: String::new(),
                elapsed_text: String::new(),
                latest_cache: HashMap::new(),
                latest_inflight: HashSet::new(),
                latest_wanted: None,
                latest_gen: 0,
                latest_loading: false,
                search_gen: 0,
                swarm_texts: HashMap::new(),
                swarm_pending: HashSet::new(),
                bt_targets,
                update_info: None,
                detail: DetailPayload::default(),
                detail_raw: None,
                active_counts: HashMap::new(),
            }),
            sources_dir,
            downloader,
            commands,
            platform,
            emitter,
            user_store,
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn emit(&self, event: &str, payload: Value) {
        (self.emitter)(event, payload);
    }

    pub fn snapshot(&self) -> Snapshot {
        let inner = self.lock();
        let mut rows = vec![SourceRow {
            name: String::new(),
            display: "全部游戏".to_string(),
            count: inner.items.len(),
            local: false,
            active: inner.active_source.is_empty(),
        }];
        for (name, meta) in inner.metas.iter() {
            rows.push(SourceRow {
                name: name.clone(),
                display: meta.display.clone(),
                count: meta.count,
                local: meta.url.is_none(),
                active: *name == inner.active_source,
            });
        }
        rows.sort_by(|left, right| {
            if left.name.is_empty() {
                std::cmp::Ordering::Less
            } else if right.name.is_empty() {
                std::cmp::Ordering::Greater
            } else {
                left.display.cmp(&right.display)
            }
        });

        let active_meta = inner.metas.get(&inner.active_source);
        let (page_title, page_sub, active_display, active_url) = match active_meta {
            Some(meta) => (
                meta.display.clone(),
                format!(
                    "{} 个游戏 · 最后更新 {}",
                    group_digits(meta.count),
                    source_updated_text(meta)
                ),
                meta.display.clone(),
                meta.url.clone().unwrap_or_default(),
            ),
            None => (
                "全部游戏".to_string(),
                if inner.metas.is_empty() {
                    "正在加载源…".to_string()
                } else {
                    format!(
                        "{} 个游戏 · 去重后 {} 组",
                        group_digits(inner.items.len()),
                        group_digits(inner.group_keys.len())
                    )
                },
                String::new(),
                String::new(),
            ),
        };

        Snapshot {
            version: APP_VERSION,
            sources_dir: self.sources_dir.to_string_lossy().to_string(),
            sources: rows,
            page_title,
            page_sub,
            scope_text: scope_text(&inner),
            count_text: inner.count_text.clone(),
            elapsed_text: inner.elapsed_text.clone(),
            status: inner.status.clone(),
            busy: self.busy_locked(&inner),
            latest_loading: inner.latest_loading,
            query: inner.query.clone(),
            fuzzy: inner.fuzzy,
            threshold: inner.threshold,
            active_source: inner.active_source.clone(),
            active_source_display: active_display,
            active_source_url: active_url,
            sort_mode: inner.sort_mode.as_str(),
            view: inner.view.as_str(),
            show_handled: inner.show_handled,
            blocked: inner.user.blocked.clone(),
            block_enabled: inner.user.block_enabled,
            favorite_count: inner.user.favorites.len(),
            handled_count: inner.user.handled.len(),
            total_groups: inner.group_keys.len(),
            user_state_path: self.user_store.path().to_string_lossy().to_string(),
            selected_index: inner
                .selected_index
                .map(|index| index as i64)
                .unwrap_or(-1),
            has_detail: inner.detail.has,
            bt_targets: inner.bt_targets.clone(),
            bt_catalog: bt::CATALOG
                .iter()
                .map(|entry| BtCatalogRow {
                    key: entry.key,
                    name: entry.name,
                    badge: entry.badge,
                    color: entry.color,
                    desc: entry.desc,
                    url: entry.url,
                    recommended: entry.recommended,
                })
                .collect(),
            update_info: inner.update_info.clone(),
        }
    }

    fn busy_locked(&self, inner: &Inner) -> bool {
        inner.active_counts.iter().any(|(kind, count)| {
            *count > 0 && kind.as_str() != "search" && kind.as_str() != "latest"
        })
    }

    pub fn results_rows(&self) -> Vec<Row> {
        let inner = self.lock();
        inner
            .groups
            .iter()
            .map(|group| {
                group.row(
                    inner.user.is_favorite(&group.key),
                    inner.user.is_handled(&group.key),
                    group_broken(group, &inner.user),
                )
            })
            .collect()
    }

    pub fn detail_payload(&self) -> DetailPayload {
        self.lock().detail.clone()
    }

    pub fn swarm_texts(&self) -> HashMap<String, String> {
        self.lock().swarm_texts.clone()
    }

    pub fn emit_state(&self) {
        let snapshot = self.snapshot();
        self.emit("state", serde_json::to_value(snapshot).unwrap_or(Value::Null));
    }

    fn emit_results(&self) {
        let rows = self.results_rows();
        self.emit("results", serde_json::to_value(rows).unwrap_or(Value::Null));
    }

    fn emit_detail(&self) {
        let detail = { self.lock().detail.clone() };
        self.emit("detail", serde_json::to_value(detail).unwrap_or(Value::Null));
    }

    fn emit_swarm(&self) {
        let texts = { self.lock().swarm_texts.clone() };
        self.emit("swarm", serde_json::to_value(texts).unwrap_or(Value::Null));
    }

    fn set_status(&self, status: impl Into<String>) {
        let mut inner = self.lock();
        inner.status = status.into();
        drop(inner);
        self.emit_state();
    }

    fn begin_task(&self, kind: &str) {
        {
            let mut inner = self.lock();
            *inner.active_counts.entry(kind.to_string()).or_insert(0) += 1;
        }
        self.emit_state();
    }

    fn end_task(&self, kind: &str) {
        {
            let mut inner = self.lock();
            if let Some(count) = inner.active_counts.get_mut(kind) {
                *count = count.saturating_sub(1);
            }
            // Progress lines such as "刷新源 (1/2): a.json" do not start with
            // 正在, so the old heuristic left the status bar stuck whenever the
            // last task ended without a reload (all sources answered 304).
            let any_active = inner.active_counts.values().any(|count| *count > 0);
            if !any_active && !inner.status.starts_with("正在搜索") {
                let status = ready_status(&inner);
                inner.status = status;
            }
        }
        self.emit_state();
    }

    fn active_tasks(&self, kind: &str) -> usize {
        self.lock().active_counts.get(kind).copied().unwrap_or(0)
    }

    // ---------------------------------------------------------------- load

    pub fn start(self: &Arc<Self>) {
        self.refresh_bt_targets();
        self.load_sources();
        self.schedule_auto_refresh();
    }

    /// Background refresh, 60 seconds after startup.
    ///
    /// Only sources whose last successful fetch is more than a day old are
    /// touched, and Cloudflare-protected sources are skipped: refreshing those
    /// can open a browser window, which must never happen unprompted.
    fn schedule_auto_refresh(self: &Arc<Self>) {
        if env_flag_off("GAMEMAGNET_AUTO_REFRESH") {
            return;
        }
        let delay = std::env::var("GAMEMAGNET_AUTO_REFRESH_DELAY_MS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or(AUTO_REFRESH_DELAY);
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            state.auto_refresh();
        });
    }

    pub fn auto_refresh(self: &Arc<Self>) {
        if self.active_tasks("load") > 0
            || self.active_tasks("refresh") > 0
            || self.active_tasks("update") > 0
        {
            return;
        }
        let names = self.commands.stale_targets(AUTO_REFRESH_STALE_SECS);
        if names.is_empty() {
            return;
        }
        self.set_status(format!("自动刷新 {} 个过期源…", names.len()));
        self.begin_task("refresh");
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let progress_state = Arc::clone(&state);
            let mut report = move |index: usize, total: usize, name: &str| {
                progress_state.set_status(format!("自动刷新 ({index}/{total}): {name}"));
            };
            let result = state.commands.refresh_passive(Some(&names), &mut report);
            state.on_auto_refreshed(result);
        });
    }

    /// Auto refresh never opens a dialog: the user did not ask for it, so it
    /// reports through the status bar and a toast only when something changed.
    fn on_auto_refreshed(self: &Arc<Self>, result: crate::commands::RefreshResult) {
        // `end_task` clears the "自动刷新 (i/n)" progress line and emits state.
        self.end_task("refresh");
        if result.updated.is_empty() {
            if !result.failed.is_empty() {
                self.toast(&format!("自动刷新：{} 个源失败", result.failed.len()));
            }
            return;
        }
        self.toast(&format!("自动刷新：更新了 {} 个源", result.updated.len()));
        self.load_sources();
    }

    pub fn load_sources(self: &Arc<Self>) {
        if self.active_tasks("load") > 0 {
            return;
        }
        {
            let mut inner = self.lock();
            let preserve = inner.active_source.clone();
            inner.items.clear();
            inner.metas.clear();
            inner.group_keys.clear();
            inner.latest_gen += 1;
            inner.latest_cache.clear();
            inner.latest_inflight.clear();
            inner.latest_wanted = None;
            inner.latest_loading = false;
            inner.groups.clear();
            inner.selected_index = None;
            inner.detail = DetailPayload::default();
            inner.detail_raw = None;
            inner.count_text.clear();
            inner.elapsed_text.clear();
            inner.active_source = preserve;
            inner.status = "正在加载源…".to_string();
        }
        self.emit_state();
        self.emit_results();
        self.emit_detail();
        self.begin_task("load");

        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let dir = state.sources_dir.clone();
            let status_state = Arc::clone(&state);
            let mut progress = move |index: usize, total: usize, name: &str| {
                status_state.set_status(format!("加载源 ({index}/{total}): {name}"));
            };
            let source_state = Arc::clone(&state);
            let mut on_source = move |name: &str, items: Vec<Item>, meta: SourceMeta| {
                source_state.add_source(name, items, meta);
            };
            let errors = stream_sources(&dir, &mut progress, &mut on_source);
            state.finish_load(errors);
        });
    }

    fn add_source(&self, name: &str, items: Vec<Item>, meta: SourceMeta) {
        let keys: Vec<u64> = items.iter().map(|item| item.key_id).collect();
        let mut inner = self.lock();
        inner.items.extend(items);
        inner.group_keys.extend(keys);
        inner.metas.insert(name.to_string(), meta);
        if !inner.active_source.is_empty() && !inner.metas.contains_key(&inner.active_source) {
            inner.active_source.clear();
        }
        drop(inner);
        self.emit_state();
    }

    fn finish_load(self: &Arc<Self>, errors: Vec<String>) {
        let mut message = None;
        {
            let mut inner = self.lock();
            inner.status = ready_status(&inner);
            if !errors.is_empty() {
                inner.status = format!("{} · {} 个源加载失败", inner.status, errors.len());
                if inner.metas.is_empty() {
                    message = Some(errors.iter().take(20).cloned().collect::<Vec<_>>().join("\n"));
                }
            }
        }
        if let Some(text) = message {
            self.show_message("加载源失败", &text, true, "");
        }
        let has_query = !self.lock().query.is_empty();
        self.end_task("load");
        if has_query {
            self.run_search_now();
        } else {
            self.run_latest();
        }
    }

    // -------------------------------------------------------------- search

    pub fn set_query(self: &Arc<Self>, text: &str) {
        let query = text.trim().to_string();
        {
            let mut inner = self.lock();
            inner.query = query.clone();
            inner.search_gen += 1;
            if inner.query.is_empty() {
                inner.status = ready_status(&inner);
            } else {
                inner.status = "正在搜索…".to_string();
            }
        }
        self.emit_state();
        if query.is_empty() {
            self.run_latest();
        } else {
            self.schedule_search(DEBOUNCE_MS);
        }
    }

    fn schedule_search(self: &Arc<Self>, delay_ms: u64) {
        let gen = { self.lock().search_gen };
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            if delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
            let current = { state.lock().search_gen };
            if current != gen {
                return;
            }
            state.search_now(gen);
        });
    }

    pub fn run_search_now(self: &Arc<Self>) {
        let gen = {
            let mut inner = self.lock();
            inner.search_gen += 1;
            inner.search_gen
        };
        self.search_now(gen);
    }

    fn search_now(self: &Arc<Self>, gen: u64) {
        let started = Instant::now();
        let mut inner = self.lock();
        if inner.search_gen != gen {
            return;
        }
        let query = inner.query.clone();
        if query.is_empty() || inner.items.is_empty() {
            inner.groups.clear();
            inner.selected_index = None;
            inner.detail = DetailPayload::default();
            inner.count_text.clear();
            inner.elapsed_text.clear();
            inner.status = ready_status(&inner);
            drop(inner);
            self.emit_state();
            self.emit_results();
            self.emit_detail();
            return;
        }

        inner.latest_wanted = None;
        inner.latest_loading = false;
        let parsed = parse_query(&query);
        if !parsed.has_positive() {
            // `-合集` alone: there is nothing to score, so the latest list is
            // the base set and `arrange_groups` drops the exclusions.
            drop(inner);
            self.run_latest();
            return;
        }
        let filter = if inner.active_source.is_empty() {
            String::new()
        } else {
            scope_text(&inner)
        };
        let (groups, total) = search_items(
            &inner.items,
            &parsed,
            inner.fuzzy,
            inner.threshold,
            &filter,
            MAX_RESULTS,
        );
        let groups = arrange_groups(groups, &inner, false);
        let shown = groups.len();
        let elapsed = started.elapsed().as_secs_f64();
        inner.groups = groups;
        inner.detail_raw = None;
        inner.count_text = if shown == total {
            format!("找到 {} 组", group_digits(shown))
        } else {
            format!("找到 {} 组 · 共 {} 组", group_digits(shown), group_digits(total))
        };
        inner.elapsed_text = format!("{elapsed:.2} 秒");
        if inner.groups.is_empty() {
            inner.selected_index = None;
            inner.detail = DetailPayload::default();
        } else {
            inner.selected_index = Some(0);
            inner.detail = build_detail_payload(&inner.groups[0], &inner.user);
        }
        inner.status = ready_status(&inner);
        drop(inner);

        self.emit_state();
        self.emit_results();
        self.emit_detail();
    }

    pub fn set_fuzzy(self: &Arc<Self>, enabled: bool) {
        {
            let mut inner = self.lock();
            inner.fuzzy = enabled;
        }
        self.emit_state();
        if !self.lock().query.is_empty() {
            self.schedule_search(DEBOUNCE_MS);
        }
    }

    pub fn set_threshold(self: &Arc<Self>, value: f64) {
        {
            let mut inner = self.lock();
            inner.threshold = value;
        }
        self.emit_state();
        if !self.lock().query.is_empty() {
            self.schedule_search(DEBOUNCE_MS);
        }
    }

    pub fn select_source(self: &Arc<Self>, name: &str) {
        {
            let mut inner = self.lock();
            if !name.is_empty() && !inner.metas.contains_key(name) {
                return;
            }
            if inner.active_source == name {
                return;
            }
            inner.active_source = name.to_string();
            inner.status = ready_status(&inner);
        }
        self.emit_state();
        if !self.lock().query.is_empty() {
            self.schedule_search(DEBOUNCE_MS);
        } else {
            self.run_latest();
        }
    }

    pub fn toggle_sort(&self) {
        let mut inner = self.lock();
        inner.sort_mode = inner.sort_mode.next();
        let Inner {
            groups,
            sort_mode,
            user,
            detail,
            selected_index,
            detail_raw,
            ..
        } = &mut *inner;
        order_groups(groups, *sort_mode, user);
        *detail_raw = None;
        if selected_index.is_some() && !groups.is_empty() {
            *selected_index = Some(0);
            *detail = build_detail_payload(&groups[0], user);
        }
        drop(inner);
        self.emit_state();
        self.emit_results();
        self.emit_detail();
    }

    // ---------------------------------------------------------- user state

    fn refresh_current(self: &Arc<Self>) {
        if !self.lock().query.is_empty() {
            self.run_search_now();
        } else {
            self.run_latest();
        }
    }

    /// Writes the current user state while holding the state lock, so two quick
    /// toggles cannot persist out of order (the second clone used to be able to
    /// land on disk before the first and lose the newer change).
    fn persist_user(&self) {
        let error = {
            let inner = self.lock();
            self.user_store.save(&inner.user).err()
        };
        if let Some(error) = error {
            log::warn!("保存用户状态失败: {error}");
            self.toast(&format!("保存用户状态失败: {error}"));
        }
    }

    pub fn set_view(self: &Arc<Self>, view: &str) {
        let next = View::parse(view);
        {
            let mut inner = self.lock();
            if inner.view == next {
                return;
            }
            inner.view = next;
            inner.sort_mode = match next {
                View::Favorites => SortMode::FavoriteDesc,
                _ if inner.sort_mode == SortMode::FavoriteDesc => SortMode::DateDesc,
                _ => inner.sort_mode,
            };
        }
        self.emit_state();
        self.refresh_current();
    }

    pub fn set_show_handled(self: &Arc<Self>, enabled: bool) {
        {
            let mut inner = self.lock();
            if inner.show_handled == enabled {
                return;
            }
            inner.show_handled = enabled;
        }
        self.emit_state();
        self.refresh_current();
    }

    pub fn toggle_favorite(self: &Arc<Self>, key: &str) {
        let now = chrono::Utc::now().timestamp();
        let favorite_view = {
            let mut inner = self.lock();
            let uri = inner
                .groups
                .iter()
                .find(|group| group.key == key)
                .map(remembered_uri_of)
                .unwrap_or_default();
            inner.user.toggle_favorite(key, &uri, now);
            if inner.detail.has && inner.detail.key == key {
                if let Some(group) = inner.groups.iter().find(|group| group.key == key) {
                    inner.detail = build_detail_payload(group, &inner.user);
                }
            }
            inner.view == View::Favorites
        };
        self.persist_user();
        if favorite_view {
            // Un-favoriting removes the row from this very view.
            self.refresh_current();
        } else {
            self.emit_state();
            self.emit_results();
            self.emit_detail();
        }
    }

    pub fn toggle_handled(self: &Arc<Self>, key: &str) {
        {
            let mut inner = self.lock();
            inner.user.toggle_handled(key);
        }
        self.persist_user();
        self.refresh_current();
    }

    /// Replaces the whole list. Adding a term turns filtering back on, so the
    /// user sees the effect of what they just typed.
    pub fn set_blocklist(self: &Arc<Self>, terms: Vec<String>) {
        {
            let mut inner = self.lock();
            inner.user.set_blocked(terms);
        }
        self.persist_user();
        self.emit_state();
        self.refresh_current();
    }

    pub fn set_block_enabled(self: &Arc<Self>, enabled: bool) {
        {
            let mut inner = self.lock();
            if inner.user.block_enabled == enabled {
                return;
            }
            inner.user.set_block_enabled(enabled);
        }
        self.persist_user();
        self.emit_state();
        self.refresh_current();
    }

    // -------------------------------------------------------------- latest

    fn run_latest(self: &Arc<Self>) {
        let cache_key = { self.lock().active_source.clone() };
        {
            let mut inner = self.lock();
            inner.search_gen += 1;
        }

        let cached = {
            let inner = self.lock();
            if inner.items.is_empty() {
                None
            } else {
                Some(inner.latest_cache.get(&cache_key).cloned())
            }
        };
        let items_empty = self.lock().items.is_empty();
        if items_empty {
            self.clear_results();
            return;
        }
        if let Some(Some((groups, total))) = cached {
            {
                let mut inner = self.lock();
                inner.latest_wanted = Some(cache_key.clone());
                inner.latest_loading = false;
            }
            self.apply_latest(groups, total);
            return;
        }
        let already_inflight = {
            let mut inner = self.lock();
            inner.latest_wanted = Some(cache_key.clone());
            if inner.latest_inflight.contains(&cache_key) {
                inner.latest_loading = true;
                true
            } else {
                false
            }
        };
        if already_inflight {
            self.emit_state();
            return;
        }

        {
            let mut inner = self.lock();
            inner.groups.clear();
            inner.selected_index = None;
            inner.detail = DetailPayload::default();
            inner.count_text.clear();
            inner.elapsed_text.clear();
            inner.latest_inflight.insert(cache_key.clone());
            inner.latest_loading = true;
        }
        self.emit_state();
        self.emit_results();
        self.emit_detail();

        let gen = { self.lock().latest_gen };
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let (groups, total) = {
                let inner = state.lock();
                let filter = if inner.active_source.is_empty() {
                    String::new()
                } else {
                    scope_text(&inner)
                };
                latest_items(&inner.items, &filter, LATEST_LIMIT)
            };
            state.finish_latest(gen, cache_key, groups, total);
        });
    }

    fn finish_latest(self: &Arc<Self>, gen: u64, cache_key: String, groups: Vec<Group>, total: usize) {
        let apply;
        {
            let mut inner = self.lock();
            if inner.latest_gen != gen {
                return;
            }
            inner.latest_inflight.remove(&cache_key);
            inner.latest_cache.insert(cache_key.clone(), (groups.clone(), total));
            if query_has_positive(&inner.query)
                || inner.latest_wanted.as_deref() != Some(cache_key.as_str())
            {
                inner.latest_loading = false;
                drop(inner);
                self.emit_state();
                return;
            }
            inner.latest_loading = false;
            apply = Some(groups);
        }
        if let Some(groups) = apply {
            self.apply_latest(groups, total);
        }
    }

    fn apply_latest(&self, groups: Vec<Group>, total: usize) {
        {
            let mut inner = self.lock();
            let groups = arrange_groups(groups, &inner, true);
            let shown = groups.len();
            inner.groups = groups;
            inner.detail_raw = None;
            inner.count_text = if total > shown {
                format!("显示最新 {} 组 · 共 {} 组", group_digits(shown), group_digits(total))
            } else {
                format!("共 {} 组", group_digits(total))
            };
            inner.elapsed_text.clear();
            inner.selected_index = None;
            inner.detail = DetailPayload::default();
            if inner.status.starts_with("正在搜索") {
                inner.status = ready_status(&inner);
            }
        }
        self.emit_state();
        self.emit_results();
        self.emit_detail();
    }

    fn clear_results(&self) {
        {
            let mut inner = self.lock();
            inner.latest_wanted = None;
            inner.latest_loading = false;
            inner.groups.clear();
            inner.selected_index = None;
            inner.count_text.clear();
            inner.elapsed_text.clear();
            inner.detail = DetailPayload::default();
            inner.detail_raw = None;
            inner.status = ready_status(&inner);
        }
        self.emit_state();
        self.emit_results();
        self.emit_detail();
    }

    // ------------------------------------------------------------- details

    pub fn select_result(&self, index: i64) {
        {
            let mut inner = self.lock();
            let position = if index < 0 { None } else { Some(index as usize) };
            let payload = position.and_then(|value| {
                inner
                    .groups
                    .get(value)
                    .map(|group| (value, build_detail_payload(group, &inner.user)))
            });
            match payload {
                Some((position, payload)) => {
                    inner.selected_index = Some(position);
                    inner.detail = payload;
                }
                None => {
                    inner.selected_index = None;
                    inner.detail = DetailPayload::default();
                }
            }
        }
        self.emit_state();
        self.emit_detail();
    }

    pub fn copy_text(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        match self.platform.copy(text) {
            Ok(()) => self.toast("已复制到剪贴板"),
            Err(error) => self.show_message("复制失败", &error, true, ""),
        }
    }

    pub fn copy_current_links(&self) {
        let text = {
            let inner = self.lock();
            inner
                .selected_index
                .and_then(|index| inner.groups.get(index))
                .map(|group| group.uris.join("\n"))
                .unwrap_or_default()
        };
        self.copy_text(&text);
    }

    pub fn request_detail_raw(self: &Arc<Self>) {
        let (index, signature, entries, cached) = {
            let inner = self.lock();
            let Some(index) = inner.selected_index else {
                return;
            };
            let Some(group) = inner.groups.get(index) else {
                return;
            };
            let signature = group_signature(group);
            let entries: Vec<(String, usize)> = group
                .items
                .iter()
                .map(|item| (item.source_name.to_string(), item.source_index))
                .collect();
            let cached = inner
                .detail_raw
                .as_ref()
                .filter(|(cached_signature, _)| cached_signature == &signature)
                .map(|(_, raw)| raw.clone());
            (index, signature, entries, cached)
        };

        if let Some(raw) = cached {
            self.emit("detail-raw", json!({"index": index as i64, "raw": raw}));
            return;
        }

        let state = Arc::clone(self);
        let dir = self.sources_dir.clone();
        std::thread::spawn(move || {
            let raw = load_group_raw(&dir, &entries);
            if raw.is_empty() {
                return;
            }
            {
                let inner = state.lock();
                let current = inner
                    .selected_index
                    .and_then(|index| inner.groups.get(index))
                    .map(group_signature);
                if current.as_deref() != Some(signature.as_str()) {
                    return;
                }
            }
            {
                let mut inner = state.lock();
                inner.detail_raw = Some((signature, raw.clone()));
            }
            state.emit("detail-raw", json!({"index": index as i64, "raw": raw}));
        });
    }

    pub fn open_sources_folder(&self) {
        if let Err(error) = self.platform.open_path(&self.sources_dir) {
            self.show_message("无法打开目录", &error, true, "");
        }
    }

    pub fn open_external_url(&self, url: &str) {
        let text = url.trim();
        if text.starts_with("http://") || text.starts_with("https://") {
            if let Err(error) = self.platform.open_url(text) {
                self.show_message("无法打开链接", &error, true, "");
            }
        }
    }

    // ------------------------------------------------------ source actions

    pub fn add_sources(self: &Arc<Self>, lines: Vec<String>, overwrite: bool) {
        let urls = normalize_source_urls(&lines);
        if urls.is_empty() {
            self.show_message("提示", "请输入至少一个源地址。", false, "");
            return;
        }
        let total = urls.len();
        self.set_status(format!("正在添加源 (0/{total})…"));
        self.begin_task("add");
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let mut added: Vec<String> = Vec::new();
            let mut failed: Vec<(String, String)> = Vec::new();
            let mut blocked: Vec<String> = Vec::new();
            for (index, url) in urls.iter().enumerate() {
                state.set_status(format!(
                    "正在添加源 ({}/{}): {url}",
                    index + 1,
                    total
                ));
                let name = crate::commands::safe_source_filename(&crate::commands::url_to_filename(url));
                if !overwrite && state.sources_dir.join(&name).exists() {
                    failed.push((url.clone(), format!("源文件 {name} 已存在，未覆盖")));
                    continue;
                }
                match state.commands.fetch(url) {
                    Ok(fetched) => match state.commands.add_fetched(&fetched, &name, overwrite) {
                        Ok(change) => added.push(change.name),
                        Err(error) => failed.push((url.clone(), error.to_string())),
                    },
                    Err(error) => {
                        if let Some(cloudflare) = error.cloudflare_url() {
                            blocked.push(cloudflare.to_string());
                        }
                        failed.push((url.clone(), error.to_string()));
                    }
                }
            }
            state.end_task("add");
            state.finish_add_sources(added, failed, blocked);
        });
    }

    fn finish_add_sources(
        self: &Arc<Self>,
        added: Vec<String>,
        failed: Vec<(String, String)>,
        blocked: Vec<String>,
    ) {
        let mut lines = vec![format!(
            "成功添加 {} 个源，失败 {} 行",
            added.len(),
            failed.len()
        )];
        if !added.is_empty() {
            lines.push(String::new());
            lines.push("已添加".to_string());
            for name in added.iter().take(20) {
                lines.push(format!("· {name}"));
            }
            if added.len() > 20 {
                lines.push(format!("… 另有 {} 个", added.len() - 20));
            }
        }
        if !failed.is_empty() {
            lines.push(String::new());
            lines.push("失败".to_string());
            for (url, error) in failed.iter().take(20) {
                lines.push(format!("· {url}
  {error}"));
            }
            if failed.len() > 20 {
                lines.push(format!("… 另有 {} 行失败", failed.len() - 20));
            }
        }
        if !blocked.is_empty() {
            lines.push(String::new());
            lines.push("受 Cloudflare 拦截的地址：".to_string());
            for url in blocked.iter().take(5) {
                lines.push(url.clone());
            }
        }
        let link = if blocked.len() == 1 {
            blocked[0].clone()
        } else {
            String::new()
        };
        let is_error = added.is_empty() && !failed.is_empty();
        self.show_message("添加源完成", &lines.join("
"), is_error, &link);
        if !added.is_empty() {
            self.load_sources();
        }
    }

    pub fn request_update_source(self: &Arc<Self>, url: &str) {
        let name = { self.lock().active_source.clone() };
        if name.is_empty() {
            self.show_message("提示", "请先在左侧选择一个源。", false, "");
            return;
        }
        let url = url.trim().to_string();
        let resolved = if url.is_empty() {
            storage::read_source_url(&self.sources_dir.join(&name)).unwrap_or_default()
        } else {
            url
        };
        {
            let mut inner = self.lock();
            inner.status = "正在更新源…".to_string();
        }
        self.emit_state();
        self.begin_task("update");
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let explicit = if resolved.is_empty() {
                None
            } else {
                Some(resolved.as_str())
            };
            let outcome = state.commands.update(&name, explicit);
            state.on_updated(outcome);
        });
    }

    fn on_updated(
        self: &Arc<Self>,
        outcome: Result<Option<crate::commands::SourceChange>, CommandError>,
    ) {
        self.end_task("update");
        match outcome {
            Ok(Some(change)) => {
                self.toast(&format!("已更新 {}", change.name));
                self.load_sources();
            }
            Ok(None) => {
                {
                    let mut inner = self.lock();
                    inner.status = ready_status(&inner);
                }
                self.emit_state();
                self.toast("已是最新，无需更新");
            }
            Err(error) => {
                {
                    let mut inner = self.lock();
                    inner.status = ready_status(&inner);
                }
                self.emit_state();
                self.report_source_error(&error);
            }
        }
    }

    pub fn delete_active_source(self: &Arc<Self>) {
        let name = { self.lock().active_source.clone() };
        if name.is_empty() {
            self.show_message("提示", "请先选择一个源。", false, "");
            return;
        }
        self.set_status("正在删除源…");
        self.begin_task("delete");
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let outcome = state.commands.delete(&name);
            state.on_deleted(outcome);
        });
    }

    fn on_deleted(self: &Arc<Self>, outcome: Result<crate::commands::SourceChange, CommandError>) {
        self.end_task("delete");
        match outcome {
            Ok(change) => {
                {
                    let mut inner = self.lock();
                    inner.active_source.clear();
                    inner.status = ready_status(&inner);
                }
                self.emit_state();
                self.toast(&format!("已删除 {}", change.name));
                self.load_sources();
            }
            Err(error) => {
                {
                    let mut inner = self.lock();
                    inner.status = ready_status(&inner);
                }
                self.emit_state();
                self.report_source_error(&error);
            }
        }
    }

    pub fn import_sources(self: &Arc<Self>, paths: Vec<String>) {
        let paths: Vec<String> = paths.into_iter().filter(|path| !path.is_empty()).collect();
        if paths.is_empty() {
            return;
        }
        self.set_status("正在导入源…");
        self.begin_task("import");
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let result = state.commands.import_files(&paths, &[]);
            state.on_imported(result);
        });
    }

    fn on_imported(self: &Arc<Self>, result: crate::commands::ImportResult) {
        self.end_task("import");
        if !result.added.is_empty() {
            let mut message = format!("成功导入 {} 个源", result.added.len());
            if !result.errors.is_empty() {
                message.push_str(&format!(
                    "，失败 {} 个\n\n{}",
                    result.errors.len(),
                    result.errors.iter().take(10).cloned().collect::<Vec<_>>().join("\n")
                ));
            }
            self.show_message("导入完成", &message, false, "");
            self.load_sources();
        } else if !result.errors.is_empty() {
            self.show_message(
                "导入失败",
                &result.errors.iter().take(20).cloned().collect::<Vec<_>>().join("\n"),
                true,
                "",
            );
        }
    }

    pub fn refresh_all(self: &Arc<Self>) {
        if self.active_tasks("refresh") > 0 {
            // Clicking refresh twice used to start two passes over the same
            // files; the second one could race the first one's temp file.
            self.toast("正在刷新，请稍候");
            return;
        }
        let count = {
            let inner = self.lock();
            inner
                .metas
                .values()
                .filter(|meta| meta.url.is_some())
                .count()
        };
        if count == 0 {
            self.show_message("提示", "没有记录了网址的源可以刷新。", false, "");
            return;
        }
        self.set_status("正在刷新全部源…");
        self.begin_task("refresh");
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let progress_state = Arc::clone(&state);
            let mut report = move |index: usize, total: usize, name: &str| {
                progress_state.set_status(format!("刷新源 ({index}/{total}): {name}"));
            };
            let result = state.commands.refresh(None, &mut report);
            state.on_refreshed(result);
        });
    }

    fn on_refreshed(self: &Arc<Self>, result: crate::commands::RefreshResult) {
        self.end_task("refresh");
        let mut lines = vec![format!(
            "成功更新 {} 个源，无变化 {} 个，失败 {} 个",
            result.updated.len(),
            result.unchanged.len(),
            result.failed.len()
        )];
        lines.extend(
            result
                .failed
                .iter()
                .take(5)
                .map(|(name, error)| format!("{name}: {error}")),
        );
        let mut blocked = Vec::new();
        for (name, error) in &result.failed {
            if !error.contains("Cloudflare") {
                continue;
            }
            let url = storage::read_source_url(&self.sources_dir.join(name));
            if let Some(url) = url {
                blocked.push(url);
            }
        }
        if !blocked.is_empty() {
            lines.push(String::new());
            lines.push("受 Cloudflare 拦截的地址：".to_string());
            lines.extend(blocked.iter().take(5).cloned());
        }
        let link = if blocked.len() == 1 {
            blocked[0].clone()
        } else {
            String::new()
        };
        self.show_message("刷新完成", &lines.join("\n"), false, &link);
        if result.updated.is_empty() {
            // Nothing was rewritten: reloading 200k entries would only make
            // the UI blink for no reason.
            self.emit_state();
        } else {
            self.load_sources();
        }
    }

    // -------------------------------------------------------------- update

    pub fn check_updates(self: &Arc<Self>) {
        self.set_status("正在检查更新…");
        self.begin_task("check_update");
        let state = Arc::clone(self);
        let downloader = Arc::clone(&self.downloader);
        std::thread::spawn(move || {
            let api_base = std::env::var("GAMEMAGNET_UPDATE_API")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| update::API_BASE.to_string());
            let outcome = update::check_latest(
                &downloader,
                "lzoreal/Game",
                APP_VERSION,
                None,
                Duration::from_secs(6),
                &api_base,
            );
            state.on_checked_for_updates(outcome);
        });
    }

    fn on_checked_for_updates(self: &Arc<Self>, outcome: Result<UpdateResult, String>) {
        self.end_task("check_update");
        match outcome {
            Err(error) => self.show_message("检查更新失败", &error, true, ""),
            Ok(result) if !result.update_available => self.show_message(
                "检查更新",
                &format!("当前已是最新版本（{}）。", result.current),
                false,
                "",
            ),
            Ok(result) => {
                let info = json!({
                    "current": result.current,
                    "latest": result.latest,
                    "name": result.release.name,
                    "url": result.release.url,
                    "downloadUrl": result.release.download_url,
                    "digest": result.release.download_digest,
                    "notes": result.release.notes,
                    "published": result.release.published_at,
                });
                {
                    let mut inner = self.lock();
                    inner.update_info = Some(info);
                }
                self.emit_state();
                self.emit("update-dialog", Value::Null);
            }
        }
    }

    pub fn install_update(self: &Arc<Self>) {
        let (download_url, digest, release_url, latest) = {
            let inner = self.lock();
            let Some(info) = inner.update_info.as_ref() else {
                return;
            };
            (
                info.get("downloadUrl")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                info.get("digest").and_then(Value::as_str).map(str::to_string),
                info.get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                info.get("latest")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            )
        };
        if download_url.is_empty() {
            self.show_message(
                "提示",
                "这个版本没有提供可直接替换的可执行文件，请到发布页手动下载。",
                true,
                &release_url,
            );
            return;
        }
        let Some(digest) = digest else {
            self.show_message(
                "提示",
                "这个版本缺少 SHA-256 校验信息，无法自动更新。请到发布页手动下载。",
                true,
                &release_url,
            );
            return;
        };

        self.begin_task("update_app");
        self.set_status(format!("正在下载更新 {latest}…"));
        let state = Arc::clone(self);
        let downloader = Arc::clone(&self.downloader);
        std::thread::spawn(move || {
            let bytes = match download_release_bytes(&downloader, &download_url) {
                Ok(bytes) => bytes,
                Err(error) => {
                    state.end_task("update_app");
                    state.show_message("更新失败", &error, true, &release_url);
                    return;
                }
            };
            if !update::verify_sha256(&bytes, &digest) {
                state.end_task("update_app");
                state.show_message(
                    "更新失败",
                    "更新包 SHA-256 校验失败，已取消安装。",
                    true,
                    &release_url,
                );
                return;
            }

            state.set_status("正在替换程序文件…");
            let exe = match replace_running_executable(&bytes) {
                Ok(exe) => exe,
                Err(error) => {
                    state.end_task("update_app");
                    state.show_message("更新失败", &error, true, &release_url);
                    return;
                }
            };

            state.set_status("更新完成，正在重启…");
            match spawn_restarted_exe(&exe) {
                Ok(()) => std::process::exit(0),
                Err(error) => {
                    state.end_task("update_app");
                    state.show_message(
                        "更新失败",
                        &format!("新版本已写入，但自动重启失败：{error}。请手动重新打开程序。"),
                        true,
                        &release_url,
                    );
                }
            }
        });
    }

    // --------------------------------------------------------------- swarm

    pub fn query_swarm(self: &Arc<Self>, uri: &str) {
        {
            let mut inner = self.lock();
            if !inner.swarm_pending.insert(uri.to_string()) {
                return;
            }
            inner
                .swarm_texts
                .insert(uri.to_string(), "正在查询…".to_string());
        }
        self.emit_swarm();
        self.begin_task("swarm");
        let state = Arc::clone(self);
        let uri = uri.to_string();
        let downloader = Arc::clone(&self.downloader);
        std::thread::spawn(move || {
            let outcome = swarm::query_swarm(
                &downloader,
                &uri,
                Duration::from_secs(3),
                swarm::MAX_TRACKERS,
                swarm::CACHE_TTL,
                None,
            );
            let text = match outcome {
                Ok(stats) if stats.trackers_ok == 0 => format!(
                    "tracker 无响应（0/{}）",
                    stats.trackers_total
                ),
                Ok(stats) => swarm::format_stats(&stats),
                Err(_) => "查询失败".to_string(),
            };
            {
                let mut inner = state.lock();
                inner.swarm_pending.remove(&uri);
                inner.swarm_texts.insert(uri, text);
            }
            state.emit_swarm();
            state.end_task("swarm");
        });
    }

    // ------------------------------------------------------------------ bt

    pub fn refresh_bt_targets(&self) {
        let targets = bt::detect(&SystemProbe);
        {
            let mut inner = self.lock();
            inner.bt_targets = targets;
        }
        self.emit_state();
    }

    pub fn send_to_bt(&self, uri: &str, key: &str) {
        let target = {
            let inner = self.lock();
            inner
                .bt_targets
                .iter()
                .find(|target| target.key == key)
                .cloned()
        };
        let Some(target) = target else {
            self.show_message("提示", "没有找到 BT 客户端。", true, "");
            return;
        };
        let Some(plan) = bt::plan(uri, Some(&target)) else {
            return;
        };
        let _ = self.platform.copy(uri);
        let started = match &plan {
            HandoffPlan::Shell { .. } => self.platform.open_url(uri),
            HandoffPlan::Process { exe, args, fallback, .. } => {
                match self.platform.launch(exe, args) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        if fallback.is_some() {
                            let _ = self.platform.open_url(uri);
                            Ok(())
                        } else {
                            Err(error)
                        }
                    }
                }
            }
        };
        match started {
            Ok(()) => self.toast(&format!("已发送到 {}（链接已复制）", target.name)),
            Err(error) => self.show_message("启动失败", &error, true, ""),
        }
    }

    // ------------------------------------------------------------- helpers

    fn report_source_error(&self, error: &CommandError) {
        let link = error.cloudflare_url().unwrap_or("");
        self.show_message("操作失败", error.message(), true, link);
    }

    pub fn show_message(&self, title: &str, text: &str, error: bool, link: &str) {
        self.emit(
            "message",
            json!({"title": title, "text": text, "error": error, "link": link}),
        );
    }

    pub fn toast(&self, text: &str) {
        self.emit("toast", Value::String(text.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{HttpClient, HttpResponse};
    use std::path::Path;

    struct FakePlatform;

    impl Platform for FakePlatform {
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Ok(())
        }
        fn open_path(&self, _path: &Path) -> Result<(), String> {
            Ok(())
        }
        fn launch(&self, _exe: &str, _args: &[String]) -> Result<(), String> {
            Ok(())
        }
        fn copy(&self, _text: &str) -> Result<(), String> {
            Ok(())
        }
    }

    struct OfflineClient;

    impl HttpClient for OfflineClient {
        fn get(
            &self,
            _url: &str,
            _headers: &[(&str, &str)],
            _timeout: Duration,
            _profile: Option<&str>,
        ) -> Result<HttpResponse, String> {
            Err("offline".to_string())
        }
    }

    fn wait_until<F: FnMut() -> bool>(mut check: F, millis: u64) -> bool {
        let deadline = Instant::now() + Duration::from_millis(millis);
        while Instant::now() < deadline {
            if check() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        check()
    }

    #[test]
    fn loads_searches_and_selects_a_group() {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let data = json!({"repacks": [
            {"title": "Alpha Game v1.0", "uris": ["magnet:a"], "fileSize": "1 GB", "uploadDate": "2024-01-01"},
            {"title": "Alpha Game v1.1", "uris": ["magnet:b"], "fileSize": "2 GB", "uploadDate": "2024-02-01"},
            {"title": "Beta", "uris": ["magnet:c"], "fileSize": "3 GB", "uploadDate": "2023-01-01"}
        ]});
        storage::write_source_file(&dir.join("demo.json.gz"), &data).unwrap();

        let captured: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&captured);
        let emitter: Emitter = Arc::new(move |event, payload| {
            sink.lock().unwrap().push((event.to_string(), payload));
        });
        let state = AppState::new(
            dir.clone(),
            Arc::new(Downloader::new(Arc::new(OfflineClient))),
            Arc::new(FakePlatform),
            emitter,
        );

        state.load_sources();
        assert!(wait_until(|| state.active_tasks("load") == 0, 5000));
        assert!(wait_until(|| state.results_rows().len() == 2, 5000));
        assert_eq!(state.results_rows()[0].title, "Alpha Game v1.0");
        assert_eq!(state.snapshot().page_title, "全部游戏");
        assert_eq!(state.snapshot().sources.len(), 2);

        state.set_query("alpha");
        assert!(wait_until(|| state.results_rows().len() == 1, 5000));
        let snapshot = state.snapshot();
        assert_eq!(snapshot.count_text, "找到 1 组");
        assert!(snapshot.elapsed_text.ends_with("秒"));

        state.select_result(0);
        let detail = state.detail_payload();
        assert!(detail.has);
        assert_eq!(detail.title, "Alpha Game v1.0");
        assert_eq!(detail.sources, vec!["demo".to_string()]);
        assert_eq!(detail.cards.len(), 2);

        state.copy_current_links();
        state.set_query("");
        assert!(wait_until(|| state.results_rows().len() == 2, 5000));
        assert!(!state.detail_payload().has);

        assert!(captured.lock().unwrap().iter().any(|(event, _)| event == "results"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn normalizes_batch_source_lines() {
        let lines = vec![
            " https://example.com/a.json ".to_string(),
            String::new(),
            "# comment".to_string(),
            "http://mirror.example/b.json".to_string(),
            "https://example.com/a.json".to_string(),
            "plain.example/c.json".to_string(),
        ];
        assert_eq!(
            normalize_source_urls(&lines),
            vec![
                "https://example.com/a.json".to_string(),
                "http://mirror.example/b.json".to_string(),
                "https://plain.example/c.json".to_string()
            ]
        );
    }

    struct ScriptedClient {
        responses: Mutex<HashMap<String, Result<String, String>>>,
        calls: Mutex<Vec<String>>,
    }

    impl ScriptedClient {
        fn new(entries: Vec<(&str, Result<String, String>)>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(
                    entries
                        .into_iter()
                        .map(|(url, result)| (url.to_string(), result))
                        .collect(),
                ),
                calls: Mutex::new(Vec::new()),
            })
        }
    }

    impl HttpClient for ScriptedClient {
        fn get(
            &self,
            url: &str,
            _headers: &[(&str, &str)],
            _timeout: Duration,
            _profile: Option<&str>,
        ) -> Result<HttpResponse, String> {
            self.calls.lock().unwrap().push(url.to_string());
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

    #[test]
    fn batch_add_skips_failures_and_reports_at_end() {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-batch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let payload =
            r#"{"downloads": [{"title": "Alpha", "uris": ["magnet:a"]}]}"#.to_string();
        let client = ScriptedClient::new(vec![
            ("https://example.com/a.json", Ok(payload)),
            ("https://example.com/b.json", Err("boom".to_string())),
        ]);

        let captured: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&captured);
        let emitter: Emitter = Arc::new(move |event, payload| {
            sink.lock().unwrap().push((event.to_string(), payload));
        });
        let state = AppState::new(
            dir.clone(),
            Arc::new(Downloader::new(client.clone())),
            Arc::new(FakePlatform),
            emitter,
        );

        state.add_sources(
            vec![
                "https://example.com/a.json".to_string(),
                "https://example.com/b.json".to_string(),
                " https://example.com/a.json ".to_string(),
            ],
            false,
        );
        assert!(wait_until(|| state.active_tasks("add") == 0, 5000));
        assert!(wait_until(
            || captured
                .lock()
                .unwrap()
                .iter()
                .any(|(event, _)| event == "message"),
            5000
        ));

        let stored = dir.join("a.json.gz");
        assert!(stored.exists());
        assert_eq!(
            storage::read_source_url(&stored).as_deref(),
            Some("https://example.com/a.json")
        );
        assert_eq!(
            client.calls.lock().unwrap().iter().filter(|url| url.contains("a.json")).count(),
            1
        );

        let message = captured
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(event, _)| event == "message")
            .and_then(|(_, payload)| payload.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        assert!(message.contains("成功添加 1 个源，失败 1 行"));
        assert!(message.contains("https://example.com/b.json"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn serves_raw_json_on_demand() {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-raw-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let data = json!({"repacks": [
            {"title": "Alpha Game v1.0", "uris": ["magnet:a"], "fileSize": "1 GB", "uploadDate": "2024-01-01", "descriptionHtml": "<p>Hello</p>"}
        ]});
        storage::write_source_file(&dir.join("demo.json.gz"), &data).unwrap();

        let captured: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&captured);
        let emitter: Emitter = Arc::new(move |event, payload| {
            sink.lock().unwrap().push((event.to_string(), payload));
        });
        let state = AppState::new(
            dir.clone(),
            Arc::new(Downloader::new(Arc::new(OfflineClient))),
            Arc::new(FakePlatform),
            emitter,
        );
        state.load_sources();
        assert!(wait_until(|| state.active_tasks("load") == 0, 5000));
        assert!(wait_until(|| state.results_rows().len() == 1, 5000));
        state.select_result(0);
        assert!(state.detail_payload().has);
        assert!(state.detail_payload().raw.is_empty());

        state.request_detail_raw();
        assert!(wait_until(
            || captured
                .lock()
                .unwrap()
                .iter()
                .any(|(event, _)| event == "detail-raw"),
            5000
        ));
        let raw = captured
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(event, _)| event == "detail-raw")
            .and_then(|(_, payload)| payload.get("raw"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        assert!(raw.contains("Alpha Game v1.0"));
        assert!(raw.contains("descriptionHtml"));

        // Second request hits the cache and still emits.
        let before = captured.lock().unwrap().len();
        state.request_detail_raw();
        assert!(wait_until(
            || captured.lock().unwrap().len() > before,
            2000
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replaces_executable_and_keeps_a_backup() {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-update-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("GameMagNet.exe");
        std::fs::write(&exe, b"old").unwrap();

        replace_executable_at(&exe, b"new").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(
            std::fs::read(dir.join("GameMagNet.exe.old")).unwrap(),
            b"old"
        );
        assert!(!dir.join("GameMagNet.exe.new").exists());

        replace_executable_at(&exe, b"newer").unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"newer");
        assert_eq!(
            std::fs::read(dir.join("GameMagNet.exe.old")).unwrap(),
            b"new"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn downloads_release_bytes_with_mirror_fallback() {
        let direct = "https://github.com/lzoreal/Game/releases/download/v1/GameMagNet.exe";
        let proxied = format!("https://gh-proxy.com/{direct}");

        let client = ScriptedClient::new(vec![(direct, Ok("direct".to_string()))]);
        let downloader = Downloader::new(client);
        assert_eq!(
            download_release_bytes(&downloader, direct).unwrap(),
            b"direct"
        );

        let client = ScriptedClient::new(vec![
            (direct, Err("direct blocked".to_string())),
            (proxied.as_str(), Ok("mirrored".to_string())),
        ]);
        let downloader = Downloader::new(client);
        assert_eq!(
            download_release_bytes(&downloader, direct).unwrap(),
            b"mirrored"
        );
    }

    #[test]
    #[ignore]
    fn live_download_latest_update() {
        use crate::net_reqwest::ReqwestClient;

        let downloader = Arc::new(Downloader::new(Arc::new(ReqwestClient::new().unwrap())));
        let result = crate::update::check_latest(
            &downloader,
            "lzoreal/Game",
            "v1.0.0",
            None,
            Duration::from_secs(15),
            crate::update::API_BASE,
        )
        .unwrap();
        let url = result.release.download_url.expect("download url");
        let bytes = download_release_bytes(&downloader, &url).unwrap();
        assert!(!bytes.is_empty());
        if let Some(digest) = result.release.download_digest.as_deref() {
            assert!(crate::update::verify_sha256(&bytes, digest));
        }
    }

    #[test]
    fn favorites_anchor_on_the_normalized_title_and_survive_reload() {
        let dir = temp_dir("favorites");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);

        let alpha = state
            .results_rows()
            .into_iter()
            .find(|row| row.title.starts_with("Alpha"))
            .expect("alpha group");
        assert_eq!(alpha.key, "alpha game");

        state.toggle_favorite(&alpha.key);
        assert_eq!(state.snapshot().favorite_count, 1);
        let marked = state
            .results_rows()
            .into_iter()
            .find(|row| row.key == "alpha game")
            .unwrap();
        assert!(marked.favorite);
        assert!(!marked.broken);

        // A fresh process must read the same favorite back from disk.
        let reloaded = state_for(&dir);
        load_and_wait(&reloaded);
        let again = reloaded
            .results_rows()
            .into_iter()
            .find(|row| row.key == "alpha game")
            .expect("favorite survives reload");
        assert!(again.favorite);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn handled_groups_leave_the_list_and_come_back_when_asked() {
        let dir = temp_dir("handled");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);
        assert_eq!(state.results_rows().len(), 2);

        let beta = state
            .results_rows()
            .into_iter()
            .find(|row| row.key == "beta")
            .unwrap();
        state.toggle_handled(&beta.key);
        assert!(wait_until(|| state.results_rows().len() == 1, 5000));
        assert_eq!(state.snapshot().handled_count, 1);

        state.set_show_handled(true);
        assert!(wait_until(|| state.results_rows().len() == 2, 5000));

        state.set_view("unhandled");
        assert!(wait_until(|| state.results_rows().len() == 1, 5000));
        assert_eq!(state.snapshot().view, "unhandled");

        state.set_view("favorites");
        assert!(wait_until(|| state.results_rows().is_empty(), 5000));
        assert_eq!(state.snapshot().sort_mode, "favoriteDesc");

        state.set_view("all");
        assert!(wait_until(|| state.results_rows().len() == 2, 5000));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_favorite_reports_a_broken_link_when_its_item_disappears() {
        let dir = temp_dir("broken");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);

        state.toggle_favorite("alpha game");
        assert!(!state
            .results_rows()
            .into_iter()
            .find(|row| row.key == "alpha game")
            .unwrap()
            .broken);

        // The newest item (`magnet:b`, the one the favorite remembered) is gone.
        write_catalog(&dir, false);
        state.load_sources();
        load_and_wait(&state);

        let row = state
            .results_rows()
            .into_iter()
            .find(|row| row.key == "alpha game")
            .expect("group survives");
        assert!(row.favorite);
        assert!(row.broken);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detail_compares_sources_and_marks_the_remembered_one() {
        let dir = temp_dir("compare");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);

        state.select_result(0);
        assert_eq!(state.detail_payload().key, "alpha game");
        state.toggle_favorite("alpha game");
        let detail = state.detail_payload();
        assert!(detail.favorite);
        assert!(!detail.broken);
        assert_eq!(detail.compare.len(), 2);
        assert_eq!(
            detail.compare.iter().filter(|row| row.chosen).count(),
            1,
            "exactly the remembered item is marked"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn blocklist_hides_groups_and_turning_it_off_brings_them_back() {
        let dir = temp_dir("blocklist-view");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);
        assert_eq!(state.results_rows().len(), 2);

        state.set_blocklist(vec![" Beta ".to_string(), "beta".to_string()]);
        assert!(wait_until(|| state.results_rows().len() == 1, 5000));
        assert_eq!(state.results_rows()[0].key, "alpha game");
        assert_eq!(state.snapshot().blocked, vec!["beta"]);
        assert!(state.snapshot().block_enabled);

        state.set_block_enabled(false);
        assert!(wait_until(|| state.results_rows().len() == 2, 5000));
        assert!(!state.snapshot().block_enabled);
        assert_eq!(
            state.snapshot().blocked,
            vec!["beta"],
            "switching it off must not delete the list"
        );

        // The choice survives a restart.
        let reloaded = state_for(&dir);
        load_and_wait(&reloaded);
        assert_eq!(reloaded.snapshot().blocked, vec!["beta"]);
        assert!(!reloaded.snapshot().block_enabled);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn phrases_and_exclusions_filter_search_results() {
        let dir = temp_dir("phrase-search");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);

        state.set_query(r#""alpha game""#);
        assert!(wait_until(|| state.results_rows().len() == 1, 5000));
        assert_eq!(state.results_rows()[0].key, "alpha game");

        // Both versions carry "v1" in the normalized title, so the exclusion
        // empties the result set instead of just lowering the score.
        state.set_query(r#""alpha game" -v1"#);
        assert!(wait_until(|| state.results_rows().is_empty(), 5000));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_phrase_with_a_version_number_keeps_the_hit() {
        let dir = temp_dir("phrase-version");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);

        // `search_items` matches the item's normalized title ("alpha game v1
        // 1"), but the group key has the version stripped ("alpha game"). The
        // arrange step must not re-test the phrase against the key and drop a
        // row the search already accepted.
        state.set_query(r#""alpha game v1.1""#);
        assert!(wait_until(|| state.results_rows().len() == 1, 5000));
        let rows = state.results_rows();
        assert_eq!(rows[0].key, "alpha game");
        assert!(state.snapshot().count_text.contains("找到 1 组"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_query_of_only_exclusions_filters_the_latest_list() {
        let dir = temp_dir("only-exclusions");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);
        assert_eq!(state.results_rows().len(), 2);

        state.set_query("-alpha");
        assert!(wait_until(|| state.results_rows().len() == 1, 5000));
        assert_eq!(state.results_rows()[0].key, "beta");

        state.set_query("-beta");
        assert!(wait_until(|| {
            let rows = state.results_rows();
            rows.len() == 1 && rows[0].key == "alpha game"
        }, 5000));

        state.set_query("");
        assert!(wait_until(|| state.results_rows().len() == 2, 5000));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finishing_a_task_clears_the_progress_line_even_without_a_reload() {
        let dir = temp_dir("refresh-status");
        write_catalog(&dir, true);
        let state = state_for(&dir);
        load_and_wait(&state);
        let ready = state.snapshot().status;

        state.begin_task("refresh");
        state.set_status("刷新源 (1/1): demo.json.gz");
        state.end_task("refresh");

        let status = state.snapshot().status;
        assert_eq!(status, ready, "an all-304 refresh must not leave progress text");
        assert!(!status.contains("刷新源"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn auto_refresh_touches_only_stale_sources_and_shows_no_dialog() {
        use crate::storage::{FileStateStore, StateStore as _};

        let dir = temp_dir("auto-refresh");
        write_catalog(&dir, true);
        storage::write_url_file(
            &dir.join("demo.json.gz"),
            "https://example.com/demo.json",
        )
        .unwrap();

        let events = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let seen = Arc::clone(&events);
        let emitter: Emitter =
            Arc::new(move |event, _| seen.lock().unwrap().push(event.to_string()));
        let state = AppState::new(
            dir.clone(),
            Arc::new(Downloader::new(Arc::new(OfflineClient))),
            Arc::new(FakePlatform),
            emitter,
        );
        load_and_wait(&state);

        // Freshly fetched: the background pass must not touch it.
        let store = FileStateStore::new(&dir);
        store.record_success("demo.json.gz", "h", None, None, false);
        state.auto_refresh();
        assert_eq!(state.active_tasks("refresh"), 0);
        assert!(!events.lock().unwrap().iter().any(|event| event == "toast"));

        // Three days later it is worth a try, but the passive pass cannot
        // reach the network here, so it must fail quietly.
        let mut snap = store.load().unwrap();
        snap.source_snapshots
            .get_mut("demo.json.gz")
            .unwrap()
            .last_success = Some(chrono::Utc::now() - chrono::Duration::days(3));
        store.save(&snap).unwrap();

        state.auto_refresh();
        assert!(wait_until(|| state.active_tasks("refresh") == 0, 5000));
        let events = events.lock().unwrap();
        assert!(
            events.iter().any(|event| event == "toast"),
            "the user still gets a quiet failure notice"
        );
        assert!(
            !events.iter().any(|event| event == "message"),
            "auto refresh must never open a modal"
        );
        drop(events);
        assert_eq!(store.snapshot("demo.json.gz").consecutive_failures, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-user-state-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn state_for(dir: &Path) -> Arc<AppState> {
        let emitter: Emitter = Arc::new(|_, _| {});
        AppState::new(
            dir.to_path_buf(),
            Arc::new(Downloader::new(Arc::new(OfflineClient))),
            Arc::new(FakePlatform),
            emitter,
        )
    }

    fn load_and_wait(state: &Arc<AppState>) {
        state.load_sources();
        assert!(wait_until(
            || state.active_tasks("load") == 0 && !state.results_rows().is_empty(),
            5000
        ));
    }

    /// Two sources of the same game (`magnet:a` older, `magnet:b` newer) plus an
    /// unrelated one. `with_newest` drops `magnet:b` to simulate a dead link.
    fn write_catalog(dir: &Path, with_newest: bool) {
        let mut rows = vec![
            json!({"title": "Alpha Game v1.0", "uris": ["magnet:a"], "fileSize": "1 GB", "uploadDate": "2024-01-01"}),
        ];
        if with_newest {
            rows.push(json!({"title": "Alpha Game v1.1", "uris": ["magnet:b"], "fileSize": "2 GB", "uploadDate": "2024-02-01"}));
        }
        rows.push(json!({"title": "Beta", "uris": ["magnet:c"], "fileSize": "3 GB", "uploadDate": "2023-01-01"}));
        storage::write_source_file(&dir.join("demo.json.gz"), &json!({ "repacks": rows })).unwrap();
    }
}
