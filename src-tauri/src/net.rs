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

/// Like [`finish`], but keeps the validators before the body is moved out.
fn finish_meta(response: HttpResponse) -> Result<Downloaded, NetError> {
    let etag = response.header("etag").map(str::to_string);
    let last_modified = response.header("last-modified").map(str::to_string);
    let body = finish(response)?;
    Ok(Downloaded {
        body,
        etag,
        last_modified,
        via_cloudflare: false,
    })
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

/// A downloaded body together with the validators that make the *next* fetch
/// cheap. Responses obtained through the Cloudflare browser channel carry no
/// usable validators (`None`, `None`), so callers fall back to hashing.
#[derive(Clone, Debug, Default)]
pub struct Downloaded {
    pub body: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub via_cloudflare: bool,
}

/// Outcome of a conditional fetch.
#[derive(Clone, Debug)]
pub enum FetchOutcome {
    /// `304`: the stored copy is still current. No body was transferred.
    NotModified,
    Fetched(Downloaded),
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
        self.download_json_with_meta(url).map(|fetched| fetched.body)
    }

    pub fn download_json_with_meta(&self, url: &str) -> Result<Downloaded, NetError> {
        self.download_json_with_meta_opts(url, true)
    }

    /// Downloads without ever starting the browser channel. Background refresh
    /// uses this: opening a Chrome window is only acceptable when the user
    /// asked for the fetch.
    pub fn download_json_with_meta_no_solver(&self, url: &str) -> Result<Downloaded, NetError> {
        self.download_json_with_meta_opts(url, false)
    }

    fn download_json_with_meta_opts(
        &self,
        url: &str,
        use_solver: bool,
    ) -> Result<Downloaded, NetError> {
        match self.fetch(url, &[], use_solver)? {
            FetchOutcome::Fetched(fetched) => Ok(fetched),
            // Only reachable if a server volunteers a 304 for a request that
            // carried no validators; retry like an ordinary download.
            FetchOutcome::NotModified => Err(NetError::plain(
                "下载失败: 服务器返回 304，但没有可用的缓存副本，请重试",
            )),
        }
    }

    /// Conditional GET: sends `If-None-Match` / `If-Modified-Since` when the
    /// caller has them. A server that ignores them simply answers `200`.
    pub fn conditional_json(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<FetchOutcome, NetError> {
        self.conditional_json_opts(url, etag, last_modified, true)
    }

    /// Same, never falling back to the browser channel.
    pub fn conditional_json_no_solver(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<FetchOutcome, NetError> {
        self.conditional_json_opts(url, etag, last_modified, false)
    }

    fn conditional_json_opts(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        use_solver: bool,
    ) -> Result<FetchOutcome, NetError> {
        let mut extra: Vec<(&str, &str)> = Vec::new();
        if let Some(value) = etag.filter(|value| !value.trim().is_empty()) {
            extra.push(("If-None-Match", value));
        }
        if let Some(value) = last_modified.filter(|value| !value.trim().is_empty()) {
            extra.push(("If-Modified-Since", value));
        }
        if extra.is_empty() {
            return self
                .download_json_with_meta_opts(url, use_solver)
                .map(FetchOutcome::Fetched);
        }
        self.fetch(url, &extra, use_solver)
    }

    fn fetch(
        &self,
        url: &str,
        extra: &[(&str, &str)],
        use_solver: bool,
    ) -> Result<FetchOutcome, NetError> {
        let mut headers = request_headers(url);
        headers.extend(
            extra
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string())),
        );
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
            if response.status == 304 {
                return Ok(FetchOutcome::NotModified);
            }
            return finish_meta(response).map(FetchOutcome::Fetched);
        }

        let mut solver_note = String::new();
        if use_solver {
            if let Some(solver) = self.solver.as_ref() {
                match solver.fetch(url) {
                    Ok(body) => {
                        return Ok(FetchOutcome::Fetched(Downloaded {
                            body,
                            etag: None,
                            last_modified: None,
                            via_cloudflare: true,
                        }))
                    }
                    Err(error) => {
                        solver_note = format!("；chaser-cf 自动验证失败: {error}");
                    }
                }
            }
        } else {
            solver_note = "；后台刷新不会自动打开验证浏览器，请手动刷新该源".to_string();
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
        seen_headers: Mutex<Vec<Vec<(String, String)>>>,
    }

    impl ScriptedClient {
        fn new(responses: Vec<Result<HttpResponse, String>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
                seen_profiles: Mutex::new(Vec::new()),
                seen_headers: Mutex::new(Vec::new()),
            })
        }

        fn last_headers(&self) -> Vec<(String, String)> {
            self.seen_headers
                .lock()
                .unwrap()
                .last()
                .cloned()
                .unwrap_or_default()
        }
    }

    impl HttpClient for ScriptedClient {
        fn get(
            &self,
            _url: &str,
            headers: &[(&str, &str)],
            _timeout: Duration,
            profile: Option<&str>,
        ) -> Result<HttpResponse, String> {
            self.seen_profiles
                .lock()
                .unwrap()
                .push(profile.map(str::to_string));
            self.seen_headers.lock().unwrap().push(
                headers
                    .iter()
                    .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                    .collect(),
            );
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

    #[test]
    fn response_validators_are_kept_for_the_next_fetch() {
        let mut ok = response(200, r#"{"downloads": []}"#);
        ok.headers
            .insert("etag".to_string(), "\"v7\"".to_string());
        ok.headers.insert(
            "last-modified".to_string(),
            "Wed, 21 Oct 2026 07:28:00 GMT".to_string(),
        );
        let downloader = Downloader::new(ScriptedClient::new(vec![Ok(ok)]));

        let fetched = downloader
            .download_json_with_meta("https://mirror.example/list.json")
            .unwrap();
        assert_eq!(fetched.body, r#"{"downloads": []}"#);
        assert_eq!(fetched.etag.as_deref(), Some("\"v7\""));
        assert_eq!(
            fetched.last_modified.as_deref(),
            Some("Wed, 21 Oct 2026 07:28:00 GMT")
        );
        assert!(!fetched.via_cloudflare);
    }

    #[test]
    fn conditional_request_sends_both_validators() {
        let client = ScriptedClient::new(vec![Ok(response(304, ""))]);
        let downloader = Downloader::new(client.clone());
        let outcome = downloader
            .conditional_json(
                "https://mirror.example/list.json",
                Some("\"v7\""),
                Some("Wed, 21 Oct 2026 07:28:00 GMT"),
            )
            .unwrap();

        assert!(matches!(outcome, FetchOutcome::NotModified));
        let headers = client.last_headers();
        assert!(headers
            .iter()
            .any(|(key, value)| key.eq_ignore_ascii_case("if-none-match") && value == "\"v7\""));
        assert!(headers.iter().any(|(key, value)| {
            key.eq_ignore_ascii_case("if-modified-since")
                && value == "Wed, 21 Oct 2026 07:28:00 GMT"
        }));
        assert!(
            client.seen_profiles.lock().unwrap().len() == 1,
            "a 304 is not a challenge, so no retry"
        );
    }

    #[test]
    fn a_server_ignoring_validators_still_yields_the_body() {
        let mut ok = response(200, r#"{"downloads": [1]}"#);
        ok.headers.insert("etag".to_string(), "\"v8\"".to_string());
        let downloader = Downloader::new(ScriptedClient::new(vec![Ok(ok)]));

        match downloader
            .conditional_json("https://mirror.example/list.json", Some("\"v7\""), None)
            .unwrap()
        {
            FetchOutcome::Fetched(fetched) => {
                assert_eq!(fetched.body, r#"{"downloads": [1]}"#);
                assert_eq!(fetched.etag.as_deref(), Some("\"v8\""));
            }
            FetchOutcome::NotModified => panic!("expected a fresh body"),
        }
    }

    #[test]
    fn a_conditional_request_without_validators_is_a_plain_download() {
        let client = ScriptedClient::new(vec![Ok(response(200, r#"{"downloads": []}"#))]);
        let downloader = Downloader::new(client.clone());

        let outcome = downloader
            .conditional_json("https://mirror.example/list.json", None, Some("  "))
            .unwrap();
        assert!(matches!(outcome, FetchOutcome::Fetched(_)));
        assert!(!client
            .last_headers()
            .iter()
            .any(|(key, _)| key.to_lowercase().starts_with("if-")));
    }

    #[test]
    fn passive_fetch_never_starts_the_browser_channel() {
        let client = ScriptedClient::new(vec![
            Ok(challenge()),
            Ok(challenge()),
            Ok(challenge()),
        ]);
        let solver = FakeSolver::new(vec![Ok(r#"{"downloads": []}"#.to_string())]);
        let downloader = Downloader::with_solver(client.clone(), solver.clone());

        let error = downloader
            .conditional_json_no_solver("https://mirror.example/list.json", None, None)
            .unwrap_err();

        assert!(error.is_cloudflare());
        assert!(error.message.contains("后台刷新不会自动打开验证浏览器"));
        assert!(solver.calls.lock().unwrap().is_empty());
        assert_eq!(client.seen_profiles.lock().unwrap().len(), 3);
    }

    #[test]
    fn cloudflare_bodies_carry_no_validators() {        let client = ScriptedClient::new(vec![
            Ok(challenge()),
            Ok(challenge()),
            Ok(challenge()),
        ]);
        let solver = FakeSolver::new(vec![Ok(r#"{"downloads": []}"#.to_string())]);
        let downloader = Downloader::with_solver(client, solver);

        let fetched = downloader
            .download_json_with_meta("https://mirror.example/list.json")
            .unwrap();
        assert!(fetched.via_cloudflare);
        assert!(fetched.etag.is_none());
        assert!(fetched.last_modified.is_none());
    }
}
