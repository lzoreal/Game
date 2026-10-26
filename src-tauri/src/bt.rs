//! BT client catalog, detection and handoff planning.

use std::collections::HashMap;
use std::path::Path;

pub const THUNDER_PATHS: [&str; 4] = [
    r"C:\Program Files (x86)\Thunder Network\Thunder\Program\Thunder.exe",
    r"C:\Program Files\Thunder Network\Thunder\Program\Thunder.exe",
    r"D:\Program Files (x86)\Thunder Network\Thunder\Program\Thunder.exe",
    r"D:\Program Files\Thunder Network\Thunder\Program\Thunder.exe",
];

pub struct CatalogEntry {
    pub key: &'static str,
    pub name: &'static str,
    pub badge: &'static str,
    pub color: &'static str,
    pub desc: &'static str,
    pub url: &'static str,
    pub recommended: bool,
    pub exe_name: Option<&'static str>,
    pub paths: &'static [&'static str],
    pub args: &'static str,
    pub fallback: Option<&'static str>,
    pub thunder: bool,
}

pub const CATALOG: [CatalogEntry; 8] = [
    CatalogEntry {
        key: "qbittorrent",
        name: "qBittorrent",
        badge: "qB",
        color: "#3b82f6",
        desc: "开源、无广告，支持 Windows / macOS / Linux。",
        url: "https://www.qbittorrent.org/download",
        recommended: true,
        exe_name: Some("qbittorrent.exe"),
        paths: &[
            r"{ProgramFiles}\qBittorrent\qbittorrent.exe",
            r"{ProgramFiles(x86)}\qBittorrent\qbittorrent.exe",
            r"{LOCALAPPDATA}\Programs\qBittorrent\qbittorrent.exe",
        ],
        args: "uri",
        fallback: None,
        thunder: false,
    },
    CatalogEntry {
        key: "bitcomet",
        name: "BitComet",
        badge: "BC",
        color: "#0ea5a4",
        desc: "老牌 BitTorrent 客户端，支持长效种子。",
        url: "https://www.bitcomet.com/en/download",
        recommended: false,
        exe_name: Some("BitComet.exe"),
        paths: &[
            r"{ProgramFiles}\BitComet\BitComet.exe",
            r"{ProgramFiles(x86)}\BitComet\BitComet.exe",
        ],
        args: "uri",
        fallback: None,
        thunder: false,
    },
    CatalogEntry {
        key: "transmission",
        name: "Transmission",
        badge: "TR",
        color: "#22c55e",
        desc: "轻量、省资源，Windows / macOS / Linux。",
        url: "https://transmissionbt.com/download/",
        recommended: false,
        exe_name: Some("transmission-qt.exe"),
        paths: &[
            r"{ProgramFiles}\Transmission\transmission-qt.exe",
            r"{ProgramFiles(x86)}\Transmission\transmission-qt.exe",
        ],
        args: "uri",
        fallback: None,
        thunder: false,
    },
    CatalogEntry {
        key: "deluge",
        name: "Deluge",
        badge: "DL",
        color: "#f59e0b",
        desc: "插件化，适合进阶用户。",
        url: "https://deluge-torrent.org/download/",
        recommended: false,
        exe_name: Some("deluge.exe"),
        paths: &[
            r"{ProgramFiles}\Deluge\deluge.exe",
            r"{ProgramFiles(x86)}\Deluge\deluge.exe",
        ],
        args: "uri",
        fallback: None,
        thunder: false,
    },
    CatalogEntry {
        key: "utorrent",
        name: "uTorrent",
        badge: "uT",
        color: "#64748b",
        desc: "经典轻量客户端；安装时注意取消捆绑组件。",
        url: "https://www.utorrent.com/downloads/",
        recommended: false,
        exe_name: Some("uTorrent.exe"),
        paths: &[
            r"{LOCALAPPDATA}\uTorrent\uTorrent.exe",
            r"{APPDATA}\uTorrent\uTorrent.exe",
            r"{ProgramFiles}\uTorrent\uTorrent.exe",
            r"{ProgramFiles(x86)}\uTorrent\uTorrent.exe",
        ],
        args: "uri",
        fallback: None,
        thunder: false,
    },
    CatalogEntry {
        key: "tixati",
        name: "Tixati",
        badge: "TX",
        color: "#8b5cf6",
        desc: "免费、功能丰富，支持 Windows / Linux。",
        url: "https://www.tixati.com/download/",
        recommended: false,
        exe_name: Some("tixati.exe"),
        paths: &[
            r"{ProgramFiles}\Tixati\tixati.exe",
            r"{ProgramFiles(x86)}\Tixati\tixati.exe",
        ],
        args: "uri",
        fallback: None,
        thunder: false,
    },
    CatalogEntry {
        key: "biglybt",
        name: "BiglyBT",
        badge: "BB",
        color: "#06b6d4",
        desc: "开源、功能全面，基于 Azureus。",
        url: "https://www.biglybt.com/download/",
        recommended: false,
        exe_name: Some("BiglyBT.exe"),
        paths: &[
            r"{ProgramFiles}\BiglyBT\BiglyBT.exe",
            r"{ProgramFiles(x86)}\BiglyBT\BiglyBT.exe",
        ],
        args: "uri",
        fallback: None,
        thunder: false,
    },
    CatalogEntry {
        key: "thunder",
        name: "迅雷",
        badge: "XL",
        color: "#eab308",
        desc: "国内常用；本程序右键菜单已支持联动迅雷。",
        url: "https://www.xunlei.com/",
        recommended: false,
        exe_name: None,
        paths: &[],
        args: "none",
        fallback: Some("shell"),
        thunder: true,
    },
];

