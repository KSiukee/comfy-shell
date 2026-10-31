//! 项目：一个文件夹就是一条项目记录，`projects/<id>/project.json`。
//!
//! id 稳定（改名不换目录、不动图）；名字只用于显示。删除先进 `_trash`，
//! 恢复要求目标 id 没被占用，永久删要二次确认（由界面做）。
//!
//! 隐私字段（`private`）现在只是占位，加密仓库在 #17 接。

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_opened_at: u64,
    /// 开工：同一时间最多一个；列表排第一
    pub active: bool,
    /// 只是不显示（明文），锁定/加密是另一件事
    pub hidden: bool,
    /// 占位：锁定区（#17）
    pub private: bool,
    /// 保留：测试项目 `_test`
    pub test: bool,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            schema_version: 1,
            id: String::new(),
            name: String::new(),
            created_at: 0,
            updated_at: 0,
            last_opened_at: 0,
            active: false,
            hidden: false,
            private: false,
            test: false,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashItem {
    pub dir: String,
    pub id: String,
    pub name: String,
    pub deleted_at: u64,
}

fn projects_dir(cfg: &Path) -> PathBuf {
    cfg.join("projects")
}

fn project_dir(cfg: &Path, id: &str) -> PathBuf {
    projects_dir(cfg).join(id)
}

fn project_file(cfg: &Path, id: &str) -> PathBuf {
    project_dir(cfg, id).join("project.json")
}

fn trash_dir(cfg: &Path) -> PathBuf {
    projects_dir(cfg).join("_trash")
}

fn clean_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("项目名不能为空".into());
    }
    if n.chars().count() > 60 {
        return Err("项目名最多 60 个字".into());
    }
    if n.chars().any(|c| c.is_control()) {
        return Err("项目名不能含控制字符".into());
    }
    Ok(n.to_string())
}

/// 只接受单段目录名：挡 `..`、`a/b`、盘符。
fn safe_component(s: &str) -> Result<(), String> {
    if s.is_empty() || s.contains(['\\', '/']) || s == "." || s == ".." {
        return Err("非法的名字".into());
    }
    Ok(())
}

pub(crate) fn load(cfg: &Path, id: &str) -> Result<Project, String> {
    safe_component(id)?;
    let text = std::fs::read_to_string(project_file(cfg, id)).map_err(|e| format!("读取项目失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析项目失败：{e}"))
}

fn save(cfg: &Path, p: &Project) -> Result<(), String> {
    let text = serde_json::to_string_pretty(p).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&project_file(cfg, &p.id), &text)
}

pub fn list(cfg: &Path, include_hidden: bool) -> Vec<Project> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(projects_dir(cfg)) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let Some(id) = p.file_name().and_then(|x| x.to_str()) else {
            continue;
        };
        if id == "_trash" {
            continue;
        }
        if let Ok(proj) = load(cfg, id) {
            if include_hidden || !proj.hidden {
                out.push(proj);
            }
        }
    }
    out.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.last_opened_at.cmp(&a.last_opened_at))
            .then(a.name.cmp(&b.name))
    });
    out
}

pub fn create(cfg: &Path, name: &str) -> Result<Project, String> {
    let name = clean_name(name)?;
    std::fs::create_dir_all(projects_dir(cfg)).map_err(|e| e.to_string())?;
    let mut id = String::new();
    for _ in 0..32 {
        let candidate = format!("p_{:08x}", rand::random::<u32>());
        if !project_dir(cfg, &candidate).exists() {
            id = candidate;
            break;
        }
    }
    if id.is_empty() {
        return Err("生成项目 id 失败".into());
    }
    let t = now_ms();
    let p = Project {
        id: id.clone(),
        name,
        created_at: t,
        updated_at: t,
        last_opened_at: t,
        active: false,
        ..Default::default()
    };
    std::fs::create_dir_all(project_dir(cfg, &id)).map_err(|e| e.to_string())?;
    save(cfg, &p)?;
    Ok(p)
}

/// 调试构建启动时保证有 `_test` 项目（调试生成只允许落在这里）。
pub fn ensure_test(cfg: &Path) -> Result<(), String> {
    if project_file(cfg, "_test").is_file() {
        return Ok(());
    }
    std::fs::create_dir_all(project_dir(cfg, "_test")).map_err(|e| e.to_string())?;
    let t = now_ms();
    let p = Project {
        id: "_test".into(),
        name: "测试项目".into(),
        created_at: t,
        updated_at: t,
        last_opened_at: t,
        test: true,
        ..Default::default()
    };
    save(cfg, &p)
}

pub fn rename(cfg: &Path, id: &str, name: &str) -> Result<Project, String> {
    let name = clean_name(name)?;
    let mut p = load(cfg, id)?;
    p.name = name;
    p.updated_at = now_ms();
    save(cfg, &p)?;
    Ok(p)
}

