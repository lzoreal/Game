//! Catalog parsing, deduplication, search and newest-first listing.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;
use serde_json::Value;

use crate::dates::{format_date, parse_upload_date};
use crate::model::{Group, Item, SourceMeta};
use crate::storage;
use crate::text::{dedup_key, display_name, normalize_text};

pub const MAX_RESULTS: usize = 1000;
pub const LATEST_LIMIT: usize = 100;
pub const REPACK_KEYS: [&str; 6] = [
    "repacks", "repack", "games", "items", "downloads", "sources",
];

struct NormItem {
    title: String,
    uris: Vec<String>,
    file_size: String,
    upload_date: String,
    description_html: String,
}

fn stringify(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "None".to_string(),
        other => other.to_string(),
    }
}

/// Locate the game list inside a source document.
pub fn extract_repacks(data: &Value) -> Option<(Vec<&Value>, String)> {
    if let Value::Object(map) = data {
        for key in REPACK_KEYS {
            if let Some(Value::Array(items)) = map.get(key) {
                return Some((items.iter().collect(), key.to_string()));
            }
        }
    }
    if let Some(items) = find_first_list(data, 0) {
        return Some((items.iter().collect(), "auto-detected".to_string()));
    }
    if let Value::Array(items) = data {
        if matches!(items.first(), Some(Value::Object(_))) {
            return Some((items.iter().collect(), "root-list".to_string()));
        }
    }
    None
}

fn find_first_list(data: &Value, depth: usize) -> Option<&Vec<Value>> {
    if depth > 3 {
        return None;
    }
    if let Value::Array(items) = data {
        if let Some(Value::Object(_)) = items.first() {
            let wanted = ["title", "uris", "url", "name", "magnet", "download"];
            if wanted.iter().any(|key| items[0].get(*key).is_some()) {
                return Some(items);
            }
        }
        return None;
    }
    if let Value::Object(map) = data {
        for value in map.values() {
            if let Some(found) = find_first_list(value, depth + 1) {
                return Some(found);
            }
        }
    }
    None
}

fn validate_repack_item(raw: &Value) -> Option<NormItem> {
    let object = raw.as_object()?;

    let mut title = None;
    for key in ["title", "name", "game", "description"] {
        if let Some(Value::String(text)) = object.get(key) {
            title = Some(text.clone());
            break;
        }
    }

    let mut uris = Vec::new();
    for key in [
        "uris", "uri", "urls", "url", "links", "link", "magnet", "torrent",
    ] {
        match object.get(key) {
            Some(Value::Array(values)) => {
                uris.extend(
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string)),
                );
            }
            Some(Value::String(value)) => uris.push(value.clone()),
            _ => {}
        }
    }

    if title.is_none() && uris.is_empty() {
        return None;
    }

    let file_size = ["fileSize", "size", "file_size"]
        .iter()
        .find_map(|key| object.get(*key))
        .map(stringify)
        .unwrap_or_else(|| "N/A".to_string());
    let upload_date = ["uploadDate", "date", "upload_date"]
        .iter()
        .find_map(|key| object.get(*key))
        .map(stringify)
        .unwrap_or_else(|| "N/A".to_string());
    let description_html = object
        .get("descriptionHtml")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    Some(NormItem {
        title: title.unwrap_or_else(|| "Unknown".to_string()),
        uris,
        file_size,
        upload_date,
        description_html,
    })
}

