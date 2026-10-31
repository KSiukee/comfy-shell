//! In-app image library: scans ComfyUI's output folder, supports starring and
//! soft-deleting (moves files to `output/_comfyshell_trash/`).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LibItem {
    pub path: String,
    pub filename: String,
    pub subfolder: String,
    pub size: u64,
    pub mtime: u64,
    pub width: u32,
    pub height: u32,
    pub starred: bool,
}


use std::sync::Mutex;

static CACHE: Mutex<Option<(u64, Vec<LibItem>)>> = Mutex::new(None);

fn dir_nanos(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Cache key over **directory** mtimes, recursively (skipping trash/favourites).
/// Root-only mtime is not enough: images saved into `projects/<id>/` change that
/// subdirectory's mtime, not the output root's, so the gallery would go stale.
fn tree_key(output: &Path) -> u64 {
    let trash = output.join("_comfyshell_trash");
    let fav = output.join("_favorites");
    let mut stack = vec![output.to_path_buf()];
    let mut acc = 0u64;
    let mut dirs = 0u64;
    while let Some(dir) = stack.pop() {
        if dir == trash || dir == fav {
            continue;
        }
        acc ^= dir_nanos(&dir).rotate_left((dirs % 63) as u32);
        dirs += 1;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    stack.push(e.path());
                }
            }
        }
    }
    acc ^ dirs.rotate_left(17)
}

fn cache_key(cfg: &Path, output: &Path) -> u64 {
    tree_key(output) ^ dir_nanos(&stars_file(cfg)).rotate_left(29)
}

fn cache_file(cfg: &Path) -> PathBuf {
    cfg.join("library_cache.json")
}

fn load_disk_cache(cfg: &Path, key: u64) -> Option<Vec<LibItem>> {
    let text = std::fs::read_to_string(cache_file(cfg)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    if v.get("key")?.as_u64()? != key {
        return None;
    }
    serde_json::from_value(v.get("items")?.clone()).ok()
}

fn save_disk_cache(cfg: &Path, key: u64, items: &[LibItem]) {
    let v = serde_json::json!({"key": key, "items": items});
    if let Ok(text) = serde_json::to_string(&v) {
        let _ = crate::fsutil::atomic_write_str(&cache_file(cfg), &text);
    }
}

const IMAGE_EXTS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

fn stars_file(cfg: &Path) -> PathBuf {
    cfg.join("stars.json")
}

fn load_stars(cfg: &Path) -> HashSet<String> {
    std::fs::read_to_string(stars_file(cfg))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

fn save_stars(cfg: &Path, set: &HashSet<String>) {
    let mut v: Vec<&String> = set.iter().collect();
    v.sort();
    if let Ok(text) = serde_json::to_string(&v) {
        let _ = crate::fsutil::atomic_write_str(&stars_file(cfg), &text);
    }
}

/// Every starred path, for the main gallery's quick-favourite buttons.
pub fn stars(cfg: &Path) -> Vec<String> {
    let mut out: Vec<String> = load_stars(cfg).into_iter().collect();
    out.sort();
    out
}

pub fn star(cfg: &Path, output: &Path, path: &str, on: bool) {
    // mirror starred files into output\_favorites so the file manager agrees
    let fav = output.join("_favorites");
    let src = Path::new(path);
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if !name.is_empty() {
        let prefix = src
            .parent()
            .and_then(|p| p.strip_prefix(output).ok())
            .map(|p| p.to_string_lossy().replace('\\', "_"))
            .unwrap_or_default();
        let target = if prefix.is_empty() {
            fav.join(&name)
        } else {
            fav.join(format!("{prefix}__{name}"))
        };
        if on {
            let _ = std::fs::create_dir_all(&fav);
            if !target.exists() && src.exists() {
                let _ = std::fs::hard_link(src, &target);
                if !target.exists() {
                    let _ = std::fs::copy(src, &target);
                }
            }
        } else {
            let _ = std::fs::remove_file(&target);
        }
    }

    let mut set = load_stars(cfg);
    if on {
        set.insert(path.to_string());
    } else {
        set.remove(path);
    }
    save_stars(cfg, &set);
}

fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() > 24 && &b[1..4] == b"PNG" {
        let w = u32::from_be_bytes([b[16], b[17], b[18], b[19]]);
        let h = u32::from_be_bytes([b[20], b[21], b[22], b[23]]);
        return Some((w, h));
    }
    None
}

fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        if (0xC0..=0xC3).contains(&marker) || (0xC5..=0xC7).contains(&marker) || (0xC9..=0xCB).contains(&marker) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if len < 2 {
            break;
        }
        i += 2 + len;
    }
    None
}

