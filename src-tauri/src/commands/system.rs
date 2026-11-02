//! 设置 / 引擎 / 模型 / 素材 / 模板 / 系统杂项。

use crate::*;
use crate::characters;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::State;

use crate::commands::gallery::image_as_data_url;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    healthy: bool,
    managed: bool,
    pid: Option<u32>,
    port: u16,
    root: String,
    version: Option<String>,
}

pub(crate) fn current_status(state: &AppState) -> Status {
    // try_lock everywhere: a status probe must never be able to deadlock the app,
    // and it must never wait behind a slow kill.
    let s = state
        .settings
        .try_lock()
        .map(|g| g.clone())
        .unwrap_or_default();

    let pid = match state.child.try_lock() {
        Ok(mut guard) => {
            let pid = guard.as_mut().and_then(|c| match c.try_wait() {
                Ok(None) => Some(c.id()),
                _ => None,
            });
            if pid.is_none() {
                *guard = None;
            }
            pid
        }
        Err(_) => None,
    };

    Status {
        healthy: api::health(s.port).is_some(),
        managed: pid.is_some(),
        pid,
        port: s.port,
        root: s.comfy_root,
        version: api::version(s.port),
    }
}

pub(crate) fn emit_status(app: &tauri::AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let st = current_status(&state);
        let _ = handle.emit("server-status", st);
    });
}

/// Start ComfyUI if needed and wait for it to answer.
pub(crate) fn ensure_server(app: tauri::AppHandle) -> Result<Status, String> {
    let (port, root) = {
        let state = app.state::<AppState>();
        let s = state.settings.lock().unwrap().clone();
        (s.port, s.comfy_root)
    };
    if api::health(port).is_some() {
        return Ok(current_status(&app.state::<AppState>()));
    }

    {
        let state = app.state::<AppState>();
        let mut guard = state.child.lock().unwrap();
        let dead = match guard.as_mut() {
            Some(c) => matches!(c.try_wait(), Ok(Some(_))),
            None => false,
        };
        if dead {
            *guard = None;
        }
        if guard.is_none() {
            let child = server::spawn(&root, port)?;
            *guard = Some(child);
        }
    }
    emit_status(&app);

    // Wait up to 4 minutes for the HTTP server to come up.
    for _ in 0..480 {
        if api::health(port).is_some() {
            emit_status(&app);
            return Ok(current_status(&app.state::<AppState>()));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Err("ComfyUI 启动超时（4 分钟），请查看 comfyshell-server.log".into())
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------


#[tauri::command]
pub fn settings_get(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn settings_set(app: tauri::AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    write_settings(&app, &settings)?;
    *state.settings.lock().unwrap() = settings;
    emit_status(&app);
    Ok(())
}

#[tauri::command]
pub async fn server_status(app: tauri::AppHandle) -> Status {
    // the health probe does network IO; never run it on the UI thread
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        current_status(&state)
    })
    .await
    .unwrap_or_else(|_| Status {
        healthy: false,
        managed: false,
        pid: None,
        port: 0,
        root: String::new(),
        version: None,
    })
}

#[tauri::command]
pub async fn server_start(app: tauri::AppHandle) -> Result<Status, String> {
    // Blocking work (spawning + up to 4 minutes of health polling) must not run on
    // the UI thread, otherwise the window turns into "not responding".
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || ensure_server(handle))
        .await
        .map_err(|e| format!("启动任务失败: {e}"))?
}

#[tauri::command]
pub fn server_stop(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
    let stopped = {
        // Scope the guard: emit_status() below takes the same lock again.
        let mut guard = state.child.lock().unwrap();
        match guard.take() {
            Some(mut child) => {
                server::kill(&mut child);
                true
            }
            None => false,
        }
    };
    if !stopped {
        let port = state.settings.lock().unwrap().port;
        let _ = api::client(3)
            .post(format!("http://127.0.0.1:{port}/interrupt"))
            .json(&json!({}))
            .send();
        emit_status(&app);
        return Err("ComfyUI 不是本壳启动的，无法关闭（可手动结束 python 进程）".into());
    }
    emit_status(&app);
    Ok(())
}

#[tauri::command]
pub async fn models_list(app: tauri::AppHandle) -> api::Models {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let (port, root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone())
        };
        let mut m = api::models(port);
        // ComfyUI caches its node info, so freshly trained LoRAs are not in it yet.
        // 递归扫（LoRA 常按风格分子文件夹放），见 models::loras_recursive 的注释。
        // 名字必须和 ComfyUI 给的一模一样：Windows 上它返回的是反斜杠（`a\b.safetensors`），
        // 这里按「分隔符无关」去重，已有的就保留 ComfyUI 的写法，缺的才补（补的时候用本机分隔符）。
        let lora_dir = std::path::PathBuf::from(&root)
            .join("ComfyUI")
            .join("models")
            .join("loras");
        let norm = |s: &str| s.replace('\\', "/").to_lowercase();
        for name in models::loras_recursive(&lora_dir) {
            if !m.loras.iter().any(|x| norm(x) == norm(&name)) {
                m.loras.push(name.replace('/', std::path::MAIN_SEPARATOR_STR));
            }
        }
        m.loras.sort();
        m
    })
    .await
    .unwrap_or(api::Models {
        unets: Vec::new(),
        clips: Vec::new(),
        vaes: Vec::new(),
        loras: Vec::new(),
    })
}

