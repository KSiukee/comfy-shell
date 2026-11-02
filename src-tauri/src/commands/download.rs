//! 下载 / 安装命令：镜像测速、ComfyUI 源码更新与便携版安装。
//!
//! 只做「用户点了才下」的事：不内置模型、不自动装。所有下载都支持取消，
//! 进度通过 `dl-progress` 事件回前端。

use crate::*;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

// ---------------------------------------------------------------- 模型清单 / 下载

#[tauri::command]
pub fn models_manifest(app: tauri::AppHandle) -> Vec<serde_json::Value> {
    let state = app.state::<AppState>();
    let root = {
        let s = state.settings.lock().unwrap();
        PathBuf::from(&s.comfy_root).join("ComfyUI").join("models")
    };
    models_manifest::with_status(&root)
}

/// 下载一个模型文件到 `<models>/<kind>/<target>`，多个源依次尝试。
#[tauri::command]
pub async fn model_download(
    app: tauri::AppHandle,
    target: String,
    kind: String,
    urls: Vec<String>,
) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        models_manifest::safe_name(&target)?;
        if !models_manifest::valid_kind(&kind) {
            return Err(format!("未知模型类型：{kind}"));
        }
        let root = {
            let state = handle.state::<AppState>();
            let s = state.settings.lock().unwrap();
            PathBuf::from(&s.comfy_root).join("ComfyUI").join("models")
        };
        let dir = root.join(&kind);
        std::fs::create_dir_all(&dir).map_err(|e| format!("建目录失败：{e}"))?;
        let dest = dir.join(&target);
        DL_CANCEL.store(false, Ordering::SeqCst);
        let mut last_err = String::new();
        for url in urls.iter().filter(|u| u.starts_with("https://")) {
            let app2 = handle.clone();
            let fname = target.clone();
            let on_progress: Arc<dyn Fn(u64, u64, f64) + Send + Sync> =
                Arc::new(move |got, total, mbps| {
                    let _ = app2.emit(
                        "model-progress",
                        serde_json::json!({"target": fname, "phase": "下载中", "got": got, "total": total, "mbps": mbps}),
                    );
                });
            let cancel: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(cancelled);
            let cfg = netdl::DlCfg {
                url,
                dest: &dest,
                threads: 6,
                proxy: netdl::detect_proxy(),
                cancel,
                on_progress,
            };
            match netdl::download(cfg) {
                Ok(p) => {
                    let _ = handle.emit(
                        "model-progress",
                        serde_json::json!({"target": target, "phase": "完成", "got": 0, "total": 0, "mbps": 0}),
                    );
                    return Ok(p.to_string_lossy().to_string());
                }
                Err(e) => last_err = format!("{url} → {e}"),
            }
        }
        let _ = handle.emit(
            "model-progress",
            serde_json::json!({"target": target, "phase": "失败", "error": last_err}),
        );
        Err(format!("下载失败：{last_err}"))
    })
    .await
    .map_err(|e| format!("下载任务失败：{e}"))?
}

// ---------------------------------------------------------------- 自定义节点安装

fn run_with_timeout(
    mut cmd: std::process::Command,
    secs: u64,
    on_line: &dyn Fn(&str),
) -> Result<(), String> {
    use std::io::BufRead;
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| format!("启动失败：{e}"))?;
    let streams: Vec<Box<dyn std::io::Read + Send>> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    let lines: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for s in streams {
        let sink = lines.clone();
        handles.push(std::thread::spawn(move || {
            let reader = std::io::BufReader::new(s);
            for line in reader.lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    if let Ok(mut v) = sink.lock() {
                        v.push(line);
                    }
                }
            }
        }));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    loop {
        {
            let mut v = lines.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default();
            for line in v.drain(..) {
                on_line(&line);
            }
        }
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(st) => {
                for h in handles {
                    let _ = h.join();
                }
                let mut v = lines.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default();
                for line in v.drain(..) {
                    on_line(&line);
                }
                if !st.success() {
                    return Err(format!("命令失败（退出码 {:?}）", st.code()));
                }
                return Ok(());
            }
            None => {
                if std::time::Instant::now() > deadline {
                    // git clone 会派生子进程，单杀父进程会留下残留
                    #[cfg(windows)]
                    {
                        let _ = Command::new("taskkill")
                            .args(["/PID", &child.id().to_string(), "/T", "/F"])
                            .output();
                    }
                    let _ = child.kill();
                    return Err(format!("超时（{secs} 秒）"));
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
}

/// 安装自定义节点：只允许 GitHub https 仓库，clone 到 custom_nodes，再装 requirements。
#[tauri::command]
pub async fn node_install(app: tauri::AppHandle, url: String) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let url = url.trim();
        if !url.starts_with("https://github.com/") {
            return Err("只支持 https://github.com/… 的仓库".into());
        }
        let repo = url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".git")
            .to_string();
        if repo.is_empty()
            || !repo
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err("认不出仓库名".into());
        }
        let (root, comfy) = {
            let state = handle.state::<AppState>();
            let s = state.settings.lock().unwrap();
            let root = PathBuf::from(&s.comfy_root);
            (root.clone(), root.join("ComfyUI"))
        };
        let target = comfy.join("custom_nodes").join(&repo);
        if target.exists() {
            return Err(format!("已经存在：{}", target.display()));
        }
        std::fs::create_dir_all(comfy.join("custom_nodes")).map_err(|e| e.to_string())?;
        let emit = |phase: &str, line: &str| {
            let _ = handle.emit(
                "node-progress",
                serde_json::json!({"phase": phase, "line": line, "repo": repo}),
            );
        };
        emit("git clone", url);
        let mut g = Command::new("git");
        g.arg("clone")
            .arg("--depth")
            .arg("1")
            .arg(url)
            .arg(&target)
            .current_dir(comfy.join("custom_nodes"));
        if let Some(p) = netdl::detect_proxy() {
            g.env("HTTPS_PROXY", &p).env("HTTP_PROXY", &p);
        }
        let f = |line: &str| emit("git clone", line);
        run_with_timeout(g, 300, &f).map_err(|e| format!("git clone 失败：{e}"))?;

        let req = target.join("requirements.txt");
        if req.is_file() {
            let py = root.join("python_embeded").join("python.exe");
            if !py.is_file() {
                return Err("缺 python_embeded，装不了依赖（节点已 clone，可手动 pip install）".into());
            }
            emit("pip install", "requirements.txt");
            let mut p = Command::new(&py);
            p.arg("-m")
                .arg("pip")
                .arg("install")
                .arg("-r")
                .arg(&req);
            if let Some(proxy) = netdl::detect_proxy() {
                p.env("HTTPS_PROXY", &proxy).env("HTTP_PROXY", &proxy);
            }
            let f2 = |line: &str| emit("pip install", line);
            run_with_timeout(p, 900, &f2).map_err(|e| format!("pip install 失败：{e}"))?;
        }
        emit("完成", &target.to_string_lossy());
        Ok(target.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| format!("装节点失败：{e}"))?
}
