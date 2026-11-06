//! 裁头（带肩膀）→ BFS 换头 → 羽化贴回。
//!
//! 为什么不是"局部重绘"：FLUX 局部重绘会按蒙版大小重画一颗头（小脸变巨脸，见 docs/99）。
//! 裁块路线从设计上绕开尺度问题——裁块里头的比例本来就是对的，贴回同一位置同一大小即可。

use crate::faceswap::Rect;
use crate::*;
use image::GenericImageView;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadSwapCall {
    /// 要改的那张图（图库里的）
    pub source: String,
    /// 这张图里脸的归一化框 [x, y, w, h]
    pub face: [f64; 4],
    /// 角色卡底板（要搬过来的头）
    pub plate: String,
    /// 底板上脸的归一化框
    pub plate_face: [f64; 4],
    /// 裁块外扩：相对脸框长边的倍数（默认 2.4，含肩膀）
    #[serde(default)]
    pub pad: Option<f64>,
    #[serde(default)]
    pub feather: u32,
    #[serde(default, deserialize_with = "crate::api::de_seed")]
    pub seed: i64,
}

pub const DEFAULT_HEAD_PAD: f64 = 2.4;

/// 脸框 → 带肩膀的正方形裁块（往上多留一点给发型）。
/// 纯函数：靠边时整体内移，不裁小。
pub fn crop_head(iw: u32, ih: u32, face: [f64; 4], pad: f64) -> Rect {
    let iw = iw.max(1);
    let ih = ih.max(1);
    let f = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    let (fx, fy, fw, fh) = (f(face[0]) * iw as f64, f(face[1]) * ih as f64, f(face[2]) * iw as f64, f(face[3]) * ih as f64);
    let long = fw.max(fh).max(8.0);
    let side = (long * pad.clamp(1.2, 4.0)).max(48.0).min(iw as f64).min(ih as f64);
    let cx = fx + fw / 2.0;
    // 头在脸框上方：整体上移一点，才能把发髻/发簪一起框进来
    let cy = fy + fh / 2.0 - side * 0.16;
    let x = (cx - side / 2.0).clamp(0.0, iw as f64 - side);
    let y = (cy - side / 2.0).clamp(0.0, ih as f64 - side);
    Rect {
        x: x.round() as u32,
        y: y.round() as u32,
        w: side.round() as u32,
        h: side.round() as u32,
    }
}

/// 把 patch 按椭圆软边贴进 base 的 rect 位置。
/// `keep` = 完全不透明的归一化半径（0.7 = 中心 70% 实心，外圈渐隐到边缘）。
pub fn blend_ellipse(base: &mut image::RgbaImage, patch: &image::RgbaImage, rect: Rect, keep: f64) {
    let keep = keep.clamp(0.2, 0.98);
    let pw = patch.width().max(1) as f64;
    let ph = patch.height().max(1) as f64;
    for py in 0..patch.height() {
        for px in 0..patch.width() {
            let bx = rect.x + px;
            let by = rect.y + py;
            if bx >= base.width() || by >= base.height() {
                continue;
            }
            let nx = (px as f64 + 0.5) / pw * 2.0 - 1.0;
            let ny = (py as f64 + 0.5) / ph * 2.0 - 1.0;
            let r = ((nx * nx + ny * ny) / 1.0).sqrt();
            let a = if r <= keep {
                1.0
            } else if r >= 1.0 {
                0.0
            } else {
                let t = (r - keep) / (1.0 - keep);
                1.0 - t * t * (3.0 - 2.0 * t)
            };
            if a <= 0.0 {
                continue;
            }
            let p = patch.get_pixel(px, py);
            let b = base.get_pixel(bx, by);
            let mix = |i: usize| ((p[i] as f64) * a + (b[i] as f64) * (1.0 - a)).round().clamp(0.0, 255.0) as u8;
            base.put_pixel(bx, by, image::Rgba([mix(0), mix(1), mix(2), 255]));
        }
    }
}

pub struct HeadSwapOutcome {
    pub prompt_id: String,
    pub seconds: f64,
    pub image: api::ImageOut,
    pub label: String,
    pub history_id: String,
}

