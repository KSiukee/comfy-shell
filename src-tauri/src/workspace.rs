//! 项目工作区：每个项目一份 UI 状态（表单草稿、面板宽、图库筛选等）。
//! 私密项目不写（workspace.json 会是明文）。

use serde_json::Value;
use std::path::{Path, PathBuf};

fn file(cfg: &Path, project: &str) -> PathBuf {
    cfg.join("projects").join(project).join("workspace.json")
}

pub fn load(cfg: &Path, project: &str) -> Result<Option<Value>, String> {
    crate::project::load(cfg, project)?;
    if crate::vault::is_private(cfg, project) {
        return Err("私密项目不保存工作区".into());
    }
    let p = file(cfg, project);
    if !p.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    let value = serde_json::from_str(&text).map_err(|e| format!("解析工作区失败：{e}"))?;
    Ok(Some(value))
}

pub fn save(cfg: &Path, project: &str, data: &Value) -> Result<(), String> {
    crate::project::load(cfg, project)?;
    if crate::vault::is_private(cfg, project) {
        return Err("私密项目不保存工作区".into());
    }
    let text = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    if text.len() > 512 * 1024 {
        return Err("工作区太大（>512KB）".into());
    }
    crate::fsutil::atomic_write_str(&file(cfg, project), &text)
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
