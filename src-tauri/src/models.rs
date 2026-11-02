//! 模型管理器（#30）：扫 `ComfyUI/models/**`，读 sidecar 里的预览图 / 触发词 / 哈希，
//! 并支持「启用 / 禁用」。禁用只写一个同名的 `.disabled` 标记——不移动、不删除模型文件，
//! ComfyUI 自己仍然能扫到它，壳里的下拉会跳过。

use base64::Engine;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const EXTS: &[&str] = &["safetensors", "ckpt", "pt", "pth", "gguf"];
const PREVIEW_EXTS: &[&str] = &["png", "jpg", "jpeg", "webp"];

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    /// 相对 `models/` 的路径（用 `/` 分隔）
    pub name: String,
    /// 顶层目录名：checkpoints / loras / vae / …
    pub kind: String,
    pub size: u64,
    pub mtime: i64,
    /// 预览图（相对 models/）；ComfyUI/CivitAI 习惯是 `<模型名>.preview.png`
    pub preview: Option<String>,
    /// 触发词（sidecar 里有就显示）
    pub triggers: Vec<String>,
    pub base_model: String,
    /// sidecar 里记录的 SHA256（没有就空；不现场算，几 GB 的文件不能点开工）
    pub hash: String,
    pub disabled: bool,
}

fn mtime_secs(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn rel_string(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| p.to_string_lossy().replace('\\', "/"))
}

fn preview_of(p: &Path) -> Option<PathBuf> {
    let base = p.to_string_lossy().to_string();
    let stem = p.with_extension("").to_string_lossy().to_string();
    for ext in PREVIEW_EXTS {
        // 三种命名都认：`模型.safetensors.preview.png`、`模型.preview.png`、`模型.png`
        for cand in [
            format!("{base}.preview.{ext}"),
            format!("{stem}.preview.{ext}"),
            format!("{stem}.{ext}"),
        ] {
            let path = PathBuf::from(cand);
            if path != p && path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn sidecar_of(p: &Path) -> Option<Value> {
    let base = p.to_string_lossy().to_string();
    let stem = p.with_extension("").to_string_lossy().to_string();
    for suffix in [".civitai.info", ".json"] {
        for cand in [format!("{base}{suffix}"), format!("{stem}{suffix}")] {
            let path = PathBuf::from(cand);
            if !path.is_file() {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                return Some(v);
            }
        }
    }
    None
}

fn string_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn meta_of(v: &Value) -> (Vec<String>, String, String) {
    let triggers = {
        let direct = string_list(v.get("trainedWords"));
        if direct.is_empty() {
            string_list(v.get("model").and_then(|m| m.get("trainedWords")))
        } else {
            direct
        }
    };
    let base = v
        .get("baseModel")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("model").and_then(|m| m.get("baseModel")).and_then(|x| x.as_str()))
        .unwrap_or("")
        .to_string();
    let hash = v
        .get("files")
        .and_then(|f| f.get(0))
        .and_then(|f| f.get("hashes"))
        .and_then(|h| h.get("SHA256"))
        .and_then(|x| x.as_str())
        .or_else(|| v.get("hashes").and_then(|h| h.get("SHA256")).and_then(|x| x.as_str()))
        .or_else(|| v.get("sha256").and_then(|x| x.as_str()))
        .unwrap_or("")
        .to_string();
    (triggers, base, hash)
}

fn disabled_marker(p: &Path) -> PathBuf {
    PathBuf::from(format!("{}.disabled", p.to_string_lossy()))
}

fn walk(root: &Path, kind: &str, dir: &Path, depth: usize, out: &mut Vec<ModelFile>) {
    if depth > 3 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, kind, &p, depth + 1, out);
            continue;
        }
        let ext = p
            .extension()
            .and_then(|x| x.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        if !EXTS.contains(&ext.as_str()) {
            continue;
        }
        let (triggers, base_model, hash) = match sidecar_of(&p) {
            Some(v) => meta_of(&v),
            None => (Vec::new(), String::new(), String::new()),
        };
        out.push(ModelFile {
            name: rel_string(root, &p),
            kind: kind.to_string(),
            size: std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
            mtime: mtime_secs(&p),
            preview: preview_of(&p).map(|pp| rel_string(root, &pp)),
            triggers,
            base_model,
            hash,
            disabled: disabled_marker(&p).is_file(),
        });
    }
}

pub fn scan(models: &Path) -> Vec<ModelFile> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(models) else {
        return out;
    };
    let mut kinds: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .filter(|s| !s.starts_with('.'))
        .collect();
    kinds.sort();
    for kind in kinds {
        walk(models, &kind, &models.join(&kind), 1, &mut out);
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    out
}

