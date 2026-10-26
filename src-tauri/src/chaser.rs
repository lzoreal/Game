//! Cloudflare challenge solving backed by the embedded chaser-cf browser.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chaser_cf::{ChaserCF, ChaserConfig};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_CONTEXT_LIMIT: usize = 4;
const PROFILE_PREFIX: &str = "gamemagnet-chaser-";
const STALE_PROFILE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Seam so source downloads can be tested without launching a browser.
pub trait CloudflareSolver: Send + Sync {
    fn fetch(&self, url: &str) -> Result<String, String>;
}

pub struct ChaserSolver {
    inner: Mutex<Option<Arc<ChaserInner>>>,
    fetch_lock: Mutex<()>,
}

struct ChaserInner {
    runtime: tokio::runtime::Runtime,
    chaser: Arc<ChaserCF>,
    user_data_dir: PathBuf,
}

impl ChaserSolver {
    pub fn new() -> Self {
        cleanup_stale_profiles();
        Self {
            inner: Mutex::new(None),
            fetch_lock: Mutex::new(()),
        }
    }

    fn inner(&self) -> Result<Arc<ChaserInner>, String> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(inner) = guard.as_ref() {
            return Ok(Arc::clone(inner));
        }
        let inner = Arc::new(ChaserInner::new()?);
        *guard = Some(Arc::clone(&inner));
        Ok(inner)
    }

    fn reset(&self) {
        let old = {
            let mut guard = self
                .inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.take()
        };
        drop(old);
    }
}

impl ChaserInner {
    fn new() -> Result<Self, String> {
        let user_data_dir = profile_dir();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("chaser-cf")
            .enable_all()
            .build()
            .map_err(|error| format!("无法启动 chaser-cf 运行时: {error}"))?;
        let chaser = runtime
            .block_on(ChaserCF::new(solver_config(user_data_dir.clone())))
            .map_err(|error| {
                format!("chaser-cf 初始化失败（需要安装 Chrome 或 Edge）: {error}")
            })?;
        Ok(Self {
            runtime,
            chaser: Arc::new(chaser),
            user_data_dir,
        })
    }
}

fn profile_dir() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "{PROFILE_PREFIX}{}-{stamp}-{sequence}",
        std::process::id()
    ))
}

fn cleanup_stale_profiles() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return;
    };
    let cutoff = now.as_nanos().saturating_sub(STALE_PROFILE_AGE.as_nanos());
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(rest) = name.strip_prefix(PROFILE_PREFIX) else {
            continue;
        };
        let mut parts = rest.split('-');
        let pid_ok = parts
            .next()
            .map(|pid| pid.parse::<u32>().is_ok())
            .unwrap_or(false);
        let stamp = parts.next().and_then(|stamp| stamp.parse::<u128>().ok());
        let seq_ok = parts
            .next()
            .map(|seq| seq.parse::<u64>().is_ok())
            .unwrap_or(false);
        if !pid_ok || !seq_ok || parts.next().is_some() {
            continue;
        }
        if stamp.map(|stamp| stamp < cutoff).unwrap_or(false) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn solver_config(user_data_dir: PathBuf) -> ChaserConfig {
    let mut config = ChaserConfig::from_env()
        .with_lazy_init(true)
        .with_user_data_dir(user_data_dir);
    if std::env::var_os("CHASER_TIMEOUT").is_none() {
        config.timeout_ms = DEFAULT_TIMEOUT_MS;
    }
    if std::env::var_os("CHASER_CONTEXT_LIMIT").is_none() {
        config.context_limit = DEFAULT_CONTEXT_LIMIT;
    }
    config
}

impl Drop for ChaserInner {
    fn drop(&mut self) {
        self.runtime.block_on(self.chaser.shutdown());
        for _ in 0..5 {
            if std::fs::remove_dir_all(&self.user_data_dir).is_ok()
                || !self.user_data_dir.exists()
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }
}

impl CloudflareSolver for ChaserSolver {
    fn fetch(&self, url: &str) -> Result<String, String> {
        let _guard = self
            .fetch_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let inner = self.inner()?;
        let result = inner.runtime.block_on(inner.chaser.get_body_text(url, None));
        // Close the visible verification window as soon as the body is in hand.
        // The cached ChaserCF and profile stay alive, so the next fetch
        // relaunches with the same clearance cookies instead of a new challenge.
        inner.runtime.block_on(inner.chaser.shutdown());
        match result {
            Ok(text) => Ok(text),
            Err(error) => {
                self.reset();
                Err(error.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_directories_are_unique() {
        let first = profile_dir();
        let second = profile_dir();
        assert_ne!(first, second);
        assert!(first.starts_with(std::env::temp_dir()));
        let name = first.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("gamemagnet-chaser-"), "{name}");
    }

    #[test]
    fn solver_config_uses_profile_directory() {
        let dir = profile_dir();
        let config = solver_config(dir.clone());
        assert_eq!(config.user_data_dir.as_deref(), Some(dir.as_path()));
        assert!(config.lazy_init);
    }

    #[test]
    fn reset_discards_cached_browser_and_profile() {
        let solver = ChaserSolver::new();
        let first = solver.inner().unwrap();
        solver.reset();
        let second = solver.inner().unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert_ne!(first.user_data_dir, second.user_data_dir);
    }

    #[test]
    fn stale_profiles_are_removed_but_fresh_ones_kept() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let old = std::env::temp_dir().join(format!("{PROFILE_PREFIX}{}-0-0", std::process::id()));
        let fresh =
            std::env::temp_dir().join(format!("{PROFILE_PREFIX}{}-{now}-0", std::process::id()));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&fresh).unwrap();

        cleanup_stale_profiles();

        assert!(!old.exists());
        assert!(fresh.exists());
        let _ = std::fs::remove_dir_all(&fresh);
    }
}
