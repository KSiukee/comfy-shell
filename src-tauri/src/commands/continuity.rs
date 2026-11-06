//! 剧情连续性的命令层：给一格拼最终提示词、写/自动补「落点」。

use crate::*;
use tauri::State;

fn cfg_of(app: &tauri::AppHandle) -> PathBuf {
    crate::state::cfg_dir(app)
}

fn port_of(app: &tauri::AppHandle) -> u16 {
    app.state::<AppState>().settings.lock().unwrap().port
}

/// 某一格最终要发出去的提示词：剧本设定前缀 + 上一格落点 + 本格原文。
/// 前端「填入 / 重画 / 整话出图」都走这一条，别在 JS 里再拼一次。
#[tauri::command]
pub fn shot_prompt(app: tauri::AppHandle, project: String, id: String) -> Result<serde_json::Value, String> {
    let cfg = cfg_of(&app);
    let shots = crate::shots::list(&cfg, &project)?;
    let idx = shots.iter().position(|s| s.id == id).ok_or("镜头不存在")?;
    let shot = &shots[idx];
    let story = crate::story::load(&cfg, &project);
    let lead = if idx == 0 {
        None
    } else {
        let prev = &shots[idx - 1];
        let c = prev.continuity.trim();
        if c.is_empty() {
            None
        } else {
            Some(c)
        }
    };
    Ok(serde_json::json!({
        "prompt": story.compose_with_lead(lead, &shot.prompt),
        "negative": if shot.negative.trim().is_empty() { story.negative.clone() } else { shot.negative.clone() },
        "lead": lead,
    }))
}

/// 手写一格的落点。
#[tauri::command]
pub fn shot_set_continuity(app: tauri::AppHandle, project: String, id: String, text: String) -> Result<serde_json::Value, String> {
    let s = crate::shots::set_continuity(&cfg_of(&app), &project, &id, &text)?;
    Ok(serde_json::json!({"id": s.id, "continuity": s.continuity}))
}

/// 一次给整话补落点（本地模型；ComfyUI 没起就报错，别静默失败）。
#[tauri::command]
pub async fn shots_continuity(app: tauri::AppHandle, project: String) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = cfg_of(&handle);
        let shots = crate::shots::list(&cfg, &project)?;
        if shots.is_empty() {
            return Err("还没有镜头".into());
        }
        let port = port_of(&handle);
        if crate::api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        let panels: Vec<String> = shots.iter().map(|s| s.prompt.clone()).collect();
        let states = crate::enhance::continuity_states(port, &panels);
        let n = crate::shots::set_continuity_all(&cfg, &project, &states)?;
        Ok(serde_json::json!({ "written": n, "total": shots.len(), "states": states }))
    })
    .await
    .map_err(|e| format!("补落点失败: {e}"))?
}
