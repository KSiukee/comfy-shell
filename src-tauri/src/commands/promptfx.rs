//! 提示词风格包 / 通配符（#26）：`<配置目录>/styles/*.json` 与 `styles/wildcards/*.txt`。
//!
//! 只读为主——用户在资源管理器里手写，这里负责读出来、以及首次打开时补一份示例。
//! 一个坏文件只跳过它自己，不让整列消失。

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub name: String,
    /// 中文显示名（可以为空）：上游风格包全是英文名，查表补上；提示词正文不动。
    pub zh: String,
    pub positive: String,
    pub negative: String,
}

/// 上游（sdxl_prompt_styler / Fooocus）风格名的中文对照，随包内置。
fn zh_map() -> &'static BTreeMap<String, String> {
    static MAP: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| serde_json::from_str(include_str!("style_zh.json")).unwrap_or_default())
}

fn zh_for(name: &str) -> String {
    zh_map()
        .get(&name.trim().to_lowercase())
        .cloned()
        .unwrap_or_default()
}

pub fn styles_dir(cfg: &Path) -> PathBuf {
    cfg.join("styles")
}

pub fn wildcards_dir(cfg: &Path) -> PathBuf {
    styles_dir(cfg).join("wildcards")
}

fn is_ext(path: &Path, want: &str) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .map(|x| x.eq_ignore_ascii_case(want))
        .unwrap_or(false)
}

