//! Magnet swarm statistics from public trackers. No torrent data is downloaded.

use std::collections::HashMap;
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::net::Downloader;

pub const USER_AGENT: &str = "GameMagNet/4.0";
pub const ANNOUNCE_PORT: u16 = 6881;
pub const CACHE_TTL: f64 = 120.0;
pub const MAX_TRACKERS: usize = 12;
pub const DEFAULT_TRACKERS: [&str; 3] = [
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.torrent.eu.org:451/announce",
];
const UDP_MAGIC: u64 = 0x41727101980;

#[derive(Clone, Debug)]
pub struct Magnet {
    pub info_hash: [u8; 20],
    pub trackers: Vec<String>,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SwarmStats {
    pub seeders: Option<i64>,
    pub leechers: Option<i64>,
    pub completed: Option<i64>,
    pub trackers_ok: usize,
    pub trackers_total: usize,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct TrackerStats {
    seeders: Option<i64>,
    leechers: Option<i64>,
    completed: Option<i64>,
}

fn cache() -> &'static Mutex<HashMap<[u8; 20], (f64, SwarmStats)>> {
    static CACHE: OnceLock<Mutex<HashMap<[u8; 20], (f64, SwarmStats)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn clear_cache() {
    cache().lock().unwrap().clear();
}

fn clock_origin() -> &'static Instant {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now)
}

fn monotonic_secs() -> f64 {
    clock_origin().elapsed().as_secs_f64()
}

pub fn parse_magnet(uri: &str) -> Result<Magnet, String> {
    if !uri.to_lowercase().starts_with("magnet:?") {
        return Err("不是磁力链接".to_string());
    }
    let query = uri.split_once('?').map(|(_, rest)| rest).unwrap_or("");
    let mut info_hash = None;
    let mut trackers: Vec<String> = Vec::new();
    let mut name = String::new();

    for part in query.split('&') {
        if part.is_empty() {
            continue;
        }
        let (key, value) = match part.split_once('=') {
            Some(pair) => pair,
            None => (part, ""),
        };
        match key.to_lowercase().as_str() {
            "xt" => {
                let candidate = String::from_utf8_lossy(&percent_decode(value)).to_string();
                let lowered = candidate.to_lowercase();
                if let Some(rest) = lowered.strip_prefix("urn:btih:") {
                    let raw = &candidate[candidate.len() - rest.len()..];
                    info_hash = Some(parse_info_hash(raw)?);
                }
            }
            "tr" => {
                let tracker = String::from_utf8_lossy(&percent_decode(value))
                    .trim()
                    .to_string();
                if !tracker.is_empty() && !trackers.contains(&tracker) {
                    trackers.push(tracker);
                }
            }
            "dn" => {
                if name.is_empty() {
                    name = String::from_utf8_lossy(&percent_decode(value)).to_string();
                }
            }
            _ => {}
        }
    }

    let info_hash = info_hash.ok_or_else(|| "磁力链接缺少 BTIH info_hash".to_string())?;
    Ok(Magnet {
        info_hash,
        trackers,
        name,
    })
}

fn percent_decode(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                match hex.and_then(|text| u8::from_str_radix(text, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    None => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    out
}

fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for byte in bytes {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn parse_info_hash(value: &str) -> Result<[u8; 20], String> {
    if value.len() == 40 && value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        let mut hash = [0u8; 20];
        for (index, chunk) in value.as_bytes().chunks(2).enumerate() {
            let text = std::str::from_utf8(chunk).unwrap_or("00");
            hash[index] = u8::from_str_radix(text, 16).unwrap_or(0);
        }
        return Ok(hash);
    }
    if value.len() == 32
        && value
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch.is_ascii_uppercase() || ch.is_ascii_lowercase())
        && value
            .chars()
            .all(|ch| !ch.is_ascii_digit() || ('2'..='7').contains(&ch))
    {
        let mut bits = 0u32;
        let mut buffer = 0u32;
        let mut out = Vec::new();
        for byte in value.bytes() {
            let val = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a',
                b'2'..=b'7' => 26 + (byte - b'2'),
                _ => return Err(format!("无效的 info_hash: {value}")),
            } as u32;
            buffer = (buffer << 5) | val;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                out.push(((buffer >> bits) & 0xff) as u8);
            }
        }
        if out.len() == 20 {
            let mut hash = [0u8; 20];
            hash.copy_from_slice(&out);
            return Ok(hash);
        }
    }
    Err(format!("无效的 info_hash: {value}"))
}

