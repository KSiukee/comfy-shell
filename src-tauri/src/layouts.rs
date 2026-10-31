//! User-saved templates for the MOSS layout editor.
//!
//! One JSON file per template under `<config>/layouts/<name>.json`. The file is
//! exactly the array the editor keeps in memory:
//!   [{"type":"text","desc":"headline","text":"SALE","bbox":[x,y,w,h]}, ...]
//! `bbox` is normalized 0..1 relative to the canvas, so a template is
//! resolution independent (the editor converts it to MOSS yxyx at generate time).

use serde_json::Value;
use std::path::{Path, PathBuf};

fn dir(cfg: &Path) -> PathBuf {
    cfg.join("layouts")
}

fn file(cfg: &Path, name: &str) -> PathBuf {
    dir(cfg).join(format!("{}.json", crate::custom::safe_name(name)))
}

pub fn list(cfg: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir(cfg)) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                if let Some(stem) = p.file_stem().and_then(|x| x.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// Validate that `v` is a layout item array the editor can render.
fn check_items(v: &Value) -> Result<(), String> {
    let arr = v.as_array().ok_or("排版模板必须是元素数组")?;
    if arr.is_empty() {
        return Err("模板里还没有元素".into());
    }
    if arr.len() > 64 {
        return Err("模板元素超过 64 个".into());
    }
    for (i, it) in arr.iter().enumerate() {
        let obj = it.as_object().ok_or_else(|| format!("第 {} 个元素不是对象", i + 1))?;
        let kind = obj.get("type").and_then(|x| x.as_str()).unwrap_or("");
        if kind != "text" && kind != "object" {
            return Err(format!("第 {} 个元素的 type 必须是 text 或 object", i + 1));
        }
        if obj.get("desc").and_then(|x| x.as_str()).is_none() {
            return Err(format!("第 {} 个元素缺 desc 字符串", i + 1));
        }
        if kind == "text" && obj.get("text").and_then(|x| x.as_str()).is_none() {
            return Err(format!("第 {} 个文字元素缺 text 字符串", i + 1));
        }
        let bbox = obj
            .get("bbox")
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("第 {} 个元素缺 bbox 数组", i + 1))?;
        if bbox.len() != 4 {
            return Err(format!("第 {} 个元素的 bbox 要 4 个数（x,y,w,h）", i + 1));
        }
        for n in bbox {
            let f = n.as_f64().ok_or_else(|| format!("第 {} 个元素 bbox 不是数字", i + 1))?;
            if !(0.0..=1.0).contains(&f) {
                return Err(format!("第 {} 个元素 bbox 超出 0..1", i + 1));
            }
        }
        if bbox[2].as_f64().unwrap_or(0.0) <= 0.0 || bbox[3].as_f64().unwrap_or(0.0) <= 0.0 {
            return Err(format!("第 {} 个元素宽或高为 0", i + 1));
        }
    }
    Ok(())
}

pub fn load(cfg: &Path, name: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(file(cfg, name))
        .map_err(|e| format!("读取模板失败：{e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("解析模板失败：{e}"))?;
    check_items(&v)?;
    Ok(v)
}

pub fn save(cfg: &Path, name: &str, items_json: &str) -> Result<String, String> {
    let name = crate::custom::safe_name(name);
    if name.is_empty() {
        return Err("模板名称不能为空".into());
    }
    let v: Value = serde_json::from_str(items_json).map_err(|e| format!("不是合法 JSON：{e}"))?;
    check_items(&v)?;
    std::fs::create_dir_all(dir(cfg)).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    std::fs::write(file(cfg, &name), text).map_err(|e| e.to_string())?;
    Ok(name)
}

pub fn delete(cfg: &Path, name: &str) -> Result<(), String> {
    let p = file(cfg, name);
    if !p.is_file() {
        return Err("模板不存在".into());
    }
    std::fs::remove_file(p).map_err(|e| e.to_string())
}