#[tauri::command]
pub fn profiles() -> serde_json::Value {
    api::profiles_json()
}

#[tauri::command]
pub fn save_ref_image(state: State<AppState>, name: String, data_url: String) -> Result<String, String> {
    let root = state.settings.lock().unwrap().comfy_root.clone();
    let dir = std::path::PathBuf::from(&root).join("ComfyUI").join("input");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let b64 = data_url
        .split_once(',')
        .map(|(_, b)| b.to_string())
        .unwrap_or(data_url);
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("base64 解码失败: {e}"))?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let safe: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let ext = if safe.to_lowercase().ends_with(".jpg") || safe.to_lowercase().ends_with(".jpeg") {
        "jpg"
    } else {
        "png"
    };
    let owner: String = safe
        .rsplit_once('.')
        .map(|(s, _)| s.to_string())
        .unwrap_or_else(|| "ref".to_string());
    let filename = format!("cs_{stamp}_{owner}.{ext}");
    std::fs::write(dir.join(&filename), bytes).map_err(|e| e.to_string())?;
    Ok(filename)
}

/// 把 ComfyUI/input 里的一张参考图读成 dataURL（参考图补白要用它读原图）。
#[tauri::command]
pub fn ref_preview(state: State<AppState>, name: String) -> Result<String, String> {
    let root = state.settings.lock().unwrap().comfy_root.clone();
    let dir = std::path::PathBuf::from(&root).join("ComfyUI").join("input");
    let safe: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let path = dir.join(&safe);
    let bytes = std::fs::read(&path).map_err(|e| format!("读不到参考图 {safe}: {e}"))?;
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mime = if safe.to_lowercase().ends_with(".jpg") || safe.to_lowercase().ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "image/png"
    };
    Ok(format!("data:{mime};base64,{b64}"))
}

/// 任意本地图片 → dataURL（角色卡拿身份底板当参考图要用）。
#[tauri::command]
pub fn image_data_url(path: String) -> Result<String, String> {
    let p = std::path::PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("读不到图片: {e}"))?;
    if !meta.is_file() {
        return Err("不是文件".into());
    }
    if meta.len() > 40 * 1024 * 1024 {
        return Err("图片太大（>40MB）".into());
    }
    let lower = path.to_lowercase();
    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    };
    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
    use base64::Engine as _;
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// 角色卡（#43）：列表 / 保存 / 删除。`project` 为空给全部，否则给「全局 + 该项目」。
#[tauri::command]
pub fn character_list(app: tauri::AppHandle, project: Option<String>) -> Vec<characters::Character> {
    characters::visible(characters::list(&cfg_dir(&app)), project.as_deref())
}

#[tauri::command]
pub fn character_save(app: tauri::AppHandle, character: characters::Character) -> Result<characters::Character, String> {
    characters::save(&cfg_dir(&app), character)
}

#[tauri::command]
pub fn character_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    characters::delete(&cfg_dir(&app), &id)
}


#[tauri::command]
pub fn read_input_image(state: State<AppState>, name: String) -> Result<String, String> {
    let root = state.settings.lock().unwrap().comfy_root.clone();
    let path = std::path::PathBuf::from(root)
        .join("ComfyUI")
        .join("input")
        .join(&name);
    image_as_data_url(&path)
}

/// Read any file under the ComfyUI output folder as a data URL (used by history).
#[tauri::command]
pub fn read_output_image(app: tauri::AppHandle, state: State<AppState>, path: String) -> Result<String, String> {
    if let Some(rest) = path.strip_prefix("vault://") {
        let (id, name) = rest.split_once('/').ok_or("私密图片路径不对")?;
        let bytes = crate::vault::read_image(&cfg_dir(&app), id, name)?;
        let lower = name.to_lowercase();
        let mime = if lower.ends_with(".webp") {
            "image/webp"
        } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
            "image/jpeg"
        } else {
            "image/png"
        };
        use base64::Engine as _;
        return Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ));
    }
    let root = std::path::PathBuf::from(state.settings.lock().unwrap().comfy_root.clone());
    let full = std::path::PathBuf::from(&path);
    let canon_root = root.canonicalize().unwrap_or(root.clone());
    let canon_full = full.canonicalize().map_err(|e| format!("找不到文件: {e}"))?;
    if !canon_full.starts_with(&canon_root) {
        return Err("路径不在 ComfyUI 目录内".into());
    }
    image_as_data_url(&canon_full)
}



