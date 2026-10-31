#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod custom;
mod enhance;
mod history;
mod library;
mod server;

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{Emitter, Manager, State};

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Settings {
    comfy_root: String,
    port: u16,
    auto_start: bool,
    stop_on_exit: bool,
    /// skip the official PE and rewrite prompts with the local uncensored model
    #[serde(default)]
    free_enhance: bool,
    /// simple UI (plain-language controls) vs pro UI
    #[serde(default = "true_b")]
    simple_ui: bool,
    #[serde(default = "default_kohya_python")]
    kohya_python: String,
    #[serde(default = "default_kohya_dir")]
    kohya_dir: String,
}

fn default_kohya_python() -> String {
    r"D:\deepseek-files\ai-tools\sdvenv\Scripts\python.exe".to_string()
}

fn default_kohya_dir() -> String {
    r"D:\deepseek-files\ai-tools\sd-scripts".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            comfy_root: r"D:\deepseek-files\ai-tools\ComfyUI_windows_portable".to_string(),
            port: 8188,
            auto_start: true,
            stop_on_exit: false,
            free_enhance: false,
            simple_ui: true,
            kohya_python: default_kohya_python(),
            kohya_dir: default_kohya_dir(),
        }
    }
}

fn cfg_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn load_settings(app: &tauri::AppHandle) -> Settings {
    let p = cfg_dir(app).join("settings.json");
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_settings(app: &tauri::AppHandle, s: &Settings) -> Result<(), String> {
    let p = cfg_dir(app).join("settings.json");
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(p, text).map_err(|e| e.to_string())
}

struct AppState {
    child: Mutex<Option<std::process::Child>>,
    settings: Mutex<Settings>,
    cancel: Mutex<bool>,
}

// ---------------------------------------------------------------------------
// Status helpers
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Status {
    healthy: bool,
    managed: bool,
    pid: Option<u32>,
    port: u16,
    root: String,
    version: Option<String>,
}

fn current_status(state: &AppState) -> Status {
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

fn emit_status(app: &tauri::AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let st = current_status(&state);
        let _ = handle.emit("server-status", st);
    });
}

/// Start ComfyUI if needed and wait for it to answer.
fn ensure_server(app: tauri::AppHandle) -> Result<Status, String> {
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
fn settings_get(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn settings_set(app: tauri::AppHandle, state: State<AppState>, settings: Settings) -> Result<(), String> {
    write_settings(&app, &settings)?;
    *state.settings.lock().unwrap() = settings;
    emit_status(&app);
    Ok(())
}

#[tauri::command]
async fn server_status(app: tauri::AppHandle) -> Status {
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
async fn server_start(app: tauri::AppHandle) -> Result<Status, String> {
    // Blocking work (spawning + up to 4 minutes of health polling) must not run on
    // the UI thread, otherwise the window turns into "not responding".
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || ensure_server(handle))
        .await
        .map_err(|e| format!("启动任务失败: {e}"))?
}

#[tauri::command]
fn server_stop(app: tauri::AppHandle, state: State<AppState>) -> Result<(), String> {
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
async fn models_list(app: tauri::AppHandle) -> api::Models {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let (port, root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone())
        };
        let mut m = api::models(port);
        // ComfyUI caches its node info, so freshly trained LoRAs are not in it yet.
        let lora_dir = std::path::PathBuf::from(&root)
            .join("ComfyUI")
            .join("models")
            .join("loras");
        if let Ok(rd) = std::fs::read_dir(&lora_dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.to_lowercase().ends_with(".safetensors") && !m.loras.contains(&name) {
                    m.loras.push(name);
                }
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
fn profiles() -> serde_json::Value {
    api::profiles_json()
}

#[tauri::command]
fn save_ref_image(state: State<AppState>, name: String, data_url: String) -> Result<String, String> {
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


#[tauri::command]
fn poses_catalog(state: State<AppState>) -> Result<String, String> {
    let root = state.settings.lock().unwrap().comfy_root.clone();
    let dir = std::path::PathBuf::from(root).join("ComfyUI").join("input").join("shisei");
    let text = std::fs::read_to_string(dir.join("catalog.json"))
        .map_err(|e| format!("找不到姿势库（{}）：{e}", dir.display()))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("姿势库 catalog 解析失败: {e}"))?;
    let available: std::collections::HashSet<String> = std::fs::read_dir(dir.join("line"))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    if let Some(list) = value.as_array_mut() {
        for pose in list.iter_mut() {
            if let Some(views) = pose.get_mut("views").and_then(|v| v.as_object_mut()) {
                views.retain(|_, v| v.as_str().map(|n| available.contains(n)).unwrap_or(false));
            }
        }
        // drop poses with no usable image at all
        list.retain(|p| {
            p.get("views")
                .and_then(|v| v.as_object())
                .map(|o| !o.is_empty())
                .unwrap_or(false)
        });
    }
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_input_image(state: State<AppState>, name: String) -> Result<String, String> {
    let root = state.settings.lock().unwrap().comfy_root.clone();
    let path = std::path::PathBuf::from(root)
        .join("ComfyUI")
        .join("input")
        .join(&name);
    image_as_data_url(&path)
}

/// Read any file under the ComfyUI output folder as a data URL (used by history).
#[tauri::command]
fn read_output_image(state: State<AppState>, path: String) -> Result<String, String> {
    let root = std::path::PathBuf::from(state.settings.lock().unwrap().comfy_root.clone());
    let full = std::path::PathBuf::from(&path);
    let canon_root = root.canonicalize().unwrap_or(root.clone());
    let canon_full = full.canonicalize().map_err(|e| format!("找不到文件: {e}"))?;
    if !canon_full.starts_with(&canon_root) {
        return Err("路径不在 ComfyUI 目录内".into());
    }
    image_as_data_url(&canon_full)
}

fn image_as_data_url(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取图片失败: {e}"))?;
    use base64::Engine as _;
    let lower = path.to_string_lossy().to_lowercase();
    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    };
    Ok(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GenResp {
    #[serde(flatten)]
    out: api::GenOut,
    turbo: bool,
    label: String,
    history_id: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn push_history(
    app: &tauri::AppHandle,
    profile: &str,
    label: &str,
    mode: &str,
    prompt: &str,
    negative: &str,
    width: u32,
    height: u32,
    steps: u32,
    cfg: f64,
    seed: i64,
    out: &api::GenOut,
) -> String {
    let cfg_dir = cfg_dir(app);
    let id = format!("{}-{}", now_ms(), out.prompt_id.chars().take(8).collect::<String>());
    let item = history::HistoryItem {
        id: id.clone(),
        ts: now_ms(),
        profile: profile.to_string(),
        label: label.to_string(),
        mode: mode.to_string(),
        prompt: prompt.to_string(),
        negative: negative.to_string(),
        width,
        height,
        steps,
        cfg,
        seed,
        seconds: out.seconds,
        images: out
            .images
            .iter()
            .map(|i| history::HistoryImage {
                filename: i.filename.clone(),
                subfolder: i.subfolder.clone(),
                path: i.path.clone(),
                width: i.width,
                height: i.height,
            })
            .collect(),
    };
    history::append(&cfg_dir, item);
    id
}

#[tauri::command]
async fn generate(app: tauri::AppHandle, req: api::GenReq, turbo: bool) -> Result<GenResp, String> {
    // Long-running: submit, poll for up to 30 minutes, read images back.
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<GenResp, String> {
        let state = handle.state::<AppState>();
        let (port, root) = {
            let s = state.settings.lock().unwrap().clone();
            (s.port, s.comfy_root)
        };
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行，请先点「启动」".into());
        }
        *state.cancel.lock().unwrap() = false;

        let cfg = cfg_dir(&handle);
        let custom_name = req.profile.strip_prefix("custom:").map(|s| s.to_string());
        let mut label = match &custom_name {
            Some(name) => format!("自定义 · {name}"),
            None => api::profile(&req.profile)
                .map(|p| p.label.to_string())
                .unwrap_or_else(|| req.profile.clone()),
        };
        if req.mode != "edit" && !req.ref_images.is_empty() {
            label.push_str(" · 参考");
        }

        let graph = if let Some(name) = &custom_name {
            let base = custom::load(&cfg, name)?;
            let seed = api::pick_seed(req.seed);
            custom::substitute(
                &base, &req.prompt, &req.negative, seed, req.width, req.height, req.steps, req.cfg,
            )
        } else {
            api::build_graph(&req, turbo)?
        };

        let app2 = handle.clone();
        let out = api::run_job(port, &root, graph, 1800, move |p| {
            let _ = app2.emit(
                "gen-progress",
                json!({
                    "elapsed": p.elapsed,
                    "running": p.queue_running,
                    "pending": p.queue_pending,
                    "started": p.started,
                }),
            );
        })?;

        let id = push_history(
            &handle, &req.profile, &label, &req.mode, &req.prompt, &req.negative,
            req.width, req.height, req.steps, req.cfg, req.seed, &out,
        );

        Ok(GenResp {
            out,
            turbo,
            label,
            history_id: id,
        })
    })
    .await
    .map_err(|e| format!("生成任务失败: {e}"))?
}

#[tauri::command]
async fn enhance_prompt(
    app: tauri::AppHandle,
    profile: String,
    mode: String,
    prompt: String,
    ref_images: Vec<String>,
) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let (port, free) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.free_enhance)
        };
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        if free {
            return enhance::enhance_free(port, &prompt, ref_images.first().map(|s| s.as_str()));
        }
        enhance::enhance(port, &profile, &mode, &prompt, &ref_images)
    })
    .await
    .map_err(|e| format!("扩写任务失败: {e}"))?
}