/// 一条风格：同时认我们的 `positive/negative` 和 SDXL/Fooocus 的 `prompt/negative_prompt`。
fn style_from_value(v: &serde_json::Value, fallback: &str) -> Option<Style> {
    let positive = v
        .get("positive")
        .or_else(|| v.get("prompt"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if positive.is_empty() {
        return None;
    }
    let negative = v
        .get("negative")
        .or_else(|| v.get("negative_prompt"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string());
    let zh = v
        .get("zh")
        .or_else(|| v.get("name_zh"))
        .or_else(|| v.get("label"))
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| zh_for(&name));
    Some(Style { name, zh, positive, negative })
}

fn styles_in_value(v: &serde_json::Value, fallback: &str) -> Vec<Style> {
    let mut out = Vec::new();
    if let Some(arr) = v.as_array() {
        for it in arr {
            if let Some(s) = style_from_value(it, fallback) {
                out.push(s);
            }
        }
    } else if let Some(arr) = v.get("styles").and_then(|x| x.as_array()) {
        for it in arr {
            if let Some(s) = style_from_value(it, fallback) {
                out.push(s);
            }
        }
    } else if let Some(s) = style_from_value(v, fallback) {
        out.push(s);
    }
    out
}

/// 目录里的风格包；支持「一个文件一条」「一整个数组」（sdxl_styles/Fooocus 那种）。
/// `positive`/`prompt` 必须非空；同名只留第一条。
pub fn read_styles(dir: &Path) -> Vec<Style> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let p = e.path();
        if !is_ext(&p, "json") {
            continue;
        }
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let stem = p
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("style")
            .to_string();
        for s in styles_in_value(&v, &stem) {
            if !out.iter().any(|x: &Style| x.name == s.name) {
                out.push(s);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// 通配符：一行一个，`#` 开头与空行跳过；文件名（去 .txt）就是 `__名字__`。
pub fn read_wildcards(dir: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let p = e.path();
        if !is_ext(&p, "txt") {
            continue;
        }
        let name = match p.file_stem().and_then(|x| x.to_str()) {
            Some(n) => n.trim().to_string(),
            None => continue,
        };
        if name.is_empty() {
            continue;
        }
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let lines: Vec<String> = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect();
        if !lines.is_empty() {
            out.insert(name, lines);
        }
    }
    out
}

fn seed_file(dir: &Path, name: &str, body: &str) -> Result<(), String> {
    let p = dir.join(name);
    if p.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(p, body).map_err(|e| e.to_string())
}

/// 建目录 + 补示例（已存在的不覆盖），返回目录路径给界面「打开文件夹」。
#[tauri::command]
pub fn style_pack_open(app: tauri::AppHandle) -> Result<String, String> {
    let dir = styles_dir(&crate::cfg_dir(&app));
    let wc = wildcards_dir(&crate::cfg_dir(&app));
    std::fs::create_dir_all(&wc).map_err(|e| e.to_string())?;
    seed_file(
        &dir,
        "README.md",
        "# 提示词风格包 / 通配符\n\n\
         ## 风格包（*.json）\n\
         ```json\n\
         {\n  \"name\": \"电影感\",\n  \"positive\": \"cinematic photo, {prompt}, film grain, shallow depth of field\",\n  \"negative\": \"cartoon, text, watermark\"\n}\n\
         ```\n\
         - `positive` 必填，`{prompt}` 会换成你写的提示词；没有占位符就直接拼在后面。\n\
         - `negative` 可选，会追加到反向提示词。\n\
         - 文件名随便取；`name` 空着就用文件名。\n\n\
         ## 通配符（wildcards/*.txt）\n\
         - 一行一个候选，`#` 开头是注释。\n\
         - 文件名（去掉 .txt）就是提示词里的 `__名字__`，例如 `lighting.txt` → `__lighting__`。\n\
         - 种子固定时同一处通配符每次都选同一条（可复现）；种子为 -1 时每次随机。\n\n\
         ## 数组\n\
         - 提示词里写 `[[白天,黄昏,夜晚]]`，每次生成会按每个候选各出一张（最多 8 组）。\n",
    )?;
    seed_file(
        &dir,
        "example.json",
        "{\n  \"name\": \"电影感\",\n  \"zh\": \"电影感\",\n  \"positive\": \"cinematic photo, {prompt}, film grain, shallow depth of field\",\n  \"negative\": \"cartoon, text, watermark\"\n}\n",
    )?;
    seed_file(
        &wc,
        "lighting.txt",
        "# 一行一个候选\nsoft window light\nneon rim light\ngolden hour backlight\nmoody single lamp\n",
    )?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn style_list(app: tauri::AppHandle) -> Vec<Style> {
    read_styles(&styles_dir(&crate::cfg_dir(&app)))
}

fn safe_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let s = s.trim().trim_end_matches('.').trim().to_string();
    let s = if s.is_empty() { "imported".to_string() } else { s };
    s.chars().take(60).collect()
}

/// 导入一份风格包（数组、`{styles:[…]}` 或单条；认 prompt/negative_prompt，也认 positive/negative），
/// 统一存成我们的规范数组。同名文件自动加 `-2`/`-3`，绝不覆盖。
pub fn import_styles_into(dir: &Path, name: &str, json_text: &str) -> Result<(usize, PathBuf), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let v: serde_json::Value =
        serde_json::from_str(json_text).map_err(|e| format!("不是合法 JSON：{e}"))?;
    let stem = safe_stem(name);
    let styles = styles_in_value(&v, &stem);
    if styles.is_empty() {
        return Err("这份文件里没有可用的风格（每条要有 prompt 或 positive）".into());
    }
    let body: Vec<serde_json::Value> = styles
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "zh": s.zh,
                "positive": s.positive,
                "negative": s.negative
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
    let mut path = dir.join(format!("{stem}.json"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.json"));
        n += 1;
    }
    crate::fsutil::atomic_write_str(&path, &text)?;
    Ok((styles.len(), path))
}

#[tauri::command]
pub fn style_import(app: tauri::AppHandle, name: String, json_text: String) -> Result<usize, String> {
    let dir = styles_dir(&crate::cfg_dir(&app));
    let (n, _path) = import_styles_into(&dir, &name, &json_text)?;
    Ok(n)
}

/// 把某个风格从它所在的文件里删掉；删空了就连文件一起删。
/// 文件按我们的规范数组重写（name/zh/positive/negative），其余条目原样保留。
pub fn delete_style_in(dir: &Path, name: &str) -> Result<bool, String> {
    let target = name.trim();
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => return Err(e.to_string()),
    };
    for e in rd.flatten() {
        let p = e.path();
        if !is_ext(&p, "json") {
            continue;
        }
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let stem = p.file_stem().and_then(|x| x.to_str()).unwrap_or("style").to_string();
        let styles = styles_in_value(&v, &stem);
        if !styles.iter().any(|s| s.name == target) {
            continue;
        }
        let remain: Vec<Style> = styles.into_iter().filter(|s| s.name != target).collect();
        if remain.is_empty() {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
        } else {
            let body: Vec<serde_json::Value> = remain
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "zh": s.zh,
                        "positive": s.positive,
                        "negative": s.negative
                    })
                })
                .collect();
            let out = serde_json::to_string_pretty(&body).map_err(|e| e.to_string())?;
            crate::fsutil::atomic_write_str(&p, &out)?;
        }
        return Ok(true);
    }
    Ok(false)
}

#[tauri::command]
pub fn style_delete(app: tauri::AppHandle, name: String) -> Result<bool, String> {
    let dir = styles_dir(&crate::cfg_dir(&app));
    delete_style_in(&dir, &name)
}

#[tauri::command]
pub fn wildcard_map(app: tauri::AppHandle) -> BTreeMap<String, Vec<String>> {
    read_wildcards(&wildcards_dir(&crate::cfg_dir(&app)))
}

#[cfg(test)]
#[path = "promptfx_tests.rs"]
mod tests;
