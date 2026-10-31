//! Custom ComfyUI API-format workflows imported by the user.
//!
//! Stored as plain JSON files under `<config>/workflows/`. Placeholders that may
//! appear anywhere in the JSON are substituted at run time:
//!   {{prompt}} {{negative}} {{seed}} {{width}} {{height}} {{steps}} {{cfg}}

use serde_json::Value;
use std::path::{Path, PathBuf};

fn dir(cfg: &Path) -> PathBuf {
    cfg.join("workflows")
}

fn safe_name(name: &str) -> String {
    // Keep CJK and spaces; only strip what Windows forbids in file names.
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(60)
        .collect();
    cleaned.trim().trim_end_matches('.').to_string()
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

pub fn save(cfg: &Path, name: &str, json_text: &str) -> Result<String, String> {
    let name = safe_name(name);
    if name.is_empty() {
        return Err("工作流名称不能为空".into());
    }
    let v: Value = serde_json::from_str(json_text).map_err(|e| format!("不是合法 JSON：{e}"))?;
    let obj = v.as_object().ok_or("工作流必须是节点映射（API 格式）")?;
    let looks_like_prompt = obj.values().all(|n| {
        n.get("class_type").and_then(|c| c.as_str()).is_some() && n.get("inputs").is_some()
    });
    if obj.is_empty() || !looks_like_prompt {
        return Err("这不像是 ComfyUI 的 API 格式工作流（每个节点应有 class_type 与 inputs）".into());
    }
    std::fs::create_dir_all(dir(cfg)).map_err(|e| e.to_string())?;
    std::fs::write(dir(cfg).join(format!("{name}.json")), json_text).map_err(|e| e.to_string())?;
    Ok(name)
}

pub fn load(cfg: &Path, name: &str) -> Result<Value, String> {
    let p = dir(cfg).join(format!("{}.json", safe_name(name)));
    let text = std::fs::read_to_string(&p).map_err(|e| format!("读取工作流失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析工作流失败：{e}"))
}

pub fn delete(cfg: &Path, name: &str) -> Result<(), String> {
    let p = dir(cfg).join(format!("{}.json", safe_name(name)));
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

/// Replace `{{token}}` placeholders in the serialized graph.
pub fn substitute(
    graph: &Value,
    prompt: &str,
    negative: &str,
    seed: i64,
    width: u32,
    height: u32,
    steps: u32,
    cfg: f64,
) -> Value {
    let raw = serde_json::to_string(graph).unwrap_or_default();
    let escaped = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    let prompt_json = escaped(prompt);
    let negative_json = escaped(negative);
    // values are JSON-escaped strings without the surrounding quotes where numeric
    let text = raw
        .replace("{{prompt}}", prompt_json.trim_matches('"'))
        .replace("{{negative}}", negative_json.trim_matches('"'))
        .replace("{{seed}}", &seed.to_string())
        .replace("{{width}}", &width.to_string())
        .replace("{{height}}", &height.to_string())
        .replace("{{steps}}", &steps.to_string())
        .replace("{{cfg}}", &cfg.to_string());
    serde_json::from_str(&text).unwrap_or_else(|_| graph.clone())
}
