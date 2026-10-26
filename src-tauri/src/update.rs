//! GitHub release update checks.

use std::time::Duration;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::net::Downloader;

pub const API_BASE: &str = "https://api.github.com";
pub const USER_AGENT: &str = "GameMagNet/4.0";

#[derive(Clone, Debug, Serialize)]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: String,
    pub url: String,
    #[serde(rename = "downloadUrl")]
    pub download_url: Option<String>,
    #[serde(rename = "downloadDigest")]
    pub download_digest: Option<String>,
    pub notes: String,
    #[serde(rename = "publishedAt")]
    pub published_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateResult {
    pub current: String,
    pub latest: String,
    #[serde(rename = "updateAvailable")]
    pub update_available: bool,
    pub release: ReleaseInfo,
}

pub fn verify_sha256(bytes: &[u8], digest: &str) -> bool {
    let expected = digest
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or(digest.trim())
        .to_lowercase();
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes)) == expected
}

pub fn parse_version(value: &str) -> Vec<u64> {
    let text = value.trim().to_lowercase();
    let text = text.strip_prefix('v').unwrap_or(&text);
    let mut parts = Vec::new();
    for token in text.split(|ch: char| !ch.is_ascii_digit()) {
        if token.is_empty() {
            continue;
        }
        if let Ok(number) = token.parse::<u64>() {
            parts.push(number);
        }
    }
    if parts.is_empty() {
        vec![0]
    } else {
        parts
    }
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    let mut newer = parse_version(latest);
    let mut existing = parse_version(current);
    let width = newer.len().max(existing.len());
    newer.resize(width, 0);
    existing.resize(width, 0);
    newer > existing
}

