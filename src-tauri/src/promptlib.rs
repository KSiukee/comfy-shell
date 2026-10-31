//! 跨项目提示词库：收藏、常用（使用次数）、搜索（前端过滤）、去 AI 套话。
//! 私密项目不往里存（避免明文泄漏）。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PromptEntry {
    pub id: String,
    pub text: String,
    pub negative: String,
    pub project_id: Option<String>,
    pub project_name: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub uses: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub text: String,
    pub removed: usize,
}

fn file(cfg: &Path) -> PathBuf {
    cfg.join("prompts.json")
}

fn load(cfg: &Path) -> Result<Vec<PromptEntry>, String> {
    let p = file(cfg);
    if !p.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("解析提示词库失败：{e}"))
}

fn save(cfg: &Path, list: &[PromptEntry]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&file(cfg), &text)
}

fn project_name(cfg: &Path, id: &str) -> String {
    if crate::vault::is_private(cfg, id) {
        return String::new();
    }
    crate::project::load(cfg, id).map(|p| p.name).unwrap_or_default()
}

pub fn list(cfg: &Path) -> Result<Vec<PromptEntry>, String> {
    let mut list = load(cfg)?;
    for e in &mut list {
        if let Some(pid) = &e.project_id {
            e.project_name = project_name(cfg, pid);
        }
    }
    list.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then(b.uses.cmp(&a.uses))
            .then(b.updated_at.cmp(&a.updated_at))
    });
    Ok(list)
}

pub fn upsert(
    cfg: &Path,
    text: &str,
    negative: &str,
    tags: Vec<String>,
    project: Option<&str>,
) -> Result<PromptEntry, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("提示词是空的".into());
    }
    if let Some(pid) = project {
        if crate::vault::is_private(cfg, pid) {
            return Err("私密项目不存进提示词库".into());
        }
    }
    let mut list = load(cfg)?;
    let t = now_ms();
    if let Some(e) = list.iter_mut().find(|e| e.text == text) {
        if !negative.trim().is_empty() {
            e.negative = negative.trim().to_string();
        }
        for tag in tags {
            let tag = tag.trim().to_string();
            if !tag.is_empty() && !e.tags.contains(&tag) {
                e.tags.push(tag);
            }
        }
        if let Some(pid) = project {
            e.project_id = Some(pid.to_string());
            e.project_name = project_name(cfg, pid);
        }
        e.updated_at = t;
        let copy = e.clone();
        save(cfg, &list)?;
        return Ok(copy);
    }
    let entry = PromptEntry {
        id: format!("pr_{:08x}", rand::random::<u32>()),
        text: text.to_string(),
        negative: negative.trim().to_string(),
        project_id: project.map(|s| s.to_string()),
        project_name: project.map(|s| project_name(cfg, s)).unwrap_or_default(),
        tags: tags.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        favorite: false,
        uses: 0,
        created_at: t,
        updated_at: t,
    };
    list.push(entry.clone());
    save(cfg, &list)?;
    Ok(entry)
}

pub fn star(cfg: &Path, id: &str, favorite: bool) -> Result<PromptEntry, String> {
    let mut list = load(cfg)?;
    let updated = {
        let e = list.iter_mut().find(|e| e.id == id).ok_or("提示词不存在")?;
        e.favorite = favorite;
        e.updated_at = now_ms();
        e.clone()
    };
    save(cfg, &list)?;
    Ok(updated)
}

pub fn bump_use(cfg: &Path, id: &str) -> Result<PromptEntry, String> {
    let mut list = load(cfg)?;
    let updated = {
        let e = list.iter_mut().find(|e| e.id == id).ok_or("提示词不存在")?;
        e.uses += 1;
        e.updated_at = now_ms();
        e.clone()
    };
    save(cfg, &list)?;
    Ok(updated)
}

pub fn delete(cfg: &Path, id: &str) -> Result<(), String> {
    let mut list = load(cfg)?;
    list.retain(|e| e.id != id);
    save(cfg, &list)
}

const CLICHES: &[&str] = &[
    "masterpiece",
    "best quality",
    "bestquality",
    "ultra detailed",
    "ultra-detailed",
    "highly detailed",
    "extremely detailed",
    "hyperrealistic",
    "hyper realistic",
    "photorealistic",
    "8k",
    "4k",
    "uhd",
    "hdr",
    "sharp focus",
    "award winning",
    "award-winning",
    "trending on artstation",
    "cinematic lighting",
    "studio lighting",
    "perfect composition",
    "intricate details",
    "high resolution",
    "大师作品",
    "最佳质量",
    "杰作",
    "超高清",
    "4k",
    "8k",
    "极致细节",
    "完美构图",
    "电影级光影",
    "超高分辨率",
    "精细细节",
];

fn strip_cliche(seg: &str) -> bool {
    let s = seg.trim().to_lowercase();
    CLICHES.iter().any(|c| s == *c || s.starts_with(&format!("{c} ")))
}

/// 去掉逗号分隔的 AI 套话段，保留其余内容。
pub fn clean(text: &str) -> CleanResult {
    let mut kept = Vec::new();
    let mut removed = 0;
    for seg in text.split(&[',', '，', '\n'][..]) {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        if strip_cliche(seg) {
            removed += 1;
        } else {
            kept.push(seg.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }
    CleanResult {
        text: kept.join(", "),
        removed,
    }
}

#[cfg(test)]
#[path = "promptlib_tests.rs"]
mod tests;
