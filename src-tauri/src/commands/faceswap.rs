//! 换脸（#60）：裁脸 → 重绘 → 羽化贴回。
//!
//! 和「局部重绘」的区别在取景：这里给模型一张**完整的脸**（脸框外扩 1.6 倍的
//! 方形裁块、缩放到 1024），蒙版只盖脸（脸框 ×1.15 的圆角矩形）——模型看到的是
//! 一整张脸，不会再在眉眼断口上补画。重绘完整块羽化贴回：裁块里蒙版外的像素
//! 本来就等于原图，所以接缝落在头发/背景上也看不出来。
//!
//! 落盘策略：原图不动，结果另存成 `<原名>_face.png` 放在同一目录；
//! 中间的「重绘出来的脸」用完即删，不往图库里留垃圾。

use crate::commands::generation::{project_id_of_path, GenResp};
use crate::faceswap::{self, Rect};
use image::ImageFormat;
use std::path::{Path, PathBuf};

use crate::*;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceSwapCall {
    /// 图库里的原图（必须是 output 目录里的）
    pub source: String,
    /// `face_detect` 给的归一化脸框 [x, y, w, h]
    pub face: [f64; 4],
    pub prompt: String,
    #[serde(default)]
    pub negative: String,
    /// 重绘模型；只有支持改图的档能用（MOSS 二编会在建图时明确报错）
    #[serde(default = "default_profile")]
    pub profile: String,
    pub steps: u32,
    pub cfg: f64,
    #[serde(default, deserialize_with = "crate::api::de_seed")]
    pub seed: i64,
    #[serde(default)]
    pub denoise: f64,
    /// 贴回羽化（原图像素）；0 = 按裁块尺寸自动
    #[serde(default)]
    pub feather: u32,
    /// 裁块外扩倍数；None = 1.6
    #[serde(default)]
    pub pad: Option<f64>,
    /// 角色底板裁出来的脸（和局部重绘同一条路：512 大头当身份参考）
    #[serde(default)]
    pub ref_image_data_url: Option<String>,
}

fn default_profile() -> String {
    "flux3".to_string()
}

pub struct SwapOutcome {
    pub prompt_id: String,
    pub seconds: f64,
    pub image: api::ImageOut,
    pub label: String,
    pub history_id: String,
    /// 裁块与羽化宽度：写文档/集成测试时要用（命令本身不读）
    #[allow(dead_code)]
    pub crop: Rect,
    #[allow(dead_code)]
    pub feather: u32,
}

/// 同目录下不撞名的目标路径：`x_face.png` → `x_face-2.png` → …
fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..1000 {
        let p = dir.join(format!("{stem}-{n}.{ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{stem}-{}.{ext}", now_ms()))
}

