//! AI 助手命令：引擎检测、key 存取、体检、规划、引擎安装。

use crate::*;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;

#[tauri::command]
pub fn ai_engine_status(app: tauri::AppHandle) -> agent::EngineStatus {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap().clone();
    agent::resolve_engine(&cfg_dir(&app), &s)
}

#[tauri::command]
pub fn ai_key_set(app: tauri::AppHandle, key: String) -> Result<(), String> {
    agent::save_key(&cfg_dir(&app), &key)
}

#[tauri::command]
pub fn ai_key_status(app: tauri::AppHandle) -> bool {
    agent::has_key(&cfg_dir(&app))
}

#[tauri::command]
pub fn ai_key_clear(app: tauri::AppHandle) -> Result<(), String> {
    agent::clear_key(&cfg_dir(&app))
}

#[tauri::command]
pub fn ai_env_check(app: tauri::AppHandle) -> agent::EnvReport {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap().clone();
    agent::env_report(&cfg_dir(&app), &s)
}

#[tauri::command]
pub async fn ai_plan(
    app: tauri::AppHandle,
    request: String,
    extra: Option<String>,
) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let s = state.settings.lock().unwrap().clone();
        let cfg = cfg_dir(&handle);
        let report = agent::env_report(&cfg, &s);
        let r = agent::run_plan(&cfg, &s, &report, &request, extra.as_deref())?;
        Ok(serde_json::json!({
            "plan": r.plan,
            "raw": r.raw,
            "ms": r.ms,
            "env": report,
        }))
    })
    .await
    .map_err(|e| format!("AI 任务失败：{e}"))?
}

/// 下载便携 Node + npm 安装 Pi 到应用数据目录（或指定目录）。
#[tauri::command]
pub async fn ai_engine_install(
    app: tauri::AppHandle,
    dir: Option<String>,
) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = cfg_dir(&handle);
        let target = dir.map(PathBuf::from).unwrap_or_else(|| cfg.join("agent"));
        let app2 = handle.clone();
        let prog: agent::EngineProgress =
            Arc::new(move |phase: &str, got: u64, total: u64, mbps: f64, line: &str| {
                let _ = app2.emit(
                    "ai-progress",
                    serde_json::json!({
                        "phase": phase, "got": got, "total": total, "mbps": mbps, "line": line
                    }),
                );
            });
        agent::install_engine(&cfg, &target, prog)
    })
    .await
    .map_err(|e| format!("引擎安装失败：{e}"))?
}

#[tauri::command]
pub fn ai_inspect(
    app: tauri::AppHandle,
    target: String,
    lines: Option<usize>,
) -> Result<serde_json::Value, String> {
    let state = app.state::<AppState>();
    let s = state.settings.lock().unwrap().clone();
    agent::inspect(&s, &target, lines.unwrap_or(120))
}
