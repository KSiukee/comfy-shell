//! kohya 外挂训练。

use crate::*;
use crate::commands::training::*;
use tauri::State;

#[tauri::command]
pub async fn train_kohya_files(app: tauri::AppHandle, req: TrainFilesCall) -> Result<TrainOut, String> {
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

pub fn parse_train_progress(line: &str) -> Option<(u64, u64)> {
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
