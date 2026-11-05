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

/// 云端解析许不许发：私密项目一律不行，没 key 只能本地。
///
/// 返回 Ok(true) = 可以发云端；Ok(false) = 走本地；Err = 用户明确要云端但项目是私密的。
fn cloud_allowed(private: bool, want_cloud: bool, has_key: bool) -> Result<bool, String> {
    if want_cloud && private {
        return Err("私密项目不走云端看图（红线）——用本地 Qwen3-VL".into());
    }
    Ok(want_cloud && has_key)
}

/// #82 看图拆提示词：五块候选（主体/镜头/光线/色彩/风格），每块 2~3 条短句，界面点一下接进提示词。
///
/// 两种引擎：`cloud`（OpenAI 兼容端点，默认 DeepSeek deepseek-flash）+ `local`（本机 Qwen3-VL）。
/// - 私密项目**只能本地**（`cloud_allowed` 直接拒）；
/// - 云端失败自动回落本地，并在结果里写清为什么；
/// - 结果里带 `source` / `model` / `notes`，界面如实显示这次是谁看的图。
#[tauri::command]
pub async fn analyze_blocks(
    app: tauri::AppHandle,
    image: String,
    project: Option<String>,
    cloud: Option<bool>,
) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let s = state.settings.lock().unwrap().clone();
        let cfg = cfg_dir(&handle);
        let private = project
            .as_deref()
            .map(|p| vault::is_private(&cfg, p))
            .unwrap_or(false);
        let want_cloud = cloud.unwrap_or(false);
        let mut notes: Vec<String> = Vec::new();
        let t0 = std::time::Instant::now();

        // ① 云端（只有被允许时才走）
        if cloud_allowed(private, want_cloud, agent::has_key(&cfg))? {
            let tried = vision::load_input_image(&s.comfy_root, &image)
                .and_then(|(mime, b64)| vision::ask(&cfg, &s, &mime, &b64, enhance::BLOCKS_PROMPT))
                .and_then(|raw| crate::vblocks::parse(&raw).map(|b| (raw, b)));
            match tried {
                Ok((raw, blocks)) => {
                    return Ok(serde_json::json!({
                        "blocks": blocks.iter().map(|b| serde_json::json!({"key": b.key, "items": b.items})).collect::<Vec<_>>(),
                        "raw": raw,
                        "source": "cloud",
                        "model": vision::model(&s),
                        "notes": notes,
                        "ms": t0.elapsed().as_millis() as u64,
                    }));
                }
                Err(e) => notes.push(format!("云端没成（{e}），改用本地")),
            }
        } else if want_cloud && !agent::has_key(&cfg) {
            notes.push("没有 API key，改走本地".into());
        }

        // ② 本地兜底
        if api::health(s.port).is_none() {
            let why = if notes.is_empty() { String::new() } else { format!("；{}", notes.join("；")) };
            return Err(format!("ComfyUI 没有在运行，云端也没成{why}"));
        }
        let raw = enhance::blocks(s.port, &image)?;
        let blocks = crate::vblocks::parse(&raw)?;
        Ok(serde_json::json!({
            "blocks": blocks
                .iter()
                .map(|b| serde_json::json!({"key": b.key, "items": b.items}))
                .collect::<Vec<_>>(),
            "raw": raw,
            "source": "local",
            "model": "Qwen3-VL（本机）",
            "notes": notes,
            "ms": t0.elapsed().as_millis() as u64,
        }))
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

#[cfg(test)]
mod policy_tests {
    use super::cloud_allowed;

    #[test]
    fn private_projects_never_go_cloud() {
        let err = cloud_allowed(true, true, true).unwrap_err();
        assert!(err.contains("私密项目"), "{err}");
        // 私密项目选了本地：正常放行到本地
        assert_eq!(cloud_allowed(true, false, true).unwrap(), false);
    }

    #[test]
    fn cloud_needs_the_flag_and_a_key() {
        assert_eq!(cloud_allowed(false, true, true).unwrap(), true);
        assert_eq!(cloud_allowed(false, true, false).unwrap(), false);
        assert_eq!(cloud_allowed(false, false, true).unwrap(), false);
    }
}
