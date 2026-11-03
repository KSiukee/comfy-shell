//! 换脸（#60）：裁脸 → 整块重绘 → 羽化贴回。
//!
//! 为什么不再用「盖住半张脸的椭圆 + inpaint」：蒙版横穿眉眼时模型只能在断口
//! 补画半套五官，必然重影；把强度降下来参考图又完全失效（实测见 docs/57 §8）。
//! 这里让模型看到的是**一整张完整的脸**（裁剪块），贴回时把羽化带放在脸外
//! （头发/背景）上，接缝就不落在五官上。
//!
//! 本模块只做纯几何与像素运算：不碰 Tauri、不碰网络、不读写磁盘——这样边界
//! 条件（贴边、小图、羽化带）都能用单测钉住。调用与落盘在
//! `commands/faceswap.rs`。

use image::{Rgba, RgbaImage};

/// 生成用的工作边长。FLUX.3 在 1024 上最稳，且方形裁块不用管长宽比。
pub const WORK: u32 = 1024;
/// 裁块下限：脸只有一百来像素时也要有足够画布，不然放大后糊成一团。
pub const MIN_SIDE: u32 = 256;
/// 默认外扩倍数：以脸框长边为基准，四周各留 0.8 个脸宽的头/颈/背景。
pub const DEFAULT_PAD: f64 = 1.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 脸框（归一化 0..1，来自 `face_detect`）→ 生成用的正方形裁块。
///
/// - 以脸框中心为心，边长 = 脸框长边 × `pad`，并保证不小于 `MIN_SIDE`；
/// - 永远留在图内：靠边时整体内移而不是裁小（宁可少一点外扩，也别把脸切了）；
/// - 图本身比 `MIN_SIDE` 还小就退化成整图宽高等长的方块。
pub fn crop_rect(img_w: u32, img_h: u32, face: [f64; 4], pad: f64) -> Rect {
    let iw = img_w.max(1);
    let ih = img_h.max(1);
    let f = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    let (fx, fy, fw, fh) = (
        f(face[0]) * iw as f64,
        f(face[1]) * ih as f64,
        f(face[2]) * iw as f64,
        f(face[3]) * ih as f64,
    );
    let cx = fx + fw / 2.0;
    let cy = fy + fh / 2.0;
    let pad = if pad.is_finite() && pad > 1.0 { pad } else { DEFAULT_PAD };

    let cap = iw.min(ih) as f64;
    let want = fw.max(fh).max(1.0) * pad;
    let side = want.max(MIN_SIDE as f64).min(cap).round().max(1.0) as u32;

    let x = (cx - side as f64 / 2.0).round().max(0.0) as u32;
    let y = (cy - side as f64 / 2.0).round().max(0.0) as u32;
    Rect {
        x: x.min(iw.saturating_sub(side)),
        y: y.min(ih.saturating_sub(side)),
        w: side,
        h: side,
    }
}

