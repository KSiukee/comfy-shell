//! 内置训练命令（txt2img / 文件数据集）。

use crate::*;
use crate::commands::training::*;
use crate::commands::training_run::train_run;
use tauri::State;

#[tauri::command]
pub async fn train_lora(app: tauri::AppHandle, req: TrainCall) -> Result<TrainOut, String> {
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
pub async fn train_lora_files(app: tauri::AppHandle, req: TrainFilesCall) -> Result<TrainOut, String> {
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
