//! Upload-date parsing matching the Python implementation's accepted formats.

use std::sync::OnceLock;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone};
use regex::Regex;

const DATE_FORMATS: &[&str] = &[
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d %H:%M",
    "%Y-%m-%d",
    "%Y/%m/%d %H:%M:%S",
    "%Y/%m/%d %H:%M",
    "%Y/%m/%d",
    "%Y.%m.%d %H:%M:%S",
    "%Y.%m.%d %H:%M",
    "%Y.%m.%d",
    "%d-%m-%Y %H:%M:%S",
    "%d-%m-%Y %H:%M",
    "%d-%m-%Y",
    "%d/%m/%Y %H:%M:%S",
    "%d/%m/%Y %H:%M",
    "%d/%m/%Y",
    "%B %d, %Y",
    "%b %d, %Y",
    "%d %B %Y",
    "%d %b %Y",
    "%B %d %Y",
    "%b %d %Y",
];

fn ordinal_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(\d+)(?:st|nd|rd|th)\b").unwrap())
}

fn loose_date_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(20\d{2})[-/.](\d{1,2})[-/.](\d{1,2})").unwrap())
}

fn from_unix(seconds: i64) -> Option<NaiveDateTime> {
    Local.timestamp_opt(seconds, 0).single().map(|dt| dt.naive_local())
}

pub fn parse_upload_date(value: Option<&str>) -> Option<NaiveDateTime> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }
    let s = ordinal_re().replace_all(raw, "$1");
    let s = s.trim();
    match s.to_uppercase().as_str() {
        "N/A" | "NA" | "NONE" | "NULL" | "-" => return None,
        _ => {}
    }

    let all_digits = s.len() >= 10 && s.len() <= 13 && s.chars().all(|c| c.is_ascii_digit());
    if all_digits {
        if let Ok(mut ts) = s.parse::<i64>() {
            if s.len() == 13 {
                ts /= 1000;
            }
            if let Some(dt) = from_unix(ts) {
                return Some(dt);
            }
        }
    }

    let iso = s.replace('Z', "+00:00");
    if let Ok(dt) = DateTime::parse_from_rfc3339(&iso) {
        return Some(dt.naive_local());
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
        if let Ok(date) = NaiveDate::parse_from_str(s, fmt) {
            return date.and_hms_opt(0, 0, 0);
        }
    }

    for fmt in DATE_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
        if let Ok(date) = NaiveDate::parse_from_str(s, fmt) {
            return date.and_hms_opt(0, 0, 0);
        }
    }

    if let Some(caps) = loose_date_re().captures(s) {
        let year = caps.get(1).and_then(|m| m.as_str().parse().ok());
        let month = caps.get(2).and_then(|m| m.as_str().parse().ok());
        let day = caps.get(3).and_then(|m| m.as_str().parse().ok());
        if let (Some(y), Some(m), Some(d)) = (year, month, day) {
            if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
                return date.and_hms_opt(0, 0, 0);
            }
        }
    }

    None
}

pub fn format_date(dt: &NaiveDateTime) -> String {
    dt.format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_formats() {
        assert_eq!(
            parse_upload_date(Some("2024-05-06")).unwrap().date().to_string(),
            "2024-05-06"
        );
        assert_eq!(
            parse_upload_date(Some("May 6, 2024")).unwrap().date().to_string(),
            "2024-05-06"
        );
        assert_eq!(
            parse_upload_date(Some("6th May 2024")).unwrap().date().to_string(),
            "2024-05-06"
        );
        assert_eq!(
            parse_upload_date(Some("2024.05.06 12:30")).unwrap().date().to_string(),
            "2024-05-06"
        );
    }

    #[test]
    fn parses_iso_and_timestamps() {
        assert_eq!(
            parse_upload_date(Some("2024-05-06T12:00:00Z"))
                .unwrap()
                .date()
                .to_string(),
            "2024-05-06"
        );
        assert!(parse_upload_date(Some("1715000000")).is_some());
        assert!(parse_upload_date(Some("1715000000000")).is_some());
    }

    #[test]
    fn rejects_placeholders() {
        assert!(parse_upload_date(None).is_none());
        assert!(parse_upload_date(Some("N/A")).is_none());
        assert!(parse_upload_date(Some("-")).is_none());
        assert!(parse_upload_date(Some("soon")).is_none());
    }
}