#[tauri::command]
fn history_list(app: tauri::AppHandle) -> Vec<history::HistoryItem> {
    history::load(&cfg_dir(&app))
}

#[tauri::command]
fn history_clear(app: tauri::AppHandle) {
    history::clear(&cfg_dir(&app));
}

#[tauri::command]
fn custom_list(app: tauri::AppHandle) -> Vec<String> {
    custom::list(&cfg_dir(&app))
}

#[tauri::command]
fn custom_save(app: tauri::AppHandle, name: String, json_text: String) -> Result<String, String> {
    custom::save(&cfg_dir(&app), &name, &json_text)
}

#[tauri::command]
fn custom_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    custom::delete(&cfg_dir(&app), &name)
}

// ---------------------------------------------------------------------------
// Image library
// ---------------------------------------------------------------------------

fn output_dir(state: &AppState) -> PathBuf {
    PathBuf::from(state.settings.lock().unwrap().comfy_root.clone())
        .join("ComfyUI")
        .join("output")
}

#[tauri::command]
async fn library_scan(app: tauri::AppHandle) -> Vec<library::LibItem> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let out = output_dir(&state);
        library::scan(&cfg_dir(&handle), &out, 3000)
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn library_delete(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let out = output_dir(&state);
        library::delete(&out, &paths)
    })
    .await
    .map_err(|e| format!("删除任务失败: {e}"))?
}

#[tauri::command]
async fn delete_results(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<usize, String> {
        let state = handle.state::<AppState>();
        let out = output_dir(&state);
        let n = library::delete(&out, &paths)?;

        // drop the deleted shots from history so they do not come back as broken cards
        let dir = cfg_dir(&handle);
        let mut items = history::load(&dir);
        let dead: std::collections::HashSet<&String> = paths.iter().collect();
        for it in items.iter_mut() {
            it.images.retain(|im| !dead.contains(&im.path));
        }
        items.retain(|it| !it.images.is_empty());
        history::save(&dir, &items);
        Ok(n)
    })
    .await
    .map_err(|e| format!("删除任务失败: {e}"))?
}

#[tauri::command]
fn stars_list(app: tauri::AppHandle) -> Vec<String> {
    library::stars(&cfg_dir(&app))
}

#[tauri::command]
fn library_star(app: tauri::AppHandle, path: String, starred: bool) {
    let state = app.state::<AppState>();
    let out = output_dir(&state);
    library::star(&cfg_dir(&app), &out, &path, starred);
}

/// Persist a PNG produced by the frontend (e.g. a composed comparison image).
#[tauri::command]
fn save_export(state: State<AppState>, name: String, data_url: String) -> Result<String, String> {
    let root = PathBuf::from(state.settings.lock().unwrap().comfy_root.clone());
    let dir = root.join("ComfyUI").join("output");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let b64 = data_url
        .split_once(',')
        .map(|(_, b)| b.to_string())
        .unwrap_or(data_url);
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("base64 解码失败: {e}"))?;

    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut filename = safe.trim().to_string();
    if !filename.to_lowercase().ends_with(".png") {
        filename.push_str(".png");
    }
    if filename == ".png" {
        filename = "export.png".to_string();
    }
    let full = dir.join(&filename);
    std::fs::write(&full, bytes).map_err(|e| e.to_string())?;
    Ok(full.to_string_lossy().to_string())
}


#[tauri::command]
async fn translate_text(app: tauri::AppHandle, text: String) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let port = handle.state::<AppState>().settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        enhance::translate(port, &text)
    })
    .await
    .map_err(|e| format!("翻译任务失败: {e}"))?
}

fn pick_upscale_model(root: &str) -> Result<String, String> {
    let dir = std::path::PathBuf::from(root)
        .join("ComfyUI")
        .join("models")
        .join("upscale_models");
    let mut names: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if lower.ends_with(".pth") || lower.ends_with(".safetensors") || lower.ends_with(".pt") {
                names.push(name);
            }
        }
    }
    if names.is_empty() {
        return Err(format!("没有超分模型：把 4x 模型放进 {}", dir.display()));
    }
    names.sort_by_key(|n| {
        let l = n.to_lowercase();
        if l.contains("ultrasharp") {
            0
        } else if l.starts_with("4x") {
            1
        } else {
            2
        }
    });
    Ok(names[0].clone())
}