/// 贴回时的羽化宽度：跟裁块尺寸走（脸越小，羽化越窄，别糊掉整张脸）。
pub fn default_feather(side: u32) -> u32 {
    (side / 28).clamp(6, 160)
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 把重绘好的裁块羽化贴回原位。
///
/// 羽化带在 `dest` 的**内侧边缘**：中心全取新块，向外平滑过渡到原图，
/// 因此过渡落在脸外的头发/背景上（裁块比脸大 1.6 倍，带宽又只有 3.5%）。
pub fn feather_composite(
    base: &mut RgbaImage,
    patch: &RgbaImage,
    dest: Rect,
    feather: u32,
) -> Result<(), String> {
    if patch.width() != dest.w || patch.height() != dest.h {
        return Err(format!(
            "贴回块尺寸对不上：{}×{} 要贴进 {}×{}",
            patch.width(),
            patch.height(),
            dest.w,
            dest.h
        ));
    }
    let (bw, bh) = base.dimensions();
    if dest.w == 0 || dest.h == 0 || dest.x + dest.w > bw || dest.y + dest.h > bh {
        return Err("贴回区域超出画布".into());
    }
    let band = feather as f64;
    for y in 0..dest.h {
        for x in 0..dest.w {
            let a = if band <= 0.0 {
                1.0
            } else {
                let dx = x.min(dest.w - 1 - x) as f64;
                let dy = y.min(dest.h - 1 - y) as f64;
                smoothstep(dx.min(dy) / band)
            };
            if a <= 0.0 {
                continue;
            }
            let p = patch.get_pixel(x, y);
            let pa = (p[3] as f64 / 255.0) * a;
            if pa <= 0.0 {
                continue;
            }
            let b = base.get_pixel_mut(dest.x + x, dest.y + y);
            for c in 0..3 {
                let v = p[c] as f64 * pa + b[c] as f64 * (1.0 - pa);
                b[c] = v.round().clamp(0.0, 255.0) as u8;
            }
            b[3] = 255;
        }
    }
    Ok(())
}

/// 脸部蒙版：只把「脸」这块涂白，让模型重画五官、**不动**头发/背景。
///
/// 为什么不整块涂白：整块重绘会把头发和背景也一起重画，贴回时必然出现
/// 一块方形的「换过」的痕迹（实测：发丝重影 + 背景亮度不一样）。
/// 裁块只是为了给模型**看**整颗头（分辨率够、上下文够），要改的只有脸。
///
/// 形状用圆角矩形而不是椭圆：椭圆的四角会切掉太阳穴/下颌角，那是身份特征。
/// `expand` 是脸框外的余量（1.15 = 四周各多 15%）。
pub fn face_mask(
    w: u32,
    h: u32,
    cx: f64,
    cy: f64,
    face_w: f64,
    face_h: f64,
    expand: f64,
) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(w.max(1), h.max(1), Rgba([0, 0, 0, 255]));
    let expand = if expand.is_finite() && expand >= 1.0 { expand } else { 1.15 };
    let hw = (face_w.max(2.0) * expand / 2.0).max(1.0);
    let hh = (face_h.max(2.0) * expand / 2.0).max(1.0);
    let corner = (hw.min(hh) * 0.45).max(1.0);
    let x0 = ((cx - hw).floor().max(0.0)) as u32;
    let x1 = ((cx + hw).ceil().min(w as f64)) as u32;
    let y0 = ((cy - hh).floor().max(0.0)) as u32;
    let y1 = ((cy + hh).ceil().min(h as f64)) as u32;
    for y in y0..y1 {
        for x in x0..x1 {
            // 圆角矩形的有符号距离（>0 在内部）
            let dx = (x as f64 + 0.5 - cx).abs();
            let dy = (y as f64 + 0.5 - cy).abs();
            let qx = (dx - (hw - corner)).max(0.0);
            let qy = (dy - (hh - corner)).max(0.0);
            let inside = if qx > 0.0 || qy > 0.0 {
                corner - (qx * qx + qy * qy).sqrt()
            } else {
                (hw - dx).min(hh - dy)
            };
            let a = ((inside + 1.0) / 2.0).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            let v = (a * 255.0).round() as u8;
            img.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_is_square_and_centred_on_the_face() {
        // 1000×1000、脸 200×300 在 (400,300)：长边 300 × 1.6 = 480，中心 (500,450)
        let r = crop_rect(1000, 1000, [0.4, 0.3, 0.2, 0.3], 1.6);
        assert_eq!(r, Rect { x: 260, y: 210, w: 480, h: 480 });
    }

    #[test]
    fn crop_stays_inside_when_face_sits_on_the_edge() {
        // 左上角的小脸：方块会移进来，但不会把脸切掉
        let r = crop_rect(1000, 1000, [0.0, 0.0, 0.1, 0.1], 1.6);
        assert_eq!(r, Rect { x: 0, y: 0, w: 256, h: 256 });
        assert!(r.x + r.w <= 1000 && r.y + r.h <= 1000);
    }

    #[test]
    fn crop_has_a_minimum_side_for_tiny_faces() {
        // 脸只有 30×36：外扩后 57.6，仍会被抬到 256
        let r = crop_rect(1024, 1024, [0.5, 0.5, 0.03, 0.035], 1.6);
        assert_eq!(r.w, MIN_SIDE);
        assert_eq!(r.h, MIN_SIDE);
    }

    #[test]
    fn crop_caps_at_the_smaller_image_side() {
        // 竖图：脸很大 + 外扩 2.5 也不会超出宽度
        let r = crop_rect(600, 1200, [0.2, 0.1, 0.6, 0.4], 2.5);
        assert_eq!(r.w, r.h);
        assert!(r.w <= 600);
        assert!(r.x + r.w <= 600 && r.y + r.h <= 1200);
    }

    #[test]
    fn crop_survives_a_degenerate_face_box() {
        // 检测失败时的兜底：宽高为 0 也给出一个合法方块
        let r = crop_rect(800, 600, [0.5, 0.4, 0.0, 0.0], f64::NAN);
        assert_eq!(r.w, r.h);
        assert!(r.w > 0);
        assert!(r.x + r.w <= 800 && r.y + r.h <= 600);
    }

    #[test]
    fn feather_composite_is_a_no_op_on_identical_pixels() {
        let mut base = RgbaImage::from_fn(64, 64, |x, y| {
            Rgba([(x * 3) as u8, (y * 5) as u8, ((x + y) * 2) as u8, 255])
        });
        let before = base.clone();
        let patch = image::imageops::crop_imm(&before, 16, 16, 32, 32).to_image();
        feather_composite(&mut base, &patch, Rect { x: 16, y: 16, w: 32, h: 32 }, 8).unwrap();
        assert_eq!(base, before, "同一个像素贴回同一个位置必须逐字节不变");
    }

    #[test]
    fn feather_composite_uses_patch_center_and_keeps_original_border() {
        let mut base = RgbaImage::from_pixel(40, 40, Rgba([0, 0, 0, 255]));
        let patch = RgbaImage::from_pixel(20, 20, Rgba([255, 255, 255, 255]));
        let dest = Rect { x: 10, y: 10, w: 20, h: 20 };
        feather_composite(&mut base, &patch, dest, 4).unwrap();
        // 中心全换成新块
        assert_eq!(base.get_pixel(20, 20)[0], 255);
        // 边界（羽化起点）保持原样
        assert_eq!(base.get_pixel(10, 10)[0], 0);
        assert_eq!(base.get_pixel(29, 29)[0], 0);
        // 中间是过渡值
        let mid = base.get_pixel(12, 20)[0];
        assert!(mid > 0 && mid < 255, "羽化带里应该是过渡值，实际 {mid}");
    }

    #[test]
    fn feather_composite_rejects_mismatched_patch() {
        let mut base = RgbaImage::new(40, 40);
        let patch = RgbaImage::new(10, 10);
        let err = feather_composite(&mut base, &patch, Rect { x: 0, y: 0, w: 20, h: 20 }, 4);
        assert!(err.is_err());
    }

    #[test]
    fn feather_composite_rejects_out_of_canvas() {
        let mut base = RgbaImage::new(40, 40);
        let patch = RgbaImage::new(20, 20);
        let err = feather_composite(&mut base, &patch, Rect { x: 30, y: 30, w: 20, h: 20 }, 4);
        assert!(err.is_err());
    }

    #[test]
    fn default_feather_scales_with_the_crop() {
        assert_eq!(default_feather(1024), 36);
        assert_eq!(default_feather(280), 10);
        assert_eq!(default_feather(64), 6);
    }

    #[test]
    fn face_mask_whitens_the_face_only() {
        let m = face_mask(256, 256, 128.0, 128.0, 80.0, 100.0, 1.15);
        assert_eq!(m.dimensions(), (256, 256));
        // 脸中心、下巴附近都是白的
        assert_eq!(m.get_pixel(128, 128)[0], 255);
        assert_eq!(m.get_pixel(128, 180)[0], 255);
        // 四角与裁块边缘是黑的（头发/背景不重画）
        for (x, y) in [(0u32, 0u32), (255, 0), (0, 255), (255, 255)] {
            assert_eq!(m.get_pixel(x, y)[0], 0, "({x},{y}) 不该被涂白");
        }
    }

    #[test]
    fn face_mask_covers_the_whole_face_box() {
        // 脸框 ×1.15 之后，框内的四条边（上下左右的中点）都该在蒙版里
        let (w, h) = (400u32, 400u32);
        let (cx, cy, fw, fh) = (200.0, 200.0, 120.0, 160.0);
        let m = face_mask(w, h, cx, cy, fw, fh, 1.15);
        for (x, y) in [
            (200, 200 - 80), // 额头（脸框上沿）
            (200, 200 + 80), // 下巴
            (200 - 60, 200), // 左耳
            (200 + 60, 200), // 右耳
        ] {
            assert_eq!(m.get_pixel(x, y)[0], 255, "({x},{y}) 应该在蒙版里");
        }
    }

    #[test]
    fn face_mask_is_clamped_inside_the_canvas() {
        // 脸贴边：蒙版不能越界（越界会 panic）
        let m = face_mask(100, 100, 2.0, 2.0, 80.0, 80.0, 1.2);
        assert_eq!(m.dimensions(), (100, 100));
        assert_eq!(m.get_pixel(0, 0)[0], 255);
        assert_eq!(m.get_pixel(99, 99)[0], 0);
    }
}
