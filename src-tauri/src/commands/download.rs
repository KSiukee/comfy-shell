//! 下载 / 安装命令：镜像测速、ComfyUI 源码更新与便携版安装。
//!
//! 只做「用户点了才下」的事：不内置模型、不自动装。所有下载都支持取消，
//! 进度通过 `dl-progress` 事件回前端。

use crate::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;

static DL_CANCEL: AtomicBool = AtomicBool::new(false);

fn emit_prog(app: &tauri::AppHandle, phase: &str, got: u64, total: u64, mbps: f64) {
    let _ = app.emit(
        "dl-progress",
        serde_json::json!({"phase": phase, "got": got, "total": total, "mbps": mbps}),
    );
}

fn cancelled() -> bool {
    DL_CANCEL.load(Ordering::SeqCst)
}

/// 给一组候选地址测速（顺序探测；每个 256KB）。
#[tauri::command]
pub async fn dl_probe(urls: Vec<String>) -> Vec<netdl::Probe> {
    tauri::async_runtime::spawn_blocking(move || {
        let proxy = netdl::detect_proxy();
        urls.into_iter()
            .map(|u| netdl::probe(&u, proxy.as_deref()))
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub fn dl_cancel() {
    DL_CANCEL.store(true, Ordering::SeqCst);
}

/// 下载当前大小（给界面显示「已下载/总量」用）。
#[tauri::command]
pub fn dl_available() -> bool {
    true
}

fn find_portable_root(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 3 {
        return None;
    }
    if dir.join("python_embeded").is_dir() && dir.join("ComfyUI").is_dir() {
        return Some(dir.to_path_buf());
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(hit) = find_portable_root(&p, depth + 1) {
                return Some(hit);
            }
        }
    }
    None
}

fn dir_is_empty_except(dir: &Path, allow: &str) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return true;
    };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy() != allow {
            return false;
        }
    }
    true
}

fn install_portable(extract_dir: &Path, target: &Path) -> Result<String, String> {
    let inner = find_portable_root(extract_dir, 0)
        .ok_or("压缩包里没找到便携版（缺 python_embeded/ComfyUI）")?;
    for e in std::fs::read_dir(&inner).map_err(|e| e.to_string())?.flatten() {
        let from = e.path();
        let to = target.join(e.file_name());
        if to.exists() {
            return Err(format!("目标里已存在 {}，先清空再装", to.display()));
        }
        netdl::move_dir(&from, &to)?;
    }
    Ok(target.to_string_lossy().to_string())
}

fn install_source(extract_dir: &Path, target: &Path) -> Result<String, String> {
    let new_root = netdl::find_comfy_root(extract_dir)
        .ok_or("压缩包里没找到 ComfyUI（缺 main.py/comfy）")?;
    let comfy = netdl::swap_source(&new_root, target)?;
    Ok(comfy.to_string_lossy().to_string())
}

/// ComfyUI 安装 / 更新。
/// mode: `source`（只换 ComfyUI/，保留模型与用户数据）或 `portable`（完整便携版，目标必须是空目录）。
#[tauri::command]
pub async fn comfy_install(
    app: tauri::AppHandle,
    mode: String,
    url: String,
    dir: Option<String>,
    threads: Option<usize>,
) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let target = match dir.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()) {
            Some(d) => PathBuf::from(d),
            None => PathBuf::from(state.settings.lock().unwrap().comfy_root.clone()),
        };
        if target.as_os_str().is_empty() {
            return Err("先选一个安装目录".into());
        }
        std::fs::create_dir_all(&target).map_err(|e| format!("建目录失败：{e}"))?;

        let portable = mode == "portable";
        if portable {
            if !dir_is_empty_except(&target, "_xianying_dl") {
                return Err("完整安装要求目标目录是空的（换一个空文件夹）".into());
            }
        } else if !target.join("python_embeded").is_dir() && !target.join("ComfyUI").is_dir() {
            return Err(
                "这个目录不像 ComfyUI 便携版（缺 python_embeded）：完整安装请选「完整便携版」".into(),
            );
        }

        let tmp = target.join("_xianying_dl");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let archive = tmp.join(if portable { "download.7z" } else { "download.zip" });

        DL_CANCEL.store(false, Ordering::SeqCst);
        emit_prog(&handle, "下载中", 0, 0, 0.0);
        let app2 = handle.clone();
        let on_progress: Arc<dyn Fn(u64, u64, f64) + Send + Sync> = Arc::new(move |got, total, mbps| {
            emit_prog(&app2, "下载中", got, total, mbps);
        });
        let cancel: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(cancelled);
        let cfg = netdl::DlCfg {
            url: &url,
            dest: &archive,
            threads: threads.unwrap_or(4),
            proxy: netdl::detect_proxy(),
            cancel,
            on_progress,
        };
        if let Err(e) = netdl::download(cfg) {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
        if cancelled() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err("已取消".into());
        }

        emit_prog(&handle, "解压中", 0, 0, 0.0);
        let ex = tmp.join("extract");
        if let Err(e) = netdl::extract(&archive, &ex) {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
        if cancelled() {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err("已取消".into());
        }

        emit_prog(&handle, "安装中", 0, 0, 0.0);
        let res = if portable {
            install_portable(&ex, &target)
        } else {
            install_source(&ex, &target)
        };
        let out = match res {
            Ok(p) => p,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(e);
            }
        };

        // 记到设置里，下一次启动/重启就用新目录
        {
            let mut s = state.settings.lock().unwrap();
            s.comfy_root = target.to_string_lossy().to_string();
            let _ = write_settings(&handle, &s);
        }
        let _ = std::fs::remove_dir_all(&tmp);
        emit_prog(&handle, "完成", 0, 0, 0.0);
        Ok(out)
    })
    .await
    .map_err(|e| format!("安装任务失败：{e}"))?
}

/// 通用文件下载（模型用）：下载到指定目录，文件名从 URL 末段取。
#[tauri::command]
pub async fn file_download(
    app: tauri::AppHandle,
    url: String,
    dir: String,
    name: Option<String>,
    threads: Option<usize>,
) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dir = PathBuf::from(dir.trim());
        std::fs::create_dir_all(&dir).map_err(|e| format!("建目录失败：{e}"))?;
        let fname = name
            .filter(|n| !n.trim().is_empty())
            .or_else(|| url.rsplit('/').next().map(|s| s.split('?').next().unwrap_or(s).to_string()))
            .filter(|n| !n.is_empty())
            .ok_or("认不出文件名")?;
        let dest = dir.join(fname);
        let part = dest.with_extension("part");
        if part.exists() {
            let _ = std::fs::remove_file(&part);
        }
        DL_CANCEL.store(false, Ordering::SeqCst);
        emit_prog(&handle, "下载中", 0, 0, 0.0);
        let app2 = handle.clone();
        let on_progress: Arc<dyn Fn(u64, u64, f64) + Send + Sync> = Arc::new(move |got, total, mbps| {
            emit_prog(&app2, "下载中", got, total, mbps);
        });
        let cancel: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(cancelled);
        let cfg = netdl::DlCfg {
            url: &url,
            dest: &dest,
            threads: threads.unwrap_or(4),
            proxy: netdl::detect_proxy(),
            cancel,
            on_progress,
        };
        netdl::download(cfg)?;
        emit_prog(&handle, "完成", 0, 0, 0.0);
        Ok(dest.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("下载任务失败：{e}"))?
}