#[tauri::command]
async fn upscale_image(
    app: tauri::AppHandle,
    path: String,
    model: Option<String>,
) -> Result<api::ImageOut, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let (port, root) = {
            let s = state.settings.lock().unwrap().clone();
            (s.port, s.comfy_root)
        };
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        let out_dir = std::path::PathBuf::from(&root).join("ComfyUI").join("output");
        let canon_out = out_dir.canonicalize().unwrap_or(out_dir.clone());
        let src = std::path::PathBuf::from(&path)
            .canonicalize()
            .map_err(|e| format!("找不到图片: {e}"))?;
        if !src.starts_with(&canon_out) {
            return Err("只能超分图库里的图片".into());
        }
        let chosen = match model {
            Some(m) if m == "__seedvr2__" => m,
            Some(m)
                if !m.is_empty()
                    && !m.contains('\\')
                    && !m.contains('/')
                    && std::path::Path::new(&root)
                        .join("ComfyUI")
                        .join("models")
                        .join("upscale_models")
                        .join(&m)
                        .exists() =>
            {
                m
            }
            _ => pick_upscale_model(&root)?,
        };
        if chosen == "__seedvr2__" {
            api::upscale_seedvr2(port, &root, &src.to_string_lossy())
        } else {
            api::upscale_image(port, &root, &src.to_string_lossy(), &chosen)
        }
    })
    .await
    .map_err(|e| format!("超分任务失败: {e}"))?
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InpaintCall {
    profile: String,
    prompt: String,
    #[serde(default)]
    negative: String,
    source: String,
    mask_data_url: String,
    steps: u32,
    cfg: f64,
    seed: i64,
    #[serde(default = "one_f")]
    denoise: f64,
    #[serde(default)]
    feather: u32,
    #[serde(default)]
    ref_image_data_url: Option<String>,
}

fn one_f() -> f64 {
    1.0
}

#[tauri::command]
async fn inpaint_image(app: tauri::AppHandle, req: InpaintCall) -> Result<GenResp, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<GenResp, String> {
        let state = handle.state::<AppState>();
        let (port, root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone())
        };
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }

        // source must be one of our outputs
        let out_dir = std::path::PathBuf::from(&root).join("ComfyUI").join("output");
        let canon_out = out_dir.canonicalize().unwrap_or(out_dir.clone());
        let src = std::path::PathBuf::from(&req.source)
            .canonicalize()
            .map_err(|e| format!("找不到原图: {e}"))?;
        if !src.starts_with(&canon_out) {
            return Err("只能对图库里的图片做局部重绘".into());
        }

        let input_dir = std::path::PathBuf::from(&root).join("ComfyUI").join("input");
        std::fs::create_dir_all(&input_dir).map_err(|e| e.to_string())?;
        let stamp = now_ms();
        let src_name = format!("cs_inp_{stamp}.png");
        std::fs::copy(&src, input_dir.join(&src_name)).map_err(|e| format!("复制原图失败: {e}"))?;

        let b64 = req
            .mask_data_url
            .split_once(',')
            .map(|(_, b)| b.to_string())
            .unwrap_or_else(|| req.mask_data_url.clone());
        use base64::Engine as _;
        let mask_bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| format!("蒙版解码失败: {e}"))?;
        let mask_name = format!("cs_mask_{stamp}.png");
        std::fs::write(input_dir.join(&mask_name), mask_bytes)
            .map_err(|e| format!("写入蒙版失败: {e}"))?;

        let ref_name = if let Some(data_url) = &req.ref_image_data_url {
            let b64 = data_url
                .split_once(',')
                .map(|(_, b)| b.to_string())
                .unwrap_or_else(|| data_url.clone());
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|e| format!("参考图解码失败: {e}"))?;
            let name = format!("cs_ref_{stamp}.png");
            std::fs::write(input_dir.join(&name), bytes).map_err(|e| e.to_string())?;
            Some(name)
        } else {
            None
        };

        let ireq = api::InpaintReq {
            profile: req.profile.clone(),
            prompt: req.prompt.clone(),
            negative: req.negative.clone(),
            source: src_name,
            mask: mask_name,
            steps: req.steps,
            cfg: req.cfg,
            seed: req.seed,
            denoise: req.denoise,
            feather: req.feather,
            ref_image: ref_name,
        };
        let graph = api::build_inpaint(&ireq)?;

        let app2 = handle.clone();
        let out = api::run_job(port, &root, graph, 1800, move |p| {
            let _ = app2.emit(
                "gen-progress",
                json!({"elapsed": p.elapsed, "running": p.queue_running,
                       "pending": p.queue_pending, "started": p.started}),
            );
        })?;

        let base = api::profile(&req.profile)
            .map(|p| p.label.to_string())
            .unwrap_or_else(|| req.profile.clone());
        let label = format!("{base} · 局部");
        let (w, h) = out
            .images
            .first()
            .map(|i| (i.width, i.height))
            .unwrap_or((0, 0));
        let id = push_history(
            &handle, &req.profile, &label, "inpaint", &req.prompt, &req.negative,
            w, h, req.steps, req.cfg, req.seed, &out,
        );
        Ok(GenResp { out, turbo: false, label, history_id: id })
    })
    .await
    .map_err(|e| format!("局部重绘失败: {e}"))?
}

#[tauri::command]
fn upscale_models_list(app: tauri::AppHandle) -> Vec<String> {
    let state = app.state::<AppState>();
    let root = state.settings.lock().unwrap().comfy_root.clone();
    let dir = std::path::PathBuf::from(root)
        .join("ComfyUI")
        .join("models")
        .join("upscale_models");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if lower.ends_with(".pth") || lower.ends_with(".safetensors") || lower.ends_with(".pt") {
                out.push(name);
            }
        }
    }
    out.sort();
    out
}


#[tauri::command]
async fn upscale_input(
    app: tauri::AppHandle,
    name: String,
    model: Option<String>,
) -> Result<api::ImageOut, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let (port, root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone())
        };
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        let chosen = model.unwrap_or_default();
        if chosen == "__seedvr2__" {
            api::upscale_seedvr2_from_input(port, &root, &name)
        } else {
            let m = if chosen.is_empty() {
                pick_upscale_model(&root)?
            } else {
                chosen
            };
            api::upscale_from_input(port, &root, &name, &m)
        }
    })
    .await
    .map_err(|e| format!("超分任务失败: {e}"))?
}

/// Dataset -> LoRA training -> register the resulting file in models/loras.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrainCall {
    name: String,
    #[serde(default)]
    base: String,
    steps: u32,
    rank: u32,
    lr: f64,
    images: Vec<String>,
    #[serde(default)]
    captions: Vec<String>,
    #[serde(default)]
    existing_lora: Option<String>,
    #[serde(default = "default_train_size")]
    size: u32,
    #[serde(default)]
    keep_aspect: bool,
    #[serde(default = "one_u32")]
    batch: u32,
    #[serde(default = "one_u32")]
    grad_accum: u32,
    #[serde(default = "three_u32")]
    depth: u32,
    #[serde(default = "true_b")]
    offload: bool,
    #[serde(default = "bf16")]
    dtype: String,
    #[serde(default = "true_b")]
    restart: bool,
    #[serde(default)]
    annotations: Vec<Ann>,
    #[serde(default)]
    eval_prompts: u32,
}

fn one_u32() -> u32 {
    1
}
fn three_u32() -> u32 {
    3
}
fn true_b() -> bool {
    true
}
fn bf16() -> String {
    "bf16".to_string()
}

