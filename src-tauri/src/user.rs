//! User-owned state: favorites and handled groups.
//!
//! Favorites are anchored to the normalized title (`text::dedup_key`), never to
//! the 64-bit group id: the id is a hash of that string, so any change to the
//! normalization rules would silently orphan every favorite.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::storage;

const USER_FILE: &str = "user.json";
const USER_TMP: &str = "user.json.tmp";
const USER_CORRUPT: &str = "user.json.corrupt";

pub const USER_SCHEMA_VERSION: u8 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    /// The link the user had picked when the group was favorited.
    pub uri: String,
    /// Unix seconds; drives the default ordering of the favorites view.
    pub added_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserState {
    pub version: u8,
    /// Keyed by normalized title (`dedup_key`).
    pub favorites: BTreeMap<String, Favorite>,
    /// Normalized titles the user marked as handled.
    pub handled: BTreeSet<String>,
    /// Title fragments hidden from the list and from search results.
    /// Matched case-insensitively as substrings of the normalized title.
    pub blocked: Vec<String>,
    /// Whether `blocked` currently filters results. Off lets the user peek at
    /// everything without deleting the list.
    pub block_enabled: bool,
}

/// Cap on the blocklist so a pasted file cannot make every title check slow.
const BLOCK_LIMIT: usize = 500;

impl Default for UserState {
    fn default() -> Self {
        Self {
            version: USER_SCHEMA_VERSION,
            favorites: BTreeMap::new(),
            handled: BTreeSet::new(),
            blocked: Vec::new(),
            block_enabled: true,
        }
    }
}

impl UserState {
    pub fn is_favorite(&self, key: &str) -> bool {
        self.favorites.contains_key(key)
    }

    pub fn is_handled(&self, key: &str) -> bool {
        self.handled.contains(key)
    }

    pub fn favorite_time(&self, key: &str) -> i64 {
        self.favorites.get(key).map(|entry| entry.added_at).unwrap_or(0)
    }

    pub fn remembered_uri(&self, key: &str) -> Option<&str> {
        self.favorites
            .get(key)
            .map(|entry| entry.uri.as_str())
            .filter(|uri| !uri.is_empty())
    }

    /// Returns the new favorite state.
    pub fn toggle_favorite(&mut self, key: &str, uri: &str, now: i64) -> bool {
        if self.favorites.remove(key).is_some() {
            false
        } else {
            self.favorites.insert(
                key.to_string(),
                Favorite {
                    uri: uri.to_string(),
                    added_at: now,
                },
            );
            true
        }
    }

    /// Returns the new handled state.
    pub fn toggle_handled(&mut self, key: &str) -> bool {
        if self.handled.remove(key) {
            false
        } else {
            self.handled.insert(key.to_string());
            true
        }
    }

    /// True only when the blocklist has entries *and* filtering is on.
    pub fn blocks(&self) -> bool {
        self.block_enabled && !self.blocked.is_empty()
    }

    /// `key` is the normalized title (`Group::key`), which is already
    /// lowercased by the normalizer; the blocklist is lowercased here so
    /// "GOG" and "gog" behave the same.
    pub fn is_blocked(&self, key: &str) -> bool {
        if !self.blocks() {
            return false;
        }
        let key = key.to_lowercase();
        self.blocked
            .iter()
            .any(|term| !term.is_empty() && key.contains(term.as_str()))
    }

    /// Trims, drops empty entries and removes case-insensitive duplicates while
    /// keeping the user's order. A term appearing twice does not cost anything
    /// but it makes the list confusing to edit.
    pub fn set_blocked(&mut self, terms: Vec<String>) {
        let mut seen = BTreeSet::new();
        let mut cleaned = Vec::new();
        for term in terms {
            let term = term.trim().to_lowercase();
            if term.is_empty() || cleaned.len() >= BLOCK_LIMIT {
                continue;
            }
            if seen.insert(term.clone()) {
                cleaned.push(term);
            }
        }
        self.blocked = cleaned;
        // Turn filtering back on when the user just configured a list.
        if !self.blocked.is_empty() {
            self.block_enabled = true;
        }
    }

    pub fn set_block_enabled(&mut self, enabled: bool) {
        self.block_enabled = enabled;
    }
}

/// Reads and writes `user.json` next to the sources, with the atomic
/// write-then-rename pattern the rest of the app uses.
pub struct UserStore {
    path: PathBuf,
}

