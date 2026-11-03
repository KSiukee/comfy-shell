//! 超分 / 局部重绘。

use crate::upscale_meta::merge_upscale_meta;
use crate::*;
use crate::commands::generation::{project_id_of_path, push_history, relocate_into_project, GenResp};
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
pub async fn upscale_image(    app: tauri::AppHandle,
    path: String,
    model: Option<String>,
) -> Result<api::ImageOut, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let (port, root, out_root, replace) = {
            let s = state.settings.lock().unwrap();
            (s.port, s.comfy_root.clone(), crate::state::output_root(&s), s.replace_original)
        };
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        let out_dir = out_root;
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
        // 统一成不带 `\\?\` 的路径：图库/收藏/历史都按这个形状比对，带了前缀就对不上
        img.path = crate::commands::generation::strip_win_prefix(&img.path);
        // #86：开启「超分后替换原图」时，先把旧图的参数块读出来（下面要合并到新图上）
        let old_name = src
            .file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_default();
        let old_meta = if replace {
            crate::imgmeta::read_meta_under(&canon_out, &src)
        } else {
            None
        };
        let base_meta = json!({
            "app": "comfy-shell", "schema": 1, "appVersion": env!("CARGO_PKG_VERSION"),
            "createdAt": now_ms(), "mode": "upscale",
            "project": project_id_of_path(&canon_out, &src),
            "source": old_name,
            "model": chosen,
            "width": img.width, "height": img.height,
        });
        let meta = merge_upscale_meta(old_meta.as_ref(), base_meta, &old_name);
        if !img.path.starts_with("vault://") {
            let _ = crate::imgmeta::write_png_meta(std::path::Path::new(&img.path), &meta);
            // 超分结果也要进历史，否则重启/切项目后图区看不到（只在图库里）
            let label = if chosen == "__seedvr2__" {
                "SeedVR2 4×".to_string()
            } else {
                format!("超分 · {chosen}")
            };
            let item = crate::history::HistoryItem {
                id: format!("up-{}", now_ms()),
                ts: now_ms(),
                profile: "upscale".to_string(),
                label,
                mode: "upscale".to_string(),
                prompt: src
                    .file_name()
                    .map(|x| x.to_string_lossy().to_string())
                    .unwrap_or_default(),
                negative: String::new(),
                width: img.width,
                height: img.height,
                steps: 0,
                cfg: 0.0,
                seed: 0,
                seconds: 0.0,
                project: project_id_of_path(&canon_out, &src),
                images: vec![crate::history::HistoryImage {
                    filename: img.filename.clone(),
                    subfolder: img.subfolder.clone(),
                    path: crate::commands::generation::strip_win_prefix(&img.path),
                    width: img.width,
                    height: img.height,
                }],
            };
            crate::history::append(&crate::state::cfg_dir(&handle), item);

            // #86：收藏跟到新图，然后把原图挪进回收站。
            // 先摘旧收藏再删，否则 _favorites 里会留一个指向废图的硬链接。
            if replace {
                let cfg = crate::state::cfg_dir(&handle);
                let old_path = crate::commands::generation::strip_win_prefix(&src.to_string_lossy());
                let starred = crate::library::stars(&cfg)
                    .iter()
                    .any(|p| crate::commands::generation::strip_win_prefix(p) == old_path);
                if starred {
                    crate::library::star(&cfg, &canon_out, &img.path, true);
                    crate::library::star(&cfg, &canon_out, &old_path, false);
                }
                let _ = crate::library::delete(&canon_out, &[old_path]);
            }
        }
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
pub async fn inpaint_image(app: tauri::AppHandle, mut req: InpaintCall) -> Result<GenResp, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<GenResp, String> {
        // #88：随机种子在提交前定下来，元数据/历史才能记真实值
        if req.seed < 0 {
            req.seed = api::pick_seed(-1);
        }
        let state = handle.state::<AppState>();
        let (port, root, out_root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone(), crate::state::output_root(&st))
        };
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }

        // source must be one of our outputs
        let out_dir = out_root;
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
        let meta = json!({
            "app": "comfy-shell", "schema": 1, "appVersion": env!("CARGO_PKG_VERSION"),
            "createdAt": now_ms(), "mode": "inpaint",
            "prompt": req.prompt, "negative": req.negative, "profile": req.profile,
            "steps": req.steps, "cfg": req.cfg, "seed": req.seed,
            "project": project_id_of_path(&canon_out, &src),
            "source": src.file_name().map(|x| x.to_string_lossy().to_string()),
        });
        for img in out.images.iter_mut() {
            relocate_into_project(&canon_out, &src, img);
            if !img.path.starts_with("vault://") {
                let _ = crate::imgmeta::write_png_meta(std::path::Path::new(&img.path), &meta);
            }
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
            None, w, h, req.steps, req.cfg, req.seed, &out,
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
        let mut img = if chosen == "__seedvr2__" {
            api::upscale_seedvr2_from_input(port, &root, &name)?
        } else {
            let m = if chosen.is_empty() {
                pick_upscale_model(&root)?
            } else {
                chosen
            };
            api::upscale_from_input(port, &root, &name, &m)?
        };
        let meta = json!({
            "app": "comfy-shell", "schema": 1, "appVersion": env!("CARGO_PKG_VERSION"),
            "createdAt": now_ms(), "mode": "upscale",
            "source": name, "width": img.width, "height": img.height,
        });
        let _ = crate::imgmeta::write_png_meta(std::path::Path::new(&img.path), &meta);
        Ok(img)
    })
    .await
    .map_err(|e| format!("超分任务失败: {e}"))?
}