#[derive(Clone, Debug, PartialEq)]
pub enum BValue {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<BValue>),
    Dict(HashMap<Vec<u8>, BValue>),
}

pub fn bdecode(data: &[u8]) -> Result<BValue, String> {
    let (value, _) = bdecode_at(data, 0)?;
    Ok(value)
}

fn bdecode_at(data: &[u8], index: usize) -> Result<(BValue, usize), String> {
    if index >= data.len() {
        return Err("bencode 数据不完整".to_string());
    }
    match data[index] {
        b'i' => {
            let end = data[index + 1..]
                .iter()
                .position(|byte| *byte == b'e')
                .map(|offset| index + 1 + offset)
                .ok_or_else(|| "bencode 整数未结束".to_string())?;
            let text = std::str::from_utf8(&data[index + 1..end])
                .map_err(|_| "bencode 整数无效".to_string())?;
            let value = text
                .parse::<i64>()
                .map_err(|_| "bencode 整数无效".to_string())?;
            Ok((BValue::Int(value), end + 1))
        }
        b'l' => {
            let mut items = Vec::new();
            let mut cursor = index + 1;
            while cursor < data.len() && data[cursor] != b'e' {
                let (value, next) = bdecode_at(data, cursor)?;
                items.push(value);
                cursor = next;
            }
            Ok((BValue::List(items), cursor + 1))
        }
        b'd' => {
            let mut map = HashMap::new();
            let mut cursor = index + 1;
            while cursor < data.len() && data[cursor] != b'e' {
                let (key, next) = bdecode_at(data, cursor)?;
                let (value, after) = bdecode_at(data, next)?;
                let BValue::Bytes(key) = key else {
                    return Err("bencode 字典键无效".to_string());
                };
                map.insert(key, value);
                cursor = after;
            }
            Ok((BValue::Dict(map), cursor + 1))
        }
        _ => {
            let colon = data[index..]
                .iter()
                .position(|byte| *byte == b':')
                .map(|offset| index + offset)
                .ok_or_else(|| "bencode 字符串长度缺失".to_string())?;
            let text = std::str::from_utf8(&data[index..colon])
                .map_err(|_| "bencode 字符串长度无效".to_string())?;
            let length: usize = text
                .parse()
                .map_err(|_| "bencode 字符串长度无效".to_string())?;
            let start = colon + 1;
            let end = start + length;
            if end > data.len() {
                return Err("bencode 字符串不完整".to_string());
            }
            Ok((BValue::Bytes(data[start..end].to_vec()), end))
        }
    }
}

fn peer_id() -> [u8; 20] {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(1)
        ^ (std::process::id() as u64) << 32;
    let mut out = [0u8; 20];
    out[..8].copy_from_slice(b"-GM4000-");
    for slot in out.iter_mut().skip(8) {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        *slot = ALPHABET[(seed >> 33) as usize % ALPHABET.len()];
    }
    out
}

fn as_count(value: Option<&BValue>) -> Option<i64> {
    match value {
        Some(BValue::Int(number)) => Some((*number).max(0)),
        _ => None,
    }
}

pub fn announce_http(
    downloader: &Downloader,
    url: &str,
    info_hash: &[u8; 20],
    timeout: Duration,
) -> Result<TrackerStats, String> {
    let peer_id = peer_id();
    let query = format!(
        "info_hash={}&peer_id={}&port={}&uploaded=0&downloaded=0&left=1&compact=1&numwant=0&event=started",
        percent_encode(info_hash),
        percent_encode(&peer_id),
        ANNOUNCE_PORT
    );
    let separator = if url.contains('?') { '&' } else { '?' };
    let full = format!("{url}{separator}{query}");
    let headers = [("User-Agent", USER_AGENT)];
    let body = downloader
        .download_bytes(&full, &headers, timeout)
        .map_err(|error| error.message)?;

    let payload = bdecode(&body)?;
    let BValue::Dict(map) = payload else {
        return Err("tracker 响应格式无效".to_string());
    };
    if let Some(failure) = map.get(b"failure reason".as_slice()) {
        let text = match failure {
            BValue::Bytes(bytes) => String::from_utf8_lossy(bytes).to_string(),
            other => format!("{other:?}"),
        };
        return Err(text);
    }

    Ok(TrackerStats {
        seeders: as_count(map.get(b"complete".as_slice())),
        leechers: as_count(map.get(b"incomplete".as_slice())),
        completed: as_count(map.get(b"downloaded".as_slice())),
    })
}