pub fn run_head_swap<F>(
    port: u16,
    comfy_root: &str,
    out_root: &std::path::Path,
    cfg_dir: &std::path::PathBuf,
    req: &HeadSwapCall,
    mut on_progress: F,
) -> Result<HeadSwapOutcome, String>
where
    F: FnMut(api::Progress),
{
    if api::health(port).is_none() {
        return Err("ComfyUI 没有在运行".into());
    }
    let canon_out = out_root.canonicalize().unwrap_or_else(|_| out_root.to_path_buf());
    let src = std::path::PathBuf::from(&req.source)
        .canonicalize()
        .map_err(|e| format!("找不到原图: {e}"))?;
    if !src.starts_with(&canon_out) {
        return Err("只能对图库里的图片换头".into());
    }
    let plate = std::path::PathBuf::from(&req.plate)
        .canonicalize()
        .map_err(|e| format!("找不到角色底板: {e}"))?;
    if !plate.starts_with(&canon_out) {
        return Err("角色底板必须在本机的图库目录里".into());
    }

    let base = image::open(&src)
        .map_err(|e| format!("读不出这张图：{e}"))?
        .to_rgba8();
    let (iw, ih) = base.dimensions();
    let crop = crop_head(iw, ih, req.face, req.pad.unwrap_or(DEFAULT_HEAD_PAD));
    let patch_src = image::imageops::crop_imm(&base, crop.x, crop.y, crop.w, crop.h).to_image();

    let plate_img = image::open(&plate)
        .map_err(|e| format!("读不出角色底板：{e}"))?
        .to_rgba8();
    let (pw, ph) = plate_img.dimensions();
    let pcrop = crop_head(pw, ph, req.plate_face, 1.9);
    let plate_patch = image::imageops::crop_imm(&plate_img, pcrop.x, pcrop.y, pcrop.w, pcrop.h).to_image();

    let input_dir = std::path::PathBuf::from(comfy_root).join("ComfyUI").join("input");
    std::fs::create_dir_all(&input_dir).map_err(|e| format!("ComfyUI input 目录不可用：{e}"))?;
    let stamp = now_ms();
    let scene_name = format!("cs_hs_scene_{stamp}.png");
    let head_name = format!("cs_hs_head_{stamp}.png");
    // 裁块进模型前统一到 1024（BFS 工作分辨率），贴回时再缩回裁块大小
    let scene_work = image::imageops::resize(&patch_src, 1024, 1024, image::imageops::FilterType::Lanczos3);
    let head_work = image::imageops::resize(&plate_patch, 512, 512, image::imageops::FilterType::Lanczos3);
    scene_work
        .save_with_format(input_dir.join(&scene_name), image::ImageFormat::Png)
        .map_err(|e| format!("写裁块失败：{e}"))?;
    head_work
        .save_with_format(input_dir.join(&head_name), image::ImageFormat::Png)
        .map_err(|e| format!("写头参考失败：{e}"))?;

    let seed = if req.seed < 0 { api::pick_seed(-1) } else { req.seed };
    let graph = api::build_head_swap(&scene_name, &head_name, 8, 1.0, seed);
    let out = api::run_job(port, comfy_root, graph, 1800, |pr| on_progress(pr));
    let cleanup = || {
        let _ = std::fs::remove_file(input_dir.join(&scene_name));
        let _ = std::fs::remove_file(input_dir.join(&head_name));
    };
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            cleanup();
            return Err(e);
        }
    };
    let first = match out.images.first() {
        Some(i) => i.clone(),
        None => {
            cleanup();
            return Err("换头没有产出图片".into());
        }
    };
    let swapped = match image::open(&first.path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            let _ = std::fs::remove_file(&first.path);
            cleanup();
            return Err(format!("读不出换头结果：{e}"));
        }
    };
    let _ = std::fs::remove_file(&first.path); // 中间产物
    // 模型爱把新头画得比原来大（参考图是紧裁的大头）。这里**按脸框反算尺度**：
    // 在换头结果里再找一次脸，把结果缩放到"新脸 = 原来的脸那么大"，再对齐到原来的脸中心。
    let swapped_name = format!("cs_hs_out_{stamp}.png");
    let mut patch = image::imageops::resize(&swapped, crop.w, crop.h, image::imageops::FilterType::Lanczos3);
    let mut dbg = serde_json::json!({"detected": false, "scale": 1.0});
    let mut rect = crop;
    if swapped
        .clone()
        .save_with_format(input_dir.join(&swapped_name), image::ImageFormat::Png)
        .is_ok()
    {
        if let Ok(v) = crate::enhance::detect_face(port, &swapped_name) {
            if let Some(bb) = v.get("bbox").and_then(|b| b.as_array()) {
                let nums: Vec<f64> = bb.iter().filter_map(|x| x.as_f64()).collect();
                if nums.len() == 4 && nums[2] > 0.002 && nums[3] > 0.002 {
                    let fw = req.face[2].clamp(0.0, 1.0) * iw as f64;
                    let fh = req.face[3].clamp(0.0, 1.0) * ih as f64;
                    let nw = nums[2] * crop.w as f64;
                    let nh = nums[3] * crop.h as f64;
                    let scale = ((fw / nw) * 0.5 + (fh / nh) * 0.5).clamp(0.35, 2.5);
                    dbg = serde_json::json!({"detected": true, "scale": scale, "nw": nw, "nh": nh, "fw": fw, "fh": fh});
                    if (scale - 1.0).abs() > 0.03 {
                        let tw = ((crop.w as f64) * scale).round().max(16.0) as u32;
                        let th = ((crop.h as f64) * scale).round().max(16.0) as u32;
                        patch = image::imageops::resize(&patch, tw, th, image::imageops::FilterType::Lanczos3);
                        // 新脸中心（缩放后 patch 坐标）→ 对齐到原脸中心（原图坐标）
                        let ncx = (nums[0] + nums[2] / 2.0) * tw as f64;
                        let ncy = (nums[1] + nums[3] / 2.0) * th as f64;
                        let ocx = (req.face[0].clamp(0.0, 1.0) + req.face[2].clamp(0.0, 1.0) / 2.0) * iw as f64;
                        let ocy = (req.face[1].clamp(0.0, 1.0) + req.face[3].clamp(0.0, 1.0) / 2.0) * ih as f64;
                        let x = (ocx - ncx).round().max(0.0) as u32;
                        let y = (ocy - ncy).round().max(0.0) as u32;
                        rect = Rect { x, y, w: tw, h: th };
                    }
                }
            }
        }
        let _ = std::fs::remove_file(input_dir.join(&swapped_name));
    }
    let mut result = base.clone();
    blend_ellipse(&mut result, &patch, rect, 0.68);
    cleanup();

    let dir = src.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| canon_out.clone());
    let stem = src
        .file_stem()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    let target = head_swap_target(&dir, &stem);
    let mut buf = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(result)
        .write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    std::fs::write(&target, buf.get_ref()).map_err(|e| format!("保存换头结果失败：{e}"))?;

    let bytes = buf.into_inner();
    use base64::Engine as _;
    let data_url = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes));
    let path_str = crate::commands::generation::strip_win_prefix(&target.to_string_lossy());
    let project = crate::commands::generation::project_id_of_path(&canon_out, &src);
    let label = "Qwen-Image 2.1 · 换头（保构图）".to_string();
    let meta = json!({
        "app": "comfy-shell", "schema": 1, "appVersion": env!("CARGO_PKG_VERSION"),
        "createdAt": now_ms(), "mode": "headswap",
        "prompt": api::HEAD_SWAP_PROMPT, "profile": "qwen21",
        "steps": 8, "cfg": 1.0, "seed": seed,
        "face": req.face, "crop": [crop.x, crop.y, crop.w, crop.h],
        "project": project,
        "source": src.file_name().map(|x| x.to_string_lossy().to_string()),
        "align": dbg,
    });
    let _ = crate::imgmeta::write_png_meta(&target, &meta);

    let image = api::ImageOut {
        filename: target.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(),
        subfolder: target
            .parent()
            .and_then(|p| p.strip_prefix(&canon_out).ok())
            .map(|s| s.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default(),
        path: path_str,
        width: iw,
        height: ih,
        data_url,
    };
    let history_id = format!("hs-{}", now_ms());
    let item = crate::history::HistoryItem {
        id: history_id.clone(),
        ts: now_ms(),
        profile: "qwen21".into(),
        label: label.clone(),
        mode: "headswap".into(),
        prompt: api::HEAD_SWAP_PROMPT.to_string(),
        negative: String::new(),
        width: iw,
        height: ih,
        steps: 8,
        cfg: 1.0,
        seed,
        seconds: out.seconds,
        project,
        images: vec![crate::history::HistoryImage {
            filename: image.filename.clone(),
            subfolder: image.subfolder.clone(),
            path: image.path.clone(),
            width: iw,
            height: ih,
        }],
    };
    crate::history::append(cfg_dir, item);
    Ok(HeadSwapOutcome { prompt_id: out.prompt_id, seconds: out.seconds, image, label, history_id })
}

