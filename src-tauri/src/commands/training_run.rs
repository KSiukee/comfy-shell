//! 内置训练执行循环。

use crate::*;
use crate::commands::training::*;
use tauri::State;

pub fn train_run(
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
    let (port, root, out_root) = {
        let st = state.settings.lock().unwrap();
        (st.port, st.comfy_root.clone(), crate::state::output_root(&st))
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

    let output_dir = out_root;
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
                wf_params: None,
                sampler: None,
                edit_resolution: None,
                project: None,
                staging: false,
                version_parent: None,
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