pub fn set_hidden(cfg: &Path, id: &str, hidden: bool) -> Result<Project, String> {
    let mut p = load(cfg, id)?;
    p.hidden = hidden;
    p.updated_at = now_ms();
    save(cfg, &p)?;
    Ok(p)
}

/// `id=None` 表示回到「临时」（不开工任何项目）。
pub fn set_active(cfg: &Path, id: Option<&str>) -> Result<Vec<Project>, String> {
    let mut all = list(cfg, true);
    if let Some(want) = id {
        if !all.iter().any(|p| p.id == want) {
            return Err("项目不存在".into());
        }
    }
    let t = now_ms();
    for p in all.iter_mut() {
        let active = id == Some(p.id.as_str());
        if active && !p.active {
            p.last_opened_at = t;
        }
        if p.active != active {
            p.active = active;
            p.updated_at = t;
            save(cfg, p)?;
        }
    }
    Ok(list(cfg, false))
}

pub fn delete(cfg: &Path, id: &str, output: &Path) -> Result<(), String> {
    let p = load(cfg, id)?;
    safe_component(id)?;
    let dir = project_dir(cfg, id);
    let dest = trash_dir(cfg).join(format!("{}-{}", id, now_ms()));
    std::fs::create_dir_all(trash_dir(cfg)).map_err(|e| e.to_string())?;
    std::fs::rename(&dir, &dest).map_err(|e| format!("移到回收站失败：{e}"))?;
    let meta = json!({"id": p.id, "name": p.name, "deletedAt": now_ms()});
    let _ = crate::fsutil::atomic_write_str(&dest.join("meta.json"), &meta.to_string());
    // 图片跟着项目一起进回收站；搬不动就如实说（配置已在回收站）。
    let images = output.join("projects").join(id);
    if images.is_dir() {
        if let Err(e) = crate::fsutil::move_dir(&images, &dest.join("images")) {
            return Err(format!("项目已进回收站，但图片没搬过去：{e}"));
        }
    }
    Ok(())
}

pub fn trash(cfg: &Path) -> Vec<TrashItem> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(trash_dir(cfg)) else {
        return out;
    };
    for e in rd.flatten() {
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(name) = dir.file_name().and_then(|x| x.to_str()) else {
            continue;
        };
        let meta: serde_json::Value = std::fs::read_to_string(dir.join("meta.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| json!({}));
        let id = meta.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let pname = meta.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let deleted_at = meta.get("deletedAt").and_then(|x| x.as_u64()).unwrap_or(0);
        out.push(TrashItem {
            dir: name.to_string(),
            id,
            name: pname,
            deleted_at,
        });
    }
    out.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
    out
}

pub fn restore(cfg: &Path, dir_name: &str, output: &Path) -> Result<Project, String> {
    safe_component(dir_name)?;
    let src = trash_dir(cfg).join(dir_name);
    if !src.is_dir() {
        return Err("回收站里没有这一项".into());
    }
    let meta: serde_json::Value = std::fs::read_to_string(src.join("meta.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}));
    let id = meta
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or("这一项缺 meta，无法恢复")?
        .to_string();
    safe_component(&id)?;
    let dest = project_dir(cfg, &id);
    if dest.exists() {
        return Err("同 id 的项目已存在，先改名或删掉那个".into());
    }
    std::fs::rename(&src, &dest).map_err(|e| format!("恢复失败：{e}"))?;
    let _ = std::fs::remove_file(dest.join("meta.json"));
    // 图片搬回 output/projects/<id>（同 id 目录已存在就把文件并进去）
    let images = dest.join("images");
    if images.is_dir() {
        let target = output.join("projects").join(&id);
        if target.exists() {
            // 同 id 目录还在：把文件逐个并进去（跨盘也能走复制+删）
            if let Ok(rd) = std::fs::read_dir(&images) {
                for e in rd.flatten() {
                    let from = e.path();
                    if from.is_file() {
                        if let Some(name) = from.file_name() {
                            let _ = crate::fsutil::move_file(&from, &target.join(name));
                        }
                    }
                }
            }
            let _ = std::fs::remove_dir(&images);
        } else {
            crate::fsutil::move_dir(&images, &target)?;
        }
    }
    let mut p = load(cfg, &id)?;
    // 恢复不当场开工：否则旧项目带着 active 回来，会和现在的开工项目打架。
    if p.active {
        p.active = false;
        p.updated_at = now_ms();
        save(cfg, &p)?;
    }
    Ok(p)
}

pub fn purge(cfg: &Path, dir_name: &str) -> Result<(), String> {
    safe_component(dir_name)?;
    let dir = trash_dir(cfg).join(dir_name);
    if !dir.is_dir() {
        return Err("回收站里没有这一项".into());
    }
    std::fs::remove_dir_all(dir).map_err(|e| format!("永久删除失败：{e}"))
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