/// 不覆盖已有文件：`x_head.png`、`x_head-2.png`…
pub fn head_swap_target(dir: &std::path::Path, stem: &str) -> std::path::PathBuf {
    let base = format!("{stem}_head");
    let mut p = dir.join(format!("{base}.png"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{base}-{n}.png"));
        n += 1;
    }
    p
}

#[tauri::command]
pub async fn head_swap(app: tauri::AppHandle, req: HeadSwapCall) -> Result<api::GenOut, String> {
    if cfg!(debug_assertions) && !req.source.replace('\\', "/").contains("/projects/_test/") {
        return Err("调试构建只能在「测试项目」里换头".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<api::GenOut, String> {
        let state = handle.state::<AppState>();
        let (port, root, out_root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone(), crate::state::output_root(&st))
        };
        let cfg = crate::state::cfg_dir(&handle);
        let app2 = handle.clone();
        let r = run_head_swap(port, &root, &out_root, &cfg, &req, |p| {
            let _ = app2.emit(
                "gen-progress",
                json!({"elapsed": p.elapsed, "running": p.queue_running,
                       "pending": p.queue_pending, "started": p.started}),
            );
        })?;
        Ok(api::GenOut { prompt_id: r.prompt_id, seconds: r.seconds, images: vec![r.image] })
    })
    .await
    .map_err(|e| format!("换头任务失败: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn crop_keeps_a_square_inside_the_image_and_moves_up() {
        let r = crop_head(832, 1216, [0.33, 0.35, 0.12, 0.12], 2.4);
        assert_eq!(r.w, r.h);
        assert!(r.x as u32 + r.w <= 832 && r.y as u32 + r.h <= 1216);
        // 脸中心 y ≈ 0.35*1216 + 146/2 ≈ 498，裁块中心应该比它高（上移）
        let cy = r.y as f64 + r.h as f64 / 2.0;
        assert!(cy < 498.0, "裁块中心 {cy} 应该高于脸中心 498");
    }

    #[test]
    fn crop_clamps_at_edges() {
        let r = crop_head(400, 400, [0.95, 0.02, 0.1, 0.1], 3.0);
        assert!(r.x as u32 + r.w <= 400);
        assert_eq!(r.y, 0);
    }

    #[test]
    fn blend_keeps_corners_and_replaces_the_centre() {
        let mut base = RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 0, 255]));
        let patch = RgbaImage::from_pixel(100, 100, image::Rgba([255, 255, 255, 255]));
        blend_ellipse(&mut base, &patch, Rect { x: 0, y: 0, w: 100, h: 100 }, 0.7);
        assert_eq!(base.get_pixel(50, 50)[0], 255, "中心要被替换");
        assert_eq!(base.get_pixel(0, 0)[0], 0, "四角要保留原图");
    }

    #[test]
    fn target_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("cs_headswap_{}", crate::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = head_swap_target(&dir, "x");
        assert_eq!(a.file_name().unwrap(), "x_head.png");
        std::fs::write(&a, b"x").unwrap();
        let b = head_swap_target(&dir, "x");
        assert_eq!(b.file_name().unwrap(), "x_head-2.png");
    }
}