fn default_train_size() -> u32 {
    512
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrainFile {
    name: String,
    data_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrainFilesCall {
    name: String,
    #[serde(default)]
    base: String,
    steps: u32,
    rank: u32,
    lr: f64,
    files: Vec<TrainFile>,
    #[serde(default)]
    caption: String,
    #[serde(default)]
    auto_caption: bool,
    #[serde(default)]
    existing_lora: Option<String>,
    #[serde(default = "default_train_size")]
    size: u32,
    #[serde(default)]
    keep_aspect: bool,
    #[serde(default = "one_u32")]
    batch: u32,
    #[serde(default = "one_u32")]
    grad_accum: u32,
    #[serde(default = "three_u32")]
    depth: u32,
    #[serde(default = "true_b")]
    offload: bool,
    #[serde(default = "bf16")]
    dtype: String,
    #[serde(default = "true_b")]
    restart: bool,
    #[serde(default)]
    annotations: Vec<Ann>,
    #[serde(default)]
    eval_prompts: u32,
}


#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrainOut {
    lora: String,
    loss_image: Option<String>,
    #[serde(default)]
    eval_images: Vec<String>,
    #[serde(default)]
    avoid: String,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
struct Ann {
    #[serde(default)]
    want: String,
    #[serde(default)]
    avoid: String,
    #[serde(default = "one_u32")]
    weight: u32,
    /// normalized crop (x, y, w, h) in 0..1
    #[serde(default)]
    crop: Option<[f64; 4]>,
    /// normalized rectangles to blur away before training (x, y, w, h)
    #[serde(default)]
    erases: Vec<[f64; 4]>,
}

const EVAL_PROMPTS: [&str; 4] = [
    "a portrait photo, natural light, 50mm",
    "full body photo, standing in a city street, daylight",
    "a quiet interior, morning window light",
    "a still life on a wooden table, soft light",
];

fn train_slug(name: &str) -> Result<String, String> {
    let slug: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let slug = slug.trim_matches('_').to_string();
    if slug.is_empty() {
        return Err("给这个 LoRA 起个英文名字（字母/数字/下划线）".into());
    }
    Ok(slug)
}

fn write_train_png(
    img: image::DynamicImage,
    dst: &std::path::Path,
    side: u32,
    keep_aspect: bool,
) -> Result<(), String> {
    let (w, h) = (img.width(), img.height());
    let out = if keep_aspect {
        // fit the whole frame inside side x side, then mirror the borders outwards
        // so nothing is cropped and no stretched bands leak into training
        let fitted = img.resize(side, side, image::imageops::FilterType::Lanczos3).to_rgb8();
        let (fw, fh) = (fitted.width(), fitted.height());
        let x0 = (side - fw) / 2;
        let y0 = (side - fh) / 2;
        let mut canvas = image::RgbImage::new(side, side);
        image::imageops::replace(&mut canvas, &fitted, x0 as i64, y0 as i64);

        fn mirror(i: i64, len: i64) -> i64 {
            if len <= 1 {
                return 0;
            }
            let period = 2 * len;
            let m = i.rem_euclid(period);
            if m >= len {
                period - 1 - m
            } else {
                m
            }
        }

        for k in 0..y0 {
            let sy = (y0 as i64 + mirror(-(k as i64) - 1, fh as i64)) as u32;
            for x in 0..side {
                let px = *canvas.get_pixel(x, sy);
                canvas.put_pixel(x, y0 - k - 1, px);
            }
        }
        for k in 0..(side - y0 - fh) {
            let sy = (y0 as i64 + mirror(fh as i64 + k as i64, fh as i64)) as u32;
            for x in 0..side {
                let px = *canvas.get_pixel(x, sy);
                canvas.put_pixel(x, y0 + fh + k, px);
            }
        }
        for k in 0..x0 {
            let sx = (x0 as i64 + mirror(-(k as i64) - 1, fw as i64)) as u32;
            for y in 0..side {
                let px = *canvas.get_pixel(sx, y);
                canvas.put_pixel(x0 - k - 1, y, px);
            }
        }
        for k in 0..(side - x0 - fw) {
            let sx = (x0 as i64 + mirror(fw as i64 + k as i64, fw as i64)) as u32;
            for y in 0..side {
                let px = *canvas.get_pixel(sx, y);
                canvas.put_pixel(x0 + fw + k, y, px);
            }
        }
        image::DynamicImage::ImageRgb8(canvas)
    } else {
        let scale = (side as f32 / w.min(h) as f32).max(1.0);
        let (rw, rh) = (
            ((w as f32 * scale).round() as u32).max(side),
            ((h as f32 * scale).round() as u32).max(side),
        );
        img.resize_exact(rw, rh, image::imageops::FilterType::Lanczos3)
            .crop_imm((rw - side) / 2, (rh - side) / 2, side, side)
    };
    out.save_with_format(dst, image::ImageFormat::Png)
        .map_err(|e| format!("写入数据集失败: {e}"))
}

fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    let b64 = data_url.split_once(',').map(|(_, b)| b).unwrap_or(data_url);
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("图片解码失败: {e}"))
}

fn spawn_train_progress(
    app: tauri::AppHandle,
    port: u16,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    std::thread::spawn(move || {
        use std::sync::atomic::Ordering;
        use tungstenite::Message;
        let url = format!("ws://127.0.0.1:{port}/ws?clientId=comfyshell");
        let Ok((mut ws, _)) = tungstenite::connect(url) else {
            return;
        };
        if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = ws.get_mut() {
            let _ = tcp.set_read_timeout(Some(std::time::Duration::from_millis(500)));
        }
        while !done.load(Ordering::Relaxed) {
            match ws.read() {
                Ok(Message::Text(text)) => {
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
                        continue;
                    };
                    if v.get("type").and_then(|x| x.as_str()) != Some("progress") {
                        continue;
                    }
                    let data = v.get("data").cloned().unwrap_or_default();
                    let value = data.get("value").and_then(|x| x.as_u64()).unwrap_or(0);
                    let max = data.get("max").and_then(|x| x.as_u64()).unwrap_or(0);
                    let _ = app.emit("train-progress", json!({"value": value, "max": max}));
                }
                Ok(Message::Close(_)) => break,
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => break,
                _ => {}
            }
        }
    });
}

fn newest_file(dir: &std::path::Path, needle: &str, after: std::time::SystemTime) -> Option<std::path::PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut best: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if !name.contains(needle) {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let Ok(t) = meta.modified() else { continue };
        if t < after {
            continue;
        }
        if best.as_ref().map(|(bt, _)| t > *bt).unwrap_or(true) {
            best = Some((t, e.path()));
        }
    }
    best.map(|(_, p)| p)
}

/// Training wants a clean card: leftovers from generation make the memory
/// compiler fail ("aimdo … could not start recording"). Restart the child.
/// One JSON per training run: params + dataset + eval shots, for later comparison.
fn save_experiment(handle: &tauri::AppHandle, name: &str, data: serde_json::Value) {
    let dir = cfg_dir(handle).join("experiments");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(&data) {
        let _ = std::fs::write(dir.join(format!("{name}.json")), text);
    }
}

