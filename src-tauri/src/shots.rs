//! 镜头（分镜）持久化：一话 = 一组镜头，存在 `projects/<id>/shots.json`。
//!
//! 私密项目暂不支持（shots.json 会是明文）；画面/排版版本引擎还没做，
//! 这里只保证「存下来、能重画、能删」。

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
pub struct Shot {
    pub id: String,
    pub index: u32,
    pub prompt: String,
    pub negative: String,
    pub profile: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f64,
    pub seed: i64,
    pub slg: bool,
    pub slg_layers: String,
    pub layout: Option<String>,
    pub images: Vec<String>,
    pub created_at: u64,
}

impl Default for Shot {
    fn default() -> Self {
        Self {
            id: String::new(),
            index: 0,
            prompt: String::new(),
            negative: String::new(),
            profile: String::new(),
            width: 1024,
            height: 1024,
            steps: 30,
            cfg: 4.0,
            seed: -1,
            slg: false,
            slg_layers: "22".into(),
            layout: None,
            images: Vec::new(),
            created_at: 0,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotDefaults {
    pub profile: String,
    #[serde(default)]
    pub negative: String,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub cfg: f64,
    pub seed: i64,
    #[serde(default)]
    pub slg: bool,
    #[serde(default = "layers")]
    pub slg_layers: String,
    #[serde(default)]
    pub layout: Option<String>,
}

fn layers() -> String {
    "22".into()
}

fn file(cfg: &Path, project: &str) -> PathBuf {
    cfg.join("projects").join(project).join("shots.json")
}

fn load(cfg: &Path, project: &str) -> Result<Vec<Shot>, String> {
    crate::project::load(cfg, project)?;
    if crate::vault::is_private(cfg, project) {
        return Err("私密项目暂不支持镜头".into());
    }
    let p = file(cfg, project);
    if !p.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("解析镜头失败：{e}"))
}

fn save(cfg: &Path, project: &str, shots: &[Shot]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(shots).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&file(cfg, project), &text)
}

pub fn list(cfg: &Path, project: &str) -> Result<Vec<Shot>, String> {
    load(cfg, project)
}

pub fn add_many(
    cfg: &Path,
    project: &str,
    prompts: Vec<String>,
    d: &ShotDefaults,
) -> Result<Vec<Shot>, String> {
    let mut shots = load(cfg, project)?;
    let mut next = shots.iter().map(|s| s.index + 1).max().unwrap_or(1);
    let t = now_ms();
    for prompt in prompts.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        let id = format!("s_{:08x}", rand::random::<u32>());
        shots.push(Shot {
            id,
            index: next,
            prompt,
            negative: d.negative.clone(),
            profile: d.profile.clone(),
            width: d.width,
            height: d.height,
            steps: d.steps,
            cfg: d.cfg,
            seed: d.seed,
            slg: d.slg,
            slg_layers: d.slg_layers.clone(),
            layout: d.layout.clone(),
            images: Vec::new(),
            created_at: t,
        });
        next += 1;
    }
    save(cfg, project, &shots)?;
    Ok(shots)
}

pub fn add_image(cfg: &Path, project: &str, id: &str, path: &str) -> Result<Shot, String> {
    let mut shots = load(cfg, project)?;
    let updated = {
        let s = shots.iter_mut().find(|s| s.id == id).ok_or("镜头不存在")?;
        s.images.push(path.to_string());
        s.clone()
    };
    save(cfg, project, &shots)?;
    Ok(updated)
}

pub fn set_layout(cfg: &Path, project: &str, id: &str, layout: Option<String>) -> Result<Shot, String> {
    let mut shots = load(cfg, project)?;
    let updated = {
        let s = shots.iter_mut().find(|s| s.id == id).ok_or("镜头不存在")?;
        s.layout = layout;
        s.clone()
    };
    save(cfg, project, &shots)?;
    Ok(updated)
}

pub fn delete(cfg: &Path, project: &str, id: &str) -> Result<(), String> {
    let mut shots = load(cfg, project)?;
    shots.retain(|s| s.id != id);
    save(cfg, project, &shots)
}

/// 整话导出：每个镜头取最新一张图，按镜号命名，附 story.md + export.json。
pub fn export_episode(
    cfg: &Path,
    output: &Path,
    project: &str,
    dest: &Path,
) -> Result<PathBuf, String> {
    let shots = load(cfg, project)?;
    if shots.is_empty() {
        return Err("还没有镜头，先存几格".into());
    }
    let p = crate::project::load(cfg, project)?;
    let base = format!(
        "{}-{}-episode",
        crate::custom::safe_name(&p.name),
        now_ms() / 1000
    );
    let mut dir = dest.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = dest.join(format!("{base}-{n}"));
        n += 1;
        if n > 999 {
            return Err("目标目录重名太多".into());
        }
    }
    let img_dir = dir.join("images");
    std::fs::create_dir_all(&img_dir).map_err(|e| e.to_string())?;

    let mut lines = vec![format!("# {} · 分镜导出", p.name), String::new()];
    let mut exported = 0usize;
    let mut missing = 0usize;
    for s in &shots {
        lines.push(format!("## {:02}", s.index));
        lines.push(String::new());
        lines.push(s.prompt.clone());
        lines.push(String::new());
        let Some(src) = s.images.last() else {
            missing += 1;
            lines.push("（还没出图）".into());
            lines.push(String::new());
            continue;
        };
        let path = Path::new(src);
        let fname = path
            .file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| "image.png".into());
        if !path.is_file() {
            missing += 1;
            lines.push("（图片文件不在了）".into());
            lines.push(String::new());
            continue;
        }
        let target = img_dir.join(format!("{:02}_{}", s.index, fname));
        std::fs::copy(path, &target).map_err(|e| format!("复制 {} 失败：{e}", path.display()))?;
        lines.push(format!("![](images/{})", target.file_name().unwrap().to_string_lossy()));
        lines.push(String::new());
        exported += 1;
    }
    crate::fsutil::atomic_write_str(&dir.join("story.md"), &lines.join("\n"))?;
    let manifest = serde_json::json!({
        "kind": "episode",
        "project": p.name,
        "shots": shots.len(),
        "images": exported,
        "missing": missing,
        "createdAt": now_ms() / 1000,
    });
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&dir.join("export.json"), &text)?;
    Ok(dir)
}

#[cfg(test)]
#[path = "shots_tests.rs"]
mod tests;
