//! System-TLS HTTP client used by the desktop app.

use std::collections::HashMap;
use std::time::Duration;

use reqwest::blocking::Client;

use crate::net::{HttpClient, HttpResponse};

const CHROME_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
const SAFARI_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15";
const FIREFOX_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:127.0) Gecko/20100101 Firefox/127.0";

pub struct ReqwestClient {
    client: Client,
}

impl ReqwestClient {
    pub fn new() -> Result<Self, String> {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { client })
    }
}

impl HttpClient for ReqwestClient {
    fn get(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        timeout: Duration,
        profile: Option<&str>,
    ) -> Result<HttpResponse, String> {
        let user_agent = match profile {
            Some("safari") => SAFARI_UA,
            Some("firefox") => FIREFOX_UA,
            _ => CHROME_UA,
        };
        let mut request = self
            .client
            .get(url)
            .timeout(timeout)
            .header("User-Agent", user_agent);
        for (key, value) in headers {
            if key.eq_ignore_ascii_case("user-agent") {
                continue;
            }
            request = request.header(*key, *value);
        }
        let response = request.send().map_err(|error| error.to_string())?;
        let status = response.status().as_u16();
        let reason = response
            .status()
            .canonical_reason()
            .unwrap_or("")
            .to_string();
        let mut map = HashMap::new();
        for (name, value) in response.headers().iter() {
            map.insert(
                name.as_str().to_lowercase(),
                value.to_str().unwrap_or("").to_string(),
            );
        }
        let body = response.bytes().map_err(|error| error.to_string())?.to_vec();
        Ok(HttpResponse {
            status,
            reason,
            headers: map,
            body,
        })
    }
}
