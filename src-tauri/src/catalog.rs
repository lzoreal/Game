//! Catalog parsing, deduplication, search and newest-first listing.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use std::sync::Arc;

use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::mpsc;

use chrono::NaiveDateTime;
use serde_json::Value;

use crate::dates::{format_date, parse_upload_date};
use crate::model::{Group, Item, SourceMeta};
use crate::search::ParsedQuery;
use crate::storage;
use crate::text::{dedup_key, display_name};

pub const MAX_RESULTS: usize = 1000;
pub const LATEST_LIMIT: usize = 100;
pub const REPACK_KEYS: [&str; 6] = [
    "repacks", "repack", "games", "items", "downloads", "sources",
];

pub fn group_key_id(key: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub fn take_repacks(data: Value) -> Option<(Vec<Value>, String)> {
    if let Value::Object(mut map) = data {
        for key in REPACK_KEYS {
            if let Some(Value::Array(items)) = map.remove(key) {
                return Some((items, key.to_string()));
            }
        }
        if let Some(items) = take_first_list(Value::Object(map), 0) {
            return Some((items, "auto-detected".to_string()));
        }
        return None;
    }

    let Value::Array(items) = data else {
        return None;
    };
    let Some(Value::Object(first)) = items.first() else {
        return None;
    };
    let wanted = ["title", "uris", "url", "name", "magnet", "download"];
    let detected = if wanted.iter().any(|key| first.contains_key(*key)) {
        "auto-detected"
    } else {
        "root-list"
    };
    Some((items, detected.to_string()))
}

fn take_first_list(data: Value, depth: usize) -> Option<Vec<Value>> {
    if depth > 3 {
        return None;
    }
    match data {
        Value::Array(items) => {
            let Some(Value::Object(first)) = items.first() else {
                return None;
            };
            let wanted = ["title", "uris", "url", "name", "magnet", "download"];
            if wanted.iter().any(|key| first.contains_key(*key)) {
                Some(items)
            } else {
                None
            }
        }
        Value::Object(map) => {
            for (_, value) in map {
                if let Some(found) = take_first_list(value, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
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

fn take_field(object: &mut serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = object.remove(*key) {
            return Some(match value {
                Value::String(text) => text,
                Value::Null => "None".to_string(),
                other => other.to_string(),
            });
        }
    }
    None
}

fn build_item(
    raw: Value,
    source_name: &Arc<str>,
    source_display: &Arc<str>,
    source_index: usize,
) -> Option<Item> {
    let Value::Object(mut object) = raw else {
        return None;
    };

    let mut title = None;
    for key in ["title", "name", "game", "description"] {
        if let Some(Value::String(text)) = object.remove(key) {
            title = Some(text);
            break;
        }
    }

    let mut uris: Vec<Box<str>> = Vec::new();
    for key in [
        "uris", "uri", "urls", "url", "links", "link", "magnet", "torrent",
    ] {
        match object.remove(key) {
            Some(Value::Array(values)) => {
                for value in values {
                    if let Value::String(uri) = value {
                        uris.push(uri.into_boxed_str());
                    }
                }
            }
            Some(Value::String(uri)) => uris.push(uri.into_boxed_str()),
            _ => {}
        }
    }

    if title.is_none() && uris.is_empty() {
        return None;
    }

    let title = title.unwrap_or_else(|| "Unknown".to_string());
    let file_size = take_field(&mut object, &["fileSize", "size", "file_size"])
        .unwrap_or_else(|| "N/A".to_string());
    let upload_date = take_field(&mut object, &["uploadDate", "date", "upload_date"])
        .unwrap_or_else(|| "N/A".to_string());
    let description_html = match object.remove("descriptionHtml") {
        Some(Value::String(text)) => text,
        _ => String::new(),
    };

    let (nt, dkey) = crate::text::normalize_title(&title);
    let key_id = group_key_id(&dkey);
    let date_dt = parse_upload_date(Some(&upload_date));

    Some(Item {
        title: title.into_boxed_str(),
        nt: nt.into_boxed_str(),
        key_id,
        uris,
        file_size: file_size.into_boxed_str(),
        upload_date: upload_date.into_boxed_str(),
        date_dt,
        source_name: Arc::clone(source_name),
        source_display: Arc::clone(source_display),
        source_index,
        description_html: description_html.into_boxed_str(),
    })
}

pub fn load_source(dir: &Path, name: &str) -> Result<(Vec<Item>, SourceMeta), String> {
    let path = dir.join(name);
    let data = storage::read_source_file(&path)?;
    let keys = match &data {
        Value::Object(map) => map.keys().cloned().collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    };
    let fallback_url = match &data {
        Value::Object(map) => match map.get("url") {
            Some(Value::String(text)) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        },
        _ => None,
    };

    let (raw_items, detected_key) = take_repacks(data)
        .ok_or_else(|| format!("无法找到游戏列表字段。JSON 顶层字段: {keys}"))?;

    let display = display_name(name);
    let source_name: Arc<str> = Arc::from(name);
    let source_display: Arc<str> = Arc::from(display.as_str());
    let mut items = Vec::with_capacity(raw_items.len());
    let mut invalid = 0usize;
    for raw in raw_items {
        match build_item(raw, &source_name, &source_display, items.len()) {
            Some(item) => items.push(item),
            None => invalid += 1,
        }
    }

    let mut url = storage::read_source_url(&path);
    if url.is_none() {
        url = fallback_url;
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

/// Load every source in parallel, handing each one to `on_source` as soon as it is ready.
pub fn stream_sources(
    dir: &Path,
    progress: &mut dyn FnMut(usize, usize, &str),
    on_source: &mut dyn FnMut(&str, Vec<Item>, SourceMeta),
) -> Vec<String> {
    let names = storage::source_filenames(dir);
    let total = names.len();
    if total == 0 {
        return Vec::new();
    }

    let workers = std::env::var("GAMEMAGNET_LOAD_WORKERS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|count| count.get())
                .unwrap_or(1)
        })
        .clamp(1, 4)
        .min(total);
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel::<(&str, Result<(Vec<Item>, SourceMeta), String>)>();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let sender = sender.clone();
            let names = &names;
            let next = &next;
            scope.spawn(move || loop {
                let index = next.fetch_add(1, AtomicOrdering::Relaxed);
                if index >= names.len() {
                    break;
                }
                let name = names[index].as_str();
                let result = load_source(dir, name).map_err(|error| format!("{name}: {error}"));
                if sender.send((name, result)).is_err() {
                    break;
                }
            });
        }
        drop(sender);

        let mut errors = Vec::new();
        let mut completed = 0usize;
        for (name, result) in receiver {
            completed += 1;
            progress(completed, total, name);
            match result {
                Ok((items, meta)) => on_source(name, items, meta),
                Err(error) => errors.push(error),
            }
        }
        errors
    })
}

fn cmp_date_desc(a: Option<NaiveDateTime>, b: Option<NaiveDateTime>) -> Ordering {
    b.cmp(&a)
}

pub fn finalize_group(mut items: Vec<Item>, score: f64) -> Group {
    let mut rep = 0usize;
    for (index, item) in items.iter().enumerate().skip(1) {
        let candidate = (item.title.chars().count(), item.title.as_ref());
        let current = (items[rep].title.chars().count(), items[rep].title.as_ref());
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

    let title = items[rep].title.to_string();
    // The group identity is the normalized title of a member, not the 64-bit
    // hash. Every member shares the same dedup key, so any of them will do.
    let key = dedup_key(&items[rep].title);
    let size = items[newest].file_size.to_string();
    let date_str = items[newest].upload_date.to_string();
    let date_dt = items[newest].date_dt;
    let date_display = match date_dt {
        Some(dt) => format_date(&dt),
        None => date_str.clone(),
    };

    let mut sources = Vec::new();
    let mut uris = Vec::new();
    let mut seen_uris = HashSet::new();
    for item in &items {
        let display = item.source_display.to_string();
        if !sources.contains(&display) {
            sources.push(display);
        }
        for uri in &item.uris {
            let uri = uri.to_string();
            if seen_uris.insert(uri.clone()) {
                uris.push(uri);
            }
        }
    }

    items.sort_by(|a, b| b.date_dt.cmp(&a.date_dt));

    Group {
        title,
        key,
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
    buckets: &mut HashMap<u64, Bucket>,
    order: &mut Vec<u64>,
    item: &Item,
    score: f64,
) {
    let key = item.key_id;
    match buckets.get_mut(&key) {
        Some(bucket) => {
            bucket.items.push(item.clone());
            if score > bucket.score {
                bucket.score = score;
            }
        }
        None => {
            order.push(key);
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
    parsed: &ParsedQuery,
    fuzzy: bool,
    min_similarity: f64,
    source_filter: &str,
    limit: usize,
) -> (Vec<Group>, usize) {
    let Some(q) = parsed.needle() else {
        return (Vec::new(), 0);
    };
    let mut buckets: HashMap<u64, Bucket> = HashMap::new();
    let mut order: Vec<u64> = Vec::new();
    let mut substring_hits = 0usize;

    for item in items {
        if !source_filter.is_empty() && item.source_display.as_ref() != source_filter {
            continue;
        }
        if !parsed.matches(&item.nt) {
            continue;
        }
        let Some(pos) = item.nt.find(q) else { continue };
        let score = if item.nt.as_ref() == q {
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
            if !source_filter.is_empty() && item.source_display.as_ref() != source_filter {
                continue;
            }
            if !parsed.matches(&item.nt) {
                continue;
            }
            let nt = &item.nt;
            if nt.contains(q) {
                continue;
            }
            if qcompact.chars().count() >= 3 && !nt.contains(&head) {
                continue;
            }
            let tchars: HashSet<char> = nt.chars().filter(|c| *c != ' ').collect();
            if qchars.intersection(&tchars).count() < min_shared {
                continue;
            }
            let score = 0.9 * sequence_ratio(q, nt);
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
    let mut newest: HashMap<u64, Option<NaiveDateTime>> = HashMap::new();
    let mut order: Vec<u64> = Vec::new();

    for item in items {
        if !source_filter.is_empty() && item.source_display.as_ref() != source_filter {
            continue;
        }
        let key = item.key_id;
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

    let selected: HashSet<u64> = keys.iter().cloned().collect();
    let mut bucket_items: HashMap<u64, Vec<Item>> = HashMap::new();
    for item in items {
        if !source_filter.is_empty() && item.source_display.as_ref() != source_filter {
            continue;
        }
        let key = item.key_id;
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
        .map(|item| item.key_id)
        .collect::<HashSet<_>>()
        .len()
}

pub fn source_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::parse_query;

    fn item(title: &str, source: &str, date: &str, uri: &str) -> Item {
        Item {
            title: title.to_string().into_boxed_str(),
            nt: crate::text::normalize_text(title).into_boxed_str(),
            key_id: group_key_id(&dedup_key(title)),
            uris: vec![uri.to_string().into_boxed_str()],
            file_size: "1 GB".into(),
            upload_date: date.into(),
            date_dt: parse_upload_date(Some(date)),
            source_name: Arc::from(source),
            source_display: Arc::from(source),
            source_index: 0,
            description_html: String::new().into_boxed_str(),
        }
    }

    #[test]
    fn deduplicates_versions_into_one_group() {
        let items = vec![
            item("Hades v1.0", "a", "2024-01-01", "magnet:a"),
            item("Hades v1.1", "b", "2024-02-01", "magnet:b"),
            item("Other Game", "a", "2024-01-01", "magnet:c"),
        ];
        let (groups, total) = search_items(&items, &parse_query("hades"), true, 0.3, "", 10);
        assert_eq!(total, 1);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Hades v1.0");
        assert_eq!(groups[0].sources, vec!["a", "b"]);
        assert_eq!(groups[0].link_count, 2);
        assert_eq!(groups[0].date_display, "2024-02-01");
        // Favorites are anchored to this string, never to `group_key_id`.
        assert_eq!(groups[0].key, "hades");
    }

    #[test]
    fn search_scores_exact_and_prefix_hits() {
        let items = vec![
            item("Elden Ring", "a", "2024-01-01", "magnet:x"),
            item("Elden Ring Nightreign", "a", "2024-01-02", "magnet:y"),
        ];
        let (groups, total) = search_items(&items, &parse_query("elden ring"), true, 0.3, "", 10);
        assert_eq!(total, 2);
        assert_eq!(groups[0].title, "Elden Ring");
    }

    #[test]
    fn filters_by_source_display() {
        let items = vec![
            item("Alpha", "a", "2024-01-01", "magnet:a"),
            item("Alpha", "b", "2024-01-01", "magnet:b"),
        ];
        let (groups, total) = search_items(&items, &parse_query("alpha"), false, 0.3, "b", 10);
        assert_eq!(total, 1);
        assert_eq!(groups[0].sources, vec!["b"]);
    }

    #[test]
    fn excluded_words_remove_hits_without_touching_the_score() {
        let items = vec![
            item("Hades", "a", "2024-01-01", "magnet:a"),
            item("Hades 合集", "a", "2024-01-02", "magnet:b"),
            item("Hades GOG", "a", "2024-01-03", "magnet:c"),
        ];
        let (groups, total) = search_items(&items, &parse_query("hades -合集 -gog"), true, 0.3, "", 10);
        assert_eq!(total, 1);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "Hades");
    }

    #[test]
    fn quoted_phrases_must_appear_in_full() {
        let items = vec![
            item("Elden Ring Shadow of the Erdtree", "a", "2024-01-01", "magnet:a"),
            item("Elden Ring Nightreign", "a", "2024-01-02", "magnet:b"),
            item("Shadow of the Erdtree", "a", "2024-01-03", "magnet:c"),
        ];
        let (groups, total) =
            search_items(&items, &parse_query(r#""shadow of the" erdtree"#), true, 0.3, "", 10);
        assert_eq!(total, 2);
        let titles: Vec<&str> = groups.iter().map(|group| group.title.as_str()).collect();
        assert!(titles.contains(&"Elden Ring Shadow of the Erdtree"));
        assert!(titles.contains(&"Shadow of the Erdtree"));
        assert!(!titles.contains(&"Elden Ring Nightreign"));
    }

    #[test]
    fn a_query_of_only_exclusions_has_no_hits() {
        let items = vec![item("Hades", "a", "2024-01-01", "magnet:a")];
        let (groups, total) = search_items(&items, &parse_query("-合集"), true, 0.3, "", 10);
        assert!(groups.is_empty());
        assert_eq!(total, 0);
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
        let mut desc = 0usize;
        let mut uris = 0usize;
        let mut uri_count = 0usize;
        for item in &items {
            title += item.title.len();
            nt += item.nt.len();
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
            "bytes title={title} nt={nt} desc={desc} uris={uris} uri_count={uri_count}"
        );
        eprintln!(
            "total_string_bytes={}",
            title + nt + desc + uris
        );
    }

    #[test]
    #[ignore]
    fn bench_parts() {
        use std::io::Read;
        use std::time::Instant;

        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sources");
        for name in ["rutor.json.gz", "skidrow_cleaned.json.gz", "fitgirl.json.gz"] {
            let path = dir.join(name);
            let started = Instant::now();
            let file = std::fs::File::open(&path).unwrap();
            let mut bytes = Vec::new();
            flate2::read::GzDecoder::new(file)
                .read_to_end(&mut bytes)
                .unwrap();
            let decompress = started.elapsed().as_millis();

            let started = Instant::now();
            let data: Value = serde_json::from_slice(&bytes).unwrap();
            let parse = started.elapsed().as_millis();
            drop(bytes);

            let started = Instant::now();
            let (raw_items, _) = take_repacks(data).unwrap();
            let split = started.elapsed().as_millis();

            let started = Instant::now();
            let source_name: Arc<str> = Arc::from(name);
            let display: Arc<str> = Arc::from(display_name(name));
            let mut count = 0usize;
            for raw in raw_items {
                if build_item(raw, &source_name, &display, count).is_some() {
                    count += 1;
                }
            }
            let build = started.elapsed().as_millis();
            eprintln!(
                "{name}: decompress={decompress}ms parse={parse}ms split={split}ms build={build}ms items={count}"
            );
        }
    }

    #[test]
    #[ignore]
    fn bench_load_and_search() {
        use std::time::Instant;

        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sources");
        let names = crate::storage::source_filenames(&dir);
        let mut items: Vec<Item> = Vec::new();
        let total_start = Instant::now();
        for name in &names {
            let started = Instant::now();
            let (source_items, meta) = load_source(&dir, name).unwrap();
            eprintln!(
                "load {} items={} {}ms",
                meta.display,
                source_items.len(),
                started.elapsed().as_millis()
            );
            items.extend(source_items);
        }
        eprintln!("load_total {}ms", total_start.elapsed().as_millis());

        let started = Instant::now();
        let (groups, total) = latest_items(&items, "", 100);
        eprintln!(
            "latest {}ms groups={} total={}",
            started.elapsed().as_millis(),
            groups.len(),
            total
        );

        for query in ["hades", "hadesz", "cyberpunk"] {
            let started = Instant::now();
            let (groups, total) = search_items(&items, &parse_query(query), true, 0.3, "", 1000);
            eprintln!(
                "search {:?} {}ms groups={} total={}",
                query,
                started.elapsed().as_millis(),
                groups.len(),
                total
            );
        }
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