fn wait_port_free(port: u16, tries: u32) {
    for _ in 0..tries {
        if api::health(port).is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

fn restart_for_training(handle: &tauri::AppHandle) -> Result<(), String> {
    let state = handle.state::<AppState>();
    let port = state.settings.lock().unwrap().port;
    {
        let mut guard = state.child.lock().unwrap();
        if let Some(mut child) = guard.take() {
            server::kill(&mut child);
        }
    }
    wait_port_free(port, 40);

    // an orphaned ComfyUI still owns the port: kill it and wait for the socket
    if api::health(port).is_some() {
        if let Some(pid) = pid_listening_on(port) {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
        }
        wait_port_free(port, 40);
    }

    ensure_server(handle.clone())?;
    // ensure_server happily attaches to an already-healthy server; make sure we own it
    if state.child.lock().map(|c| c.is_none()).unwrap_or(true) {
        if let Some(pid) = pid_listening_on(port) {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
        }
        wait_port_free(port, 40);
        ensure_server(handle.clone())?;
    }
    Ok(())
}

#[cfg(windows)]
fn pid_listening_on(port: u16) -> Option<u32> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("netstat")
        .args(["-ano", "-p", "TCP"])
        .creation_flags(0x08000000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let needle = format!("127.0.0.1:{port}");
    for line in text.lines() {
        if line.contains(&needle) && line.contains("LISTENING") {
            if let Some(pid) = line.split_whitespace().last() {
                return pid.parse::<u32>().ok();
            }
        }
    }
    None
}

fn train_run(
    handle: &tauri::AppHandle,
    base: &str,
    slug: &str,
    ds_rel: &str,
    steps: u32,
    rank: u32,
    lr: f64,
    existing_lora: Option<String>,
    bucket: bool,
    opts_batch: u32,
    opts_grad: u32,
    opts_depth: u32,
    opts_offload: bool,
    opts_dtype: String,
    opts_restart: bool,
    avoid: Vec<String>,
    eval_prompts: u32,
) -> Result<TrainOut, String> {
    let state = handle.state::<AppState>();
    let (port, root) = {
        let st = state.settings.lock().unwrap();
        (st.port, st.comfy_root.clone())
    };
    if api::health(port).is_none() {
        return Err("ComfyUI 没有在运行".into());
    }
    let (ckpt, min_size) = api::train_checkpoint(base);
    let ckpt_path = std::path::PathBuf::from(&root)
        .join("ComfyUI")
        .join("models")
        .join("diffusion_models")
        .join(ckpt);
    let size = std::fs::metadata(&ckpt_path).map(|m| m.len()).unwrap_or(0);
    if size < min_size {
        return Err(format!(
            "{ckpt} 还没下载完整（当前 {:.1} GB），int8/fp8 量化权重无法反传。",
            size as f64 / 1_073_741_824.0
        ));
    }

    if opts_restart {
        restart_for_training(handle)?;
    }
    let managed = state.child.lock().map(|c| c.is_some()).unwrap_or(false);
    if !managed {
        return Err(
            "训练需要壳托管的 ComfyUI（带 --disable-pinned-memory）。请用顶部「停止」再「启动」重启一次。"
                .into(),
        );
    }
    // the caption model may still be resident; drop it before the training model loads
    api::free_models(port);
    std::thread::sleep(std::time::Duration::from_millis(1500));

    let resume = match existing_lora.as_deref() {
        Some(name) if !name.is_empty() => {
            let lora_dir = std::path::PathBuf::from(&root)
                .join("ComfyUI")
                .join("models")
                .join("loras");
            let ok = !name.contains('\\')
                && !name.contains('/')
                && lora_dir.join(name).exists();
            if !ok {
                return Err(format!("找不到要续训的 LoRA：{name}"));
            }
            if name == format!("{slug}.safetensors") {
                return Err("续训要换一个新名字（例如 my_style_v2），否则会覆盖原 LoRA".into());
            }
            Some(name.to_string())
        }
        _ => None,
    };
    let graph = api::build_train_graph(&api::TrainOpts {
        base,
        dataset_folder: ds_rel,
        name: slug,
        steps,
        rank,
        lr,
        existing_lora: resume.as_deref(),
        bucket,
        batch: opts_batch,
        grad_accum: opts_grad,
        depth: opts_depth,
        offload: opts_offload,
        dtype: &opts_dtype,
    })?;
    let started = std::time::SystemTime::now();
    let started_instant = std::time::Instant::now();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    spawn_train_progress(handle.clone(), port, done.clone());
    let app2 = handle.clone();
    let res = api::run_job_any(port, &root, graph, 7200, move |p| {
        let _ = app2.emit(
            "gen-progress",
            json!({"elapsed": p.elapsed, "running": p.queue_running,
                   "pending": p.queue_pending, "started": p.started}),
        );
    });
    done.store(true, std::sync::atomic::Ordering::Relaxed);
    let job = res?;
    let _ = started_instant;

    let output_dir = std::path::PathBuf::from(&root).join("ComfyUI").join("output");
    let lora_dir = std::path::PathBuf::from(&root)
        .join("ComfyUI")
        .join("models")
        .join("loras");
    std::fs::create_dir_all(&lora_dir).map_err(|e| e.to_string())?;
    let found = newest_file(&output_dir.join("loras"), slug, started)
        .or_else(|| newest_file(&lora_dir, slug, started))
        .or_else(|| newest_file(&output_dir, slug, started))
        .ok_or_else(|| "训练结束但没找到导出的 LoRA 文件".to_string())?;
    let final_path = lora_dir.join(format!("{slug}.safetensors"));
    if found != final_path {
        let _ = std::fs::remove_file(&final_path);
        std::fs::rename(&found, &final_path).map_err(|e| format!("移动训练产物失败: {e}"))?;
    }

    // LossGraphNode renders a temp preview; run_job already read it for us.
    let loss_image = job.images.first().map(|i| i.data_url.clone()).or_else(|| {
        newest_file(&output_dir, "_loss", started)
            .or_else(|| newest_file(&output_dir, "loss_graph", started))
            .and_then(|p| std::fs::read(p).ok())
            .map(|b| {
                use base64::Engine as _;
                format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(&b)
                )
            })
    });

    // fixed eval set: same prompts + seeds, generated with the fresh LoRA
    let mut eval_images: Vec<String> = Vec::new();
    let avoid_text = avoid.join(", ");
    if eval_prompts > 0 {
        for k in 0..eval_prompts as usize {
            let req = api::GenReq {
                profile: "flux3base".to_string(),
                mode: "t2i".to_string(),
                prompt: EVAL_PROMPTS[k % EVAL_PROMPTS.len()].to_string(),
                negative: avoid_text.clone(),
                width: 768,
                height: 512,
                steps: 20,
                cfg: 3.5,
                seed: 1000 + k as i64,
                batch: 1,
                lora: Some(format!("{slug}.safetensors")),
                lora_strength: 0.9,
                lora2: None,
                lora2_strength: 1.0,
                ref_images: Vec::new(),
                slg: false,
                slg_layers: "22".to_string(),
                layout: None,
            };
            if let Ok(graph) = api::build_graph(&req, false) {
                if let Ok(out) = api::run_job(port, &root, graph, 600, |_| {}) {
                    if let Some(im) = out.images.into_iter().next() {
                        eval_images.push(im.path);
                    }
                }
            }
        }
    }

    let _ = job;

    Ok(TrainOut {
        lora: format!("{slug}.safetensors"),
        loss_image,
        eval_images,
        avoid: avoid_text,
    })
}

#[tauri::command]
async fn train_lora(app: tauri::AppHandle, req: TrainCall) -> Result<TrainOut, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<TrainOut, String> {
        if req.images.is_empty() {
            return Err("没有选图".into());
        }
        let slug = train_slug(&req.name)?;
        let state = handle.state::<AppState>();
        let root = state.settings.lock().unwrap().comfy_root.clone();
        let input_dir = std::path::PathBuf::from(&root).join("ComfyUI").join("input");
        let ds_rel = format!("lora_ds_{slug}");
        let ds_dir = input_dir.join(&ds_rel);
        if ds_dir.exists() {
            std::fs::remove_dir_all(&ds_dir).map_err(|e| format!("清理旧数据集失败: {e}"))?;
        }
        std::fs::create_dir_all(&ds_dir).map_err(|e| e.to_string())?;

        for (i, src) in req.images.iter().enumerate() {
            let img = image::open(src).map_err(|e| format!("读取 {src} 失败: {e}"))?;
            let stem = format!("img_{i:03}");
            write_train_png(img, &ds_dir.join(format!("{stem}.png")), req.size, req.keep_aspect)?;
            let caption = req
                .captions
                .get(i)
                .filter(|c| !c.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| slug.replace('_', " "));
            std::fs::write(ds_dir.join(format!("{stem}.txt")), caption).map_err(|e| e.to_string())?;
        }

        let avoid: Vec<String> = req
            .annotations
            .iter()
            .map(|a| a.avoid.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();

        let out = train_run(
            &handle,
            &req.base,
            &slug,
            &ds_rel,
            req.steps,
            req.rank,
            req.lr,
            req.existing_lora.clone(),
            false,
            req.batch,
            req.grad_accum,
            req.depth,
            req.offload,
            req.dtype.clone(),
            req.restart,
            avoid.clone(),
            req.eval_prompts,
        )?;
        save_experiment(
            &handle,
            &slug,
            json!({
                "name": slug,
                "ts": now_ms(),
                "base": req.base,
                "size": req.size,
                "keepAspect": req.keep_aspect,
                "steps": req.steps,
                "rank": req.rank,
                "lr": req.lr,
                "batch": req.batch,
                "gradAccum": req.grad_accum,
                "depth": req.depth,
                "offload": req.offload,
                "dtype": req.dtype,
                "restart": req.restart,
                "samples": req.images.len(),
                "annotations": req.annotations.iter().map(|a| json!({
                    "want": a.want, "avoid": a.avoid, "weight": a.weight
                })).collect::<Vec<_>>(),
                "avoid": out.avoid,
                "lora": out.lora,
                "eval": out.eval_images,
            }),
        );
        Ok(out)
    })
    .await
    .map_err(|e| format!("训练任务失败: {e}"))?
}