pub fn announce_udp(
    url: &str,
    info_hash: &[u8; 20],
    timeout: Duration,
) -> Result<TrackerStats, String> {
    let (host, port) = parse_udp_target(url)?;
    let address = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| format!("无法解析 tracker 主机: {host} ({error})"))?
        .next()
        .ok_or_else(|| format!("无法解析 tracker 主机: {host}"))?;

    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|error| error.to_string())?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;
    socket
        .set_write_timeout(Some(timeout))
        .map_err(|error| error.to_string())?;

    let connect_tx = random_u32(info_hash);
    let mut connect = Vec::with_capacity(16);
    connect.extend_from_slice(&UDP_MAGIC.to_be_bytes());
    connect.extend_from_slice(&0u32.to_be_bytes());
    connect.extend_from_slice(&connect_tx.to_be_bytes());
    socket
        .send_to(&connect, address)
        .map_err(|error| error.to_string())?;

    let mut buffer = [0u8; 2048];
    let (size, _) = socket
        .recv_from(&mut buffer)
        .map_err(|error| format!("UDP tracker 超时: {host} ({error})"))?;
    if size < 16 {
        return Err("UDP tracker 连接响应过短".to_string());
    }
    let action = u32::from_be_bytes(buffer[0..4].try_into().unwrap());
    let txid = u32::from_be_bytes(buffer[4..8].try_into().unwrap());
    if action != 0 || txid != connect_tx {
        return Err("UDP tracker 连接响应无效".to_string());
    }
    let connection_id = u64::from_be_bytes(buffer[8..16].try_into().unwrap());

    let announce_tx = random_u32(&connection_id.to_be_bytes());
    let mut packet = Vec::with_capacity(98);
    packet.extend_from_slice(&connection_id.to_be_bytes());
    packet.extend_from_slice(&1u32.to_be_bytes());
    packet.extend_from_slice(&announce_tx.to_be_bytes());
    packet.extend_from_slice(info_hash);
    packet.extend_from_slice(&peer_id());
    packet.extend_from_slice(&0u64.to_be_bytes());
    packet.extend_from_slice(&1u64.to_be_bytes());
    packet.extend_from_slice(&0u64.to_be_bytes());
    packet.extend_from_slice(&2u32.to_be_bytes());
    packet.extend_from_slice(&0u32.to_be_bytes());
    packet.extend_from_slice(&random_u32(info_hash).to_be_bytes());
    packet.extend_from_slice(&0i32.to_be_bytes());
    packet.extend_from_slice(&ANNOUNCE_PORT.to_be_bytes());
    socket
        .send_to(&packet, address)
        .map_err(|error| error.to_string())?;

    let (size, _) = socket
        .recv_from(&mut buffer)
        .map_err(|error| format!("UDP tracker 超时: {host} ({error})"))?;
    if size < 8 {
        return Err("UDP tracker 应答过短".to_string());
    }
    let action = u32::from_be_bytes(buffer[0..4].try_into().unwrap());
    let txid = u32::from_be_bytes(buffer[4..8].try_into().unwrap());
    if action == 3 {
        let message = String::from_utf8_lossy(&buffer[8..size]).trim().to_string();
        return Err(if message.is_empty() {
            "UDP tracker 返回错误".to_string()
        } else {
            message
        });
    }
    if size < 20 {
        return Err("UDP tracker 应答过短".to_string());
    }
    let _interval = u32::from_be_bytes(buffer[8..12].try_into().unwrap());
    let leechers = u32::from_be_bytes(buffer[12..16].try_into().unwrap());
    let seeders = u32::from_be_bytes(buffer[16..20].try_into().unwrap());
    if action != 1 || txid != announce_tx {
        return Err("UDP tracker 应答无效".to_string());
    }

    Ok(TrackerStats {
        seeders: Some(seeders as i64),
        leechers: Some(leechers as i64),
        completed: None,
    })
}

