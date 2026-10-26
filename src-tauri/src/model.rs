//! Shared data types for the catalog.

use chrono::NaiveDateTime;
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct Item {
    pub title: String,
    pub nt: String,
    pub dkey: String,
    pub uris: Vec<String>,
    pub file_size: String,
    pub upload_date: String,
    pub date_dt: Option<NaiveDateTime>,
    pub source_name: String,
    pub source_display: String,
    pub source_index: usize,
    pub description_html: String,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct SourceMeta {
    pub display: String,
    pub count: usize,
    pub url: Option<String>,
    pub path: String,
    pub updated: f64,
    pub detected_key: Option<String>,
    pub invalid_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub title: String,
    pub sources: Vec<String>,
    pub size: String,
    pub date_str: String,
    pub date_display: String,
    pub uris: Vec<String>,
    pub link_count: usize,
    pub score: f64,
    #[serde(skip)]
    pub date_dt: Option<NaiveDateTime>,
    #[serde(skip)]
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub title: String,
    pub sources: String,
    pub size: String,
    pub date: String,
    #[serde(rename = "linkCount")]
    pub link_count: usize,
}

impl Group {
    pub fn row(&self) -> Row {
        Row {
            title: self.title.clone(),
            sources: self.sources.join(" + "),
            size: self.size.clone(),
            date: self.date_display.clone(),
            link_count: self.link_count,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SourceRow {
    pub name: String,
    pub display: String,
    pub count: usize,
    pub local: bool,
    pub active: bool,
}
