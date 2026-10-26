//! Search state: tokens, timing and date sorting.

use std::time::Instant;

use crate::model::Group;

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
}