/// 换脸主流程（不依赖 Tauri，方便直接跑真机集成测试）。
pub fn run_swap<F>(
    port: u16,
    comfy_root: &str,
    out_root: &Path,
    cfg_dir: &Path,
    req: &FaceSwapCall,
    mut on_progress: F,
) -> Result<SwapOutcome, String>
where
    F: FnMut(api::Progress),
{
    if api::health(port).is_none() {
        return Err("ComfyUI 没有在运行".into());
    }
    let p = api::profile(&req.profile).ok_or_else(|| format!("未知模型配置: {}", req.profile))?;
    if !p.supports_edit {
        return Err("这个模型不支持改图，换图请用 FLUX.3".into());
    }
    if req.prompt.trim().is_empty() {
        return Err("写一下这张脸要画成什么".into());
    }

    let canon_out = out_root.canonicalize().unwrap_or_else(|_| out_root.to_path_buf());
    let src = PathBuf::from(&req.source)
        .canonicalize()
        .map_err(|e| format!("找不到原图: {e}"))?;
    if !src.starts_with(&canon_out) {
        return Err("只能对图库里的图片换脸".into());
    }

    let base = image::open(&src)
        .map_err(|e| format!("读不出这张图（只支持 png/jpg/webp）：{e}"))?
        .to_rgba8();
    let (iw, ih) = base.dimensions();
    let crop = faceswap::crop_rect(iw, ih, req.face, req.pad.unwrap_or(faceswap::DEFAULT_PAD));
    let patch_src = image::imageops::crop_imm(&base, crop.x, crop.y, crop.w, crop.h).to_image();

    // 脸框（原图坐标）→ 裁块坐标 → 工作分辨率（1024）
    let fx = req.face[0].clamp(0.0, 1.0) * iw as f64;
    let fy = req.face[1].clamp(0.0, 1.0) * ih as f64;
    let fw = req.face[2].clamp(0.0, 1.0) * iw as f64;
    let fh = req.face[3].clamp(0.0, 1.0) * ih as f64;
    let s = faceswap::WORK as f64 / crop.w as f64;
    // 蒙版只盖**脸上的皮肤**：左右各收 4%、上边从发际线往下让 10%、下边到下巴。
    // 不把头发圈进来，是因为模型一旦重画发际线，蒙版外没动过的头发就会和它断开
    // （实测：左边一缕头发像悬空的黑色三角）。边界落在皮肤上，羽化才糊得住。
    let (mx, my, mw, mh) = if fw < 2.0 || fh < 2.0 {
        // 没有可信脸框时兜底：中间一块（宁可保守，也别把头发也重画）
        let w = faceswap::WORK as f64 * 0.4;
        let h = faceswap::WORK as f64 * 0.5;
        (faceswap::WORK as f64 / 2.0, faceswap::WORK as f64 / 2.0, w, h)
    } else {
        (
            (fx + fw / 2.0 - crop.x as f64) * s,
            (fy + fh * 0.52 - crop.y as f64) * s,
            fw * 0.94 * s,
            fh * 0.94 * s,
        )
    };

    // 送进模型的是一张 1024 的正方形脸——不管原图里脸只有 90px 还是 900px。
    let work = image::imageops::resize(
        &patch_src,
        faceswap::WORK,
        faceswap::WORK,
        image::imageops::FilterType::Lanczos3,
    );

    let input_dir = PathBuf::from(comfy_root).join("ComfyUI").join("input");
    std::fs::create_dir_all(&input_dir).map_err(|e| format!("ComfyUI input 目录不可用：{e}"))?;
    let stamp = now_ms();
    let src_name = format!("cs_face_{stamp}.png");
    let mask_name = format!("cs_facemask_{stamp}.png");
    work.save_with_format(input_dir.join(&src_name), ImageFormat::Png)
        .map_err(|e| format!("写裁块失败：{e}"))?;
    // 只把脸这块涂白：头发/背景保持原样，贴回时才不会有一块方形的痕迹。
    faceswap::face_mask(
        faceswap::WORK,
        faceswap::WORK,
        mx,
        my,
        mw,
        mh,
        1.0,
    )
    .save_with_format(input_dir.join(&mask_name), ImageFormat::Png)
    .map_err(|e| format!("写蒙版失败：{e}"))?;

    let mut temp_inputs = vec![src_name.clone(), mask_name.clone()];
    let ref_name = match &req.ref_image_data_url {
        Some(data_url) if !data_url.trim().is_empty() => {
            let bytes = crate::commands::gallery::decode_data_url(data_url)?;
            let name = format!("cs_faceref_{stamp}.png");
            std::fs::write(input_dir.join(&name), bytes).map_err(|e| format!("写参考脸失败：{e}"))?;
            temp_inputs.push(name.clone());
            Some(name)
        }
        _ => None,
    };

    // #88：随机种子在提交前定下来——换脸的最终图是壳自己合成的 PNG（没有 ComfyUI 的
    // 工作流块），种子只能靠我们自己记牢，否则永远找不回来。
    let seed = if req.seed < 0 { api::pick_seed(-1) } else { req.seed };
    let ireq = api::InpaintReq {
        profile: req.profile.clone(),
        prompt: req.prompt.clone(),
        negative: req.negative.clone(),
        source: src_name,
        mask: mask_name,
        steps: req.steps.max(1),
        cfg: req.cfg,
        seed,
        denoise: if req.denoise > 0.0 { req.denoise.clamp(0.05, 1.0) } else { 0.88 },
        // 蒙版边缘柔化（1024 工作分辨率上的像素）：边界落在皮肤上，羽化要宽一点
        feather: 24,
        ref_image: ref_name,
    };
    let graph = api::build_inpaint(&ireq)?;
    let out = api::run_job(port, comfy_root, graph, 1800, |pr| on_progress(pr))?;
    let first = out
        .images
        .first()
        .ok_or("重绘完成但没有产出图片")?
        .clone();

    // 中间产物用完即删：图库里不该出现「一张裁出来的脸」。
    let cleanup = || {
        for name in &temp_inputs {
            let _ = std::fs::remove_file(input_dir.join(name));
        }
        let _ = std::fs::remove_file(&first.path);
    };

    let redrawn = match image::open(&first.path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            cleanup();
            return Err(format!("读不出重绘结果：{e}"));
        }
    };
    let patch = image::imageops::resize(
        &redrawn,
        crop.w,
        crop.h,
        image::imageops::FilterType::Lanczos3,
    );
    let feather = if req.feather > 0 {
        let cap = crop.w.saturating_div(2).saturating_sub(1).max(1);
        req.feather.min(cap).max(1)
    } else {
        faceswap::default_feather(crop.w)
    };
    let mut result = base.clone();
    if let Err(e) = faceswap::feather_composite(&mut result, &patch, crop, feather) {
        cleanup();
        return Err(e);
    }

    // 落盘：原图旁边另存一张，不覆盖原图。
    let dir = src
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| canon_out.clone());
    let stem = src
        .file_stem()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    let target = unique_path(&dir, &format!("{stem}_face"), "png");
    let mut buf = std::io::Cursor::new(Vec::new());
    if let Err(e) = image::DynamicImage::ImageRgba8(result)
        .write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| e.to_string())
        .and_then(|_| std::fs::write(&target, buf.get_ref()).map_err(|e| e.to_string()))
    {
        cleanup();
        return Err(format!("保存换脸结果失败：{e}"));
    }
    cleanup();

    let bytes = buf.into_inner();
    use base64::Engine as _;
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    let path_str = target.to_string_lossy().to_string();
    let project = project_id_of_path(&canon_out, &src);
    let label = format!("{} · 换脸", p.label);
    let meta = json!({
        "app": "comfy-shell", "schema": 1, "appVersion": env!("CARGO_PKG_VERSION"),
        "createdAt": now_ms(), "mode": "faceswap",
        "prompt": req.prompt, "negative": req.negative, "profile": req.profile,
        "steps": req.steps, "cfg": req.cfg, "seed": seed,
        "denoise": ireq.denoise, "feather": feather,
        "face": req.face, "crop": [crop.x, crop.y, crop.w, crop.h],
        "ref": req.ref_image_data_url.is_some(),
        "project": project,
        "source": src.file_name().map(|x| x.to_string_lossy().to_string()),
    });
    let _ = crate::imgmeta::write_png_meta(&target, &meta);

    let image = api::ImageOut {
        filename: target
            .file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_default(),
        subfolder: target
            .parent()
            .and_then(|parent| parent.strip_prefix(&canon_out).ok())
            .map(|s| s.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default(),
        path: crate::commands::generation::strip_win_prefix(&path_str),
        width: iw,
        height: ih,
        data_url,
    };

    let history_id = format!("fs-{}", now_ms());
    let item = crate::history::HistoryItem {
        id: history_id.clone(),
        ts: now_ms(),
        profile: req.profile.clone(),
        label: label.clone(),
        mode: "faceswap".to_string(),
        prompt: req.prompt.clone(),
        negative: req.negative.clone(),
        width: iw,
        height: ih,
        steps: req.steps,
        cfg: req.cfg,
        seed,
        seconds: out.seconds,
        project: project.clone(),
        images: vec![crate::history::HistoryImage {
            filename: image.filename.clone(),
            subfolder: image.subfolder.clone(),
            path: crate::commands::generation::strip_win_prefix(&path_str),
            width: iw,
            height: ih,
        }],
    };
    crate::history::append(&cfg_dir.to_path_buf(), item);

    Ok(SwapOutcome {
        prompt_id: out.prompt_id,
        seconds: out.seconds,
        image,
        label,
        history_id,
        crop,
        feather,
    })
}

