//! 生成 / 扩写 / 超分 / 局部重绘。

use crate::*;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenResp {
    #[serde(flatten)]
    pub(crate) out: api::GenOut,
    pub(crate) turbo: bool,
    pub(crate) label: String,
    pub(crate) history_id: String,
}


pub fn push_history(
    app: &tauri::AppHandle,
    profile: &str,
    label: &str,
    mode: &str,
    prompt: &str,
    negative: &str,
    project: Option<String>,
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
        project,
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
pub async fn generate(app: tauri::AppHandle, mut req: api::GenReq, turbo: bool) -> Result<GenResp, String> {
    // 调试构建只许在 `_test` 里生成，别拿真实项目做调试。
    if cfg!(debug_assertions) && req.project.as_deref() != Some("_test") {
        return Err("调试构建只能在「测试项目」里生成（先在项目栏选「测试项目」）".into());
    }
    // 私密项目：先落 staging，任务结束后用任务开始时拿到的密钥封存。
    let cfg0 = cfg_dir(&app);
    let private = req.project.as_deref().and_then(|id| {
        if vault::is_private(&cfg0, id) {
            Some(id.to_string())
        } else {
            None
        }
    });
    let priv_key = if let Some(id) = &private {
        let key = vault::key_clone().ok_or("锁定区没解锁")?;
        req.staging = true;
        Some((id.clone(), key))
    } else {
        None
    };
    // Long-running: submit, poll for up to 30 minutes, read images back.
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<GenResp, String> {
        let state = handle.state::<AppState>();
        let (port, root, out_root) = {
            let s = state.settings.lock().unwrap().clone();
            let out = crate::state::output_root(&s);
            (s.port, s.comfy_root, out)
        };
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行，请先点「启动」".into());
        }
        *state.cancel.lock().unwrap() = false;

        // #88：随机种子（-1）在提交前就定下来——后面元数据/历史/版本都写这个真实值，
        // 不然它们只能记 -1，「用这张的参数再生成」也就复现不了。
        if req.seed < 0 {
            req.seed = api::pick_seed(-1);
        }

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
            let mut g = custom::substitute(
                &base, &req.prompt, &req.negative, seed, req.width, req.height, req.steps, req.cfg,
            );
            if let Some(ov) = &req.wf_params {
                custom::apply_overrides(&mut g, ov);
            }
            g
        } else {
            api::build_graph(&req, turbo)?
        };

        let app2 = handle.clone();
        let mut out = api::run_job(port, &root, graph, 1800, move |p| {
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

        let id = if let Some((pid, key)) = &priv_key {
            let paths: Vec<String> = out.images.iter().map(|i| i.path.clone()).collect();
            let sealed = vault::seal_paths(&cfg, pid, &paths, key)?;
            for (img, url) in out.images.iter_mut().zip(sealed) {
                img.path = url;
            }
            let staging = out_root.join("_private_staging").join(pid);
            let _ = std::fs::remove_dir_all(staging);
            String::new()
        } else {
            push_history(
                &handle, &req.profile, &label, &req.mode, &req.prompt, &req.negative,
                req.project.clone(), req.width, req.height, req.steps, req.cfg, req.seed, &out,
            )
        };

        let mut version_id = String::new();
        if priv_key.is_none() {
            if let Some(pid) = req.project.as_deref() {
                if !vault::is_private(&cfg, pid) {
                    let seed = versions::VersionSeed {
                        prompt: req.prompt.clone(),
                        negative: req.negative.clone(),
                        profile: req.profile.clone(),
                        mode: req.mode.clone(),
                        width: req.width,
                        height: req.height,
                        steps: req.steps,
                        cfg: req.cfg,
                        seed: req.seed,
                        layout: req.layout.clone(),
                    };
                    let paths: Vec<String> = out.images.iter().map(|i| i.path.clone()).collect();
                    if let Ok(v) = versions::append(
                        &cfg, pid, &seed, &paths, &id, req.version_parent.as_deref(),
                    ) {
                        version_id = v.id;
                    }
                }
            }
            let meta = json!({
                "app": "comfy-shell",
                "schema": 1,
                "appVersion": env!("CARGO_PKG_VERSION"),
                "createdAt": now_ms(),
                "mode": req.mode,
                "prompt": req.prompt,
                "negative": req.negative,
                "profile": req.profile,
                "width": req.width,
                "height": req.height,
                "steps": req.steps,
                "cfg": req.cfg,
                "seed": req.seed,
                "batch": req.batch,
                "lora": req.lora,
                "loraStrength": req.lora_strength,
                "lora2": req.lora2,
                "lora2Strength": req.lora2_strength,
                "slg": req.slg,
                "slgLayers": req.slg_layers,
                "layout": req.layout,
                "project": req.project,
                "versionId": version_id,
                "historyId": id,
            });
            for img in out.images.iter_mut() {
                if !img.path.starts_with("vault://") {
                    let _ = crate::imgmeta::write_png_meta(
                        std::path::Path::new(&img.path),
                        &meta,
                    );
                }
            }
        }

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
pub async fn enhance_prompt(
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


pub(crate) fn project_id_of_path(out_dir: &std::path::Path, src: &std::path::Path) -> Option<String> {
    let rel = src.strip_prefix(out_dir).ok()?;
    let mut comps = rel.components();
    if comps.next()?.as_os_str() != "projects" {
        return None;
    }
    let id = comps.next()?.as_os_str().to_str()?;
    if id.is_empty() || id == "_trash" {
        return None;
    }
    Some(id.to_string())
}

/// 改图/超分的产物默认落 output 根；源图在项目里时搬回同项目目录。
/// Windows 的 canonicalize 会带 \\?\ 前缀，存进元数据/历史前统一去掉。
pub(crate) fn strip_win_prefix(p: &str) -> String {
    p.strip_prefix(r"\\?\").unwrap_or(p).to_string()
}

pub(crate) fn relocate_into_project(out_dir: &std::path::Path, src: &std::path::Path, img: &mut api::ImageOut) {
    let Some(id) = project_id_of_path(out_dir, src) else {
        return;
    };
    let dir = out_dir.join("projects").join(&id);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let name = std::path::Path::new(&img.path)
        .file_name()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_default();
    if name.is_empty() {
        return;
    }
    let mut target = dir.join(&name);
    // 超分输出名固定是 upscaled_00001_，多次超分会撞名；撞了就补时间戳
    if target.exists() {
        let stem = std::path::Path::new(&name)
            .file_stem()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| "img".to_string());
        let ext = std::path::Path::new(&name)
            .extension()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_else(|| "png".to_string());
        target = dir.join(format!("{stem}_{}.{}", now_ms(), ext));
    }
    let from = std::path::PathBuf::from(strip_win_prefix(&img.path));
    if from == target {
        img.path = target.to_string_lossy().to_string();
        return;
    }
    if std::fs::rename(&from, &target).is_ok() {
        img.path = target.to_string_lossy().to_string();
        img.subfolder = format!("projects/{id}");
    }
}