/// Train from an arbitrary folder picked in the UI (images are auto-cropped + captioned).
#[tauri::command]
async fn train_lora_files(app: tauri::AppHandle, req: TrainFilesCall) -> Result<TrainOut, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<TrainOut, String> {
        if req.files.is_empty() {
            return Err("文件夹里没有图片".into());
        }
        let slug = train_slug(&req.name)?;
        let state = handle.state::<AppState>();
        let root = state.settings.lock().unwrap().comfy_root.clone();
        let input_dir = std::path::PathBuf::from(&root).join("ComfyUI").join("input");
        let ds_rel = format!("lora_ds_{slug}");
        let ds_dir = input_dir.join(&ds_rel);
        if ds_dir.exists() {
            std::fs::remove_dir_all(&ds_dir).map_err(|e| format!("清理旧数据集失败: {e}"))?;
        }
        std::fs::create_dir_all(&ds_dir).map_err(|e| e.to_string())?;

        let prefix = req.caption.trim().to_string();
        let fallback = if prefix.is_empty() {
            slug.replace('_', " ")
        } else {
            prefix.clone()
        };
        let port = state.settings.lock().unwrap().port;
        let total = req.files.len();
        let stamp = now_ms();

        // first pass: decode, crop into the dataset, keep the raw bytes for captioning
        let mut written = 0usize;
        let mut valid: Vec<usize> = Vec::new();
        let mut cap_names: Vec<String> = Vec::new();
        for (i, f) in req.files.iter().enumerate() {
            let Ok(bytes) = decode_data_url(&f.data_url) else { continue };
            let Ok(img) = image::load_from_memory(&bytes) else { continue };
            let ann = req.annotations.get(i).cloned().unwrap_or_default();
            // red marks: blur those regions out before anything else is learned
            let mut rgba = img.to_rgba8();
            for r in &ann.erases {
                let (w, h) = (rgba.width(), rgba.height());
                let x = (r[0].clamp(0.0, 1.0) * w as f64) as u32;
                let y = (r[1].clamp(0.0, 1.0) * h as f64) as u32;
                let cw = ((r[2].clamp(0.01, 1.0) * w as f64) as u32).clamp(4, w.saturating_sub(x).max(4));
                let ch = ((r[3].clamp(0.01, 1.0) * h as f64) as u32).clamp(4, h.saturating_sub(y).max(4));
                let patch = image::imageops::crop_imm(&rgba, x, y, cw, ch).to_image();
                let blurred = image::imageops::blur(&patch, (cw.min(ch) as f32 / 6.0).clamp(4.0, 28.0));
                image::imageops::replace(&mut rgba, &blurred, x as i64, y as i64);
            }
            let img = image::DynamicImage::ImageRgba8(rgba);
            let img = if let Some(c) = ann.crop {
                let (w, h) = (img.width(), img.height());
                let x = (c[0].clamp(0.0, 1.0) * w as f64) as u32;
                let y = (c[1].clamp(0.0, 1.0) * h as f64) as u32;
                let cw = ((c[2].clamp(0.02, 1.0) * w as f64) as u32).clamp(16, w.saturating_sub(x).max(16));
                let ch = ((c[3].clamp(0.02, 1.0) * h as f64) as u32).clamp(16, h.saturating_sub(y).max(16));
                img.crop_imm(x, y, cw, ch)
            } else {
                img
            };
            let stem = format!("img_{written:03}");
            write_train_png(img, &ds_dir.join(format!("{stem}.png")), req.size, req.keep_aspect)?;
            if req.auto_caption {
                let temp_name = format!("cs_cap_{stamp}_{i}.png");
                if std::fs::write(input_dir.join(&temp_name), &bytes).is_ok() {
                    cap_names.push(temp_name);
                }
            }
            valid.push(i);
            written += 1;
        }
        if written == 0 {
            return Err("这些文件都不是可读的图片".into());
        }

        // one prompt for all captions: the vision model loads once
        let mut auto_texts: Vec<Option<String>> = vec![None; written];
        if req.auto_caption && !cap_names.is_empty() {
            let _ = handle.emit("caption-progress", json!({"done": 0, "total": cap_names.len()}));
            let texts = enhance::caption_batch(port, &cap_names);
            for (i, text) in texts.into_iter().enumerate() {
                if i < auto_texts.len() {
                    auto_texts[i] = text;
                }
            }
            for name in &cap_names {
                let _ = std::fs::remove_file(input_dir.join(name));
            }
            let _ = handle.emit("caption-progress", json!({"done": cap_names.len(), "total": cap_names.len()}));
        }

        // annotations: "want" overrides the caption, "weight" duplicates the sample
        for (slot, &src_idx) in valid.iter().enumerate() {
            let ann = req.annotations.get(src_idx).cloned().unwrap_or_default();
            let want = ann.want.trim();
            let mut caption = if !want.is_empty() {
                if prefix.is_empty() {
                    want.to_string()
                } else {
                    format!("{prefix}, {want}")
                }
            } else if let Some(Some(text)) = auto_texts.get(slot) {
                if prefix.is_empty() {
                    text.trim().to_string()
                } else {
                    format!("{prefix}, {}", text.trim())
                }
            } else {
                fallback.clone()
            };
            if caption.trim().is_empty() {
                caption = fallback.clone();
            }
            let stem = format!("img_{slot:03}");
            std::fs::write(ds_dir.join(format!("{stem}.txt")), &caption).map_err(|e| e.to_string())?;
            for k in 1..ann.weight.clamp(1, 3) {
                let dup = format!("{stem}_w{k}");
                let _ = std::fs::copy(ds_dir.join(format!("{stem}.png")), ds_dir.join(format!("{dup}.png")));
                let _ = std::fs::write(ds_dir.join(format!("{dup}.txt")), &caption);
            }
        }

        let avoid: Vec<String> = req
            .annotations
            .iter()
            .map(|a| a.avoid.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();

        let out = train_run(
            &handle,
            &req.base,
            &slug,
            &ds_rel,
            req.steps,
            req.rank,
            req.lr,
            req.existing_lora.clone(),
            false,
            req.batch,
            req.grad_accum,
            req.depth,
            req.offload,
            req.dtype.clone(),
            req.restart,
            avoid,
            req.eval_prompts,
        )?;
        save_experiment(
            &handle,
            &slug,
            json!({
                "name": slug,
                "ts": now_ms(),
                "base": req.base,
                "size": req.size,
                "steps": req.steps,
                "rank": req.rank,
                "lr": req.lr,
                "samples": req.files.len(),
                "lora": out.lora,
                "eval": out.eval_images,
            }),
        );
        Ok(out)
    })
    .await
    .map_err(|e| format!("训练任务失败: {e}"))?
}


