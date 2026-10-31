//! 超分 / 局部重绘。

use crate::*;
use crate::commands::generation::{push_history, relocate_into_project, GenResp};
use tauri::State;

pub fn pick_upscale_model(root: &str) -> Result<String, String> {
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
pub async fn upscale_image(
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
        let mut img = if chosen == "__seedvr2__" {
            api::upscale_seedvr2(port, &root, &src.to_string_lossy())?
        } else {
            api::upscale_image(port, &root, &src.to_string_lossy(), &chosen)?
        };
        relocate_into_project(&canon_out, &src, &mut img);
        Ok(img)
    })
    .await
    .map_err(|e| format!("超分任务失败: {e}"))?
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InpaintCall {
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

pub fn one_f() -> f64 {
    1.0
}

#[tauri::command]
pub async fn inpaint_image(app: tauri::AppHandle, req: InpaintCall) -> Result<GenResp, String> {
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
        let mut out = api::run_job(port, &root, graph, 1800, move |p| {
            let _ = app2.emit(
                "gen-progress",
                json!({"elapsed": p.elapsed, "running": p.queue_running,
                       "pending": p.queue_pending, "started": p.started}),
            );
        })?;
        for img in out.images.iter_mut() {
            relocate_into_project(&canon_out, &src, img);
        }

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
pub fn upscale_models_list(app: tauri::AppHandle) -> Vec<String> {
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
pub async fn upscale_input(
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