pub trait Probe {
    fn environment(&self) -> HashMap<String, String>;
    fn exists(&self, path: &str) -> bool;
    fn app_path(&self, exe_name: &str) -> Option<String>;
    fn magnet_handler(&self) -> Option<String>;
    fn thunder_path(&self) -> Option<String>;
}

pub struct SystemProbe;

impl SystemProbe {
    fn env_or(name: &str, fallback: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| fallback.to_string())
    }
}

impl Probe for SystemProbe {
    fn environment(&self) -> HashMap<String, String> {
        HashMap::from([
            (
                "ProgramFiles".to_string(),
                Self::env_or("ProgramFiles", r"C:\Program Files"),
            ),
            (
                "ProgramFiles(x86)".to_string(),
                Self::env_or("ProgramFiles(x86)", r"C:\Program Files (x86)"),
            ),
            (
                "LOCALAPPDATA".to_string(),
                Self::env_or("LOCALAPPDATA", ""),
            ),
            ("APPDATA".to_string(), Self::env_or("APPDATA", "")),
        ])
    }

    fn exists(&self, path: &str) -> bool {
        !path.is_empty() && Path::new(path).exists()
    }

    fn app_path(&self, exe_name: &str) -> Option<String> {
        #[cfg(windows)]
        {
            use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
            use winreg::RegKey;

            let bases = [
                (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"),
                (
                    HKEY_LOCAL_MACHINE,
                    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths",
                ),
                (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths"),
                (
                    HKEY_CURRENT_USER,
                    r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths",
                ),
            ];
            for (root, base) in bases {
                let key = RegKey::predef(root);
                let Ok(subkey) = key.open_subkey(format!(r"{base}\{exe_name}")) else {
                    continue;
                };
                let Ok(value) = subkey.get_value::<String, _>("") else {
                    continue;
                };
                let value = value.trim().trim_matches('"').to_string();
                if !value.is_empty() && Path::new(&value).exists() {
                    return Some(value);
                }
            }
        }
        None
    }

    fn magnet_handler(&self) -> Option<String> {
        #[cfg(windows)]
        {
            use winreg::enums::HKEY_CLASSES_ROOT;
            use winreg::RegKey;

            let root = RegKey::predef(HKEY_CLASSES_ROOT);
            let key = root.open_subkey(r"magnet\shell\open\command").ok()?;
            let command = key.get_value::<String, _>("").ok()?;
            let exe = command_exe(&command)?;
            if Path::new(&exe).exists() {
                return Some(exe);
            }
        }
        None
    }

    fn thunder_path(&self) -> Option<String> {
        #[cfg(windows)]
        {
            use winreg::enums::HKEY_LOCAL_MACHINE;
            use winreg::RegKey;

            let root = RegKey::predef(HKEY_LOCAL_MACHINE);
            if let Ok(key) = root.open_subkey(r"SOFTWARE\Classes\thunder\shell\open\command") {
                if let Ok(command) = key.get_value::<String, _>("") {
                    if let Some(exe) = command_exe(&command) {
                        if Path::new(&exe).exists() {
                            return Some(exe);
                        }
                    }
                }
            }
        }
        THUNDER_PATHS
            .iter()
            .find(|path| Path::new(path).exists())
            .map(|path| (*path).to_string())
    }
}

fn command_exe(command: &str) -> Option<String> {
    let trimmed = command.trim_start();
    if let Some(rest) = trimmed.strip_prefix('"') {
        return rest.split('"').next().map(str::to_string);
    }
    trimmed.split_whitespace().next().map(str::to_string)
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Target {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub exe: String,
}

fn normalize_path(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

pub fn detect(probe: &dyn Probe) -> Vec<Target> {
    let env = probe.environment();
    let mut targets = Vec::new();

    for entry in CATALOG.iter() {
        let mut exe = None;
        if entry.thunder {
            exe = probe.thunder_path();
        } else {
            for template in entry.paths {
                let mut path = template.to_string();
                for (key, value) in &env {
                    path = path.replace(&format!("{{{key}}}"), value);
                }
                if probe.exists(&path) {
                    exe = Some(path);
                    break;
                }
            }
            if exe.is_none() {
                if let Some(exe_name) = entry.exe_name {
                    exe = probe.app_path(exe_name);
                }
            }
        }
        if let Some(exe) = exe {
            targets.push(Target {
                key: entry.key.to_string(),
                name: entry.name.to_string(),
                kind: "client".to_string(),
                exe,
            });
        }
    }

    if let Some(handler_exe) = probe.magnet_handler() {
        let normalized = normalize_path(&handler_exe);
        let duplicate = targets
            .iter()
            .any(|target| normalize_path(&target.exe) == normalized);
        if !duplicate {
            let name = Path::new(&handler_exe)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("system")
                .to_string();
            targets.push(Target {
                key: "__system__".to_string(),
                name,
                kind: "system".to_string(),
                exe: handler_exe,
            });
        }
    }

    targets
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum HandoffPlan {
    Shell {
        clipboard: String,
        exe: String,
    },
    Process {
        exe: String,
        args: Vec<String>,
        clipboard: String,
        fallback: Option<Box<HandoffPlan>>,
    },
}

pub fn plan(uri: &str, target: Option<&Target>) -> Option<HandoffPlan> {
    let target = target?;
    if target.kind == "system" {
        return Some(HandoffPlan::Shell {
            clipboard: uri.to_string(),
            exe: target.exe.clone(),
        });
    }
    let entry = CATALOG.iter().find(|entry| entry.key == target.key)?;
    let args = if entry.args == "uri" {
        vec![uri.to_string()]
    } else {
        Vec::new()
    };
    let fallback = if entry.fallback == Some("shell") {
        Some(Box::new(HandoffPlan::Shell {
            clipboard: uri.to_string(),
            exe: target.exe.clone(),
        }))
    } else {
        None
    };
    Some(HandoffPlan::Process {
        exe: target.exe.clone(),
        args,
        clipboard: uri.to_string(),
        fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProbe {
        existing: Vec<String>,
        env: HashMap<String, String>,
        app_paths: HashMap<String, String>,
        handler: Option<String>,
        thunder: Option<String>,
    }

    impl FakeProbe {
        fn new() -> Self {
            Self {
                existing: Vec::new(),
                env: HashMap::from([
                    ("ProgramFiles".to_string(), r"C:\Program Files".to_string()),
                    (
                        "ProgramFiles(x86)".to_string(),
                        r"C:\Program Files (x86)".to_string(),
                    ),
                    (
                        "LOCALAPPDATA".to_string(),
                        r"C:\Users\Test\AppData\Local".to_string(),
                    ),
                    (
                        "APPDATA".to_string(),
                        r"C:\Users\Test\AppData\Roaming".to_string(),
                    ),
                ]),
                app_paths: HashMap::new(),
                handler: None,
                thunder: None,
            }
        }
    }

    impl Probe for FakeProbe {
        fn environment(&self) -> HashMap<String, String> {
            self.env.clone()
        }

        fn exists(&self, path: &str) -> bool {
            !path.is_empty()
                && self
                    .existing
                    .iter()
                    .any(|candidate| normalize_path(candidate) == normalize_path(path))
        }

        fn app_path(&self, exe_name: &str) -> Option<String> {
            self.app_paths.get(exe_name).cloned()
        }

        fn magnet_handler(&self) -> Option<String> {
            self.handler.clone()
        }

        fn thunder_path(&self) -> Option<String> {
            self.thunder.clone()
        }
    }

    const QBIT: &str = r"C:\Program Files\qBittorrent\qbittorrent.exe";
    const THUNDER: &str = r"C:\Thunder\Thunder.exe";
    const QUARK: &str = r"C:\Quark\quark.exe";

    #[test]
    fn catalog_is_consistent() {
        let keys: Vec<&str> = CATALOG.iter().map(|entry| entry.key).collect();
        assert_eq!(
            keys,
            vec![
                "qbittorrent",
                "bitcomet",
                "transmission",
                "deluge",
                "utorrent",
                "tixati",
                "biglybt",
                "thunder"
            ]
        );
        for entry in CATALOG.iter() {
            assert!(!entry.name.is_empty());
            assert!(!entry.badge.is_empty());
            assert!(!entry.color.is_empty());
            assert!(!entry.desc.is_empty());
            assert!(!entry.url.is_empty());
        }
        assert!(CATALOG[0].recommended);
    }

    #[test]
    fn detects_catalog_order_and_system_last() {
        let mut probe = FakeProbe::new();
        probe.existing = vec![QBIT.to_string()];
        probe.thunder = Some(THUNDER.to_string());
        probe.handler = Some(QUARK.to_string());
        let targets = detect(&probe);
        let keys: Vec<&str> = targets.iter().map(|target| target.key.as_str()).collect();
        assert_eq!(keys, vec!["qbittorrent", "thunder", "__system__"]);
        assert_eq!(targets[2].kind, "system");
        assert_eq!(targets[2].name, "quark");
    }

    #[test]
    fn default_handler_matching_a_client_is_not_duplicated() {
        let mut probe = FakeProbe::new();
        probe.existing = vec![QBIT.to_string()];
        probe.handler = Some(QBIT.to_string());
        let targets = detect(&probe);
        assert_eq!(targets.len(), 1);
        assert!(targets.iter().all(|target| target.kind != "system"));
    }

    #[test]
    fn app_path_is_the_last_resort() {
        let mut probe = FakeProbe::new();
        probe
            .app_paths
            .insert("qbittorrent.exe".to_string(), r"C:\Portable\qbittorrent.exe".to_string());
        let targets = detect(&probe);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].exe, r"C:\Portable\qbittorrent.exe");
    }

    #[test]
    fn plans_match_client_kind() {
        let target = Target {
            key: "qbittorrent".to_string(),
            name: "qBittorrent".to_string(),
            kind: "client".to_string(),
            exe: QBIT.to_string(),
        };
        match plan("magnet:?xt=urn:btih:abc", Some(&target)).unwrap() {
            HandoffPlan::Process {
                exe,
                args,
                fallback,
                ..
            } => {
                assert_eq!(exe, QBIT);
                assert_eq!(args, vec!["magnet:?xt=urn:btih:abc"]);
                assert!(fallback.is_none());
            }
            other => panic!("unexpected plan: {other:?}"),
        }

        let thunder = Target {
            key: "thunder".to_string(),
            name: "迅雷".to_string(),
            kind: "client".to_string(),
            exe: THUNDER.to_string(),
        };
        match plan("magnet:?xt=urn:btih:def", Some(&thunder)).unwrap() {
            HandoffPlan::Process { args, fallback, .. } => {
                assert!(args.is_empty());
                assert!(matches!(fallback.as_deref(), Some(HandoffPlan::Shell { .. })));
            }
            other => panic!("unexpected plan: {other:?}"),
        }

        let system = Target {
            key: "__system__".to_string(),
            name: "quark".to_string(),
            kind: "system".to_string(),
            exe: QUARK.to_string(),
        };
        assert!(matches!(
            plan("magnet:x", Some(&system)),
            Some(HandoffPlan::Shell { .. })
        ));
        assert!(plan("magnet:x", None).is_none());
    }
}