#[tauri::command]
fn open_folder(path: String) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

#[tauri::command]
fn reveal_file(path: String) {
    let _ = std::process::Command::new("explorer")
        .arg(format!("/select,{path}"))
        .spawn();
}

#[tauri::command]
async fn story_split(app: tauri::AppHandle, text: String, max_panels: u32) -> Result<Vec<String>, String> {
    if text.trim().is_empty() {
        return Err("先贴一段剧情".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let port = handle.state::<AppState>().settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        Ok(enhance::story_split(port, &text, max_panels))
    })
    .await
    .map_err(|e| format!("拆分任务失败: {e}"))?
}




#[tauri::command]
async fn assist(app: tauri::AppHandle, request: String, image: Option<String>) -> Result<serde_json::Value, String> {
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
async fn analyze_image(app: tauri::AppHandle, image: String, two_pass: Option<bool>) -> Result<String, String> {
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
async fn train_kohya_files(app: tauri::AppHandle, req: TrainFilesCall) -> Result<TrainOut, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<TrainOut, String> {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};

        if req.files.is_empty() {
            return Err("文件夹里没有图片".into());
        }
        let slug = train_slug(&req.name)?;
        let state = handle.state::<AppState>();
        let (root, kohya_py, kohya_dir) = {
            let st = state.settings.lock().unwrap();
            (st.comfy_root.clone(), st.kohya_python.clone(), st.kohya_dir.clone())
        };
        if !std::path::Path::new(&kohya_py).exists() {
            return Err(format!("找不到 kohya 的 python：{kohya_py}"));
        }

        let input_dir = std::path::PathBuf::from(&root).join("ComfyUI").join("input");
        let ds_rel = format!("lora_ds_{slug}");
        let ds_dir = input_dir.join(&ds_rel);
        if ds_dir.exists() {
            let _ = std::fs::remove_dir_all(&ds_dir);
        }
        std::fs::create_dir_all(&ds_dir).map_err(|e| e.to_string())?;

        let prefix = req.caption.trim().to_string();
        let fallback = if prefix.is_empty() { slug.replace('_', " ") } else { prefix.clone() };
        let mut written = 0usize;
        for (i, f) in req.files.iter().enumerate() {
            let Ok(bytes) = decode_data_url(&f.data_url) else { continue };
            let Ok(img) = image::load_from_memory(&bytes) else { continue };
            let ann = req.annotations.get(i).cloned().unwrap_or_default();
            let mut rgba = img.to_rgba8();
            for r in &ann.erases {
                let (w, h) = (rgba.width(), rgba.height());
                let x = (r[0].clamp(0.0, 1.0) * w as f64) as u32;
                let y = (r[1].clamp(0.0, 1.0) * h as f64) as u32;
                let cw = ((r[2].clamp(0.01, 1.0) * w as f64) as u32).clamp(4, w.saturating_sub(x).max(4));
                let ch = ((r[3].clamp(0.01, 1.0) * h as f64) as u32).clamp(4, h.saturating_sub(y).max(4));
                let patch = image::imageops::crop_imm(&rgba, x, y, cw, ch).to_image();
                let blurred = image::imageops::blur(&patch, (cw.min(ch) as f32 / 6.0).clamp(4.0, 28.0));
                image::imageops::replace(&mut rgba, &blurred, x as i64, y as i64);
            }
            let mut img = image::DynamicImage::ImageRgba8(rgba);
            if let Some(c) = ann.crop {
                let (w, h) = (img.width(), img.height());
                let x = (c[0].clamp(0.0, 1.0) * w as f64) as u32;
                let y = (c[1].clamp(0.0, 1.0) * h as f64) as u32;
                let cw = ((c[2].clamp(0.02, 1.0) * w as f64) as u32).clamp(16, w.saturating_sub(x).max(16));
                let ch = ((c[3].clamp(0.02, 1.0) * h as f64) as u32).clamp(16, h.saturating_sub(y).max(16));
                img = img.crop_imm(x, y, cw, ch);
            }
            let stem = format!("img_{written:03}");
            write_train_png(img, &ds_dir.join(format!("{stem}.png")), req.size, req.keep_aspect)?;
            let want = ann.want.trim();
            let caption = if want.is_empty() { fallback.clone() } else { want.to_string() };
            std::fs::write(ds_dir.join(format!("{stem}.txt")), caption).map_err(|e| e.to_string())?;
            written += 1;
        }
        if written == 0 {
            return Err("这些文件都不是可读的图片".into());
        }

        let cfg_dir = cfg_dir(&handle);
        let run_dir = cfg_dir.join("kohya_runs").join(&slug);
        let _ = std::fs::create_dir_all(&run_dir);
        let toml_path = run_dir.join("dataset.toml");
        let ds_posix = ds_dir.to_string_lossy().replace('\\', "/");
        std::fs::write(
            &toml_path,
            format!(
                "[general]\nenable_bucket = true\nbucket_reso_steps = 64\ncaption_extension = \".txt\"\n\n[[datasets]]\nresolution = {}\nbatch_size = 1\n  [[datasets.subsets]]\n  image_dir = \"{}\"\n  num_repeats = 1\n",
                req.size, ds_posix
            ),
        )
        .map_err(|e| e.to_string())?;

        let unet = if req.base == "mossbase" {
            "moss_image_base_int8_convrot.safetensors"
        } else {
            "moss_image_sft_int8_convrot.safetensors"
        };
        let models = std::path::PathBuf::from(&root).join("ComfyUI").join("models");
        let script = std::path::Path::new(&kohya_dir).join("moss_image_train_network.py");
        if !script.exists() {
            return Err(format!("找不到 kohya 脚本：{}", script.display()));
        }
        let mut cmd = Command::new(&kohya_py);
        cmd.current_dir(&kohya_dir)
            .arg(&script)
            .arg(format!("--pretrained_model_name_or_path={}", models.join("diffusion_models").join(unet).display()))
            .arg(format!(
                "--text_encoder={}",
                models.join("text_encoders").join("qwen3.5_9b_int8_convrot.safetensors").display()
            ))
            .arg(format!("--vae={}", models.join("vae").join("flux2-vae.safetensors").display()))
            .arg(format!("--dataset_config={}", toml_path.display()))
            .arg(format!("--output_dir={}", run_dir.display()))
            .arg(format!("--output_name={slug}"))
            .arg("--save_model_as=safetensors")
            .arg("--network_module=networks.lora_generic")
            .arg(format!("--network_dim={}", req.rank.max(1)))
            .arg("--network_train_unet_only")
            .arg(format!("--learning_rate={}", req.lr))
            .arg("--optimizer_type=AdamW")
            .arg("--lr_scheduler=constant")
            .arg("--timestep_sampling=shift")
            .arg("--discrete_flow_shift=3.0")
            .arg(format!("--max_train_steps={}", req.steps.max(1)))
            .arg("--mixed_precision=bf16")
            .arg("--gradient_checkpointing")
            .arg("--cache_latents")
            .arg("--cache_latents_to_disk")
            .arg("--cache_text_encoder_outputs")
            .arg("--cache_text_encoder_outputs_to_disk")
            .arg("--blocks_to_swap=20")
            .arg("--max_data_loader_n_workers=2")
            .arg("--persistent_data_loader_workers")
            .arg("--seed=42")
            .arg(format!("--logging_dir={}", run_dir.join("logs").display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("启动 kohya 失败: {e}"))?;
        let stdout = child.stdout.take().ok_or("无法读取 kohya 输出")?;
        let stderr = child.stderr.take().ok_or("无法读取 kohya 错误")?;

        let app_out = handle.clone();
        let reader = std::thread::spawn(move || {
            let r = BufReader::new(stdout);
            for line in r.lines().flatten() {
                if let Some((cur, max)) = parse_train_progress(&line) {
                    let _ = app_out.emit("train-progress", json!({"value": cur, "max": max}));
                }
            }
        });
        let app_err = handle.clone();
        let errs = std::thread::spawn(move || {
            let r = BufReader::new(stderr);
            let mut tail: Vec<String> = Vec::new();
            for line in r.lines().flatten() {
                tail.push(line);
                if tail.len() > 40 {
                    tail.remove(0);
                }
            }
            let _ = app_err;
            tail
        });

        let status = child.wait().map_err(|e| e.to_string())?;
        let _ = reader.join();
        let tail = errs.join().unwrap_or_default();
        if !status.success() {
            let msg = tail.last().cloned().unwrap_or_else(|| "kohya 退出码非 0".into());
            return Err(format!("kohya 训练失败: {msg}"));
        }

        let produced = run_dir.join(format!("{slug}.safetensors"));
        if !produced.exists() {
            return Err("kohya 结束但没有产物".into());
        }
        let lora_dir = models.join("loras");
        std::fs::create_dir_all(&lora_dir).ok();
        let final_path = lora_dir.join(format!("{slug}.safetensors"));
        std::fs::copy(&produced, &final_path).map_err(|e| format!("复制产物失败: {e}"))?;

        save_experiment(
            &handle,
            &slug,
            json!({
                "name": slug,
                "ts": now_ms(),
                "engine": "kohya",
                "base": req.base,
                "size": req.size,
                "steps": req.steps,
                "rank": req.rank,
                "lr": req.lr,
                "samples": req.files.len(),
                "lora": format!("{slug}.safetensors"),
                "eval": [],
            }),
        );

        Ok(TrainOut {
            lora: format!("{slug}.safetensors"),
            loss_image: None,
            eval_images: Vec::new(),
            avoid: String::new(),
        })
    })
    .await
    .map_err(|e| format!("kohya 任务失败: {e}"))?
}