pub fn load_source(dir: &Path, name: &str) -> Result<(Vec<Item>, SourceMeta), String> {
    let path = dir.join(name);
    let data = storage::read_source_file(&path)?;
    let (raw_items, detected_key) = extract_repacks(&data).ok_or_else(|| {
        let keys = match &data {
            Value::Object(map) => map.keys().cloned().collect::<Vec<_>>().join(", "),
            other => other.to_string(),
        };
        format!("无法找到游戏列表字段。JSON 顶层字段: {keys}")
    })?;

    let display = display_name(name);
    let mut items = Vec::with_capacity(raw_items.len());
    let mut invalid = 0usize;
    for raw in raw_items {
        let Some(norm) = validate_repack_item(raw) else {
            invalid += 1;
            continue;
        };
        let nt = normalize_text(&norm.title);
        let key = dedup_key(&norm.title);
        let date_dt = parse_upload_date(Some(&norm.upload_date));
        let source_index = items.len();
        items.push(Item {
            title: norm.title,
            nt,
            dkey: key,
            uris: norm.uris,
            file_size: norm.file_size,
            upload_date: norm.upload_date,
            date_dt,
            source_name: name.to_string(),
            source_display: display.clone(),
            source_index,
            description_html: norm.description_html,
        });
    }

    let mut url = storage::read_source_url(&path);
    if url.is_none() {
        if let Value::Object(map) = &data {
            if let Some(Value::String(fallback)) = map.get("url") {
                let trimmed = fallback.trim();
                if !trimmed.is_empty() {
                    url = Some(trimmed.to_string());
                }
            }
        }
    }

    let meta = SourceMeta {
        display,
        count: items.len(),
        url,
        path: path.to_string_lossy().to_string(),
        updated: storage::file_updated(&path),
        detected_key: Some(detected_key),
        invalid_count: invalid,
    };
    Ok((items, meta))
}

/// Load every source, handing each one to `on_source` as soon as it is ready.
pub fn stream_sources(
    dir: &Path,
    progress: &mut dyn FnMut(usize, usize, &str),
    on_source: &mut dyn FnMut(&str, Vec<Item>, SourceMeta),
) -> Vec<String> {
    let names = storage::source_filenames(dir);
    let total = names.len();
    let mut errors = Vec::new();
    for (index, name) in names.iter().enumerate() {
        progress(index + 1, total, name);
        match load_source(dir, name) {
            Ok((items, meta)) => {
                on_source(name, items, meta);
            }
            Err(error) => errors.push(format!("{name}: {error}")),
        }
    }
    errors
}

fn cmp_date_desc(a: Option<NaiveDateTime>, b: Option<NaiveDateTime>) -> Ordering {
    b.cmp(&a)
}

pub fn finalize_group(mut items: Vec<Item>, score: f64) -> Group {
    let mut rep = 0usize;
    for (index, item) in items.iter().enumerate().skip(1) {
        let candidate = (item.title.chars().count(), item.title.as_str());
        let current = (items[rep].title.chars().count(), items[rep].title.as_str());
        if candidate < current {
            rep = index;
        }
    }

    let mut newest = None;
    for (index, item) in items.iter().enumerate() {
        let Some(date) = item.date_dt else { continue };
        match newest {
            None => newest = Some(index),
            Some(current) => {
                if date > items[current].date_dt.unwrap() {
                    newest = Some(index);
                }
            }
        }
    }
    let newest = newest.unwrap_or(0);

    let title = items[rep].title.clone();
    let size = items[newest].file_size.clone();
    let date_str = items[newest].upload_date.clone();
    let date_dt = items[newest].date_dt;
    let date_display = match date_dt {
        Some(dt) => format_date(&dt),
        None => date_str.clone(),
    };

    let mut sources = Vec::new();
    let mut uris = Vec::new();
    let mut seen_uris = HashSet::new();
    for item in &items {
        if !sources.contains(&item.source_display) {
            sources.push(item.source_display.clone());
        }
        for uri in &item.uris {
            if seen_uris.insert(uri.clone()) {
                uris.push(uri.clone());
            }
        }
    }

    items.sort_by(|a, b| b.date_dt.cmp(&a.date_dt));

    Group {
        title,
        sources,
        size,
        date_str,
        date_display,
        link_count: uris.len(),
        uris,
        score,
        date_dt,
        items,
    }
}

struct Bucket {
    items: Vec<Item>,
    score: f64,
}