fn image_size(path: &Path) -> (u32, u32) {
    use std::io::Read;
    let mut f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return (0, 0),
    };
    let mut buf = vec![0u8; 64 * 1024];
    let n = f.read(&mut buf).unwrap_or(0);
    let b = &buf[..n];
    png_size(b).or_else(|| jpeg_size(b)).unwrap_or((0, 0))
}

/// Recursively collect images under `output`, newest first, capped.
pub fn scan(cfg: &Path, output: &Path, cap: usize) -> Vec<LibItem> {
    let key = cache_key(cfg, output);
    if let Ok(guard) = CACHE.lock() {
        if let Some((k, items)) = guard.as_ref() {
            if *k == key && items.len() <= cap {
                return items.clone();
            }
        }
    }
    if let Some(items) = load_disk_cache(cfg, key) {
        if items.len() <= cap {
            if let Ok(mut guard) = CACHE.lock() {
                *guard = Some((key, items.clone()));
            }
            return items;
        }
    }
    let items = scan_uncached(cfg, output, cap);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((key, items.clone()));
    }
    save_disk_cache(cfg, key, &items);
    items
}

fn scan_uncached(cfg: &Path, output: &Path, cap: usize) -> Vec<LibItem> {
    let stars = load_stars(cfg);
    let mut out = Vec::new();
    let mut stack = vec![output.to_path_buf()];
    let trash = output.join("_comfyshell_trash");
    let fav = output.join("_favorites");

    while let Some(dir) = stack.pop() {
        if dir == trash || dir == fav {
            continue;
        }
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.to_lowercase())
                .unwrap_or_default();
            if !IMAGE_EXTS.contains(&ext.as_str()) {
                continue;
            }
            let meta = match e.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let (w, h) = image_size(&p);
            let path_str = p.to_string_lossy().to_string();
            out.push(LibItem {
                starred: stars.contains(&path_str),
                path: path_str,
                filename: p.file_name().and_then(|x| x.to_str()).unwrap_or("").to_string(),
                subfolder: p
                    .parent()
                    .and_then(|parent| parent.strip_prefix(output).ok())
                    .map(|s| s.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default(),
                size: meta.len(),
                mtime,
                width: w,
                height: h,
            });
        }
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    out.truncate(cap);
    out
}

/// Move files into `output/_comfyshell_trash/` (recoverable by hand).
pub fn delete(output: &Path, paths: &[String]) -> Result<usize, String> {
    let trash = output.join("_comfyshell_trash");
    std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let out_canon = output.canonicalize().unwrap_or_else(|_| output.to_path_buf());
    let mut moved = 0;
    for p in paths {
        let src = PathBuf::from(p);
        let canon = match src.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if !canon.starts_with(&out_canon) {
            continue;
        }
        let name = canon
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("image.png");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let dst = trash.join(format!("{stamp}_{name}"));
        if std::fs::rename(&canon, &dst).is_ok() {
            moved += 1;
        }
    }
    Ok(moved)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_key_changes_when_a_subfolder_gets_a_file() {
        let out = std::env::temp_dir().join(format!("cs_lib_tree_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let sub = out.join("projects").join("p1");
        std::fs::create_dir_all(&sub).unwrap();
        let k1 = tree_key(&out);
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(sub.join("a.png"), b"not-a-real-png").unwrap();
        let k2 = tree_key(&out);
        assert_ne!(k1, k2, "子目录里加图后，缓存键必须变");
        let _ = std::fs::remove_dir_all(&out);
    }
}
