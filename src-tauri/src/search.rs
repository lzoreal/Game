//! Search state: tokens, timing and date sorting.

use std::time::Instant;

use crate::model::Group;
use crate::text::normalize_text;

/// The whole search grammar: free words, `"quoted phrases"` and `-excluded`
/// words. Anything else is treated as literal text.
///
/// The normalized title is the haystack, so every term is normalized the same
/// way (`Elden Ring!` and `elden ring` must match). Terms that normalize to
/// nothing (e.g. `-!!!`) are dropped rather than matching everything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    /// Free words, joined by single spaces. Drives the fuzzy scoring.
    pub text: String,
    /// Phrases that must appear in the title, in any position.
    pub phrases: Vec<String>,
    /// Fragments that must not appear in the title.
    pub excluded: Vec<String>,
}

const QUOTE_CHARS: [char; 5] = ['"', '“', '”', '「', '」'];

impl ParsedQuery {
    /// The substring the scorer anchors on: the free text, or the first phrase
    /// when the query is only phrases.
    pub fn needle(&self) -> Option<&str> {
        if !self.text.is_empty() {
            Some(self.text.as_str())
        } else {
            self.phrases.first().map(String::as_str)
        }
    }

    /// True when there is something to search *for*. A query made only of
    /// exclusions has no positive term; the caller shows the latest list and
    /// lets `matches` filter it.
    pub fn has_positive(&self) -> bool {
        self.needle().is_some()
    }

    /// Hard filter applied to every candidate title: all phrases present, no
    /// excluded fragment present.
    pub fn matches(&self, key: &str) -> bool {
        self.phrases.iter().all(|phrase| key.contains(phrase))
            && !self.excluded.iter().any(|term| key.contains(term))
    }
}

pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut tokens: Vec<(String, bool, bool)> = Vec::new();
    let mut buf = String::new();
    let mut in_quote = false;
    let mut quoted = false;
    let mut excluded = false;

    for ch in raw.chars() {
        if QUOTE_CHARS.contains(&ch) {
            if in_quote {
                tokens.push((std::mem::take(&mut buf), excluded, true));
                in_quote = false;
                quoted = false;
                excluded = false;
            } else {
                if !buf.is_empty() {
                    tokens.push((std::mem::take(&mut buf), excluded, quoted));
                    excluded = false;
                }
                in_quote = true;
                quoted = true;
            }
        } else if ch.is_whitespace() && !in_quote {
            if !buf.is_empty() {
                tokens.push((std::mem::take(&mut buf), excluded, quoted));
            }
            excluded = false;
            quoted = false;
        } else if ch == '-' && !in_quote && buf.is_empty() {
            excluded = true;
        } else {
            buf.push(ch);
        }
    }
    // An unterminated quote is still a phrase; the user is mid-typing.
    if !buf.is_empty() {
        tokens.push((buf, excluded, quoted));
    }

    let mut parsed = ParsedQuery::default();
    for (token, is_excluded, is_quoted) in tokens {
        let value = normalize_text(&token);
        if value.is_empty() {
            continue;
        }
        if is_excluded {
            if !parsed.excluded.contains(&value) {
                parsed.excluded.push(value);
            }
        } else if is_quoted {
            if !parsed.phrases.contains(&value) {
                parsed.phrases.push(value);
            }
        } else {
            if !parsed.text.is_empty() {
                parsed.text.push(' ');
            }
            parsed.text.push_str(&value);
        }
    }
    parsed
}

#[derive(Clone, Debug)]
pub struct SearchTicket {
    pub token: u64,
    pub query: String,
    pub source_filter: String,
    pub fuzzy: bool,
    pub threshold: f64,
    pub started: Instant,
}

pub fn sort_by_date(mut groups: Vec<Group>, desc: bool) -> Vec<Group> {
    let (mut dated, undated): (Vec<Group>, Vec<Group>) =
        groups.drain(..).partition(|group| group.date_dt.is_some());
    dated.sort_by(|a, b| {
        if desc {
            b.date_dt.cmp(&a.date_dt)
        } else {
            a.date_dt.cmp(&b.date_dt)
        }
    });
    dated.extend(undated);
    dated
}

pub struct SearchController {
    pub debounce_ms: u64,
    pub token: u64,
    pub query: String,
    pub sort_desc: bool,
}

impl Default for SearchController {
    fn default() -> Self {
        Self {
            debounce_ms: 250,
            token: 0,
            query: String::new(),
            sort_desc: true,
        }
    }
}

impl SearchController {
    pub fn observe(&mut self, text: &str) -> bool {
        self.query = text.trim().to_string();
        self.token += 1;
        !self.query.is_empty()
    }

    pub fn begin(
        &mut self,
        query: &str,
        source_filter: &str,
        fuzzy: bool,
        threshold: f64,
    ) -> Option<SearchTicket> {
        self.query = query.trim().to_string();
        self.token += 1;
        if self.query.is_empty() {
            return None;
        }
        Some(SearchTicket {
            token: self.token,
            query: self.query.clone(),
            source_filter: source_filter.to_string(),
            fuzzy,
            threshold,
            started: Instant::now(),
        })
    }

