//! 白模姿势库：多 pack（Shisei 旧目录 + `input/pose_packs/<id>`）。
//! catalog 里的图片名统一改写成相对 `ComfyUI/input` 的路径，前端直接交给 read_input_image。

use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::State;

use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PosePack {
    pub id: String,
    pub name: String,
    pub count: usize,
    pub legacy: bool,
    pub license: String,
    pub publishable: bool,
    pub note: String,
}

fn input_root(state: &State<AppState>) -> PathBuf {
    let root = state.settings.lock().unwrap().comfy_root.clone();
    PathBuf::from(root).join("ComfyUI").join("input")
}

/// 返回 (id, 目录, 是否旧版 shisei)。
pub fn pack_dirs(input: &Path) -> Vec<(String, PathBuf, bool)> {
    let mut out = Vec::new();
    let packs = input.join("pose_packs");
    if let Ok(rd) = std::fs::read_dir(&packs) {
        for e in rd.flatten() {
            let dir = e.path();
            if dir.join("catalog.json").is_file() {
                let id = e.file_name().to_string_lossy().to_string();
                out.push((id, dir, false));
            }
        }
    }
    let legacy = input.join("shisei");
    if legacy.join("catalog.json").is_file() && !out.iter().any(|(id, _, _)| id == "shisei") {
        out.push(("shisei".into(), legacy, true));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

struct PackMeta {
    name: String,
    license: String,
    publishable: bool,
    note: String,
}

/// 读 pack.json 的授权信息；旧 shisei 默认 CC0 可发布，未知 pack 默认保守不可发布。
pub fn pack_meta(dir: &Path, id: &str, legacy: bool) -> PackMeta {
    let v = std::fs::read_to_string(dir.join("pack.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .unwrap_or(Value::Null);
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| id.to_string());
    let license = v
        .get("license")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if legacy {
                "CC0（图片）".to_string()
            } else {
                "未标注".to_string()
            }
        });
    let publishable = v
        .get("publishable")
        .and_then(|n| n.as_bool())
        .unwrap_or(legacy);
    let note = v
        .get("note")
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    PackMeta {
        name,
        license,
        publishable,
        note,
    }
}

fn image_subdir(dir: &Path) -> &'static str {
    if dir.join("line").is_dir() {
        "line"
    } else if dir.join("images").is_dir() {
        "images"
    } else {
        ""
    }
}

pub fn load_catalog(
    input: &Path,
    id: &str,
    dir: &Path,
    legacy: bool,
) -> Result<Vec<Value>, String> {
    let text = std::fs::read_to_string(dir.join("catalog.json"))
        .map_err(|e| format!("找不到姿势库（{}）：{e}", dir.display()))?;
    let mut list: Vec<Value> =
        serde_json::from_str(&text).map_err(|e| format!("姿势库 catalog 解析失败：{e}"))?;
    let sub = image_subdir(dir);
    let prefix = if legacy {
        "shisei".to_string()
    } else {
        format!("pose_packs/{id}")
    };
    for pose in &mut list {
        if let Some(views) = pose.get_mut("views").and_then(|v| v.as_object_mut()) {
            let mut kept = serde_json::Map::new();
            for (k, v) in views.iter() {
                let name = v.as_str().unwrap_or("").to_string();
                if name.is_empty() {
                    continue;
                }
                let rel = if name.contains('/') {
                    name.clone()
                } else if sub.is_empty() {
                    format!("{prefix}/{name}")
                } else {
                    format!("{prefix}/{sub}/{name}")
                };
                if input.join(&rel).is_file() {
                    kept.insert(k.clone(), json!(rel));
                }
            }
            *views = kept;
        }
    }
    list.retain(|p| {
        p.get("views")
            .and_then(|v| v.as_object())
            .map(|o| !o.is_empty())
            .unwrap_or(false)
    });
    Ok(list)
}

#[tauri::command]
pub fn pose_pack_list(state: State<AppState>) -> Result<Vec<PosePack>, String> {
    let input = input_root(&state);
    let mut out = Vec::new();
    for (id, dir, legacy) in pack_dirs(&input) {
        let Ok(list) = load_catalog(&input, &id, &dir, legacy) else {
            continue;
        };
        if list.is_empty() {
            continue;
        }
        let meta = pack_meta(&dir, &id, legacy);
        out.push(PosePack {
            name: meta.name,
            count: list.len(),
            id,
            legacy,
            license: meta.license,
            publishable: meta.publishable,
            note: meta.note,
        });
    }
    if out.is_empty() {
        return Err("没有可用的姿势库（input/shisei 或 input/pose_packs/*）".into());
    }
    Ok(out)
}

#[tauri::command]
pub fn poses_catalog(state: State<AppState>, pack: Option<String>) -> Result<String, String> {
    let input = input_root(&state);
    let dirs = pack_dirs(&input);
    let wanted = pack.unwrap_or_default();
    let (id, dir, legacy) = if wanted.is_empty() {
        dirs.into_iter().next().ok_or("没有可用的姿势库")?
    } else {
        dirs.into_iter()
            .find(|(pid, _, _)| pid == &wanted)
            .ok_or_else(|| format!("姿势库不存在：{wanted}"))?
    };
    let list = load_catalog(&input, &id, &dir, legacy)?;
    serde_json::to_string(&list).map_err(|e| e.to_string())
}

/// 供本地小助手搜姿势（跨 pack，最多 3 条）。
pub fn search(input: &Path, query: &str) -> Vec<(String, String)> {
    let q = query.to_lowercase();
    let mut hits = Vec::new();
    for (id, dir, legacy) in pack_dirs(input) {
        let Ok(list) = load_catalog(input, &id, &dir, legacy) else {
            continue;
        };
        for p in &list {
            let hay = format!(
                "{} {} {} {} {}",
                p.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                p.get("name_cn").and_then(|v| v.as_str()).unwrap_or(""),
                p.get("name_en").and_then(|v| v.as_str()).unwrap_or(""),
                p.get("class_cn").and_then(|v| v.as_str()).unwrap_or(""),
                p.get("desc_en").and_then(|v| v.as_str()).unwrap_or("")
            )
            .to_lowercase();
            if q.is_empty() || hay.contains(&q) {
                hits.push((
                    p.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    p.get("desc_en").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                ));
                if hits.len() >= 3 {
                    return hits;
                }
            }
        }
    }
    hits
}

#[cfg(test)]
#[path = "pose_tests.rs"]
mod tests;
