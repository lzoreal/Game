//! HTTP download seam with Cloudflare-challenge detection and retry.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::chaser::CloudflareSolver;

pub const IMPERSONATE_PROFILES: [&str; 3] = ["chrome", "safari", "firefox"];
const CF_STATUS_CODES: [u16; 3] = [403, 429, 503];
const CF_BODY_MARKERS: [&str; 9] = [
    "just a moment",
    "checking your browser",
    "cf-chl",
    "cf_chl_opt",
    "attention required",
    "cf-mitigated",
    "enable javascript and cookies to continue",
    "请稍候",
    "正在验证您是否是真人",
];
pub const CF_BLOCKED_HINT: &str = "自动验证未通过：可在浏览器中打开该地址，保存返回的 JSON 后通过「从本地导入 JSON」导入；或改用其他镜像地址。";

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_lowercase())
            .map(String::as_str)
    }
}

/// Transport seam so source commands can be tested without a network.
pub trait HttpClient: Send + Sync {
    fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        timeout: Duration,
        profile: Option<&str>,
    ) -> Result<HttpResponse, String>;
}

#[derive(Clone, Debug)]
pub struct NetError {
    pub message: String,
    pub cloudflare_url: Option<String>,
}

impl NetError {
    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cloudflare_url: None,
        }
    }

    pub fn is_cloudflare(&self) -> bool {
        self.cloudflare_url.is_some()
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for NetError {}

fn finish(response: HttpResponse) -> Result<String, NetError> {
    if response.status >= 400 {
        let reason = response.reason.trim();
        let message = if reason.is_empty() {
            format!("下载失败: HTTP {}", response.status)
        } else {
            format!("下载失败: HTTP {} {reason}", response.status)
        };
        return Err(NetError::plain(message));
    }
    String::from_utf8(response.body)
        .map_err(|error| NetError::plain(format!("下载失败: 响应内容无效: {error}")))
}

pub fn is_cloudflare_challenge(status: u16, headers: &HashMap<String, String>, body: &[u8]) -> bool {
    if !CF_STATUS_CODES.contains(&status) {
        return false;
    }
    let header_text = headers
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    if header_text.contains("cf-mitigated") {
        return true;
    }
    let take = body.len().min(8192);
    let snippet = String::from_utf8_lossy(&body[..take]).to_lowercase();
    CF_BODY_MARKERS
        .iter()
        .any(|marker| snippet.contains(marker))
}

pub fn request_headers(url: &str) -> Vec<(String, String)> {
    let referer = referer_for(url);
    vec![
        (
            "User-Agent".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36".to_string(),
        ),
        (
            "Accept".to_string(),
            "application/json, text/plain, */*".to_string(),
        ),
        ("Accept-Language".to_string(), "zh-CN,zh;q=0.9,en;q=0.8".to_string()),
        ("Referer".to_string(), referer),
    ]
}

pub fn referer_for(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return String::new();
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    format!("{scheme}://{host}/")
}

pub struct Downloader {
    client: Arc<dyn HttpClient>,
    solver: Option<Arc<dyn CloudflareSolver>>,
}

impl Downloader {
    pub fn new(client: Arc<dyn HttpClient>) -> Self {
        Self {
            client,
            solver: None,
        }
    }

    pub fn with_solver(
        client: Arc<dyn HttpClient>,
        solver: Arc<dyn CloudflareSolver>,
    ) -> Self {
        Self {
            client,
            solver: Some(solver),
        }
    }

    pub fn download_json(&self, url: &str) -> Result<String, NetError> {
        let headers = request_headers(url);
        let header_refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();

        let mut blocked = Vec::new();
        for profile in IMPERSONATE_PROFILES {
            let response = self
                .client
                .get(url, &header_refs, DEFAULT_TIMEOUT, Some(profile))
                .map_err(|error| NetError::plain(format!("下载失败: {error}")))?;

            if is_cloudflare_challenge(response.status, &response.headers, &response.body) {
                blocked.push(profile);
                continue;
            }
            return finish(response);
        }

        let mut solver_note = String::new();
        if let Some(solver) = self.solver.as_ref() {
            match solver.fetch(url) {
                Ok(body) => return Ok(body),
                Err(error) => {
                    solver_note = format!("；chaser-cf 自动验证失败: {error}");
                }
            }
        }

        Err(NetError {
            message: format!(
                "下载失败: 目标站点启用了 Cloudflare 人机验证，已尝试 {} 浏览器指纹仍被拦截{}{CF_BLOCKED_HINT}",
                blocked.join(" / "),
                solver_note
            ),
            cloudflare_url: Some(url.to_string()),
        })
    }

    pub fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<HttpResponse, NetError> {
        self.client
            .get(url, headers, timeout, None)
            .map_err(|error| NetError::plain(format!("请求失败: {error}")))
    }

    pub fn download_bytes(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<Vec<u8>, NetError> {
        let response = self.get(url, headers, timeout)?;
        if response.status >= 400 {
            return Err(NetError::plain(format!("HTTP {}", response.status)));
        }
        let gzipped = response
            .header("content-encoding")
            .map(|value| value.eq_ignore_ascii_case("gzip"))
            .unwrap_or(false);
        let body = response.body;
        if gzipped {
            let mut decoded = Vec::new();
            use std::io::Read;
            flate2::read::GzDecoder::new(&body[..])
                .read_to_end(&mut decoded)
                .map_err(|error| NetError::plain(format!("gzip 解压失败: {error}")))?;
            return Ok(decoded);
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct ScriptedClient {
        responses: Mutex<Vec<Result<HttpResponse, String>>>,
        seen_profiles: Mutex<Vec<Option<String>>>,
    }

    impl ScriptedClient {
        fn new(responses: Vec<Result<HttpResponse, String>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
                seen_profiles: Mutex::new(Vec::new()),
            })
        }
    }

    impl HttpClient for ScriptedClient {
        fn get(
            &self,
            _url: &str,
            _headers: &[(&str, &str)],
            _timeout: Duration,
            profile: Option<&str>,
        ) -> Result<HttpResponse, String> {
            self.seen_profiles
                .lock()
                .unwrap()
                .push(profile.map(str::to_string));
            self.responses.lock().unwrap().remove(0)
        }
    }

    struct FakeSolver {
        bodies: Mutex<Vec<Result<String, String>>>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeSolver {
        fn new(bodies: Vec<Result<String, String>>) -> Arc<Self> {
            Arc::new(Self {
                bodies: Mutex::new(bodies),
                calls: Mutex::new(Vec::new()),
            })
        }
    }

    impl CloudflareSolver for FakeSolver {
        fn fetch(&self, url: &str) -> Result<String, String> {
            self.calls.lock().unwrap().push(url.to_string());
            self.bodies.lock().unwrap().remove(0)
        }
    }

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            reason: "OK".to_string(),
            headers: HashMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn challenge() -> HttpResponse {
        let mut headers = HashMap::new();
        headers.insert("cf-mitigated".to_string(), "challenge".to_string());
        HttpResponse {
            status: 403,
            reason: "Forbidden".to_string(),
            headers,
            body: b"<title>Just a moment...</title>".to_vec(),
        }
    }

    #[test]
    fn detects_only_interstitials() {
        let mut headers = HashMap::new();
        headers.insert("server".to_string(), "cloudflare".to_string());
        assert!(is_cloudflare_challenge(
            403,
            &headers,
            b"<title>Just a moment...</title>"
        ));
        assert!(!is_cloudflare_challenge(403, &headers, br#"{"error":"forbidden"}"#));
        assert!(!is_cloudflare_challenge(200, &HashMap::new(), b"Just a moment..."));
    }

    #[test]
    fn retries_with_next_browser_profile() {
        let client = ScriptedClient::new(vec![
            Ok(challenge()),
            Ok(response(200, r#"{"downloads": []}"#)),
        ]);
        let downloader = Downloader::new(client.clone());
        let body = downloader
            .download_json("https://mirror.example/list.json")
            .unwrap();
        assert_eq!(body, r#"{"downloads": []}"#);
        assert_eq!(
            client.seen_profiles.lock().unwrap().clone(),
            vec![Some("chrome".to_string()), Some("safari".to_string())]
        );
    }

    #[test]
    fn reports_cloudflare_after_all_profiles() {
        let client = ScriptedClient::new(vec![
            Ok(challenge()),
            Ok(challenge()),
            Ok(challenge()),
        ]);
        let downloader = Downloader::new(client);
        let error = downloader
            .download_json("https://mirror.example/list.json")
            .unwrap_err();
        assert!(error.is_cloudflare());
        assert!(error.message.contains("Cloudflare 人机验证"));
        assert!(error.message.contains("chrome"));
        assert!(error.message.contains("从本地导入"));
    }

    #[test]
    fn http_errors_are_not_retried() {
        let mut failed = response(404, "");
        failed.reason = "Not Found".to_string();
        let client = ScriptedClient::new(vec![Ok(failed)]);
        let downloader = Downloader::new(client.clone());
        let error = downloader
            .download_json("https://mirror.example/list.json")
            .unwrap_err();
        assert!(error.message.contains("HTTP 404"));
        assert_eq!(client.seen_profiles.lock().unwrap().len(), 1);
    }

    #[test]
    fn solver_body_used_after_profiles_blocked() {
        let client = ScriptedClient::new(vec![
            Ok(challenge()),
            Ok(challenge()),
            Ok(challenge()),
        ]);
        let solver = FakeSolver::new(vec![Ok(r#"{"downloads": []}"#.to_string())]);
        let downloader = Downloader::with_solver(client.clone(), solver.clone());

        let body = downloader
            .download_json("https://mirror.example/list.json")
            .unwrap();

        assert_eq!(body, r#"{"downloads": []}"#);
        assert_eq!(
            solver.calls.lock().unwrap().clone(),
            vec!["https://mirror.example/list.json".to_string()]
        );
        assert_eq!(client.seen_profiles.lock().unwrap().len(), 3);
    }

    #[test]
    fn solver_not_called_when_profile_succeeds() {
        let client = ScriptedClient::new(vec![Ok(response(200, r#"{"downloads": []}"#))]);
        let solver = FakeSolver::new(vec![]);
        let downloader = Downloader::with_solver(client, solver.clone());

        downloader
            .download_json("https://mirror.example/list.json")
            .unwrap();

        assert!(solver.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn solver_failure_keeps_cloudflare_hint() {
        let client = ScriptedClient::new(vec![
            Ok(challenge()),
            Ok(challenge()),
            Ok(challenge()),
        ]);
        let solver = FakeSolver::new(vec![Err("未找到 Chrome".to_string())]);
        let downloader = Downloader::with_solver(client, solver);

        let error = downloader
            .download_json("https://mirror.example/list.json")
            .unwrap_err();

        assert!(error.is_cloudflare());
        assert!(error.message.contains("chaser-cf"));
        assert!(error.message.contains("未找到 Chrome"));
        assert!(error.message.contains("从本地导入"));
    }
}