fn random_u32(seed: &[u8]) -> u32 {
    let mut hash: u64 = 1469598103934665603;
    for byte in seed {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash ^= monotonic_secs().to_bits();
    (hash >> 32) as u32
}

fn parse_udp_target(url: &str) -> Result<(String, u16), String> {
    let rest = url
        .strip_prefix("udp://")
        .or_else(|| url.strip_prefix("UDP://"))
        .ok_or_else(|| "UDP tracker 地址无效".to_string())?;
    let authority = rest.split('/').next().unwrap_or("");
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, port.parse::<u16>().unwrap_or(6969)),
        None => (authority, 6969),
    };
    if host.is_empty() {
        return Err("UDP tracker 地址无效".to_string());
    }
    Ok((host.to_string(), port))
}

fn tracker_scheme(url: &str) -> &str {
    url.split_once("://")
        .map(|(scheme, _)| scheme)
        .unwrap_or("")
}

pub fn query_swarm(
    downloader: &Arc<Downloader>,
    uri: &str,
    timeout: Duration,
    max_trackers: usize,
    cache_ttl: f64,
    now: Option<f64>,
) -> Result<SwarmStats, String> {
    query_swarm_with_defaults(
        downloader,
        uri,
        timeout,
        max_trackers,
        cache_ttl,
        now,
        &DEFAULT_TRACKERS,
    )
}

fn compose_trackers(magnet_trackers: &[String], defaults: &[&str], max_trackers: usize) -> Vec<String> {
    let mut trackers: Vec<String> = Vec::new();
    for tracker in magnet_trackers {
        let scheme = tracker_scheme(tracker).to_lowercase();
        if matches!(scheme.as_str(), "http" | "https" | "udp") && !trackers.contains(tracker) {
            trackers.push(tracker.clone());
        }
    }
    for fallback in defaults {
        if !trackers.iter().any(|tracker| tracker == fallback) {
            trackers.push((*fallback).to_string());
        }
    }
    trackers.truncate(max_trackers);
    trackers
}

pub fn query_swarm_with_defaults(
    downloader: &Arc<Downloader>,
    uri: &str,
    timeout: Duration,
    max_trackers: usize,
    cache_ttl: f64,
    now: Option<f64>,
    defaults: &[&str],
) -> Result<SwarmStats, String> {
    let parsed = parse_magnet(uri)?;
    let current = now.unwrap_or_else(monotonic_secs);

    if let Some((stamp, stats)) = cache().lock().unwrap().get(&parsed.info_hash) {
        if current - stamp < cache_ttl {
            return Ok(stats.clone());
        }
    }

    let trackers = compose_trackers(&parsed.trackers, defaults, max_trackers);

    if trackers.is_empty() {
        return Ok(SwarmStats {
            errors: vec!["磁力链接中没有可查询的 tracker".to_string()],
            ..Default::default()
        });
    }

    let total = trackers.len();
    let mut results: Vec<Result<TrackerStats, String>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = trackers
            .iter()
            .map(|tracker| {
                let tracker = tracker.clone();
                let downloader = Arc::clone(downloader);
                let info_hash = parsed.info_hash;
                scope.spawn(move || {
                    if tracker_scheme(&tracker).eq_ignore_ascii_case("udp") {
                        announce_udp(&tracker, &info_hash, timeout)
                    } else {
                        announce_http(&downloader, &tracker, &info_hash, timeout)
                    }
                })
            })
            .collect();
        for (tracker, handle) in trackers.iter().zip(handles) {
            match handle.join() {
                Ok(Ok(stats)) => results.push(Ok(stats)),
                Ok(Err(error)) => results.push(Err(format!("{tracker}: {error}"))),
                Err(_) => results.push(Err(format!("{tracker}: 查询线程异常"))),
            }
        }
    });

    let mut seeders = Vec::new();
    let mut leechers = Vec::new();
    let mut completed = Vec::new();
    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok(stats) => {
                if let Some(value) = stats.seeders {
                    seeders.push(value);
                }
                if let Some(value) = stats.leechers {
                    leechers.push(value);
                }
                if let Some(value) = stats.completed {
                    completed.push(value);
                }
            }
            Err(error) => errors.push(error),
        }
    }

    let stats = SwarmStats {
        seeders: seeders.into_iter().max(),
        leechers: leechers.into_iter().max(),
        completed: completed.into_iter().max(),
        trackers_ok: total - errors.len(),
        trackers_total: total,
        errors,
    };
    if stats.trackers_ok > 0 {
        cache()
            .lock()
            .unwrap()
            .insert(parsed.info_hash, (current, stats.clone()));
    }
    Ok(stats)
}

