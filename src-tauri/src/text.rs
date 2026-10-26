//! Title normalization, dedup keys and HTML cleanup.

use std::sync::OnceLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

fn version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bv\d+(?:\.\d+)+\w*\b").unwrap())
}

fn br_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)<br\s*/?>").unwrap())
}

fn p_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)</p\s*>").unwrap())
}

fn tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<[^>]+>").unwrap())
}

fn space_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[ \t]+").unwrap())
}

fn newline_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\n{3,}").unwrap())
}

fn collapse_alnum(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        } else {
            pending_space = true;
        }
    }
    out
}

fn lower_nfkc(text: &str) -> String {
    text.nfkc().flat_map(char::to_lowercase).collect()
}

pub fn normalize_text(value: &str) -> String {
    collapse_alnum(&lower_nfkc(value))
}

pub fn dedup_key(title: &str) -> String {
    let lowered = lower_nfkc(title);
    let stripped = version_re().replace_all(&lowered, " ");
    let key = collapse_alnum(&stripped);
    if key.is_empty() {
        normalize_text(title)
    } else {
        key
    }
}

pub fn normalize_title(value: &str) -> (String, String) {
    let lowered = lower_nfkc(value);
    let nt = collapse_alnum(&lowered);
    let stripped = version_re().replace_all(&lowered, " ");
    let mut key = collapse_alnum(&stripped);
    if key.is_empty() {
        key = nt.clone();
    }
    (nt, key)
}

pub fn display_name(filename: &str) -> String {
    let lower = filename.to_lowercase();
    let stem = if lower.ends_with(".json.gz") {
        &filename[..filename.len() - 8]
    } else if lower.ends_with(".json") {
        &filename[..filename.len() - 5]
    } else {
        filename
    };
    let mut stem = stem.to_lowercase();
    for suffix in [
        "_cleaned", "-cleaned", "_source", "-source", "source", "-games", "_games",
    ] {
        if stem.ends_with(suffix) {
            let cut = stem.len() - suffix.len();
            stem.truncate(cut);
            break;
        }
    }
    if stem.is_empty() {
        filename.to_string()
    } else {
        stem
    }
}

pub fn clean_html(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let t = br_re().replace_all(text, "\n");
    let t = p_re().replace_all(&t, "\n\n");
    let t = tag_re().replace_all(&t, " ");
    let t = html_escape::decode_html_entities(&t);
    let t = space_re().replace_all(&t, " ");
    let t = newline_re().replace_all(&t, "\n\n");
    t.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_case_punctuation_and_versions() {
        assert_eq!(normalize_text("  Cyberpunk 2077: Deluxe!  "), "cyberpunk 2077 deluxe");
        assert_eq!(dedup_key("Game v1.2.3 Deluxe!"), "game deluxe");
        assert_eq!(dedup_key("Game v1.2 Deluxe"), dedup_key("game  deluxe"));
    }

    #[test]
    fn shared_normalization_matches_individual_helpers() {
        for title in ["Game v1.2.3 Deluxe!", "  Cyberpunk 2077  ", "v1.2.3", "特别版 v2.0"] {
            let (nt, key) = normalize_title(title);
            assert_eq!(nt, normalize_text(title));
            assert_eq!(key, dedup_key(title));
        }
    }

    #[test]
    fn falls_back_to_normalized_title_for_empty_keys() {
        assert_eq!(dedup_key("v1.2.3"), "v1 2 3");
    }

    #[test]
    fn strips_known_filename_suffixes() {
        assert_eq!(display_name("fitgirl_cleaned.json.gz"), "fitgirl");
        assert_eq!(display_name("dodi.json.gz"), "dodi");
        assert_eq!(display_name("XATAB-source.json"), "xatab");
    }

    #[test]
    fn cleans_description_html() {
        assert_eq!(
            clean_html("<p>Hello<br>world</p><b>!</b> &amp; more"),
            "Hello\nworld\n\n ! & more"
        );
    }
}
