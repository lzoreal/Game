//! Display model for the details pane.

use std::collections::HashSet;

use serde::Serialize;

use crate::dates::format_date;
use crate::model::{Group, Item};
use crate::text::clean_html;

pub const LINK_LIMIT: usize = 6;
pub const RAW_LIMIT: usize = 100_000;

#[derive(Clone, Debug, Serialize)]
pub struct DetailLink {
    pub uri: String,
    #[serde(rename = "isMagnet")]
    pub is_magnet: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DetailLinkGroup {
    pub label: String,
    pub total: usize,
    pub visible: Vec<DetailLink>,
    pub hidden: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DetailCard {
    pub key: String,
    pub source: String,
    pub date: String,
    pub size: String,
    pub description: String,
    pub links: Vec<DetailLinkGroup>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DetailModel {
    pub title: String,
    pub sources: Vec<String>,
    pub meta: String,
    pub cards: Vec<DetailCard>,
    pub hint: String,
    #[serde(rename = "rawVisible")]
    pub raw_visible: bool,
    #[serde(rename = "rawText")]
    pub raw_text: String,
    #[serde(rename = "rawLabel")]
    pub raw_label: String,
}

pub fn card_key(item: &Item) -> String {
    let source = if item.source_name.is_empty() {
        item.source_display.as_ref()
    } else {
        item.source_name.as_ref()
    };
    format!("{}|{}|{}", source, item.key_id, item.upload_date)
}

pub fn build_detail(
    group: &Group,
    expanded: &HashSet<String>,
    raw_visible: bool,
    link_limit: usize,
    raw_limit: usize,
) -> DetailModel {
    let mut cards = Vec::with_capacity(group.items.len());
    let mut used: HashSet<String> = HashSet::new();
    for item in &group.items {
        let base = card_key(item);
        let mut key = base.clone();
        if used.contains(&key) {
            let mut suffix = 2;
            while used.contains(&format!("{base}#{suffix}")) {
                suffix += 1;
            }
            key = format!("{base}#{suffix}");
        }
        used.insert(key.clone());
        cards.push(build_card(item, &key, expanded.contains(&key), link_limit));
    }

    let raw_text = String::new();
    let _ = raw_limit;

    DetailModel {
        title: group.title.clone(),
        sources: group.sources.clone(),
        meta: format!(
            "{}   ·   {}   ·   {} 条链接",
            group.date_display, group.size, group.link_count
        ),
        cards,
        hint: format!("{} 个来源", group.sources.len()),
        raw_visible,
        raw_text,
        raw_label: if raw_visible {
            "收起原始 JSON".to_string()
        } else {
            "查看原始 JSON".to_string()
        },
    }
}

fn build_card(item: &Item, key: &str, expanded: bool, link_limit: usize) -> DetailCard {
    let description = clean_html(&item.description_html);
    let date_text = match item.date_dt {
        Some(dt) => format_date(&dt),
        None => item.upload_date.to_string(),
    };

    let magnets: Vec<DetailLink> = item
        .uris
        .iter()
        .filter(|uri| uri.starts_with("magnet"))
        .map(|uri| DetailLink {
            uri: uri.to_string(),
            is_magnet: true,
        })
        .collect();
    let https: Vec<DetailLink> = item
        .uris
        .iter()
        .filter(|uri| uri.starts_with("http"))
        .map(|uri| DetailLink {
            uri: uri.to_string(),
            is_magnet: false,
        })
        .collect();
    let others: Vec<DetailLink> = item
        .uris
        .iter()
        .filter(|uri| !uri.starts_with("magnet") && !uri.starts_with("http"))
        .map(|uri| DetailLink {
            uri: uri.to_string(),
            is_magnet: false,
        })
        .collect();

    let mut links = Vec::new();
    for (label, entries) in [
        ("磁力链接", magnets),
        ("HTTP 链接", https),
        ("其他链接", others),
    ] {
        if entries.is_empty() {
            continue;
        }
        let total = entries.len();
        let visible: Vec<DetailLink> = if expanded || total <= link_limit {
            entries
        } else {
            entries.into_iter().take(link_limit).collect()
        };
        let hidden = total - visible.len();
        links.push(DetailLinkGroup {
            label: label.to_string(),
            total,
            visible,
            hidden,
        });
    }

    DetailCard {
        key: key.to_string(),
        source: item.source_display.to_string(),
        date: date_text,
        size: item.file_size.to_string(),
        description,
        links,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::finalize_group;
    use crate::dates::parse_upload_date;
    use crate::model::Item;
    use crate::text::normalize_text;

    fn item(uris: Vec<&str>, date: &str, description: &str) -> Item {
        let title = "Demo Game";
        Item {
            title: title.to_string().into_boxed_str(),
            nt: normalize_text(title).into_boxed_str(),
            key_id: 1,
            uris: uris
                .into_iter()
                .map(|uri| uri.to_string().into_boxed_str())
                .collect(),
            file_size: "10 GB".into(),
            upload_date: date.into(),
            date_dt: parse_upload_date(Some(date)),
            source_name: std::sync::Arc::from("src.json.gz"),
            source_display: std::sync::Arc::from("src"),
            source_index: 0,
            description_html: description.to_string().into_boxed_str(),
        }
    }

    #[test]
    fn builds_header_hint_and_meta() {
        let group = finalize_group(vec![item(vec!["magnet:a"], "2024-01-02", "Hi")], 0.0);
        let model = build_detail(&group, &HashSet::new(), false, LINK_LIMIT, RAW_LIMIT);
        assert_eq!(model.title, "Demo Game");
        assert_eq!(model.sources, vec!["src"]);
        assert_eq!(model.hint, "1 个来源");
        assert!(model.meta.contains("2024-01-02"));
        assert!(model.meta.contains("1 条链接"));
    }

    #[test]
    fn classifies_and_truncates_links() {
        let uris = vec![
            "magnet:1", "magnet:2", "magnet:3", "magnet:4", "magnet:5", "magnet:6", "magnet:7",
            "http://a", "other://b",
        ];
        let group = finalize_group(vec![item(uris, "2024-01-02", "")], 0.0);
        let model = build_detail(&group, &HashSet::new(), false, 6, RAW_LIMIT);
        let card = &model.cards[0];
        assert_eq!(card.links.len(), 3);
        assert_eq!(card.links[0].total, 7);
        assert_eq!(card.links[0].visible.len(), 6);
        assert_eq!(card.links[0].hidden, 1);
        assert_eq!(card.links[1].label, "HTTP 链接");
        assert_eq!(card.links[2].label, "其他链接");
    }

    #[test]
    fn expanded_key_reveals_everything() {
        let uris = vec!["magnet:1", "magnet:2", "magnet:3", "magnet:4", "magnet:5", "magnet:6", "magnet:7"];
        let group = finalize_group(vec![item(uris, "2024-01-02", "")], 0.0);
        let collapsed = build_detail(&group, &HashSet::new(), false, 6, RAW_LIMIT);
        let key = collapsed.cards[0].key.clone();
        let mut expanded = HashSet::new();
        expanded.insert(key);
        let model = build_detail(&group, &expanded, false, 6, RAW_LIMIT);
        assert_eq!(model.cards[0].links[0].visible.len(), 7);
        assert_eq!(model.cards[0].links[0].hidden, 0);
    }

    #[test]
    fn keys_are_stable_and_unique() {
        let group = finalize_group(
            vec![
                item(vec!["magnet:a"], "2024-01-02", ""),
                item(vec!["magnet:b"], "2024-01-02", ""),
            ],
            0.0,
        );
        let model = build_detail(&group, &HashSet::new(), false, 6, RAW_LIMIT);
        assert_ne!(model.cards[0].key, model.cards[1].key);
        assert!(model.cards[1].key.contains("#2"));
    }

    #[test]
    fn raw_label_tracks_visibility() {
        let group = finalize_group(vec![item(vec!["magnet:a"], "2024-01-02", "Hi")], 0.0);
        let model = build_detail(&group, &HashSet::new(), true, LINK_LIMIT, RAW_LIMIT);
        assert!(model.raw_visible);
        assert!(model.raw_text.is_empty());
        assert_eq!(model.raw_label, "收起原始 JSON");
    }
}