fn add_hit(
    buckets: &mut HashMap<String, Bucket>,
    order: &mut Vec<String>,
    item: &Item,
    score: f64,
) {
    let key = if item.dkey.is_empty() {
        item.nt.clone()
    } else {
        item.dkey.clone()
    };
    match buckets.get_mut(&key) {
        Some(bucket) => {
            bucket.items.push(item.clone());
            if score > bucket.score {
                bucket.score = score;
            }
        }
        None => {
            order.push(key.clone());
            buckets.insert(
                key,
                Bucket {
                    items: vec![item.clone()],
                    score,
                },
            );
        }
    }
}

pub fn search_items(
    items: &[Item],
    query: &str,
    fuzzy: bool,
    min_similarity: f64,
    source_filter: &str,
    limit: usize,
) -> (Vec<Group>, usize) {
    let q = normalize_text(query);
    if q.is_empty() {
        return (Vec::new(), 0);
    }

    let mut buckets: HashMap<String, Bucket> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut substring_hits = 0usize;

    for item in items {
        if !source_filter.is_empty() && item.source_display != source_filter {
            continue;
        }
        let Some(pos) = item.nt.find(&q) else { continue };
        let score = if item.nt == q {
            1.0
        } else {
            let coverage = q.chars().count() as f64 / item.nt.chars().count().max(1) as f64;
            0.9 + 0.09 * coverage + if pos == 0 { 0.005 } else { 0.0 }
        };
        add_hit(&mut buckets, &mut order, item, score);
        substring_hits += 1;
    }

    if fuzzy && substring_hits < 50 {
        let qcompact: String = q.chars().filter(|c| *c != ' ').collect();
        let head: String = qcompact.chars().take(3).collect();
        let qchars: HashSet<char> = qcompact.chars().collect();
        let min_shared = ((qchars.len() as f64) * 0.7) as usize;
        let min_shared = min_shared.max(1);

        for item in items {
            if !source_filter.is_empty() && item.source_display != source_filter {
                continue;
            }
            let nt = &item.nt;
            if nt.contains(&q) {
                continue;
            }
            if qcompact.chars().count() >= 3 && !nt.contains(&head) {
                continue;
            }
            let tchars: HashSet<char> = nt.chars().filter(|c| *c != ' ').collect();
            if qchars.intersection(&tchars).count() < min_shared {
                continue;
            }
            let score = 0.9 * sequence_ratio(&q, nt);
            if score >= min_similarity {
                add_hit(&mut buckets, &mut order, item, score);
            }
        }
    }

    let total = order.len();
    order.sort_by(|a, b| {
        let left = buckets.get(a).map(|bucket| bucket.score).unwrap_or(0.0);
        let right = buckets.get(b).map(|bucket| bucket.score).unwrap_or(0.0);
        right.partial_cmp(&left).unwrap_or(Ordering::Equal)
    });

    let mut groups: Vec<Group> = order
        .into_iter()
        .take(limit)
        .filter_map(|key| buckets.remove(&key))
        .map(|bucket| finalize_group(bucket.items, bucket.score))
        .collect();

    groups.sort_by(|a, b| {
        let by_score = b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal);
        if by_score == Ordering::Equal {
            cmp_date_desc(a.date_dt, b.date_dt)
        } else {
            by_score
        }
    });
    (groups, total)
}

pub fn latest_items(items: &[Item], source_filter: &str, limit: usize) -> (Vec<Group>, usize) {
    let mut newest: HashMap<String, Option<NaiveDateTime>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();

    for item in items {
        if !source_filter.is_empty() && item.source_display != source_filter {
            continue;
        }
        let key = if item.dkey.is_empty() {
            item.nt.clone()
        } else {
            item.dkey.clone()
        };
        match item.date_dt {
            None => {
                if !newest.contains_key(&key) {
                    newest.insert(key.clone(), None);
                    order.push(key);
                }
            }
            Some(date) => match newest.get(&key) {
                Some(Some(current)) if *current >= date => {}
                _ => {
                    if !newest.contains_key(&key) {
                        order.push(key.clone());
                    }
                    newest.insert(key, Some(date));
                }
            },
        }
    }

    let total = newest.len();
    let mut keys = order;
    keys.sort_by(|a, b| {
        let left = newest.get(a).copied().flatten();
        let right = newest.get(b).copied().flatten();
        cmp_date_desc(left, right)
    });
    keys.truncate(limit);

    let selected: HashSet<String> = keys.iter().cloned().collect();
    let mut bucket_items: HashMap<String, Vec<Item>> = HashMap::new();
    for item in items {
        if !source_filter.is_empty() && item.source_display != source_filter {
            continue;
        }
        let key = if item.dkey.is_empty() {
            item.nt.clone()
        } else {
            item.dkey.clone()
        };
        if selected.contains(&key) {
            bucket_items.entry(key).or_default().push(item.clone());
        }
    }

    let groups: Vec<Group> = keys
        .into_iter()
        .filter_map(|key| bucket_items.remove(&key))
        .map(|items| finalize_group(items, 0.0))
        .collect();
    (groups, total)
}

