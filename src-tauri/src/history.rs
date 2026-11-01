//! Persistent generation history (survives restarts).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HistoryImage {
    pub filename: String,
    pub subfolder: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: String,
    pub ts: u64,
    pub profile: String,
    pub label: String,
    pub mode: String,
    pub prompt: String,
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f64,
    pub seed: i64,
    pub seconds: f64,
    pub images: Vec<HistoryImage>,
    /// 生成时所在项目。私密项目根本不写历史；老记录没有这个字段 = 全局/未知。
    #[serde(default)]
    pub project: Option<String>,
}

const MAX_ITEMS: usize = 300;

fn path(dir: &PathBuf) -> PathBuf {
    dir.join("history.json")
}

pub fn load(dir: &PathBuf) -> Vec<HistoryItem> {
    std::fs::read_to_string(path(dir))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<HistoryItem>>(&t).ok())
        .unwrap_or_default()
}

pub fn save(dir: &PathBuf, items: &[HistoryItem]) {
    let trimmed: Vec<&HistoryItem> = items.iter().take(MAX_ITEMS).collect();
    if let Ok(text) = serde_json::to_string(&trimmed) {
        let _ = crate::fsutil::atomic_write_str(&path(dir), &text);
    }
}

/// Prepend one entry and trim to the cap.
pub fn append(dir: &PathBuf, item: HistoryItem) {
    let mut items = load(dir);
    items.insert(0, item);
    save(dir, &items);
}

pub fn clear(dir: &PathBuf) {
    let _ = std::fs::remove_file(path(dir));
}