fn validate_repo(repo: &str) -> bool {
    static REPO_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    REPO_RE
        .get_or_init(|| Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$").unwrap())
        .is_match(repo.trim())
}

pub fn check_latest(
    downloader: &Downloader,
    repo: &str,
    current: &str,
    token: Option<&str>,
    timeout: Duration,
    api_base: &str,
) -> Result<UpdateResult, String> {
    if !validate_repo(repo) {
        return Err(format!("无效的仓库名: '{repo}'"));
    }

    let url = format!(
        "{}/repos/{}/releases/latest",
        api_base.trim_end_matches('/'),
        repo
    );
    let mut headers: Vec<(String, String)> = vec![
        (
            "Accept".to_string(),
            "application/vnd.github+json".to_string(),
        ),
        ("User-Agent".to_string(), USER_AGENT.to_string()),
    ];
    if let Some(token) = token {
        headers.push(("Authorization".to_string(), format!("Bearer {token}")));
    }
    let header_refs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();

    let response = downloader
        .get(&url, &header_refs, timeout)
        .map_err(|error| format!("无法连接 GitHub: {error}"))?;
    match response.status {
        404 => {
            return Err("未找到 Release（仓库不存在、未公开或尚未发布版本）".to_string())
        }
        401 | 403 => {
            return Err(format!(
                "GitHub API 拒绝访问（HTTP {}）",
                response.status
            ))
        }
        status if status >= 400 => {
            return Err(format!("GitHub API 错误（HTTP {status}）"))
        }
        _ => {}
    }

    let payload: Value = serde_json::from_slice(&response.body)
        .map_err(|error| format!("无法连接 GitHub: {error}"))?;
    let Some(tag_value) = payload.get("tag_name") else {
        return Err("GitHub 返回的 Release 数据无效".to_string());
    };
    if !tag_value.is_string() {
        return Err("GitHub 返回的 Release 数据无效".to_string());
    }
    let tag = tag_value.as_str().unwrap_or("").trim().to_string();

    let mut download_url = None;
    let mut download_digest = None;
    if let Some(Value::Array(assets)) = payload.get("assets") {
        let mut best: Option<(u8, String, Option<String>)> = None;
        for asset in assets {
            let Some(candidate) = asset.get("browser_download_url").and_then(Value::as_str) else {
                continue;
            };
            let asset_name = asset.get("name").and_then(Value::as_str).unwrap_or("");
            let digest = asset
                .get("digest")
                .and_then(Value::as_str)
                .map(str::to_string);
            let rank = if asset_name.eq_ignore_ascii_case("GameMagNet.exe") {
                3
            } else if candidate.to_lowercase().ends_with(".exe")
                && !asset_name.to_lowercase().contains("setup")
            {
                2
            } else if candidate.to_lowercase().ends_with(".exe") {
                1
            } else {
                0
            };
            match &best {
                Some((best_rank, _, _)) if *best_rank >= rank => {}
                _ => best = Some((rank, candidate.to_string(), digest)),
            }
        }
        if let Some((_, url, digest)) = best {
            download_url = Some(url);
            download_digest = digest;
        }
    }

    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .unwrap_or(&tag)
        .to_string();
    let release = ReleaseInfo {
        tag: tag.clone(),
        name,
        url: payload
            .get("html_url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        download_url,
        download_digest,
        notes: payload
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        published_at: payload
            .get("published_at")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    };

    Ok(UpdateResult {
        current: current.to_string(),
        latest: tag.clone(),
        update_available: is_newer(&tag, current),
        release,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{HttpClient, HttpResponse};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct FakeClient {
        status: u16,
        body: String,
    }

    impl HttpClient for FakeClient {
        fn get(
            &self,
            _url: &str,
            _headers: &[(&str, &str)],
            _timeout: Duration,
            _profile: Option<&str>,
        ) -> Result<HttpResponse, String> {
            Ok(HttpResponse {
                status: self.status,
                reason: String::new(),
                headers: HashMap::new(),
                body: self.body.as_bytes().to_vec(),
            })
        }
    }

    fn downloader(status: u16, body: &str) -> Arc<Downloader> {
        Arc::new(Downloader::new(Arc::new(FakeClient {
            status,
            body: body.to_string(),
        })))
    }

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("v1.2.0", "v1.1.9"));
        assert!(is_newer("1.10", "1.9"));
        assert!(!is_newer("v1.1", "v1.1.0"));
        assert!(!is_newer("v1.0", "v1.1"));
    }

    #[test]
    fn prefers_portable_exe_over_installer() {
        let body = r#"{
            "tag_name": "v1.3",
            "name": "GameMagNet v1.3",
            "html_url": "https://github.com/lzoreal/Game/releases/tag/v1.3",
            "body": "notes",
            "published_at": "2026-01-01T00:00:00Z",
            "assets": [
                {"name": "GameMagNet_1.3.0_x64-setup.exe", "browser_download_url": "https://example.com/setup.exe", "digest": "sha256:aaa"},
                {"name": "GameMagNet-windows.zip", "browser_download_url": "https://example.com/a.zip"},
                {"name": "GameMagNet.exe", "browser_download_url": "https://example.com/GameMagNet.exe", "digest": "sha256:bbb"}
            ]
        }"#;
        let client = downloader(200, body);
        let result = check_latest(
            &client,
            "lzoreal/Game",
            "v1.2",
            None,
            Duration::from_secs(5),
            API_BASE,
        )
        .unwrap();
        assert!(result.update_available);
        assert_eq!(
            result.release.download_url.as_deref(),
            Some("https://example.com/GameMagNet.exe")
        );
        assert_eq!(
            result.release.download_digest.as_deref(),
            Some("sha256:bbb")
        );
    }

    #[test]
    fn verifies_sha256_digests() {
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(b"abc", hash));
        assert!(verify_sha256(b"abc", &format!("sha256:{hash}")));
        assert!(!verify_sha256(b"abd", hash));
    }

    #[test]
    fn reports_missing_release_and_bad_payloads() {
        let missing = downloader(404, "{}");
        let error = check_latest(
            &missing,
            "lzoreal/Game",
            "v1.2",
            None,
            Duration::from_secs(5),
            API_BASE,
        )
        .unwrap_err();
        assert!(error.contains("未找到 Release"));

        let bad = downloader(200, "{}");
        let error = check_latest(
            &bad,
            "lzoreal/Game",
            "v1.2",
            None,
            Duration::from_secs(5),
            API_BASE,
        )
        .unwrap_err();
        assert!(error.contains("Release 数据无效"));

        assert!(check_latest(
            &bad,
            "not a repo",
            "v1.2",
            None,
            Duration::from_secs(5),
            API_BASE
        )
        .is_err());
    }

    #[test]
    fn parses_release_fields() {
        let body = r#"{"tag_name":"v2.0","name":"","html_url":"u","body":"b","published_at":"p","assets":[]}"#;
        let client = downloader(200, body);
        let result = check_latest(
            &client,
            "lzoreal/Game",
            "v1.2",
            None,
            Duration::from_secs(5),
            API_BASE,
        )
        .unwrap();
        assert_eq!(result.release.name, "v2.0");
        assert_eq!(result.latest, "v2.0");
        assert!(result.release.download_url.is_none());
    }
}