/// 递归列出 `loras/` 下的 `.safetensors`，名字是相对 `loras/` 的路径（`/` 分隔）。
///
/// 为什么需要它：ComfyUI 的节点信息是**缓存**的，新丢进 loras 的文件要等它重启才出现；
/// 壳里补这一份，才能「下载完立刻在下拉里选到」。**必须递归**——LoRA 常常按风格分文件夹放
/// （模型卡自己也建议 `loras/flux3 image klein/`），只扫顶层会漏掉整类。
pub fn loras_recursive(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
        if depth > 3 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                // `_trash` / `.tmp` 这类目录是回收站或临时区，别放进下拉
                let skip = e
                    .file_name()
                    .to_string_lossy()
                    .starts_with(|c| c == '_' || c == '.');
                if !skip {
                    walk(base, &p, depth + 1, out);
                }
                continue;
            }
            if !p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("safetensors"))
                .unwrap_or(false)
            {
                continue;
            }
            if let Ok(rel) = p.strip_prefix(base) {
                let name = rel.to_string_lossy().replace('\\', "/");
                if !name.is_empty() {
                    out.push(name);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, 0, &mut out);
    out
}

/// 只接受扫描会返回的扩展名，避免命令行被拿来当任意文件开关。
pub fn toggle(models: &Path, rel: &str, enabled: bool) -> Result<bool, String> {    let path = models.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if !EXTS.contains(&ext.as_str()) || !path.is_file() {
        return Err(format!("不是可管理的模型文件：{rel}"));
    }
    let marker = disabled_marker(&path);
    if enabled {
        if marker.is_file() {
            std::fs::remove_file(&marker).map_err(|e| e.to_string())?;
        }
    } else {
        std::fs::write(&marker, b"").map_err(|e| e.to_string())?;
    }
    Ok(!enabled)
}

pub fn preview_data(models: &Path, rel: &str) -> Result<String, String> {
    let path = models.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !path.is_file() {
        return Err("预览图不存在".into());
    }
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if !PREVIEW_EXTS.contains(&ext.as_str()) {
        return Err("不是图片".into());
    }
    let mime = match ext.as_str() {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    };
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cs_models_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("loras/sub")).unwrap();
        std::fs::write(d.join("loras/a.safetensors"), b"x").unwrap();
        std::fs::write(d.join("loras/sub/b.gguf"), b"y").unwrap();
        d
    }

    #[test]
    fn loras_recursive_sees_subfolders_only_safetensors() {
        let m = temp("loras_rec");
        std::fs::create_dir_all(m.join("loras/flux3 image klein")).unwrap();
        std::fs::write(m.join("loras/flux3 image klein/skin.safetensors"), b"z").unwrap();
        std::fs::write(m.join("loras/notes.txt"), b"no").unwrap();
        std::fs::create_dir_all(m.join("loras/_trash")).unwrap();
        std::fs::write(m.join("loras/_trash/old.safetensors"), b"t").unwrap();
        let list = loras_recursive(&m.join("loras"));
        assert!(list.contains(&"a.safetensors".to_string()));
        assert!(list.contains(&"flux3 image klein/skin.safetensors".to_string()), "{list:?}");
        assert!(!list.iter().any(|x| x.ends_with(".txt") || x.ends_with(".gguf")));
        assert!(!list.iter().any(|x| x.starts_with("_trash/")), "{list:?}");
        let _ = std::fs::remove_dir_all(&m);
    }

    #[test]
    fn scans_and_reads_sidecars() {
        let m = temp("scan");
        std::fs::write(m.join("loras/a.preview.png"), b"png").unwrap();
        std::fs::write(
            m.join("loras/a.civitai.info"),
            br#"{"trainedWords":["mydress","red hair"],"baseModel":"Flux.1 D","files":[{"hashes":{"SHA256":"abc123"}}]}"#,
        )
        .unwrap();
        let list = scan(&m);
        assert_eq!(list.len(), 2);
        let a = list.iter().find(|x| x.name == "loras/a.safetensors").unwrap();
        assert_eq!(a.kind, "loras");
        assert_eq!(a.triggers, vec!["mydress", "red hair"]);
        assert_eq!(a.base_model, "Flux.1 D");
        assert_eq!(a.hash, "abc123");
        assert_eq!(a.preview.as_deref(), Some("loras/a.preview.png"));
        assert!(!a.disabled);
        let b = list.iter().find(|x| x.name == "loras/sub/b.gguf").unwrap();
        assert!(b.triggers.is_empty());
        let _ = std::fs::remove_dir_all(&m);
    }

    #[test]
    fn toggle_writes_and_removes_marker() {
        let m = temp("toggle");
        assert!(toggle(&m, "loras/a.safetensors", false).unwrap()); // -> disabled
        assert!(disabled_marker(&m.join("loras/a.safetensors")).is_file());
        assert!(scan(&m).iter().find(|x| x.name == "loras/a.safetensors").unwrap().disabled);
        assert!(!toggle(&m, "loras/a.safetensors", true).unwrap()); // -> enabled
        assert!(!scan(&m).iter().find(|x| x.name == "loras/a.safetensors").unwrap().disabled);
        assert!(toggle(&m, "loras/nope.safetensors", true).is_err());
        assert!(toggle(&m, "loras/a.txt", true).is_err());
        let _ = std::fs::remove_dir_all(&m);
    }
}
