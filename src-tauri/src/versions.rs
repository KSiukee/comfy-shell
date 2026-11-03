//! 项目内的版本谱系（提示词/参数/排版）。
//!
//! 一次生成追加一个版本节点，`parent` 指向当时的最新版本；默认 1 星，
//! 可改 1–5 星；界面只和父版本做 diff。私密项目不记录（避免明文）。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Version {
    pub id: String,
    pub parent: Option<String>,
    pub created_at: u64,
    pub prompt: String,
    pub negative: String,
    pub profile: String,
    pub mode: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f64,
    #[serde(serialize_with = "crate::api::seed_str", deserialize_with = "crate::api::de_seed")]
    pub seed: i64,
    pub stars: u8,
    pub layout: Option<String>,
    pub images: Vec<String>,
    pub history_id: String,
    #[serde(default = "image_kind")]
    pub kind: String,
    #[serde(default)]
    pub model_family: String,
}

impl Default for Version {
    fn default() -> Self {
        Self {
            id: String::new(),
            parent: None,
            created_at: 0,
            prompt: String::new(),
            negative: String::new(),
            profile: String::new(),
            mode: String::new(),
            width: 0,
            height: 0,
            steps: 0,
            cfg: 0.0,
            seed: -1,
            stars: 1,
            layout: None,
            images: Vec::new(),
            history_id: String::new(),
            kind: "image".into(),
            model_family: String::new(),
        }
    }
}

pub struct VersionSeed {
    pub prompt: String,
    pub negative: String,
    pub profile: String,
    pub mode: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f64,
    pub seed: i64,
    pub layout: Option<String>,
}

fn file(cfg: &Path, project: &str) -> PathBuf {
    cfg.join("projects").join(project).join("versions.json")
}

fn load(cfg: &Path, project: &str) -> Result<Vec<Version>, String> {
    crate::project::load(cfg, project)?;
    let p = file(cfg, project);
    if !p.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("解析版本失败：{e}"))
}

fn save(cfg: &Path, project: &str, versions: &[Version]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(versions).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&file(cfg, project), &text)
}

pub fn list(cfg: &Path, project: &str) -> Result<Vec<Version>, String> {
    load(cfg, project)
}

pub fn append(
    cfg: &Path,
    project: &str,
    seed: &VersionSeed,
    images: &[String],
    history_id: &str,
    branch_from: Option<&str>,
) -> Result<Version, String> {
    if crate::vault::is_private(cfg, project) {
        return Err("私密项目不记录版本".into());
    }
    let mut list = load(cfg, project)?;
    let parent = branch_from
        .filter(|id| list.iter().any(|v| v.id == *id))
        .map(|id| id.to_string())
        .or_else(|| list.last().map(|v| v.id.clone()));
    let v = Version {
        id: format!("v_{:08x}", rand::random::<u32>()),
        parent,
        created_at: now_ms(),
        prompt: seed.prompt.clone(),
        negative: seed.negative.clone(),
        profile: seed.profile.clone(),
        mode: seed.mode.clone(),
        width: seed.width,
        height: seed.height,
        steps: seed.steps,
        cfg: seed.cfg,
        seed: seed.seed,
        stars: 1,
        layout: seed.layout.clone(),
        images: images.to_vec(),
        history_id: history_id.to_string(),
        kind: "image".into(),
        model_family: String::new(),
    };
    list.push(v.clone());
    save(cfg, project, &list)?;
    Ok(v)
}

fn image_kind() -> String {
    "image".into()
}

/// 把当前排版存成一个 `kind=layout` 的版本节点（父 = 上一条排版版本）。
pub fn save_layout(
    cfg: &Path,
    project: &str,
    name: &str,
    items: &str,
) -> Result<Version, String> {
    if crate::vault::is_private(cfg, project) {
        return Err("私密项目不记录排版版本".into());
    }
    if items.trim().is_empty() {
        return Err("排版是空的".into());
    }
    let mut list = load(cfg, project)?;
    let parent = list
        .iter()
        .rev()
        .find(|v| v.kind == "layout")
        .map(|v| v.id.clone());
    let v = Version {
        id: format!("v_{:08x}", rand::random::<u32>()),
        parent,
        created_at: now_ms(),
        prompt: name.trim().to_string(),
        negative: String::new(),
        profile: String::new(),
        mode: "layout".into(),
        width: 0,
        height: 0,
        steps: 0,
        cfg: 0.0,
        seed: -1,
        stars: 1,
        layout: Some(items.to_string()),
        images: Vec::new(),
        history_id: String::new(),
        kind: "layout".into(),
        model_family: "moss".into(),
    };
    list.push(v.clone());
    save(cfg, project, &list)?;
    Ok(v)
}

pub fn star(cfg: &Path, project: &str, id: &str, stars: u8) -> Result<Version, String> {
    let mut list = load(cfg, project)?;
    let updated = {
        let v = list.iter_mut().find(|v| v.id == id).ok_or("版本不存在")?;
        v.stars = stars.clamp(1, 5);
        v.clone()
    };
    save(cfg, project, &list)?;
    Ok(updated)
}

pub fn delete(cfg: &Path, project: &str, id: &str) -> Result<(), String> {
    let mut list = load(cfg, project)?;
    list.retain(|v| v.id != id);
    save(cfg, project, &list)
}

#[cfg(test)]
#[path = "versions_tests.rs"]
mod tests;
