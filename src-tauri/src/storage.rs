//! Source file storage: gzip JSON, URL sidecars and atomic writes.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use flate2::read::GzDecoder;
use flate2::{Compression, GzBuilder};
use serde_json::Value;

pub const SOURCE_URL_SUFFIX: &str = ".source-url";

pub fn sources_dir() -> PathBuf {
    if let Ok(path) = std::env::var("GAMEMAGNET_SOURCES") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    #[cfg(debug_assertions)]
    {
        let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sources");
        if dev.is_dir() {
            return dev;
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sources")
}

pub fn source_filenames(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    let packed: Vec<&String> = names
        .iter()
        .filter(|name| name.to_lowercase().ends_with(".json.gz"))
        .collect();
    let plain: Vec<&String> = names
        .iter()
        .filter(|name| name.to_lowercase().ends_with(".json"))
        .collect();
    let shadowed: Vec<String> = packed
        .iter()
        .map(|name| name[..name.len() - 3].to_string())
        .collect();

    let mut result: Vec<String> = packed
        .iter()
        .map(|name| (*name).clone())
        .chain(
            plain
                .iter()
                .filter(|name| !shadowed.contains(name))
                .map(|name| (*name).clone()),
        )
        .collect();
    result.sort();
    result
}

pub fn read_source_url(path: &Path) -> Option<String> {
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(SOURCE_URL_SUFFIX);
    let text = fs::read_to_string(PathBuf::from(sidecar)).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn write_url_file(path: &Path, url: &str) -> std::io::Result<()> {
    let mut sidecar = path.as_os_str().to_owned();
    sidecar.push(SOURCE_URL_SUFFIX);
    fs::write(PathBuf::from(sidecar), url)
}

pub fn read_source_file(path: &Path) -> Result<Value, String> {
    let is_gzip = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("gz"))
        .unwrap_or(false);
    let mut bytes = Vec::new();
    if is_gzip {
        let file = fs::File::open(path).map_err(|err| err.to_string())?;
        GzDecoder::new(file)
            .read_to_end(&mut bytes)
            .map_err(|err| err.to_string())?;
    } else {
        bytes = fs::read(path).map_err(|err| err.to_string())?;
    }
    serde_json::from_slice(&bytes).map_err(|err| err.to_string())
}

pub fn write_source_file(path: &Path, data: &Value) -> std::io::Result<()> {
    let mut payload = serde_json::to_vec(data)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
    let is_gzip = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("gz"))
        .unwrap_or(false);
    if is_gzip {
        let mut encoder = GzBuilder::new()
            .mtime(0)
            .write(Vec::new(), Compression::new(6));
        encoder.write_all(&payload)?;
        payload = encoder.finish()?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, &payload)?;
    fs::rename(&tmp, path)
}

pub fn file_updated(path: &Path) -> f64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gamemagnet-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn gzip_roundtrip_and_sidecar() {
        let dir = temp_dir("storage");
        let path = dir.join("demo.json.gz");
        let data = serde_json::json!({"repacks": [{"title": "游戏"}]});
        write_source_file(&path, &data).unwrap();
        write_url_file(&path, "https://example.com/demo.json").unwrap();
        assert_eq!(read_source_file(&path).unwrap(), data);
        assert_eq!(
            read_source_url(&path).as_deref(),
            Some("https://example.com/demo.json")
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn packed_sources_shadow_plain_ones() {
        let dir = temp_dir("shadow");
        for name in ["a.json", "a.json.gz", "b.json", "c.json.gz"] {
            fs::write(dir.join(name), b"{}").unwrap();
        }
        let names = source_filenames(&dir);
        assert_eq!(names, vec!["a.json.gz", "b.json", "c.json.gz"]);
        fs::remove_dir_all(&dir).ok();
    }
}