impl UserStore {
    pub fn new(sources_dir: &Path) -> Self {
        Self {
            path: storage::state_dir(sources_dir).join(USER_FILE),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Never fails: a missing or unreadable file yields an empty state so the
    /// app still starts. A corrupt file is moved aside instead of overwritten.
    pub fn load(&self) -> UserState {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return UserState::default();
        };
        match serde_json::from_str::<UserState>(&text) {
            Ok(state) => state,
            Err(error) => {
                log::warn!("用户状态解析失败，已另存为 {}: {}", USER_CORRUPT, error);
                let corrupt = self.path.with_file_name(USER_CORRUPT);
                // Windows refuses to rename onto an existing file, so a second
                // corruption would otherwise never be moved aside.
                let _ = fs::remove_file(&corrupt);
                let _ = fs::rename(&self.path, corrupt);
                UserState::default()
            }
        }
    }

    pub fn save(&self, state: &UserState) -> Result<(), String> {
        let Some(dir) = self.path.parent() else {
            return Err("用户状态路径无效".to_string());
        };
        fs::create_dir_all(dir).map_err(|error| format!("创建状态目录失败: {error}"))?;
        let tmp = dir.join(USER_TMP);
        let text = serde_json::to_string_pretty(state)
            .map_err(|error| format!("序列化用户状态失败: {error}"))?;
        fs::write(&tmp, text).map_err(|error| format!("写入用户状态失败: {error}"))?;
        fs::rename(&tmp, &self.path).map_err(|error| format!("保存用户状态失败: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-user-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn favorite_roundtrip_keeps_anchor_and_link() {
        let dir = temp_dir("roundtrip");
        let store = UserStore::new(&dir);
        let mut state = UserState::default();
        assert!(state.toggle_favorite("alpha game", "magnet:a", 100));
        assert!(state.toggle_handled("beta"));
        store.save(&state).unwrap();

        let loaded = store.load();
        assert_eq!(loaded, state);
        assert!(loaded.is_favorite("alpha game"));
        assert_eq!(loaded.remembered_uri("alpha game"), Some("magnet:a"));
        assert!(loaded.is_handled("beta"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn toggling_twice_returns_to_the_previous_state() {
        let mut state = UserState::default();
        assert!(state.toggle_favorite("alpha", "magnet:a", 1));
        assert!(!state.toggle_favorite("alpha", "magnet:a", 2));
        assert!(!state.is_favorite("alpha"));
        assert!(state.toggle_handled("beta"));
        assert!(!state.toggle_handled("beta"));
        assert!(!state.is_handled("beta"));
    }

    #[test]
    fn refavoriting_keeps_the_newest_timestamp() {
        let mut state = UserState::default();
        state.toggle_favorite("alpha", "magnet:a", 10);
        state.toggle_favorite("alpha", "magnet:a", 20);
        state.toggle_favorite("alpha", "magnet:b", 30);
        assert_eq!(state.favorite_time("alpha"), 30);
        assert_eq!(state.remembered_uri("alpha"), Some("magnet:b"));
    }

    #[test]
    fn corrupt_file_is_moved_aside_and_starts_empty() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(dir.join(".gm")).unwrap();
        fs::write(dir.join(".gm").join(USER_FILE), b"not json").unwrap();
        let store = UserStore::new(&dir);
        let state = store.load();
        assert!(state.favorites.is_empty());
        assert!(!store.path().exists());
        assert!(dir.join(".gm").join(USER_CORRUPT).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_loads_as_empty_state() {
        let dir = temp_dir("missing");
        let store = UserStore::new(&dir);
        let state = store.load();
        assert_eq!(state, UserState::default());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn blocklist_trims_dedupes_and_survives_a_reload() {
        let dir = temp_dir("blocklist");
        let store = UserStore::new(&dir);
        let mut state = UserState::default();
        state.set_blocked(vec![
            "  合集 ".to_string(),
            "GOG".to_string(),
            "gog".to_string(),
            String::new(),
            "整合".to_string(),
        ]);
        assert_eq!(state.blocked, vec!["合集", "gog", "整合"]);
        assert!(state.blocks());
        store.save(&state).unwrap();
        assert_eq!(store.load().blocked, vec!["合集", "gog", "整合"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn blocking_is_case_insensitive_and_can_be_switched_off() {
        let mut state = UserState::default();
        state.set_blocked(vec!["GOG".to_string(), "合集".to_string()]);
        assert!(state.is_blocked("hades gog edition"));
        assert!(state.is_blocked("hades 合集"));
        assert!(!state.is_blocked("hades"));
        state.set_block_enabled(false);
        assert!(!state.is_blocked("hades gog edition"));
        assert_eq!(state.blocked.len(), 2, "turning it off keeps the list");
    }

    #[test]
    fn empty_blocklist_never_blocks() {
        let mut state = UserState::default();
        state.set_block_enabled(true);
        assert!(!state.blocks());
        assert!(!state.is_blocked("anything"));
    }

    #[test]
    fn old_state_files_get_an_enabled_empty_blocklist() {
        let dir = temp_dir("blocklist-legacy");
        fs::create_dir_all(dir.join(".gm")).unwrap();
        fs::write(
            dir.join(".gm").join(USER_FILE),
            br#"{"version":1,"favorites":{},"handled":[]}"#,
        )
        .unwrap();
        let state = UserStore::new(&dir).load();
        assert!(state.blocked.is_empty());
        assert!(state.block_enabled);
        assert!(!state.blocks());
        fs::remove_dir_all(&dir).ok();
    }
}
