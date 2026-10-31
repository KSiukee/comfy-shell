//! 锁定区的写操作（移入/移出、封存生成图、开工、删除）。
//! 拆出来只为别让 vault.rs 顶过 400 行。

use crate::project::Project;
use crate::vault::*;
use std::path::Path;

/// 把刚生成的明文图逐张加密进仓库，返回 `vault://` 路径；最后才删明文。
pub fn seal_paths(
    cfg: &Path,
    id: &str,
    paths: &[String],
    key: &[u8; KEY_LEN],
) -> Result<Vec<String>, String> {
    safe_id(id)?;
    let mut data = read_meta(cfg, id, key)?;
    let mut next = data.images.iter().map(|x| x.idx + 1).max().unwrap_or(0);
    let mut urls = Vec::new();
    std::fs::create_dir_all(images_dir(cfg, id)).map_err(|e| e.to_string())?;
    for p in paths {
        let path = Path::new(p);
        let bytes = std::fs::read(path).map_err(|e| format!("读取生成图失败：{e}"))?;
        let (width, height) = crate::library::image_size(path);
        let size = bytes.len() as u64;
        let mtime = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut name = path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("image.png")
            .to_string();
        if data.images.iter().any(|x| x.name == name) {
            name = format!("{next}_{name}");
        }
        let sealed = seal(key, &bytes)?;
        crate::fsutil::atomic_write(&images_dir(cfg, id).join(format!("{next}.enc")), &sealed)?;
        data.images.push(VaultImage {
            name: name.clone(),
            idx: next,
            width,
            height,
            size,
            mtime,
            starred: false,
        });
        urls.push(format!("vault://{id}/{name}"));
        next += 1;
    }
    write_meta(cfg, &data, key)?;
    for p in paths {
        std::fs::remove_file(p).map_err(|e| format!("封存后删除明文失败：{e}"))?;
    }
    Ok(urls)
}

/// 私密项目里的开工标记（写回加密 meta）。
pub fn set_active(cfg: &Path, id: Option<&str>) -> Result<Vec<Project>, String> {
    let key = session_key()?;
    if let Some(want) = id {
        if !meta_path(cfg, want).is_file() {
            return Err("私密项目不存在".into());
        }
    }
    let mut ids = Vec::new();
    if let Ok(rd) = std::fs::read_dir(vault_dir(cfg)) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("enc") {
                if let Some(stem) = p.file_stem().and_then(|x| x.to_str()) {
                    ids.push(stem.to_string());
                }
            }
        }
    }
    for pid in ids {
        let mut data = read_meta(cfg, &pid, &key)?;
        let should = id == Some(data.project.id.as_str());
        if should && !data.project.active {
            data.project.last_opened_at = now_ms();
        }
        if data.project.active != should {
            data.project.active = should;
            data.project.updated_at = now_ms();
            write_meta(cfg, &data, &key)?;
        }
    }
    Ok(list(cfg))
}

/// 永久删除一个私密项目（配置 + 加密图片）。
pub fn delete(cfg: &Path, id: &str) -> Result<(), String> {
    safe_id(id)?;
    if !meta_path(cfg, id).is_file() {
        return Err("私密项目不存在".into());
    }
    std::fs::remove_file(meta_path(cfg, id)).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(images_dir(cfg, id));
    Ok(())
}

