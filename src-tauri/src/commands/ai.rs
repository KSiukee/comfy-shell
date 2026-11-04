//! 剧情拆分 / 助手 / 看图 / 排版检测。

use crate::*;
use tauri::State;

/// 用本机模型写一份 MOSS 排版草稿（官方 layout.md 的草稿格式），失败会带着校验错误重试一次。
#[tauri::command]
pub async fn layout_draft(
    app: tauri::AppHandle,
    request: String,
    width: Option<i64>,
    height: Option<i64>,
) -> Result<serde_json::Value, String> {
    if request.trim().is_empty() {
        return Err("先写一句想画什么".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let port = handle.state::<AppState>().settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        enhance::layout_draft(port, &request, width.unwrap_or(1024), height.unwrap_or(1024))
    })
    .await
    .map_err(|e| format!("排版草稿失败: {e}"))?
}

#[tauri::command]
pub async fn story_split(
    app: tauri::AppHandle,
    text: String,
    max_panels: u32,
    project: Option<String>,
) -> Result<Vec<String>, String> {
    if text.trim().is_empty() {
        return Err("先贴一段剧情".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let port = handle.state::<AppState>().settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        // #91：拆的时候就带上这部戏的设定（画风/时代/光线），别等出图才对齐
        let ctx = project
            .map(|p| story::load(&state::cfg_dir(&handle), &p).split_context())
            .filter(|c| !c.trim().is_empty());
        Ok(enhance::story_split(port, &text, max_panels, ctx.as_deref()))
    })
    .await
    .map_err(|e| format!("拆分任务失败: {e}"))?
}




#[tauri::command]
pub async fn assist(app: tauri::AppHandle, request: String, image: Option<String>) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let (port, root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone())
        };
        std::env::set_var("COMFY_ROOT", &root);
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".to_string());
        }
        let (prompt, note, steps) = enhance::assist(port, &request, image.as_deref())?;
        Ok(serde_json::json!({
            "prompt": prompt,
            "note": note,
            "steps": steps.iter().map(|s| serde_json::json!({"tool": s.tool, "output": s.output})).collect::<Vec<_>>(),
        }))
    })
    .await
    .map_err(|e| format!("助手任务失败: {e}"))?
}

#[tauri::command]
pub async fn analyze_image(app: tauri::AppHandle, image: String, two_pass: Option<bool>) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let port = state.settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".to_string());
        }
        if two_pass.unwrap_or(true) {
            enhance::analyze_two_pass(port, &image)
        } else {
            enhance::analyze(port, &image)
        }
    })
    .await
    .map_err(|e| format!("解析任务失败: {e}"))?
}

#[tauri::command]
pub async fn layout_detect(app: tauri::AppHandle, image: String) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let port = state.settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".to_string());
        }
        let items = enhance::detect_layout(port, &image)?;
        Ok(serde_json::Value::Array(items))
    })
    .await
    .map_err(|e| format!("识框任务失败: {e}"))?
}

/// 只找人脸（局部重绘「自动圈脸」用）。返回 {type,desc,text,bbox}，bbox 是归一化 x,y,w,h。
#[tauri::command]
pub async fn face_detect(app: tauri::AppHandle, image: String) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let port = state.settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".to_string());
        }
        enhance::detect_face(port, &image)
    })
    .await
    .map_err(|e| format!("找脸任务失败: {e}"))?
}

/// #91：拆分实际会发给模型的正文（界面/测试都能看到设定有没有带上）。
#[tauri::command]
pub fn story_split_preview(app: tauri::AppHandle, project: Option<String>, text: String, max_panels: u32) -> String {
    let ctx = project
        .map(|p| crate::story::load(&crate::state::cfg_dir(&app), &p).split_context())
        .filter(|c| !c.trim().is_empty());
    crate::enhance::split_prompt(&text, max_panels.clamp(1, 40), ctx.as_deref())
}