fn parse_train_progress(line: &str) -> Option<(u64, u64)> {
    for tok in line.split(|c: char| c == '|' || c == ' ' || c == '[' || c == ']') {
        let tok = tok.trim();
        if let Some((a, b)) = tok.split_once('/') {
            if let (Ok(a), Ok(b)) = (a.trim().parse::<u64>(), b.trim().parse::<u64>()) {
                if b > 0 && a <= b {
                    return Some((a, b));
                }
            }
        }
    }
    None
}

#[tauri::command]
fn job_cancel(state: State<AppState>) {
    *state.cancel.lock().unwrap() = true;
    let port = state.settings.lock().unwrap().port;
    api::interrupt(port);
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = load_settings(&handle);
            app.manage(AppState {
                child: Mutex::new(None),
                settings: Mutex::new(settings.clone()),
                cancel: Mutex::new(false),
            });

            // Silent auto-start in the background: the window shows up immediately.
            if settings.auto_start {
                std::thread::spawn(move || {
                    let _ = ensure_server(handle);
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle().clone();
                let state = app.state::<AppState>();
                let stop = state.settings.lock().unwrap().stop_on_exit;
                if stop {
                    let mut guard = state.child.lock().unwrap();
                    if let Some(mut child) = guard.take() {
                        server::kill(&mut child);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            settings_get,
            settings_set,
            server_status,
            server_start,
            server_stop,
            models_list,
            profiles,
            save_ref_image,
            read_input_image,
            poses_catalog,
            read_output_image,
            generate,
            enhance_prompt,
            history_list,
            history_clear,
            custom_list,
            custom_save,
            custom_delete,
            library_scan,
            library_delete,
            delete_results,
            library_star,
            stars_list,
            save_export,
            translate_text,
            upscale_image,
            upscale_models_list,
            inpaint_image,
            upscale_input,
            train_lora,
            assist,
            analyze_image,
            train_lora_files,
            train_kohya_files,
            story_split,
            job_cancel,
            open_folder,
            reveal_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}