/// Python's difflib ratio: 2 * matched characters / total length.
pub fn sequence_ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let matched = matching_chars(&a, &b, 0, a.len(), 0, b.len());
    (2 * matched) as f64 / (a.len() + b.len()) as f64
}

fn matching_chars(
    a: &[char],
    b: &[char],
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> usize {
    let (start_a, start_b, size) = longest_match(a, b, alo, ahi, blo, bhi);
    if size == 0 {
        return 0;
    }
    size
        + matching_chars(a, b, alo, start_a, blo, start_b)
        + matching_chars(a, b, start_a + size, ahi, start_b + size, bhi)
}

fn longest_match(
    a: &[char],
    b: &[char],
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> (usize, usize, usize) {
    if alo >= ahi || blo >= bhi {
        return (alo, blo, 0);
    }
    let width = bhi - blo;
    let mut prev = vec![0usize; width + 1];
    let mut cur = vec![0usize; width + 1];
    let mut best = (alo, blo, 0usize);
    for i in alo..ahi {
        for slot in cur.iter_mut() {
            *slot = 0;
        }
        for j in blo..bhi {
            if a[i] != b[j] {
                continue;
            }
            let run = prev[j - blo] + 1;
            cur[j - blo + 1] = run;
            if run > best.2 {
                best = (i + 1 - run, j + 1 - run, run);
            }
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    best
}

pub fn is_valid_repack(raw: &Value) -> bool {
    let Some(object) = raw.as_object() else {
        return false;
    };
    let has_title = ["title", "name", "game", "description"]
        .iter()
        .any(|key| matches!(object.get(*key), Some(Value::String(_))));
    if has_title {
        return true;
    }
    ["uris", "uri", "urls", "url", "links", "link", "magnet", "torrent"]
        .iter()
        .any(|key| match object.get(*key) {
            Some(Value::String(_)) => true,
            Some(Value::Array(items)) => items.iter().any(Value::is_string),
            _ => false,
        })
}

pub fn total_groups(items: &[Item]) -> usize {
    items
        .iter()
        .map(|item| {
            if item.dkey.is_empty() {
                item.nt.as_str()
            } else {
                item.dkey.as_str()
            }
        })
        .collect::<HashSet<_>>()
        .len()
}

pub fn source_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, source: &str, date: &str, uri: &str) -> Item {
        let nt = normalize_text(title);
        Item {
            title: title.to_string(),
            dkey: dedup_key(title),
            nt,
            uris: vec![uri.to_string()],
            file_size: "1 GB".to_string(),
            upload_date: date.to_string(),
            date_dt: parse_upload_date(Some(date)),
            source_name: source.to_string(),
            source_display: source.to_string(),
            source_index: 0,
            description_html: String::new(),
        }
    }

    #[test]
    fn deduplicates_versions_into_one_group() {
        let items = vec![
            item("Hades v1.0", "a", "2024-01-01", "magnet:a"),
            item("Hades v1.1", "b", "2024-02-01", "magnet:b"),
            item("Other Game", "a", "2024-01-01", "magnet:c"),
        ];
        let (groups, total) = search_items(&items, "hades", true, 0.3, "", 10);
        assert_eq!(total, 1);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Hades v1.0");
        assert_eq!(groups[0].sources, vec!["a", "b"]);
        assert_eq!(groups[0].link_count, 2);
        assert_eq!(groups[0].date_display, "2024-02-01");
    }

    #[test]
    fn search_scores_exact_and_prefix_hits() {
        let items = vec![
            item("Elden Ring", "a", "2024-01-01", "magnet:x"),
            item("Elden Ring Nightreign", "a", "2024-01-02", "magnet:y"),
        ];
        let (groups, total) = search_items(&items, "elden ring", true, 0.3, "", 10);
        assert_eq!(total, 2);
        assert_eq!(groups[0].title, "Elden Ring");
    }

    #[test]
    fn filters_by_source_display() {
        let items = vec![
            item("Alpha", "a", "2024-01-01", "magnet:a"),
            item("Alpha", "b", "2024-01-01", "magnet:b"),
        ];
        let (groups, total) = search_items(&items, "alpha", false, 0.3, "b", 10);
        assert_eq!(total, 1);
        assert_eq!(groups[0].sources, vec!["b"]);
    }

    #[test]
    fn latest_keeps_undated_groups_last_and_caps_results() {
        let items = vec![
            item("Old", "a", "2020-01-01", "magnet:1"),
            item("New", "a", "2024-05-05", "magnet:2"),
            item("NoDate", "a", "N/A", "magnet:3"),
        ];
        let (groups, total) = latest_items(&items, "", 2);
        assert_eq!(total, 3);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "New");
        assert_eq!(groups[1].title, "Old");
    }

    #[test]
    fn sequence_ratio_matches_difflib_examples() {
        let ratio = sequence_ratio("hades", "hades");
        assert!((ratio - 1.0).abs() < 1e-9);
        let ratio = sequence_ratio("eldn ring", "elden ring");
        assert!(ratio > 0.9 && ratio < 1.0);
        assert_eq!(sequence_ratio("", "abc"), 0.0);
    }

    #[test]
    #[ignore]
    fn measure_real_sources() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("sources");
        let mut items: Vec<Item> = Vec::new();
        let mut sources = 0usize;
        let mut progress = |_: usize, _: usize, _: &str| {};
        let mut on_source = |_: &str, source_items: Vec<Item>, meta: SourceMeta| {
            sources += 1;
            eprintln!("{}: {}", meta.display, meta.count);
            items.extend(source_items);
        };
        let errors = stream_sources(&dir, &mut progress, &mut on_source);
        let mut title = 0usize;
        let mut nt = 0usize;
        let mut dkey = 0usize;
        let mut desc = 0usize;
        let mut uris = 0usize;
        let mut uri_count = 0usize;
        for item in &items {
            title += item.title.len();
            nt += item.nt.len();
            dkey += item.dkey.len();
            desc += item.description_html.len();
            for uri in &item.uris {
                uris += uri.len();
                uri_count += 1;
            }
        }
        eprintln!(
            "sources={sources} errors={} items={} groups={} size_of_item={}",
            errors.len(),
            items.len(),
            total_groups(&items),
            std::mem::size_of::<Item>()
        );
        eprintln!(
            "bytes title={title} nt={nt} dkey={dkey} desc={desc} uris={uris} uri_count={uri_count}"
        );
        eprintln!(
            "total_string_bytes={}",
            title + nt + dkey + desc + uris
        );
    }

    #[test]
    fn loads_synthetic_source_and_skips_invalid_rows() {
        let dir = std::env::temp_dir().join(format!("gamemagnet-cat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("demo.json.gz");
        let data = serde_json::json!({"repacks": [
            {"title": "Game A", "uris": ["magnet:a"], "fileSize": "2 GB", "uploadDate": "2024-01-01"},
            {"foo": "bar"}
        ]});
        crate::storage::write_source_file(&path, &data).unwrap();
        let (items, meta) = load_source(&dir, "demo.json.gz").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(meta.count, 1);
        assert_eq!(meta.invalid_count, 1);
        assert_eq!(meta.display, "demo");
        std::fs::remove_dir_all(&dir).ok();
    }
}
