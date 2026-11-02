//! AI 助手命令：引擎检测、key 存取、体检、规划。

use crate::*;

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
) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let s = state.settings.lock().unwrap().clone();
        let cfg = cfg_dir(&handle);
        let report = agent::env_report(&cfg, &s);
        let r = agent::run_plan(&cfg, &s, &report, &request)?;
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
