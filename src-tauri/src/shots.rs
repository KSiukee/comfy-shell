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

/// 一镜最多挂几个角色（和 characters.js 的「最多挂 3 张底板」同一口径）。
pub const MAX_CAST: usize = 3;

/// 出图那一刻真正用到的角色：名字 + 当时那张身份底板的路径。
///
/// 为什么要把「当时用哪张底板」也存下来（#33 整话一致性体检）：
/// 同一角色换了底板（重出过底板）而前后镜头没跟上，两格画出来就是两个人；
/// 只比对底板路径是**机械可查**的证据，不靠肉眼。
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CastRef {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// 本次出图挂的身份底板（没有底板 = 只注入了文字）
    #[serde(default)]
    pub plate: Option<String>,
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
    /// 这一镜挂了哪些角色卡（存 id；用户意图）
    pub characters: Vec<String>,
    /// 上一次出图实际用到的角色与底板（存证据）
    pub cast: Vec<CastRef>,
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
            characters: Vec::new(),
            cast: Vec::new(),
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
            characters: Vec::new(),
            cast: Vec::new(),
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

/// 去空、去重、截到 MAX_CAST——顺序按用户点选的先后保留。
fn clean_ids(ids: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if id.is_empty() || out.iter().any(|x| x == &id) {
            continue;
        }
        if out.len() >= MAX_CAST {
            break;
        }
        out.push(id);
    }
    out
}

/// 给一镜挂角色卡（存 id，不校验存在性：角色删了要能被体检报出来，而不是悄悄抹掉）。
pub fn set_characters(cfg: &Path, project: &str, id: &str, ids: Vec<String>) -> Result<Shot, String> {
    let mut shots = load(cfg, project)?;
    let updated = {
        let s = shots.iter_mut().find(|s| s.id == id).ok_or("镜头不存在")?;
        s.characters = clean_ids(ids);
        s.clone()
    };
    save(cfg, project, &shots)?;
    Ok(updated)
}

/// 记下这次出图实际用到的角色与底板（体检据此判「跨格是不是同一张底板」）。
pub fn set_cast(cfg: &Path, project: &str, id: &str, cast: Vec<CastRef>) -> Result<Shot, String> {
    let mut shots = load(cfg, project)?;
    let updated = {
        let s = shots.iter_mut().find(|s| s.id == id).ok_or("镜头不存在")?;
        let mut out: Vec<CastRef> = Vec::new();
        for c in cast {
            let cid = c.id.trim().to_string();
            if cid.is_empty() || out.iter().any(|x| x.id == cid) {
                continue;
            }
            if out.len() >= MAX_CAST {
                break;
            }
            out.push(CastRef {
                id: cid,
                name: c.name.trim().to_string(),
                plate: c.plate.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()),
            });
        }
        s.cast = out;
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
    let mut cast_shots = 0usize;
    for s in &shots {
        lines.push(format!("## {:02}", s.index));
        lines.push(String::new());
        let names: Vec<String> = s
            .cast
            .iter()
            .map(|c| c.name.clone())
            .filter(|n| !n.is_empty())
            .collect();
        if !names.is_empty() {
            cast_shots += 1;
            lines.push(format!("角色：{}", names.join("、")));
            lines.push(String::new());
        }
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
        "castShots": cast_shots,
        "createdAt": now_ms() / 1000,
    });
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&dir.join("export.json"), &text)?;
    Ok(dir)
}

#[cfg(test)]
#[path = "shots_tests.rs"]
mod tests;