    pub fn accepts(&self, token: u64) -> bool {
        token == self.token
    }

    pub fn invalidate(&mut self) {
        self.token += 1;
    }

    pub fn elapsed_ms(&self, ticket: &SearchTicket) -> f64 {
        ticket.started.elapsed().as_secs_f64() * 1000.0
    }

    pub fn toggle_date_sort(&mut self) -> bool {
        self.sort_desc = !self.sort_desc;
        self.sort_desc
    }

    pub fn apply_sort(&self, groups: Vec<Group>) -> Vec<Group> {
        sort_by_date(groups, self.sort_desc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::parse_upload_date;
    use crate::model::Group;

    fn group(date: Option<&str>) -> Group {
        Group {
            title: date.unwrap_or("undated").to_string(),
            key: date.unwrap_or("undated").to_string(),
            sources: vec!["src".to_string()],
            size: "1 GB".to_string(),
            date_str: date.unwrap_or("N/A").to_string(),
            date_display: date.unwrap_or("N/A").to_string(),
            uris: vec![],
            link_count: 0,
            score: 0.0,
            date_dt: date.and_then(|text| parse_upload_date(Some(text))),
            items: vec![],
        }
    }

    #[test]
    fn observe_and_begin_issue_tokens() {
        let mut controller = SearchController::default();
        assert!(controller.observe("had"));
        let first = controller.begin("hades", "", true, 0.3).unwrap();
        let second = controller.begin("hades", "", true, 0.3).unwrap();
        assert!(controller.accepts(second.token));
        assert!(!controller.accepts(first.token));
    }

    #[test]
    fn empty_queries_are_not_searchable() {
        let mut controller = SearchController::default();
        assert!(!controller.observe("   "));
        assert!(controller.begin("  ", "", true, 0.3).is_none());
    }

    #[test]
    fn invalidate_cancels_in_flight_search() {
        let mut controller = SearchController::default();
        let ticket = controller.begin("hades", "", true, 0.3).unwrap();
        controller.invalidate();
        assert!(!controller.accepts(ticket.token));
    }

    #[test]
    fn date_sort_keeps_undated_last_in_both_directions() {
        let groups = vec![
            group(Some("2020-01-01")),
            group(None),
            group(Some("2024-01-01")),
        ];
        let desc = sort_by_date(groups.clone(), true);
        assert_eq!(desc[0].title, "2024-01-01");
        assert_eq!(desc[2].title, "undated");
        let asc = sort_by_date(groups, false);
        assert_eq!(asc[0].title, "2020-01-01");
        assert_eq!(asc[2].title, "undated");
    }

    #[test]
    fn parses_free_words_phrases_and_exclusions() {
        let parsed = parse_query(r#"  Hades   -合集 "GOG 版" -"mod"  "#);
        assert_eq!(parsed.text, "hades");
        assert_eq!(parsed.phrases, vec!["gog 版"]);
        assert_eq!(parsed.excluded, vec!["合集", "mod"]);
        assert!(parsed.has_positive());
    }

    #[test]
    fn phrases_keep_their_spaces_and_punctuation_is_dropped() {
        let parsed = parse_query(r#""Elden Ring: Shadow!"#);
        assert_eq!(parsed.needle(), Some("elden ring shadow"));
        assert!(parsed.text.is_empty());
        assert_eq!(parsed.phrases, vec!["elden ring shadow"]);
    }

    #[test]
    fn full_width_quotes_count_as_phrases() {
        let parsed = parse_query("「赛博朋克 2077」");
        assert_eq!(parsed.phrases, vec!["赛博朋克 2077"]);
    }

    #[test]
    fn only_exclusions_has_no_positive_term_but_still_filters() {
        let parsed = parse_query("-合集 -gog");
        assert!(!parsed.has_positive());
        assert_eq!(parsed.excluded, vec!["合集", "gog"]);
        assert!(parsed.matches("hades"));
        assert!(!parsed.matches("hades 合集"));
    }

    #[test]
    fn unterminated_quote_is_treated_as_a_phrase() {
        let parsed = parse_query(r#"hades "game of"#);
        assert_eq!(parsed.text, "hades");
        assert_eq!(parsed.phrases, vec!["game of"]);
    }

    #[test]
    fn punctuation_only_terms_are_dropped() {
        let parsed = parse_query("-!!!");
        assert!(!parsed.has_positive());
        assert!(parsed.excluded.is_empty());
        assert!(parsed.matches("anything"));
    }

    #[test]
    fn every_phrase_must_match() {
        let parsed = parse_query(r#""hades" "ii""#);
        assert!(parsed.matches("hades ii"));
        assert!(!parsed.matches("hades"));
    }
}
