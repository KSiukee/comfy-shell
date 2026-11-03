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
    let mut stack = vec![output.to_path_buf()];
    let mut acc = 0u64;
    let mut dirs = 0u64;
    while let Some(dir) = stack.pop() {
        if dir != output && is_reserved_dir(&dir) {
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

/// 任意层级都要跳过的保留目录：回收站 / 收藏镜像 / 私密暂存。
/// 项目和根目录各有一套 `_trash`、`_favorites`，所以不能只按根目录的绝对路径判。
fn is_reserved_dir(p: &Path) -> bool {
    p.file_name()
        .and_then(|x| x.to_str())
        .map(|n| {
            matches!(
                n,
                "_trash" | "_favorites" | "_comfyshell_trash" | "_private_staging"
            )
        })
        .unwrap_or(false)
}

/// 文件所属项目目录（`output/projects/<id>`）；根目录的图返回 None。
fn project_root(output: &Path, file: &Path) -> Option<PathBuf> {
    let rel = file.parent()?.strip_prefix(output).ok()?;
    let mut comps = rel.components();
    if comps.next()?.as_os_str() != "projects" {
        return None;
    }
    let id = comps.next()?.as_os_str();
    Some(output.join("projects").join(id))
}

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

/// 收藏镜像的目标文件：项目里的图进 `<项目>/_favorites/`，根目录的图进 `output/_favorites/`。
/// 子目录用 `a_b__文件名` 压平，避免同名打架。
///
/// 两个入参都先尽量 canonicalize：调用方有的传 `\\?\D:\...`（canonicalize 的结果）、
/// 有的传 `D:\...`，不统一就会 strip_prefix 失败 ⇒ 该摘的镜像摘不掉、该建的建不出。
fn favorite_target(output: &Path, src: &Path) -> Option<PathBuf> {
    let output = output.canonicalize().unwrap_or_else(|_| output.to_path_buf());
    let src = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    let base = project_root(&output, &src).unwrap_or_else(|| output.clone());
    let name = src.file_name()?.to_string_lossy().to_string();
    if name.is_empty() {
        return None;
    }
    let prefix = src
        .strip_prefix(&base)
        .ok()
        .and_then(|rel| rel.parent())
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_string_lossy().replace('\\', "_"))
        .unwrap_or_default();
    let dir = base.join("_favorites");
    Some(if prefix.is_empty() {
        dir.join(&name)
    } else {
        dir.join(format!("{prefix}__{name}"))
    })
}

fn mirror_favorite(output: &Path, src: &Path, on: bool) {
    let Some(target) = favorite_target(output, src) else {
        return;
    };
    if on {
        if let Some(dir) = target.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
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

pub fn star(cfg: &Path, output: &Path, path: &str, on: bool) {
    // 镜像到**项目目录里**的 _favorites，这样每个项目文件夹自带「收藏」，
    // 拷走一个项目不会把收藏留在别处。
    mirror_favorite(output, Path::new(path), on);

    let mut set = load_stars(cfg);
    if on {
        set.insert(path.to_string());
    } else {
        set.remove(path);
    }
    save_stars(cfg, &set);
}

/// 把 stars.json 里已经在收藏的图补一份镜像（幂等）。给老数据用：
/// 之前收藏只记在 json 里，项目文件夹里没有 `_favorites`。
pub fn sync_star_mirrors(cfg: &Path, output: &Path) -> usize {
    let set = load_stars(cfg);
    let mut n = 0;
    for p in set {
        let src = Path::new(&p);
        if src.is_file() {
            mirror_favorite(output, src, true);
            n += 1;
        }
    }
    n
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

pub(crate) fn image_size(path: &Path) -> (u32, u32) {
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

/// Recursively collect images under `output` + unlocked vault items, newest first.
pub fn scan(cfg: &Path, output: &Path, cap: usize) -> Vec<LibItem> {
    let mut items = scan_plain(cfg, output, cap);
    items.extend(crate::vault::lib_items(cfg));
    items.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    items.truncate(cap);
    items
}

fn scan_plain(cfg: &Path, output: &Path, cap: usize) -> Vec<LibItem> {
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

    while let Some(dir) = stack.pop() {
        if dir != output && is_reserved_dir(&dir) {
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

/// 移到回收站：项目里的图进 `<项目>/_trash/`，根目录的图进 `output/_trash/`。
/// 名字前缀时间戳，避免同名覆盖；文件管理器里按项目一眼能找到。
pub fn delete(output: &Path, paths: &[String]) -> Result<usize, String> {
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
        let base = project_root(&out_canon, &canon).unwrap_or_else(|| out_canon.clone());
        let trash = base.join("_trash");
        let _ = std::fs::create_dir_all(&trash);
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

    #[test]
    fn trash_and_favorites_live_inside_the_project() {
        let root = std::env::temp_dir().join(format!("cs_lib_trash_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cfg = root.join("cfg");
        let out = root.join("out");
        let p1 = out.join("projects").join("p1");
        std::fs::create_dir_all(&p1).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();
        let a = p1.join("a.png");
        let b = p1.join("b.png");
        let root_img = out.join("root.png");
        std::fs::write(&a, b"x").unwrap();
        std::fs::write(&b, b"y").unwrap();
        std::fs::write(&root_img, b"z").unwrap();

        // 项目里的图 → 项目自己的 _trash
        delete(&out, &[a.to_string_lossy().to_string()]).unwrap();
        assert!(p1.join("_trash").is_dir(), "项目里要有 _trash");
        assert_eq!(std::fs::read_dir(p1.join("_trash")).unwrap().count(), 1);

        // 根目录的图 → output/_trash
        delete(&out, &[root_img.to_string_lossy().to_string()]).unwrap();
        assert!(out.join("_trash").is_dir(), "根目录也要有自己的 _trash");

        // 收藏镜像进项目的 _favorites；取消后镜像消失
        let bs = b.to_string_lossy().to_string();
        star(&cfg, &out, &bs, true);
        assert!(p1.join("_favorites").join("b.png").is_file(), "收藏要镜像进项目");
        star(&cfg, &out, &bs, false);
        assert!(!p1.join("_favorites").join("b.png").exists());

        // 扫描不能把 _trash/_favorites 里的文件当成图（按路径段判，别被临时目录名带偏）
        let items = scan_uncached(&cfg, &out, 100);
        let reserved = |p: &str| {
            p.contains("\\_trash\\")
                || p.contains("/_trash/")
                || p.contains("\\_favorites\\")
                || p.contains("/_favorites/")
        };
        let bad: Vec<&str> = items
            .iter()
            .map(|i| i.path.as_str())
            .filter(|p| reserved(p))
            .collect();
        assert!(bad.is_empty(), "不该被扫到：{bad:?}");
        assert!(items.iter().any(|i| i.filename == "b.png"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