fn thousands(value: i64) -> String {
    let negative = value < 0;
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if negative {
        format!("-{out}")
    } else {
        out
    }
}

pub fn format_stats(stats: &SwarmStats) -> String {
    let mut parts = Vec::new();
    if let Some(seeders) = stats.seeders {
        parts.push(format!("做种 {}", thousands(seeders)));
    }
    if let Some(leechers) = stats.leechers {
        parts.push(format!("下载 {}", thousands(leechers)));
    }
    if let Some(completed) = stats.completed {
        parts.push(format!("完成 {}", thousands(completed)));
    }
    if parts.is_empty() {
        "暂无数据".to_string()
    } else {
        parts.join(" · ")
    }
}

pub fn describe_stats(stats: &SwarmStats) -> String {
    let mut summary = format!(
        "{}（{}/{} 个 tracker 返回数据）",
        format_stats(stats),
        stats.trackers_ok,
        stats.trackers_total
    );
    if !stats.errors.is_empty() {
        summary.push('\n');
        summary.push_str(&stats.errors.iter().take(3).cloned().collect::<Vec<_>>().join("\n"));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{HttpClient, HttpResponse};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const INFO_HASH: [u8; 20] = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd,
        0xef, 0x01, 0x23, 0x45, 0x67,
    ];
    const INFO_HASH_HEX: &str = "0123456789abcdef0123456789abcdef01234567";

    fn magnet(trackers: &[&str]) -> String {
        let mut uri = format!("magnet:?xt=urn:btih:{INFO_HASH_HEX}");
        for tracker in trackers {
            uri.push_str(&format!("&tr={}", percent_encode(tracker.as_bytes())));
        }
        uri
    }

    fn bencoded_counts(seeders: i64, leechers: i64, completed: i64) -> String {
        format!(
            "d8:completei{seeders}e10:downloadedi{completed}e10:incompletei{leechers}e5:peers0:e"
        )
    }

    struct CountClient {
        seeders: i64,
        leechers: i64,
        completed: i64,
        calls: AtomicUsize,
        last_url: Mutex<String>,
    }

    impl CountClient {
        fn new(seeders: i64, leechers: i64, completed: i64) -> Arc<Self> {
            Arc::new(Self {
                seeders,
                leechers,
                completed,
                calls: AtomicUsize::new(0),
                last_url: Mutex::new(String::new()),
            })
        }
    }

    impl HttpClient for CountClient {
        fn get(
            &self,
            url: &str,
            _headers: &[(&str, &str)],
            _timeout: Duration,
            _profile: Option<&str>,
        ) -> Result<HttpResponse, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last_url.lock().unwrap() = url.to_string();
            Ok(HttpResponse {
                status: 200,
                reason: "OK".to_string(),
                headers: HashMap::new(),
                body: bencoded_counts(self.seeders, self.leechers, self.completed).into_bytes(),
            })
        }
    }

    #[test]
    fn parses_hex_and_base32_hashes() {
        let parsed = parse_magnet(&magnet(&[
            "http://tracker.example/announce",
            "udp://tracker.example:6969/announce",
        ]))
        .unwrap();
        assert_eq!(parsed.info_hash, INFO_HASH);
        assert_eq!(
            parsed.trackers,
            vec![
                "http://tracker.example/announce".to_string(),
                "udp://tracker.example:6969/announce".to_string()
            ]
        );

        let base32 = "AERUKZ4JVPG66AJDIVTYTK6N54ASGRLH";
        let parsed = parse_magnet(&format!("magnet:?xt=urn:btih:{base32}")).unwrap();
        assert_eq!(parsed.info_hash, INFO_HASH);

        assert!(parse_magnet("https://example.com/game").is_err());
        assert!(parse_magnet("magnet:?dn=no-hash").is_err());
    }

    #[test]
    fn decodes_nested_bencode() {
        assert_eq!(
            bdecode(b"d8:completei5e10:incompletei2ee").unwrap(),
            BValue::Dict(HashMap::from([
                (b"complete".to_vec(), BValue::Int(5)),
                (b"incomplete".to_vec(), BValue::Int(2)),
            ]))
        );
        assert_eq!(
            bdecode(b"l4:spam3:eggi3ee").unwrap(),
            BValue::List(vec![
                BValue::Bytes(b"spam".to_vec()),
                BValue::Bytes(b"egg".to_vec()),
                BValue::Int(3),
            ])
        );
    }

    #[test]
    fn http_announce_reads_counts_and_sends_binary_hash() {
        let client = CountClient::new(12, 3, 45);
        let downloader = Downloader::new(client.clone());
        let stats = announce_http(
            &downloader,
            "http://tracker.example/announce",
            &INFO_HASH,
            Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!(
            (stats.seeders, stats.leechers, stats.completed),
            (Some(12), Some(3), Some(45))
        );
        let url = client.last_url.lock().unwrap().clone();
        assert!(url.contains("info_hash=%01%23Eg"));
        assert!(url.contains("numwant=0"));
    }

    #[test]
    fn http_failure_reason_becomes_error() {
        struct FailureClient;
        impl HttpClient for FailureClient {
            fn get(
                &self,
                _url: &str,
                _headers: &[(&str, &str)],
                _timeout: Duration,
                _profile: Option<&str>,
            ) -> Result<HttpResponse, String> {
                Ok(HttpResponse {
                    status: 200,
                    reason: "OK".to_string(),
                    headers: HashMap::new(),
                    body: b"d14:failure reason20:unregistered torrente".to_vec(),
                })
            }
        }
        let downloader = Downloader::new(Arc::new(FailureClient));
        let error = announce_http(
            &downloader,
            "http://tracker.example/announce",
            &INFO_HASH,
            Duration::from_secs(3),
        )
        .unwrap_err();
        assert!(error.contains("unregistered torrent"));
    }

    #[test]
    fn udp_announce_reads_counts() {
        let tracker = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = tracker.local_addr().unwrap().port();
        let hash = INFO_HASH;
        let handle = std::thread::spawn(move || {
            tracker.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut buffer = [0u8; 2048];
            let (size, addr) = tracker.recv_from(&mut buffer).unwrap();
            assert_eq!(size, 16);
            let txid = u32::from_be_bytes(buffer[12..16].try_into().unwrap());
            let mut reply = Vec::new();
            reply.extend_from_slice(&0u32.to_be_bytes());
            reply.extend_from_slice(&txid.to_be_bytes());
            reply.extend_from_slice(&0x1122334455667788u64.to_be_bytes());
            tracker.send_to(&reply, addr).unwrap();

            let (size, addr) = tracker.recv_from(&mut buffer).unwrap();
            assert_eq!(size, 98);
            assert_eq!(&buffer[16..36], &hash);
            let txid = u32::from_be_bytes(buffer[12..16].try_into().unwrap());
            let mut reply = Vec::new();
            reply.extend_from_slice(&1u32.to_be_bytes());
            reply.extend_from_slice(&txid.to_be_bytes());
            reply.extend_from_slice(&1800u32.to_be_bytes());
            reply.extend_from_slice(&4u32.to_be_bytes());
            reply.extend_from_slice(&9u32.to_be_bytes());
            tracker.send_to(&reply, addr).unwrap();
        });

        let stats = announce_udp(
            &format!("udp://127.0.0.1:{port}/announce"),
            &INFO_HASH,
            Duration::from_secs(3),
        )
        .unwrap();
        assert_eq!((stats.seeders, stats.leechers), (Some(9), Some(4)));
        assert!(stats.completed.is_none());
        handle.join().unwrap();
    }

    #[test]
    fn query_takes_max_counts_and_caches() {
        clear_cache();
        let first = CountClient::new(10, 2, 20);
        let downloader = Arc::new(Downloader::new(first.clone()));
        let stats = query_swarm_with_defaults(
            &downloader,
            &magnet(&[
                "http://127.0.0.1:1/announce",
                "http://127.0.0.1:2/announce",
            ]),
            Duration::from_secs(3),
            4,
            CACHE_TTL,
            None,
            &[],
        )
        .unwrap();
        assert_eq!(
            (stats.seeders, stats.leechers, stats.completed),
            (Some(10), Some(2), Some(20))
        );
        assert_eq!((stats.trackers_ok, stats.trackers_total), (2, 2));

        let cached = query_swarm_with_defaults(
            &downloader,
            &magnet(&[
                "http://127.0.0.1:1/announce",
                "http://127.0.0.1:2/announce",
            ]),
            Duration::from_secs(3),
            4,
            CACHE_TTL,
            None,
            &[],
        )
        .unwrap();
        assert_eq!(first.calls.load(Ordering::SeqCst), 2);
        assert_eq!(cached.seeders, Some(10));
    }

    #[test]
    fn query_reports_missing_trackers() {
        clear_cache();
        let downloader = Arc::new(Downloader::new(CountClient::new(1, 1, 1)));
        let stats = query_swarm_with_defaults(
            &downloader,
            &magnet(&[]),
            Duration::from_secs(1),
            4,
            CACHE_TTL,
            None,
            &[],
        )
        .unwrap();
        assert!(stats.seeders.is_none());
        assert_eq!(stats.trackers_total, 0);
        assert_eq!(stats.errors.len(), 1);
    }

    #[test]
    #[ignore]
    fn live_probe() {
        use crate::net_reqwest::ReqwestClient;

        let downloader = Arc::new(Downloader::new(Arc::new(ReqwestClient::new().unwrap())));
        let public = "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=Big+Buck+Bunny&tr=udp%3A%2F%2Ftracker.opentrackr.org%3A1337%2Fannounce&tr=udp%3A%2F%2Fopen.stealth.si%3A80%2Fannounce&tr=udp%3A%2F%2Ftracker.torrent.eu.org%3A451%2Fannounce&tr=udp%3A%2F%2Fexodus.desync.com%3A6969%2Fannounce&tr=http%3A%2F%2Ftracker.openbittorrent.com%3A80%2Fannounce".to_string();

        let mut magnets = vec![("public".to_string(), public)];
        let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sources")
            .join("dodi.json.gz");
        if let Ok(data) = crate::storage::read_source_file(&repo) {
            if let Some((items, _)) = crate::catalog::extract_repacks(&data) {
                for raw in items {
                    let Some(uris) = raw.get("uris").and_then(|value| value.as_array()) else {
                        continue;
                    };
                    if let Some(uri) = uris
                        .iter()
                        .filter_map(|value| value.as_str())
                        .find(|uri| uri.starts_with("magnet"))
                    {
                        magnets.push(("dodi".to_string(), uri.to_string()));
                        break;
                    }
                }
            }
        }

        for (label, uri) in magnets {
            eprintln!("=== {label}");
            match parse_magnet(&uri) {
                Ok(parsed) => eprintln!("trackers({})={:?}", parsed.trackers.len(), parsed.trackers),
                Err(error) => {
                    eprintln!("parse error: {error}");
                    continue;
                }
            }
            let started = Instant::now();
            match query_swarm(
                &downloader,
                &uri,
                Duration::from_secs(4),
                12,
                0.0,
                None,
            ) {
                Ok(stats) => eprintln!(
                    "elapsed={:?} seeders={:?} leechers={:?} completed={:?} ok={}/{} errors={:?}",
                    started.elapsed(),
                    stats.seeders,
                    stats.leechers,
                    stats.completed,
                    stats.trackers_ok,
                    stats.trackers_total,
                    stats.errors
                ),
                Err(error) => eprintln!("query error: {error}"),
            }
        }
    }

    #[test]
    fn composes_magnet_and_default_trackers() {
        let magnet = vec![
            "udp://tracker.example:6969/announce".to_string(),
            "http://tracker.example/announce".to_string(),
        ];
        let defaults = [
            "udp://tracker.opentrackr.org:1337/announce",
            "udp://tracker.opentrackr.org:1337/announce",
        ];
        assert_eq!(
            compose_trackers(&magnet, &defaults, 10),
            vec![
                "udp://tracker.example:6969/announce".to_string(),
                "http://tracker.example/announce".to_string(),
                "udp://tracker.opentrackr.org:1337/announce".to_string()
            ]
        );
        assert_eq!(
            compose_trackers(&magnet, &defaults, 2),
            vec![
                "udp://tracker.example:6969/announce".to_string(),
                "http://tracker.example/announce".to_string()
            ]
        );
        assert_eq!(
            compose_trackers(&[], &defaults, 10),
            vec!["udp://tracker.opentrackr.org:1337/announce".to_string()]
        );
    }

    #[test]
    fn formats_counts_with_separators() {
        let stats = SwarmStats {
            seeders: Some(12345),
            leechers: Some(4),
            completed: Some(231),
            trackers_ok: 2,
            trackers_total: 3,
            errors: vec!["udp://tracker.example: timeout".to_string()],
        };
        assert_eq!(format_stats(&stats), "做种 12,345 · 下载 4 · 完成 231");
        let summary = describe_stats(&stats);
        assert!(summary.contains("2/3"));
        assert!(summary.contains("udp://tracker.example"));
    }
}
