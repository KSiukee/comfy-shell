//! 小状态文件的原子写：**同目录临时文件 → fsync → rename 覆盖**。
//!
//! 顺序不能反：先 rename 再写内容、或者直接写目标文件，崩溃时读者会看到半截
//! JSON——这类损坏不报错，只安静地毁数据。改名失败时退避重试（杀软/索引器
//! 会短暂占住文件），最后如实报错，不留碎片。
//!
//! 血统：圈记 `qj-fs/src/atomic.rs`，按本项目的体量裁成一个小模块。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static SEQ: AtomicU64 = AtomicU64::new(0);

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_else(|| "state".into());
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!("{name}.{}.{seq}.part", std::process::id()))
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("建目录失败：{e}"))?;
    }
    let tmp = temp_path(path);
    let written = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("写入 {} 失败：{e}", path.display()));
    }

    let mut last: Option<std::io::Error> = None;
    for delay in [0u64, 50, 150, 400] {
        if delay > 0 {
            std::thread::sleep(Duration::from_millis(delay));
        }
        match std::fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => last = Some(e),
        }
    }
    let _ = std::fs::remove_file(&tmp);
    Err(format!(
        "替换 {} 失败：{}",
        path.display(),
        last.map(|e| e.to_string()).unwrap_or_default()
    ))
}

pub fn atomic_write_str(path: &Path, text: &str) -> Result<(), String> {
    atomic_write(path, text.as_bytes())
}

/// 启动时清掉上次崩溃留下的 `.part`（同目录、非递归）。
pub fn cleanup_parts(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) == Some("part") {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// 文件移动：同盘 rename；跨盘（C: 配置 ↔ D: ComfyUI 输出）退化成复制+删除。
pub fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| format!("复制失败：{e}"))?;
    std::fs::remove_file(from).map_err(|e| format!("删除源文件失败：{e}"))?;
    Ok(())
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
            std::fs::copy(&p, &dest).map_err(|e| format!("复制失败：{e}"))?;
        }
    }
    Ok(())
}

/// 目录移动：同盘 rename；跨盘复制整棵树再删源。
pub fn move_dir(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir_all(from, to)?;
    std::fs::remove_dir_all(from).map_err(|e| format!("删除源目录失败：{e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cs_fsutil_{tag}_{}", std::process::id()))
    }

    #[test]
    fn writes_and_overwrites_without_leftovers() {
        let dir = tmpdir("rw");
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("settings.json");
        atomic_write_str(&p, "{\"a\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{\"a\":1}");
        atomic_write_str(&p, "{\"a\":2}").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{\"a\":2}");
        let parts = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("part"))
            .count();
        assert_eq!(parts, 0, "不应留下 .part");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn move_dir_and_file_work_on_same_volume() {
        let base = tmpdir("move");
        let _ = std::fs::remove_dir_all(&base);
        let a = base.join("a");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::write(a.join("x.txt"), b"hi").unwrap();
        let b = base.join("b");
        move_dir(&a, &b).unwrap();
        assert!(b.join("x.txt").exists());
        assert!(!a.exists());
        let c = base.join("c.txt");
        move_file(&b.join("x.txt"), &c).unwrap();
        assert!(c.exists());
        assert!(!b.join("x.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_removes_stale_parts_only() {
        let dir = tmpdir("cleanup");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("history.json"), "{}").unwrap();
        std::fs::write(dir.join("history.json.1.0.part"), "half").unwrap();
        cleanup_parts(&dir);
        assert!(dir.join("history.json").exists());
        assert!(!dir.join("history.json.1.0.part").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