#[tauri::command]
pub fn custom_list(app: tauri::AppHandle) -> Vec<String> {
    custom::list(&cfg_dir(&app))
}

#[tauri::command]
pub fn custom_save(app: tauri::AppHandle, name: String, json_text: String) -> Result<String, String> {
    custom::save(&cfg_dir(&app), &name, &json_text)
}

#[tauri::command]
pub fn custom_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    custom::delete(&cfg_dir(&app), &name)
}

/// 从自定义工作流推断可调参数（给生成页画表单用）。
#[tauri::command]
pub fn custom_params(app: tauri::AppHandle, name: String) -> Result<Vec<custom::Param>, String> {
    let graph = custom::load(&cfg_dir(&app), &name)?;
    Ok(custom::infer_params(&graph))
}

// ---------------------------------------------------------------------------
// 配方库（#28）

#[tauri::command]
pub fn recipe_list(app: tauri::AppHandle) -> Vec<recipes::Recipe> {
    recipes::list(&cfg_dir(&app))
}

#[tauri::command]
pub fn recipe_save(app: tauri::AppHandle, recipe: String) -> Result<recipes::Recipe, String> {
    let r: recipes::Recipe =
        serde_json::from_str(&recipe).map_err(|e| format!("配方格式不对：{e}"))?;
    recipes::save(&cfg_dir(&app), r)
}

#[tauri::command]
pub fn recipe_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    recipes::delete(&cfg_dir(&app), &id)
}

// ---------------------------------------------------------------------------
// 模型管理器（#30）：扫 ComfyUI/models/**、预览、启用/禁用

fn models_root(app: &tauri::AppHandle) -> std::path::PathBuf {
    let root = app.state::<crate::AppState>().settings.lock().unwrap().comfy_root.clone();
    std::path::PathBuf::from(root).join("ComfyUI").join("models")
}

#[tauri::command]
pub fn models_scan(app: tauri::AppHandle) -> Vec<models::ModelFile> {
    models::scan(&models_root(&app))
}

#[tauri::command]
pub fn model_toggle(app: tauri::AppHandle, name: String, enabled: bool) -> Result<bool, String> {
    models::toggle(&models_root(&app), &name, enabled)
}

#[tauri::command]
pub fn model_preview(app: tauri::AppHandle, name: String) -> Result<String, String> {
    models::preview_data(&models_root(&app), &name)
}

/// 从历史提示词里挖「你反复写的短语」。
/// `project` 为 Some 时只看那个项目；`excluded` 里的项目完全不参与（明文/隐藏/锁定一视同仁，按 id 排除）。
#[tauri::command]
pub fn phrase_mine(
    app: tauri::AppHandle,
    project: Option<String>,
    hidden: Vec<String>,
    excluded: Vec<String>,
) -> Vec<hints::Phrase> {
    let items = history::load(&cfg_dir(&app));
    hints::mine(&items, project.as_deref(), &hidden, &excluded)
}

// ---------------------------------------------------------------------------
// Saved MOSS layout templates
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn layout_tpl_list(app: tauri::AppHandle) -> Vec<String> {
    layouts::list(&cfg_dir(&app))
}

#[tauri::command]
pub fn layout_tpl_load(app: tauri::AppHandle, name: String) -> Result<serde_json::Value, String> {
    layouts::load(&cfg_dir(&app), &name)
}

#[tauri::command]
pub fn layout_tpl_save(app: tauri::AppHandle, name: String, items: String) -> Result<String, String> {
    layouts::save(&cfg_dir(&app), &name, &items)
}

#[tauri::command]
pub fn layout_tpl_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    layouts::delete(&cfg_dir(&app), &name)
}

/// 按 MOSS 官方 layout.md 校验一份排版 JSON；返回问题列表（空 = 通过）。
#[tauri::command]
pub fn layout_check(text: String) -> Result<Vec<String>, String> {
    layouts::check_layout(&text)
}


#[tauri::command]
pub fn open_folder(path: String) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

#[tauri::command]
pub fn reveal_file(path: String) {
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{path}"))
        .spawn();
}

#[tauri::command]
pub fn job_cancel(state: State<AppState>) {
    *state.cancel.lock().unwrap() = true;
    let port = state.settings.lock().unwrap().port;
    api::interrupt(port);
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------