#[tauri::command]
pub async fn face_swap(app: tauri::AppHandle, req: FaceSwapCall) -> Result<GenResp, String> {
    // 调试构建只许拿「测试项目」的图做实验，别动真实项目（和 generate 同一条规矩）。
    if cfg!(debug_assertions) && !req.source.replace('\\', "/").contains("/projects/_test/") {
        return Err("调试构建只能在「测试项目」里换脸（先在项目栏选「测试项目」）".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<GenResp, String> {
        let state = handle.state::<AppState>();
        let (port, root, out_root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone(), crate::state::output_root(&st))
        };
        let cfg = crate::state::cfg_dir(&handle);
        let app2 = handle.clone();
        let r = run_swap(port, &root, &out_root, &cfg, &req, |p| {
            let _ = app2.emit(
                "gen-progress",
                json!({"elapsed": p.elapsed, "running": p.queue_running,
                       "pending": p.queue_pending, "started": p.started}),
            );
        })?;
        Ok(GenResp {
            out: api::GenOut {
                prompt_id: r.prompt_id,
                seconds: r.seconds,
                images: vec![r.image],
            },
            turbo: false,
            label: r.label,
            history_id: r.history_id,
        })
    })
    .await
    .map_err(|e| format!("换脸任务失败: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::unique_path;
    use crate::faceswap::Rect;
    use image::GenericImageView;

    #[test]
    fn unique_path_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("cs_faceswap_paths_{}", crate::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = unique_path(&dir, "x_face", "png");
        assert_eq!(a.file_name().unwrap(), "x_face.png");
        std::fs::write(&a, b"x").unwrap();
        let b = unique_path(&dir, "x_face", "png");
        assert_eq!(b.file_name().unwrap(), "x_face-2.png");
        std::fs::write(&b, b"x").unwrap();
        let c = unique_path(&dir, "x_face", "png");
        assert_eq!(c.file_name().unwrap(), "x_face-3.png");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 真机集成测试（默认忽略）：跑一次完整的「找脸 → 裁脸 → 重绘 → 贴回」。
    ///
    /// 跑法（PowerShell）：
    ///   $env:CS_COMFY_ROOT="D:\...\ComfyUI_windows_portable"
    ///   $env:CS_TEST_SRC="D:\...\有人脸的图.png"   # 不给就现生成一张中性人像
    ///   cargo test live_swap -- --ignored --nocapture
    #[test]
    #[ignore = "真机：需要本机 ComfyUI 在跑，且给出 CS_COMFY_ROOT"]
    fn live_swap_uses_the_detected_face_and_writes_a_new_image() {
        let root = std::env::var("CS_COMFY_ROOT").expect("CS_COMFY_ROOT=ComfyUI 根目录");
        let port: u16 = std::env::var("CS_COMFY_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8188);

        // 没有现成测试图就现生成一张中性人像：不拿用户项目里的图做实验，
        // 也顺带验证「壳生成出来的图」能直接进换脸链路。
        let mut generated = None;
        let src = match std::env::var("CS_TEST_SRC") {
            Ok(p) => p,
            Err(_) => {
                let p = live_make_portrait(port, &root).expect("生成测试人像失败");
                generated = Some(p.clone());
                p
            }
        };

        let tmp = std::env::temp_dir().join(format!("cs_faceswap_live_{}", crate::now_ms()));
        let out = tmp.join("output");
        let cfg = tmp.join("cfg");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::create_dir_all(&cfg).unwrap();
        let target = out.join("live_src.png");
        std::fs::copy(&src, &target).unwrap();
        if let Some(p) = generated {
            let _ = std::fs::remove_file(p);
        }

        // 找脸：和界面同一条路（图先放进 ComfyUI/input，再让本机视觉模型出框）
        let input = std::path::Path::new(&root).join("ComfyUI").join("input");
        std::fs::create_dir_all(&input).unwrap();
        let probe = format!("cs_live_face_{}.png", crate::now_ms());
        std::fs::copy(&target, input.join(&probe)).unwrap();
        let boxv = crate::enhance::detect_face(port, &probe).expect("detect_face 失败");
        let _ = std::fs::remove_file(input.join(&probe));
        let b: Vec<f64> = boxv["bbox"]
            .as_array()
            .expect("bbox")
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect();
        assert_eq!(b.len(), 4);

        let p = crate::api::profile("flux3").unwrap();
        // CS_TEST_REF=self：拿这张图自己的脸当参考（模拟角色底板的「裁脸」），
        // 结果应该回到原来那个人——用来肉眼验「参考脸真的起作用」。
        let ref_data_url = match std::env::var("CS_TEST_REF").ok().as_deref() {
            Some("self") => Some(face_ref_data_url(&target, &b)),
            Some(path) => {
                use base64::Engine as _;
                let bytes = std::fs::read(path).expect("读不到 CS_TEST_REF");
                Some(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ))
            }
            None => None,
        };
        // 调参用：CS_TEST_PROMPT / CS_TEST_DENOISE 可以覆盖默认值（只影响这个真机测试）
        let prompt = std::env::var("CS_TEST_PROMPT")
            .unwrap_or_else(|_| "同一个人的脸，正面，五官清晰，皮肤质感自然".to_string());
        let denoise: f64 = std::env::var("CS_TEST_DENOISE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.88);
        let req = super::FaceSwapCall {
            source: target.to_string_lossy().to_string(),
            face: [b[0], b[1], b[2], b[3]],
            prompt,
            negative: String::new(),
            profile: "flux3".into(),
            steps: p.steps,
            cfg: p.cfg,
            seed: 7,
            denoise,
            feather: 0,
            pad: None,
            ref_image_data_url: ref_data_url,
        };
        let r = super::run_swap(port, &root, &out, &cfg, &req, |_| {}).expect("run_swap 失败");

        assert!(std::path::Path::new(&r.image.path).is_file(), "结果图没落盘");
        assert_eq!(r.crop.w, r.crop.h, "裁块必须是正方形");
        let src_img = image::open(&target).unwrap().to_rgba8();
        let out_img = image::open(&r.image.path).unwrap().to_rgba8();
        assert_eq!(src_img.dimensions(), out_img.dimensions(), "贴回后尺寸必须和原图一致");
        let (iw, ih) = src_img.dimensions();

        // 机械判据①：裁块之外必须一个像素都不动（头发/衣服/背景/构图）
        let outside = mean_abs_diff_outside(&src_img, &out_img, r.crop);
        assert_eq!(outside, 0.0, "裁块外的像素被改了");

        // 机械判据②：脸那点必须真的换了（不是原图直出）
        let fw = (b[2] * iw as f64 * 0.6).max(8.0);
        let fh = (b[3] * ih as f64 * 0.6).max(8.0);
        let cx = (b[0] + b[2] / 2.0) * iw as f64;
        let cy = (b[1] + b[3] / 2.0) * ih as f64;
        let face_diff = mean_abs_diff(
            &src_img,
            &out_img,
            Rect {
                x: (cx - fw / 2.0).max(0.0) as u32,
                y: (cy - fh / 2.0).max(0.0) as u32,
                w: fw as u32,
                h: fh as u32,
            },
        );
        assert!(face_diff > 6.0, "脸几乎没变（{face_diff:.1}），换脸没生效");
        println!(
            "结果：{}\n裁块：x{} y{} {}×{}，羽化 {}px，用时 {:.1}s\n脸区平均差 {:.1}，裁块外差 0",
            r.image.path, r.crop.x, r.crop.y, r.crop.w, r.crop.h, r.feather, r.seconds, face_diff
        );
    }

    /// 从原图按脸框裁一张 512 的脸当参考图（模拟界面的「用角色底板（自动裁脸）」）。
    fn face_ref_data_url(src: &std::path::Path, b: &[f64]) -> String {
        use base64::Engine as _;
        let img = image::open(src).unwrap().to_rgba8();
        let (iw, ih) = img.dimensions();
        let fx = b[0] * iw as f64;
        let fy = b[1] * ih as f64;
        let fw = b[2] * iw as f64;
        let fh = b[3] * ih as f64;
        // 外扩一圈（额头/下巴/耳侧），再补成正方形
        let side = (fw.max(fh) * 1.5).max(32.0);
        let cx = fx + fw / 2.0;
        let cy = fy + fh / 2.0;
        let x = (cx - side / 2.0).max(0.0).min((iw as f64 - side).max(0.0)) as u32;
        let y = (cy - side / 2.0).max(0.0).min((ih as f64 - side).max(0.0)) as u32;
        let s = side.min(iw as f64).min(ih as f64) as u32;
        let crop = image::imageops::crop_imm(&img, x, y, s.max(1), s.max(1)).to_image();
        let face = image::imageops::resize(&crop, 512, 512, image::imageops::FilterType::Lanczos3);
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(face)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
        )
    }

    /// 一个矩形内的平均像素差（0-255）。
    fn mean_abs_diff(a: &image::RgbaImage, b: &image::RgbaImage, rect: Rect) -> f64 {
        let (w, h) = a.dimensions();
        let mut sum: f64 = 0.0;
        let mut n: f64 = 0.0;
        for y in rect.y..(rect.y + rect.h).min(h) {
            for x in rect.x..(rect.x + rect.w).min(w) {
                for c in 0..3 {
                    sum += (a.get_pixel(x, y)[c] as f64 - b.get_pixel(x, y)[c] as f64).abs();
                    n += 1.0;
                }
            }
        }
        sum / n.max(1.0)
    }

    /// 某个矩形之外的平均像素差。
    fn mean_abs_diff_outside(a: &image::RgbaImage, b: &image::RgbaImage, skip: Rect) -> f64 {
        let (w, h) = a.dimensions();
        let mut sum: f64 = 0.0;
        let mut n: f64 = 0.0;
        for y in 0..h {
            for x in 0..w {
                let inside =
                    x >= skip.x && x < skip.x + skip.w && y >= skip.y && y < skip.y + skip.h;
                if inside {
                    continue;
                }
                for c in 0..3 {
                    sum += (a.get_pixel(x, y)[c] as f64 - b.get_pixel(x, y)[c] as f64).abs();
                    n += 1.0;
                }
            }
        }
        sum / n.max(1.0)
    }

    /// 用壳自己的 t2i 图跑一张中性人像（只给集成测试用，跑完就删）。
    fn live_make_portrait(port: u16, root: &str) -> Result<String, String> {
        let p = crate::api::profile("flux3").unwrap();
        let req = crate::api::GenReq {
            profile: "flux3".into(),
            mode: "t2i".into(),
            prompt: "一位年轻亚洲女性的半身肖像，正面，浅灰色背景，柔和自然光，写实摄影".into(),
            negative: String::new(),
            width: 832,
            height: 1216,
            steps: p.steps,
            cfg: p.cfg,
            // 每次换一颗种子：ComfyUI 对完全相同的图会走缓存，缓存条目指向的
            // 文件可能已经被上一次测试删掉了（实测踩过）。
            seed: (crate::now_ms() % 1_000_000) as i64,
            batch: 1,
            lora: None,
            lora_strength: 1.0,
            lora2: None,
            lora2_strength: 1.0,
            ref_images: Vec::new(),
            slg: false,
            slg_layers: "22".into(),
            layout: None,
            wf_params: None,
            sampler: None,
            edit_resolution: None,
            project: None,
            staging: false,
            version_parent: None,
        };
        let graph = crate::api::build_graph(&req, false)?;
        let out = crate::api::run_job(port, root, graph, 600, |_| {})?;
        out.images
            .first()
            .map(|i| i.path.clone())
            .ok_or_else(|| "生成测试人像没有产出".to_string())
    }
}
