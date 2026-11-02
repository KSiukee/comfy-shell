//! 模型清单：公开下载源（HF + hf-mirror）+ 安装状态。
//!
//! 只列「点了才下」的文件；本机定制的 heretic TE 用公开版同名替代并标注。

use serde::{Deserialize, Serialize};
use std::path::Path;

const RAW: &str = include_str!("model_manifest.json");

#[derive(Serialize, Deserialize, Clone)]
pub struct Entry {
    pub target: String,
    pub kind: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub sources: Vec<String>,
}

pub fn entries() -> Vec<Entry> {
    serde_json::from_str(RAW).unwrap_or_default()
}

pub fn with_status(models_root: &Path) -> Vec<serde_json::Value> {
    entries()
        .into_iter()
        .map(|e| {
            let path = models_root.join(&e.kind).join(&e.target);
            serde_json::json!({
                "target": e.target,
                "kind": e.kind,
                "size": e.size,
                "note": e.note,
                "sources": e.sources,
                "exists": path.is_file(),
                "path": path.to_string_lossy(),
            })
        })
        .collect()
}

pub fn valid_kind(kind: &str) -> bool {
    matches!(
        kind,
        "diffusion_models" | "text_encoders" | "vae" | "loras" | "upscale_models"
    )
}

/// 单个文件名护栏（不许路径穿越）。
pub fn safe_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') || name.contains("..")
    {
        return Err("文件名不合法".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_and_targets_are_unique() {
        let e = entries();
        assert!(e.len() >= 10);
        let mut seen = std::collections::HashSet::new();
        for x in &e {
            assert!(seen.insert(x.target.clone()), "重复：{}", x.target);
            assert!(valid_kind(&x.kind), "kind 不合法：{}", x.kind);
            assert!(safe_name(&x.target).is_ok());
            assert!(!x.sources.is_empty());
            for s in &x.sources {
                assert!(s.starts_with("https://"), "只允许 https：{s}");
            }
        }
    }
}
