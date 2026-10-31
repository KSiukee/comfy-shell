//! 备份 / 项目导出 / 项目导入。
//!
//! 备份只复制**文件**：私密项目的 `vault/` 原样是密文，绝不顺手解密。
//! 导出私密项目是例外：用户明确选择目标目录并二次确认后，解密成明文包，
//! manifest 里标注 `plaintext: true`，方便以后识别。

use crate::project::Project;
use serde_json::json;
use std::path::{Path, PathBuf};

fn stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn unique_dir(dest: &Path, base: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dest).map_err(|e| format!("创建目标目录失败：{e}"))?;
    let mut dir = dest.join(base);
    let mut i = 2;
    while dir.exists() {
        dir = dest.join(format!("{base}-{i}"));
        i += 1;
        if i > 999 {
            return Err("同名目录太多了，换个目标文件夹".into());
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn copy_dir_all(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let rd = std::fs::read_dir(from).map_err(|e| e.to_string())?;
    for e in rd.flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_dir_all(&p, &dest)?;
        } else {
            std::fs::copy(&p, &dest).map_err(|e| format!("复制 {} 失败：{e}", p.display()))?;
        }
    }
    Ok(())
}

fn write_manifest(dir: &Path, v: serde_json::Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&dir.join("export.json"), &text)
}

fn copy_images(from: &Path, to: &Path) -> Result<usize, String> {
    if !from.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut n = 0;
    for e in std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        if p.is_file() {
            std::fs::copy(&p, to.join(e.file_name())).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

/// 整份配置备份（不含缓存）；vault 原样复制密文。
pub fn backup(cfg: &Path, dest: &Path) -> Result<PathBuf, String> {
    let dir = unique_dir(dest, &format!("comfy-shell-backup-{}", stamp()))?;
    for f in ["settings.json", "stars.json", "history.json"] {
        let src = cfg.join(f);
        if src.is_file() {
            std::fs::copy(&src, dir.join(f)).map_err(|e| e.to_string())?;
        }
    }
    for d in ["projects", "vault", "layouts", "workflows", "experiments"] {
        let src = cfg.join(d);
        if src.is_dir() {
            copy_dir_all(&src, &dir.join(d))?;
        }
    }
    write_manifest(
        &dir,
        json!({
            "kind": "backup",
            "createdAt": stamp(),
            "version": env!("CARGO_PKG_VERSION"),
            "note": "vault/ 是密文，需要锁定区口令才能解开"
        }),
    )?;
    Ok(dir)
}

/// 导出普通项目，或（解锁后、显式确认）解密导出私密项目。
pub fn export_project(cfg: &Path, output: &Path, id: &str, dest: &Path) -> Result<PathBuf, String> {
    if crate::vault::is_private(cfg, id) {
        let key = crate::vault::session_key()?;
        let data = crate::vault::read_meta(cfg, id, &key)?;
        let base = format!(
            "{}-{}-private",
            crate::custom::safe_name(&data.project.name),
            stamp()
        );
        let dir = unique_dir(dest, &base)?;
        let mut project = data.project.clone();
        project.private = false;
        let text = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
        crate::fsutil::atomic_write_str(&dir.join("project.json"), &text)?;
        let mut n = 0;
        if !data.images.is_empty() {
            let img_dir = dir.join("images");
            std::fs::create_dir_all(&img_dir).map_err(|e| e.to_string())?;
            for im in &data.images {
                let bytes = crate::vault::read_image(cfg, id, &im.name)?;
                crate::fsutil::atomic_write(&img_dir.join(&im.name), &bytes)?;
                n += 1;
            }
        }
        write_manifest(
            &dir,
            json!({"kind": "project", "name": project.name, "id": id, "images": n,
                   "createdAt": stamp(), "private": true, "plaintext": true,
                   "note": "私密项目导出：这是明文档，注意存放位置"}),
        )?;
        return Ok(dir);
    }

    let project = crate::project::load(cfg, id)?;
    let base = format!(
        "{}-{}",
        crate::custom::safe_name(&project.name),
        stamp()
    );
    let dir = unique_dir(dest, &base)?;
    let text = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&dir.join("project.json"), &text)?;
    let n = copy_images(&output.join("projects").join(id), &dir.join("images"))?;
    write_manifest(
        &dir,
        json!({"kind": "project", "name": project.name, "id": id, "images": n,
               "createdAt": stamp(), "private": false, "plaintext": true}),
    )?;
    Ok(dir)
}

/// 从一个导出目录导入项目：新 id、新目录，不覆盖已有项目。
pub fn import_project(cfg: &Path, output: &Path, folder: &Path) -> Result<Project, String> {
    let text = std::fs::read_to_string(folder.join("project.json"))
        .map_err(|_| "这个目录里没有 project.json（不是显影导出的项目）".to_string())?;
    let mut p: Project = serde_json::from_str(&text).map_err(|e| format!("解析 project.json 失败：{e}"))?;
    if p.name.trim().is_empty() {
        p.name = "导入项目".into();
    }
    let mut id = String::new();
    for _ in 0..32 {
        let cand = format!("p_{:08x}", rand::random::<u32>());
        if !cfg.join("projects").join(&cand).exists() {
            id = cand;
            break;
        }
    }
    if id.is_empty() {
        return Err("生成导入项目 id 失败".into());
    }
    let t = now_ms();
    p.id = id.clone();
    p.name = format!("{}（导入）", p.name);
    p.active = false;
    p.hidden = false;
    p.private = false;
    p.test = false;
    p.created_at = t;
    p.updated_at = t;
    p.last_opened_at = t;

    let dir = cfg.join("projects").join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&dir.join("project.json"), &out)?;
    let _ = copy_images(&folder.join("images"), &output.join("projects").join(&id))?;
    Ok(p)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "dataio_tests.rs"]
mod tests;
