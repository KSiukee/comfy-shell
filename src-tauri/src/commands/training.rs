//! 训练相关命令与辅助（内置训练 / kohya）。

use crate::*;
use tauri::State;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainCall {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) base: String,
    pub(crate) steps: u32,
    pub(crate) rank: u32,
    pub(crate) lr: f64,
    pub(crate) images: Vec<String>,
    #[serde(default)]
    pub(crate) captions: Vec<String>,
    #[serde(default)]
    pub(crate) existing_lora: Option<String>,
    #[serde(default = "default_train_size")]
    pub(crate) size: u32,
    #[serde(default)]
    pub(crate) keep_aspect: bool,
    #[serde(default = "one_u32")]
    pub(crate) batch: u32,
    #[serde(default = "one_u32")]
    pub(crate) grad_accum: u32,
    #[serde(default = "three_u32")]
    pub(crate) depth: u32,
    #[serde(default = "true_b")]
    pub(crate) offload: bool,
    #[serde(default = "bf16")]
    pub(crate) dtype: String,
    #[serde(default = "true_b")]
    pub(crate) restart: bool,
    #[serde(default)]
    pub(crate) annotations: Vec<Ann>,
    #[serde(default)]
    pub(crate) eval_prompts: u32,
}

pub fn one_u32() -> u32 {
    1
}
pub fn three_u32() -> u32 {
    3
}
pub fn bf16() -> String {
    "bf16".to_string()
}

pub fn default_train_size() -> u32 {
    512
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainFile {
    pub(crate) name: String,
    pub(crate) data_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainFilesCall {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) base: String,
    pub(crate) steps: u32,
    pub(crate) rank: u32,
    pub(crate) lr: f64,
    pub(crate) files: Vec<TrainFile>,
    #[serde(default)]
    pub(crate) caption: String,
    #[serde(default)]
    pub(crate) auto_caption: bool,
    #[serde(default)]
    pub(crate) existing_lora: Option<String>,
    #[serde(default = "default_train_size")]
    pub(crate) size: u32,
    #[serde(default)]
    pub(crate) keep_aspect: bool,
    #[serde(default = "one_u32")]
    pub(crate) batch: u32,
    #[serde(default = "one_u32")]
    pub(crate) grad_accum: u32,
    #[serde(default = "three_u32")]
    pub(crate) depth: u32,
    #[serde(default = "true_b")]
    pub(crate) offload: bool,
    #[serde(default = "bf16")]
    pub(crate) dtype: String,
    #[serde(default = "true_b")]
    pub(crate) restart: bool,
    #[serde(default)]
    pub(crate) annotations: Vec<Ann>,
    #[serde(default)]
    pub(crate) eval_prompts: u32,
}


#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainOut {
    pub(crate) lora: String,
    pub(crate) loss_image: Option<String>,
    #[serde(default)]
    pub(crate) eval_images: Vec<String>,
    #[serde(default)]
    pub(crate) avoid: String,
}

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Ann {
    #[serde(default)]
    pub(crate) want: String,
    #[serde(default)]
    pub(crate) avoid: String,
    #[serde(default = "one_u32")]
    pub(crate) weight: u32,
    /// normalized crop (x, y, w, h) in 0..1
    #[serde(default)]
    pub(crate) crop: Option<[f64; 4]>,
    /// normalized rectangles to blur away before training (x, y, w, h)
    #[serde(default)]
    pub(crate) erases: Vec<[f64; 4]>,
}

pub(crate) const EVAL_PROMPTS: [&str; 4] = [
    "a portrait photo, natural light, 50mm",
    "full body photo, standing in a city street, daylight",
    "a quiet interior, morning window light",
    "a still life on a wooden table, soft light",
];

pub fn train_slug(name: &str) -> Result<String, String> {
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

pub fn write_train_png(
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

pub fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    let b64 = data_url.split_once(',').map(|(_, b)| b).unwrap_or(data_url);
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("图片解码失败: {e}"))
}

pub fn spawn_train_progress(
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

pub fn newest_file(dir: &std::path::Path, needle: &str, after: std::time::SystemTime) -> Option<std::path::PathBuf> {
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
pub fn save_experiment(handle: &tauri::AppHandle, name: &str, data: serde_json::Value) {
    let dir = cfg_dir(handle).join("experiments");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(text) = serde_json::to_string_pretty(&data) {
        let _ = fsutil::atomic_write_str(&dir.join(format!("{name}.json")), &text);
    }
}

pub fn wait_port_free(port: u16, tries: u32) {
    for _ in 0..tries {
        if api::health(port).is_none() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

pub fn restart_for_training(handle: &tauri::AppHandle) -> Result<(), String> {
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
pub fn pid_listening_on(port: u16) -> Option<u32> {
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
