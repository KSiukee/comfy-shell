//! 提示词风格包 / 通配符（#26）：`<配置目录>/styles/*.json` 与 `styles/wildcards/*.txt`。
//!
//! 只读为主——用户在资源管理器里手写，这里负责读出来、以及首次打开时补一份示例。
//! 一个坏文件只跳过它自己，不让整列消失。

use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    pub name: String,
    pub positive: String,
    pub negative: String,
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

/// 目录里的风格包；`positive` 必须非空，`{prompt}` 是原提示词的占位符。
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
        let positive = v.get("positive").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if positive.is_empty() {
            continue;
        }
        let name = v
            .get("name")
            .and_then(|x| x.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                p.file_stem()
                    .and_then(|x| x.to_str())
                    .unwrap_or("style")
                    .to_string()
            });
        out.push(Style {
            name,
            positive,
            negative: v.get("negative").and_then(|x| x.as_str()).unwrap_or("").trim().to_string(),
        });
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
        "{\n  \"name\": \"电影感\",\n  \"positive\": \"cinematic photo, {prompt}, film grain, shallow depth of field\",\n  \"negative\": \"cartoon, text, watermark\"\n}\n",
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

#[tauri::command]
pub fn wildcard_map(app: tauri::AppHandle) -> BTreeMap<String, Vec<String>> {
    read_wildcards(&wildcards_dir(&crate::cfg_dir(&app)))
}

#[cfg(test)]
#[path = "promptfx_tests.rs"]
mod tests;
