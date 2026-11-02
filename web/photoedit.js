// 图片编辑器（v0.94）：裁切 / 套 LUT / 基础调色。
// 入口：图库卡片与详情、大图查看器、首页结果菜单与灯箱。
// 口径：原图不动，导出按原分辨率另存为新图（放在原图同目录）。
// 依赖：app.js 的 $/api/toast、gallery.rs 的 read_output_image / save_export、luts.rs 的三个命令。

const PE_SLIDERS = [
  { key: "exposure", label: "曝光", min: -2, max: 2, step: 0.05 },
  { key: "contrast", label: "对比度", min: -100, max: 100, step: 1 },
  { key: "highlights", label: "高光", min: -100, max: 100, step: 1 },
  { key: "shadows", label: "阴影", min: -100, max: 100, step: 1 },
  { key: "saturation", label: "饱和度", min: -100, max: 100, step: 1 },
  { key: "temp", label: "色温", min: -100, max: 100, step: 1 },
  { key: "tint", label: "色调", min: -100, max: 100, step: 1 },
  { key: "fade", label: "褪色", min: 0, max: 100, step: 1 },
  { key: "vignette", label: "暗角", min: 0, max: 100, step: 1 },
];

const PE_LOOKS = [
  { name: "原图", adj: {} },
  { name: "清透", adj: { exposure: 0.12, contrast: 10, highlights: -12, shadows: 18, saturation: 6 } },
  { name: "柔和", adj: { exposure: 0.05, contrast: -8, highlights: -16, shadows: 12, saturation: -6, fade: 16 } },
  { name: "胶片", adj: { contrast: 8, highlights: -18, shadows: 14, saturation: -12, temp: 8, fade: 12 } },
  { name: "暖阳", adj: { exposure: 0.1, temp: 24, saturation: 10, shadows: 8 } },
  { name: "冷夜", adj: { exposure: -0.06, temp: -22, contrast: 8, saturation: -6, shadows: -10 } },
  { name: "黑白", adj: { saturation: -100, contrast: 14 } },
];

const PE_ASPECTS = [
  { label: "自由", v: 0 },
  { label: "1:1", v: 1 },
  { label: "4:5", v: 0.8 },
  { label: "3:4", v: 0.75 },
  { label: "2:3", v: 2 / 3 },
  { label: "16:9", v: 16 / 9 },
  { label: "9:16", v: 9 / 16 },
  { label: "原图", v: -1 },
];

function peAdjZero() {
  return { exposure: 0, contrast: 0, highlights: 0, shadows: 0, saturation: 0, temp: 0, tint: 0, fade: 0, vignette: 0 };
}

const peEd = {
  open: false,
  path: "",
  filename: "",
  img: null,
  base: null,
  crop: null, // {x,y,w,h} in base pixels; null = 不裁
  aspect: 0,
  adj: peAdjZero(),
  lut: null, // {name,size,data}
  lutName: "",
  lutStrength: 100,
  lutList: [],
  lutCache: new Map(),
  tab: "light",
  view: { dx: 0, dy: 0, dw: 0, dh: 0, scale: 1, src: null },
  saving: false,
};

let peScratchCanvas = null;
let peRaf = 0;
let peIdle = 0;

// ---------------------------------------------------------------- 颜色管线

function peHasAdj() {
  const a = peEd.adj;
  for (const k in a) if (Math.abs(a[k]) > 1e-6) return true;
  return false;
}

function peLutOn() {
  return !!(peEd.lut && peEd.lut.size > 1 && peEd.lutStrength > 0.5);
}

/// 逐像素调色：白平衡/曝光 → 对比度 → 高光阴影 → 饱和度 → LUT → 褪色 → 暗角。
/// 全部是点运算，所以预览与导出的画面一致（预览只是分辨率低）。
function peApplyPixels(data, w, h, adj, lut, lutStrength) {
  const exposure = Math.pow(2, adj.exposure || 0);
  const con = 1 + (adj.contrast || 0) / 100;
  const sat = 1 + (adj.saturation || 0) / 100;
  const t = (adj.temp || 0) / 100;
  const tn = (adj.tint || 0) / 100;
  const rWB = (1 + t * 0.24) * (1 + tn * 0.05) * exposure;
  const gWB = (1 - tn * 0.16) * exposure;
  const bWB = (1 - t * 0.24) * (1 + tn * 0.05) * exposure;
  const hi = (adj.highlights || 0) / 100;
  const sh = (adj.shadows || 0) / 100;
  const fade = (adj.fade || 0) / 100;
  const vig = (adj.vignette || 0) / 100;

  const useLut = !!(lut && lut.size > 1 && lutStrength > 0.5);
  const N = useLut ? lut.size : 0;
  const LD = useLut ? lut.data : null;
  const lm1 = N - 1;
  const N3 = N * 3;
  const N3N = N * N * 3;
  const ls = useLut ? lutStrength / 100 : 0;

  const cx = w / 2;
  const cy = h / 2;
  const invD = 1 / Math.hypot(cx, cy);

  const doTone = con !== 1 || !!hi || !!sh;
  const doSat = sat !== 1;

  for (let y = 0, i = 0; y < h; y++) {
    const dy = (y - cy) * invD;
    for (let x = 0; x < w; x++, i += 4) {
      let r = data[i] / 255;
      let g = data[i + 1] / 255;
      let b = data[i + 2] / 255;

      // 白平衡 + 曝光（按 gamma 空间近似，够用且便宜）
      r *= rWB;
      g *= gWB;
      b *= bWB;

      if (doTone) {
        // 对比度：以 0.5 为轴
        if (con !== 1) {
          r = (r - 0.5) * con + 0.5;
          g = (g - 0.5) * con + 0.5;
          b = (b - 0.5) * con + 0.5;
        }
        // 高光 / 阴影：按亮度加权，暗部与亮部分开推
        if (hi || sh) {
          const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
          let dl = 0;
          if (sh) {
            const k = 1 - l * 1.55; // 1（纯黑）→ 0（0.65 以上）
            if (k > 0) dl += sh * 0.42 * k;
          }
          if (hi) {
            const k = (l - 0.32) * 1.47; // 0.32 以下不起作用
            if (k > 0) dl += hi * 0.42 * Math.min(1, k);
          }
          r += dl;
          g += dl;
          b += dl;
        }
      }

      if (doSat) {
        const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        r = l + (r - l) * sat;
        g = l + (g - l) * sat;
        b = l + (b - l) * sat;
      }

      // 夹到 0..1 再进 LUT
      r = r < 0 ? 0 : r > 1 ? 1 : r;
      g = g < 0 ? 0 : g > 1 ? 1 : g;
      b = b < 0 ? 0 : b > 1 ? 1 : b;

      if (useLut) {
        const fr = r * lm1;
        const fg = g * lm1;
        const fb = b * lm1;
        const r0 = fr | 0;
        const g0 = fg | 0;
        const b0 = fb | 0;
        const wr = fr - r0;
        const wg = fg - g0;
        const wb = fb - b0;
        const dr = (r0 < lm1 ? 1 : 0) * 3;
        const dg = (g0 < lm1 ? 1 : 0) * N3;
        const db = (b0 < lm1 ? 1 : 0) * N3N;
        const i00 = b0 * N3N + g0 * N3 + r0 * 3;
        const i01 = i00 + dg;
        const i10 = i00 + db;
        const i11 = i01 + db;

        const r00 = LD[i00] + (LD[i00 + dr] - LD[i00]) * wr;
        const g00 = LD[i00 + 1] + (LD[i00 + dr + 1] - LD[i00 + 1]) * wr;
        const b00 = LD[i00 + 2] + (LD[i00 + dr + 2] - LD[i00 + 2]) * wr;
        const r01 = LD[i01] + (LD[i01 + dr] - LD[i01]) * wr;
        const g01 = LD[i01 + 1] + (LD[i01 + dr + 1] - LD[i01 + 1]) * wr;
        const b01 = LD[i01 + 2] + (LD[i01 + dr + 2] - LD[i01 + 2]) * wr;
        const r10 = LD[i10] + (LD[i10 + dr] - LD[i10]) * wr;
        const g10 = LD[i10 + 1] + (LD[i10 + dr + 1] - LD[i10 + 1]) * wr;
        const b10 = LD[i10 + 2] + (LD[i10 + dr + 2] - LD[i10 + 2]) * wr;
        const r11 = LD[i11] + (LD[i11 + dr] - LD[i11]) * wr;
        const g11 = LD[i11 + 1] + (LD[i11 + dr + 1] - LD[i11 + 1]) * wr;
        const b11 = LD[i11 + 2] + (LD[i11 + dr + 2] - LD[i11 + 2]) * wr;

        const r0g = r00 + (r01 - r00) * wg;
        const g0g = g00 + (g01 - g00) * wg;
        const b0g = b00 + (b01 - b00) * wg;
        const r1g = r10 + (r11 - r10) * wg;
        const g1g = g10 + (g11 - g10) * wg;
        const b1g = b10 + (b11 - b10) * wg;

        r += (r0g + (r1g - r0g) * wb - r) * ls;
        g += (g0g + (g1g - g0g) * wb - g) * ls;
        b += (b0g + (b1g - b0g) * wb - b) * ls;
      }

      if (fade) {
        r = r * (1 - fade * 0.32) + fade * 0.055;
        g = g * (1 - fade * 0.32) + fade * 0.055;
        b = b * (1 - fade * 0.32) + fade * 0.055;
      }

      if (vig) {
        const dx = (x - cx) * invD;
        const d2 = dx * dx + dy * dy; // 0（中心）→ 1（角落）
        const k = 1 - vig * 0.72 * Math.max(0, d2 - 0.18) / 0.82;
        r *= k;
        g *= k;
        b *= k;
      }

      data[i] = r * 255;
      data[i + 1] = g * 255;
      data[i + 2] = b * 255;
    }
  }
}

// ---------------------------------------------------------------- .cube

/// 解析 3D .cube（TITLE/DOMAIN 行忽略，DOMAIN_MIN/MAX 做线性归一）。
function peParseCube(text) {
  const lines = String(text || "").split(/\r?\n/);
  let size = 0;
  let dmin = [0, 0, 0];
  let dmax = [1, 1, 1];
  const vals = [];
  for (const raw of lines) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const up = line.toUpperCase();
    if (up.startsWith("TITLE")) continue;
    if (up.startsWith("LUT_3D_SIZE")) {
      size = parseInt(line.split(/\s+/)[1], 10) || 0;
      continue;
    }
    if (up.startsWith("LUT_1D_SIZE")) throw new Error("这是 1D LUT，暂不支持（请用 3D .cube）");
    if (up.startsWith("DOMAIN_MIN")) {
      dmin = line.split(/\s+/).slice(1, 4).map(Number);
      continue;
    }
    if (up.startsWith("DOMAIN_MAX")) {
      dmax = line.split(/\s+/).slice(1, 4).map(Number);
      continue;
    }
    const parts = line.split(/\s+/);
    if (parts.length < 3) continue;
    const r = Number(parts[0]);
    const g = Number(parts[1]);
    const b = Number(parts[2]);
    if (!Number.isFinite(r) || !Number.isFinite(g) || !Number.isFinite(b)) continue;
    vals.push(r, g, b);
  }
  if (!size) throw new Error("LUT 缺 LUT_3D_SIZE");
  if (size < 2 || size > 128) throw new Error(`LUT 尺寸不支持：${size}`);
  const need = size * size * size * 3;
  if (vals.length < need) throw new Error(`LUT 数据不完整（${vals.length}/${need}）`);

  // 有的文件给 0..255 或 0..65535，按最大值兜一下
  let maxV = 0;
  for (let i = 0; i < need; i++) if (vals[i] > maxV) maxV = vals[i];
  const scale = maxV > 2 ? (maxV > 300 ? 65535 : 255) : 1;

  const data = new Float32Array(need);
  const span = [dmax[0] - dmin[0] || 1, dmax[1] - dmin[1] || 1, dmax[2] - dmin[2] || 1];
  for (let i = 0; i < need; i++) {
    const c = i % 3;
    const v = vals[i] / scale;
    data[i] = Math.min(1, Math.max(0, (v - dmin[c]) / span[c]));
  }
  return { size, data };
}

// ---------------------------------------------------------------- 画布

function peScratch(w, h) {
  if (!peScratchCanvas) peScratchCanvas = document.createElement("canvas");
  if (peScratchCanvas.width !== w || peScratchCanvas.height !== h) {
    peScratchCanvas.width = w;
    peScratchCanvas.height = h;
  }
  return peScratchCanvas;
}

function peFull() {
  return { x: 0, y: 0, w: peEd.base.width, h: peEd.base.height };
}

function peCropFull() {
  const b = peEd.crop;
  if (!b || !peEd.base) return true;
  return b.x <= 1 && b.y <= 1 && b.w >= peEd.base.width - 2 && b.h >= peEd.base.height - 2;
}

/// 当前要显示/导出的源矩形（裁切后）。
function peSrcRect() {
  if (!peEd.base) return { x: 0, y: 0, w: 1, h: 1 };
  if (peCropFull()) return peFull();
  return peEd.crop;
}

function peSchedule(fast) {
  if (!peEd.open) return;
  if (fast) {
    clearTimeout(peIdle);
    if (!peRaf) {
      peRaf = requestAnimationFrame(() => {
        peRaf = 0;
        peDraw(true);
      });
    }
  } else {
    clearTimeout(peIdle);
    peIdle = setTimeout(() => peDraw(false), 120);
  }
}

function peDraw(fast) {
  if (!peEd.open || !peEd.base) return;
  const stage = $("peStage");
  const canvas = $("peCanvas");
  if (!stage || !canvas) return;
  const W = stage.clientWidth;
  const H = stage.clientHeight;
  if (W < 20 || H < 20) return;

  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const pw = Math.round(W * dpr);
  const ph = Math.round(H * dpr);
  if (canvas.width !== pw || canvas.height !== ph) {
    canvas.width = pw;
    canvas.height = ph;
  }
  canvas.style.width = W + "px";
  canvas.style.height = H + "px";

  const ctx = canvas.getContext("2d");
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.fillStyle = "#080a0e";
  ctx.fillRect(0, 0, W, H);

  // 裁切页看全图（框还在），其它页看裁切结果
  const inCrop = peEd.tab === "crop";
  const src = inCrop ? peFull() : peSrcRect();
  const scale = Math.min(W / src.w, H / src.h);
  const dw = Math.max(1, Math.round(src.w * scale));
  const dh = Math.max(1, Math.round(src.h * scale));
  const dx = Math.round((W - dw) / 2);
  const dy = Math.round((H - dh) / 2);
  peEd.view = { dx, dy, dw, dh, scale, src };

  // 预览降采样：拖动时小（快），停手后再细
  const cap = fast ? 620 : 1500;
  const k = Math.min(1, cap / Math.max(dw, dh));
  const tw = Math.max(1, Math.round(dw * k));
  const th = Math.max(1, Math.round(dh * k));

  const tmp = peScratch(tw, th);
  const tctx = tmp.getContext("2d", { willReadFrequently: true });
  tctx.clearRect(0, 0, tw, th);
  tctx.imageSmoothingQuality = "high";
  tctx.drawImage(peEd.base, src.x, src.y, src.w, src.h, 0, 0, tw, th);

  if (peHasAdj() || peLutOn()) {
    const img = tctx.getImageData(0, 0, tw, th);
    peApplyPixels(img.data, tw, th, peEd.adj, peEd.lut, peEd.lutStrength);
    tctx.putImageData(img, 0, 0);
  }

  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(tmp, 0, 0, tw, th, dx, dy, dw, dh);

  peSyncCrop();
  peSyncSizeLabel();
}

// ---------------------------------------------------------------- 裁切框

function peClampCrop(b) {
  const W = peEd.base.width;
  const H = peEd.base.height;
  const min = 24;
  b.w = Math.min(Math.max(b.w, min), W);
  b.h = Math.min(Math.max(b.h, min), H);
  b.x = Math.min(Math.max(b.x, 0), W - b.w);
  b.y = Math.min(Math.max(b.y, 0), H - b.h);
  return b;
}

function peSetAspect(v) {
  peEd.aspect = v;
  document.querySelectorAll("#peAspects button").forEach((b) => {
    const want = Number(b.dataset.v);
    b.classList.toggle("active", want === peEd.aspect);
  });
  if (peEd.aspect !== 0) {
    const r = peEd.aspect < 0 ? peEd.base.width / peEd.base.height : peEd.aspect;
    const cur = peEd.crop || peFull();
    const cx = cur.x + cur.w / 2;
    const cy = cur.y + cur.h / 2;
    let w = cur.w;
    let h = w / r;
    if (h > peEd.base.height) {
      h = peEd.base.height;
      w = h * r;
    }
    if (w > peEd.base.width) {
      w = peEd.base.width;
      h = w / r;
    }
    const box = peClampCrop({ x: cx - w / 2, y: cy - h / 2, w, h });
    peEd.crop = peCropFullBox(box) ? null : box;
  }
  peSyncCrop();
  peSchedule(false);
}

function peCropFullBox(box) {
  return box.x <= 1 && box.y <= 1 && box.w >= peEd.base.width - 2 && box.h >= peEd.base.height - 2;
}

function peSyncCrop() {
  const on = peEd.tab === "crop" && !!peEd.base;
  const ov = $("peCrop");
  if (!ov) return;
  ov.hidden = !on;
  if (!on) return;

  const v = peEd.view;
  ov.style.left = v.dx + "px";
  ov.style.top = v.dy + "px";
  ov.style.width = v.dw + "px";
  ov.style.height = v.dh + "px";

  const b = peEd.crop || peFull();
  const box = $("peCropBox");
  box.style.left = (b.x - v.src.x) * v.scale + "px";
  box.style.top = (b.y - v.src.y) * v.scale + "px";
  box.style.width = Math.max(10, b.w * v.scale) + "px";
  box.style.height = Math.max(10, b.h * v.scale) + "px";

  const hint = $("peCropHint");
  if (hint) {
    const base = peEd.base;
    hint.textContent = peCropFull()
      ? `未裁切 · 原图 ${base.width}×${base.height}`
      : `框选 ${Math.round(b.w)}×${Math.round(b.h)} · 原图 ${base.width}×${base.height}`;
  }
}

function peSyncSizeLabel() {
  const el = $("peSize");
  if (!el || !peEd.base) return;
  const s = peSrcRect();
  el.textContent = `${Math.round(s.w)}×${Math.round(s.h)}`;
}

// 裁切拖动
let peDrag = null;

function peBaseFromEvent(ev) {
  const ov = $("peCrop");
  const rect = ov.getBoundingClientRect();
  const v = peEd.view;
  return {
    x: v.src.x + (ev.clientX - rect.left) / v.scale,
    y: v.src.y + (ev.clientY - rect.top) / v.scale,
  };
}

function peCropPointerDown(ev) {
  if (peEd.tab !== "crop" || !peEd.base) return;
  ev.preventDefault();
  const ov = $("peCrop");
  const p = peBaseFromEvent(ev);
  const b = peEd.crop || peFull();
  const h = ev.target && ev.target.dataset ? ev.target.dataset.h : "";
  const inside = p.x >= b.x && p.x <= b.x + b.w && p.y >= b.y && p.y <= b.y + b.h;

  if (h) {
    // 对角锚点：拖哪个角，对面那个角不动
    const ax = h.includes("w") ? b.x + b.w : b.x;
    const ay = h.includes("n") ? b.y + b.h : b.y;
    peDrag = { mode: "corner", ax, ay, sx: h.includes("e") ? 1 : -1, sy: h.includes("s") ? 1 : -1 };
  } else if (inside && !peCropFull()) {
    peDrag = { mode: "move", from: p, box: { ...b } };
  } else {
    // 框还是整张时，框内拖动也应该重新拉一个框——否则整张图没有「框外」可点
    peDrag = { mode: "new", ax: p.x, ay: p.y, sx: 1, sy: 1 };
  }
  try {
    ov.setPointerCapture(ev.pointerId);
  } catch {}
}

function peCropPointerMove(ev) {
  if (!peDrag || !peEd.base) return;
  const p = peBaseFromEvent(ev);
  const W = peEd.base.width;
  const H = peEd.base.height;

  if (peDrag.mode === "move") {
    const nx = peDrag.box.x + (p.x - peDrag.from.x);
    const ny = peDrag.box.y + (p.y - peDrag.from.y);
    peEd.crop = peClampCrop({ x: nx, y: ny, w: peDrag.box.w, h: peDrag.box.h });
    peSyncCrop();
    return;
  }

  const ax = peDrag.ax;
  const ay = peDrag.ay;
  let w = Math.abs(p.x - ax);
  let h = Math.abs(p.y - ay);
  const sx = p.x >= ax ? 1 : -1;
  const sy = p.y >= ay ? 1 : -1;

  const r = peEd.aspect === 0 ? 0 : peEd.aspect < 0 ? W / H : peEd.aspect;
  if (r > 0) {
    if (w / Math.max(1e-6, h) > r) h = w / r;
    else w = h * r;
  }

  // 别超出画布：以锚点为准向内收
  const maxW = sx > 0 ? W - ax : ax;
  const maxH = sy > 0 ? H - ay : ay;
  if (w > maxW) {
    w = maxW;
    if (r > 0) h = w / r;
  }
  if (h > maxH) {
    h = maxH;
    if (r > 0) w = h * r;
  }
  w = Math.max(24, w);
  h = Math.max(24, h);

  const box = {
    x: sx > 0 ? ax : ax - w,
    y: sy > 0 ? ay : ay - h,
    w,
    h,
  };
  peEd.crop = peClampCrop(box);
  peSyncCrop();
}

function peCropPointerUp(ev) {
  if (!peDrag) return;
  peDrag = null;
  const ov = $("peCrop");
  try {
    ov.releasePointerCapture(ev.pointerId);
  } catch {}
  // 框=整张就别当裁切，省得导出走一次无意义的重采样
  if (peCropFull()) peEd.crop = null;
  else peEd.crop = {
    x: Math.round(peEd.crop.x),
    y: Math.round(peEd.crop.y),
    w: Math.round(peEd.crop.w),
    h: Math.round(peEd.crop.h),
  };
  peSyncCrop();
  peSyncSizeLabel();
}

// ---------------------------------------------------------------- 旋转 / 重置

function peBaseFromImage() {
  const img = peEd.img;
  const c = document.createElement("canvas");
  c.width = img.naturalWidth;
  c.height = img.naturalHeight;
  c.getContext("2d").drawImage(img, 0, 0);
  peEd.base = c;
}

function peRotate(dir) {
  if (!peEd.base) return;
  const b = peEd.base;
  const c = document.createElement("canvas");
  c.width = b.height;
  c.height = b.width;
  const ctx = c.getContext("2d");
  ctx.translate(c.width / 2, c.height / 2);
  ctx.rotate((dir * Math.PI) / 2);
  ctx.drawImage(b, -b.width / 2, -b.height / 2);
  peEd.base = c;
  peEd.crop = null;
  peSetAspect(0);
  peSchedule(false);
}

function peResetCrop() {
  peEd.crop = null;
  peSetAspect(0);
  peSyncCrop();
  peSchedule(false);
}

function peResetAdj() {
  peEd.adj = peAdjZero();
  peSyncSliders();
  peSchedule(false);
}

function peResetAll() {
  peBaseFromImage();
  peEd.crop = null;
  peEd.adj = peAdjZero();
  peEd.lut = null;
  peEd.lutName = "";
  peEd.lutStrength = 100;
  peSetAspect(0);
  peSyncSliders();
  peSyncLut();
  peSchedule(false);
}

// ---------------------------------------------------------------- 界面同步

function peSyncSliders() {
  document.querySelectorAll("#peSliders input[type=range]").forEach((inp) => {
    const k = inp.dataset.k;
    inp.value = peEd.adj[k] || 0;
  });
  peSyncSliderLabels();
  document.querySelectorAll("#pePresets button").forEach((b) => b.classList.remove("active"));
}

function peSyncSliderLabels() {
  document.querySelectorAll("#peSliders .pe-row").forEach((row) => {
    const inp = row.querySelector("input");
    const val = row.querySelector(".pe-val");
    if (!inp || !val) return;
    const k = inp.dataset.k;
    const v = peEd.adj[k] || 0;
    val.textContent = k === "exposure" ? (v > 0 ? "+" : "") + Number(v).toFixed(2) : Math.round(v);
    val.classList.toggle("on", Math.abs(v) > 1e-6);
  });
}

function peSyncLut() {
  const sel = $("peLutSelect");
  if (!sel) return;
  sel.value = peEd.lutName || "";
  const str = $("peLutStr");
  if (str) str.value = peEd.lutStrength;
  const val = $("peLutStrVal");
  if (val) val.textContent = Math.round(peEd.lutStrength) + "%";
  const hint = $("peLutHint");
  if (hint) {
    if (peEd.lut) {
      const n = peEd.lut.size;
      hint.textContent = `已载入 ${n}³（${n * n * n} 个网格点）· ${peEd.lutName}`;
    } else if (peEd.lutList.length) {
      hint.textContent = `luts 目录里有 ${peEd.lutList.length} 个 .cube，选一个套上`;
    } else {
      hint.textContent = "还没有 LUT：点「导入 .cube…」把文件放进壳的 luts 目录";
    }
  }
}

async function peLoadLutList(keep) {
  try {
    peEd.lutList = await api.call("luts_list");
  } catch {
    peEd.lutList = [];
  }
  const sel = $("peLutSelect");
  if (!sel) return;
  const cur = keep === undefined ? peEd.lutName : keep;
  sel.innerHTML = "";
  const none = document.createElement("option");
  none.value = "";
  none.textContent = "不用 LUT";
  sel.appendChild(none);
  for (const l of peEd.lutList) {
    const o = document.createElement("option");
    o.value = l.name;
    o.textContent = l.name.replace(/\.cube$/i, "");
    sel.appendChild(o);
  }
  sel.value = cur && peEd.lutList.some((l) => l.name === cur) ? cur : "";
  if (sel.value !== peEd.lutName) {
    peEd.lutName = sel.value;
    if (!sel.value) peEd.lut = null;
  }
  peSyncLut();
}

async function peUseLut(name) {
  if (!name) {
    peEd.lut = null;
    peEd.lutName = "";
    peSyncLut();
    peSchedule(false);
    return;
  }
  try {
    let lut = peEd.lutCache.get(name);
    if (!lut) {
      const text = await api.call("luts_read", { name });
      lut = peParseCube(text);
      peEd.lutCache.set(name, lut);
    }
    peEd.lut = lut;
    peEd.lutName = name;
    peSyncLut();
    peSchedule(false);
  } catch (e) {
    toast(String(e && e.message ? e.message : e), true);
    peEd.lut = null;
    peEd.lutName = "";
    $("peLutSelect").value = "";
    peSyncLut();
    peSchedule(false);
  }
}

// ---------------------------------------------------------------- 导出

function pePad2(n) {
  return String(n).padStart(2, "0");
}

function peOutName(fmt) {
  const stem =
    (peEd.filename || "image")
      .replace(/\.[^.]+$/, "")
      .replace(/[_\-\s]+$/, "") || "image";
  const d = new Date();
  const stamp =
    `${d.getFullYear()}${pePad2(d.getMonth() + 1)}${pePad2(d.getDate())}-` +
    `${pePad2(d.getHours())}${pePad2(d.getMinutes())}${pePad2(d.getSeconds())}`;
  return `${stem}_edit_${stamp}.${fmt}`;
}

/// 原图在 output 下的子目录（projects/<id> 之类），另存时放回同一处。
function peSubdir() {
  const p = String(peEd.path || "");
  const m = /[\\/]output[\\/](.+)[\\/][^\\/]+$/.exec(p);
  return m ? m[1].replace(/\\/g, "/") : "";
}

async function peSave() {
  if (peEd.saving || !peEd.base) return;
  peEd.saving = true;
  const btn = $("peSave");
  const busy = $("peBusy");
  btn.disabled = true;
  btn.textContent = "导出中…";
  if (busy) {
    busy.textContent = "导出中…";
    busy.hidden = false;
  }
  // 逐像素循环会占住主线程：先让「导出中…」画出来，不然用户以为点了没反应
  await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0)));
  try {
    const src = peSrcRect();
    const c = document.createElement("canvas");
    c.width = Math.max(1, Math.round(src.w));
    c.height = Math.max(1, Math.round(src.h));
    const ctx = c.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(peEd.base, src.x, src.y, src.w, src.h, 0, 0, c.width, c.height);
    if (peHasAdj() || peLutOn()) {
      const img = ctx.getImageData(0, 0, c.width, c.height);
      peApplyPixels(img.data, c.width, c.height, peEd.adj, peEd.lut, peEd.lutStrength);
      ctx.putImageData(img, 0, 0);
    }
    const fmt = ($("peFormat") && $("peFormat").value) === "jpg" ? "jpg" : "png";
    const dataUrl = c.toDataURL(fmt === "jpg" ? "image/jpeg" : "image/png", 0.94);
    const name = peOutName(fmt);
    const path = await api.call("save_export", { name, dataUrl, subdir: peSubdir() });
    toast(`已另存：${path}`);
    await peAfterSave({ path, filename: name, width: c.width, height: c.height, dataUrl });
  } catch (e) {
    toast(String(e), true);
  } finally {
    peEd.saving = false;
    btn.disabled = false;
    btn.textContent = "另存为新图";
    if (busy) busy.hidden = true;
  }
}

async function peAfterSave(saved) {
  const from = peEd.path;
  peClose();
  const lib = typeof $ === "function" ? $("library") : null;
  if (lib && !lib.hidden && typeof loadLibrary === "function") {
    try {
      await loadLibrary();
    } catch {}
  }
  if (typeof mergeResults === "function") {
    mergeResults([
      {
        path: saved.path,
        filename: saved.filename,
        width: saved.width,
        height: saved.height,
        label: "调色",
        seconds: 0,
        ts: Date.now() / 1000,
        prompt: "",
        dataUrl: saved.dataUrl,
      },
    ]);
  }
  void from;
}

// ---------------------------------------------------------------- 开关

function peSetTab(tab) {
  peEd.tab = tab;
  document.querySelectorAll("#peTabs .tab").forEach((b) => b.classList.toggle("active", b.dataset.tab === tab));
  document.querySelectorAll(".pe-pane").forEach((p) => (p.hidden = p.dataset.pane !== tab));
  peSchedule(false);
}

async function peOpen(item) {
  if (!item || !item.path) return;
  if (String(item.path).startsWith("vault://")) {
    toast("私密图不支持调色（不把明文解到界面）", true);
    return;
  }
  const modal = $("peModal");
  if (!modal) return;

  // 重置到干净状态
  peEd.path = item.path;
  peEd.filename = item.filename || String(item.path).split(/[\\/]/).pop() || "image";
  peEd.img = null;
  peEd.base = null;
  peEd.crop = null;
  peEd.aspect = 0;
  peEd.adj = peAdjZero();
  peEd.lut = null;
  peEd.lutName = "";
  peEd.lutStrength = 100;
  peEd.saving = false;
  peEd.tab = "light";

  $("peTitle").textContent = peEd.filename;
  $("peSize").textContent = "";
  $("peBusy").hidden = false;
  $("peBusy").textContent = "载入中…";
  modal.hidden = false;
  peEd.open = true;

  peBuildSliders();
  peBuildLooks();
  peBuildAspects();
  peSyncSliders();
  peSetTab("light");

  try {
    const url = item.dataUrl || (await api.call("read_output_image", { path: item.path }));
    const img = await peLoadImage(url);
    peEd.img = img;
    peBaseFromImage();
    await peLoadLutList("");
    $("peBusy").hidden = true;
    requestAnimationFrame(() => peDraw(false));
  } catch (e) {
    $("peBusy").hidden = true;
    toast(String(e), true);
    peClose();
  }
}

function peLoadImage(src) {
  return new Promise((res, rej) => {
    const img = new Image();
    img.onload = () => res(img);
    img.onerror = () => rej(new Error("图片载入失败"));
    img.src = src;
  });
}

function peClose() {
  peEd.open = false;
  peEd.saving = false;
  peEd.img = null;
  peEd.base = null;
  const modal = $("peModal");
  if (modal) modal.hidden = true;
}

// ---------------------------------------------------------------- 构建控件

function peBuildSliders() {
  const box = $("peSliders");
  if (!box || box.childElementCount) return;
  for (const s of PE_SLIDERS) {
    const row = document.createElement("div");
    row.className = "pe-row";
    const lab = document.createElement("label");
    lab.textContent = s.label;
    const inp = document.createElement("input");
    inp.type = "range";
    inp.min = s.min;
    inp.max = s.max;
    inp.step = s.step;
    inp.value = 0;
    inp.dataset.k = s.key;
    inp.oninput = () => {
      peEd.adj[s.key] = Number(inp.value);
      peSyncSliderLabels();
      peSchedule(true);
      document.querySelectorAll("#pePresets button").forEach((b) => b.classList.remove("active"));
    };
    inp.onchange = () => peSchedule(false);
    inp.ondblclick = () => {
      peEd.adj[s.key] = 0;
      inp.value = 0;
      peSyncSliderLabels();
      peSchedule(false);
    };
    const val = document.createElement("span");
    val.className = "pe-val mono";
    row.append(lab, inp, val);
    box.appendChild(row);
  }
}

function peBuildLooks() {
  const box = $("pePresets");
  if (!box || box.childElementCount) return;
  for (const look of PE_LOOKS) {
    const b = document.createElement("button");
    b.className = "btn ghost small";
    b.textContent = look.name;
    b.onclick = () => {
      peEd.adj = Object.assign(peAdjZero(), look.adj || {});
      peSyncSliders();
      document.querySelectorAll("#pePresets button").forEach((x) => x.classList.toggle("active", x === b));
      peSchedule(false);
    };
    box.appendChild(b);
  }
}

function peBuildAspects() {
  const box = $("peAspects");
  if (!box || box.childElementCount) return;
  for (const a of PE_ASPECTS) {
    const b = document.createElement("button");
    b.className = "btn ghost small" + (a.v === 0 ? " active" : "");
    b.textContent = a.label;
    b.dataset.v = String(a.v);
    b.onclick = () => peSetAspect(a.v);
    box.appendChild(b);
  }
}

// ---------------------------------------------------------------- 绑定

(function peBind() {
  const bind = (id, fn) => {
    const el = $(id);
    if (el) el.onclick = fn;
  };

  // 入口：首页灯箱 / 大图查看器（图库卡片与菜单在 detail.js，首页卡片菜单在 app.js）
  bind("lbEdit", () => {
    const it = typeof lightboxItem !== "undefined" ? lightboxItem : null;
    if (it) peOpen({ path: it.path, filename: it.filename, dataUrl: it.dataUrl });
  });
  bind("vwEdit", () => {
    const it = typeof viewer !== "undefined" ? viewer.list[viewer.idx] : null;
    if (it) peOpen({ path: it.path, filename: it.filename });
  });

  bind("peClose", peClose);
  bind("peResetAll", peResetAll);
  bind("peResetAdj", peResetAdj);
  bind("peCropReset", peResetCrop);
  bind("peRotL", () => peRotate(-1));
  bind("peRotR", () => peRotate(1));
  bind("peSave", peSave);

  document.querySelectorAll("#peTabs .tab").forEach((b) => {
    b.onclick = () => peSetTab(b.dataset.tab);
  });

  const str = $("peLutStr");
  if (str) {
    str.oninput = () => {
      peEd.lutStrength = Number(str.value);
      const val = $("peLutStrVal");
      if (val) val.textContent = Math.round(peEd.lutStrength) + "%";
      peSchedule(true);
    };
    str.onchange = () => peSchedule(false);
  }

  const sel = $("peLutSelect");
  if (sel) sel.onchange = () => peUseLut(sel.value);

  bind("peLutRefresh", async () => {
    await peLoadLutList(peEd.lutName);
    toast("LUT 列表已刷新");
  });

  bind("peLutFolder", async () => {
    try {
      const dir = await api.call("luts_dir_path");
      await api.call("open_folder", { path: dir });
    } catch (e) {
      toast(String(e), true);
    }
  });

  bind("peLutImport", () => $("peLutFile").click());

  const file = $("peLutFile");
  if (file) {
    file.onchange = async () => {
      const f = file.files && file.files[0];
      file.value = "";
      if (!f) return;
      try {
        const text = await f.text();
        await api.call("luts_save", { name: f.name, text });
        toast(`已导入 ${f.name}`);
        await peLoadLutList(f.name);
        await peUseLut(f.name);
      } catch (e) {
        toast(String(e), true);
      }
    };
  }

  bind("peLutDelete", async () => {
    const name = peEd.lutName;
    if (!name) {
      toast("先选一个 LUT", true);
      return;
    }
    if (!confirm(`删除 LUT「${name}」？\n只删壳的 luts 目录里的这一份。`)) return;
    try {
      await api.call("luts_delete", { name });
      peEd.lutCache.delete(name);
      peEd.lut = null;
      peEd.lutName = "";
      await peLoadLutList("");
      peSchedule(false);
      toast("已删除");
    } catch (e) {
      toast(String(e), true);
    }
  });

  const ov = $("peCrop");
  if (ov) {
    ov.addEventListener("pointerdown", peCropPointerDown);
    ov.addEventListener("pointermove", peCropPointerMove);
    ov.addEventListener("pointerup", peCropPointerUp);
    ov.addEventListener("pointercancel", peCropPointerUp);
  }

  // Esc：只关这一层，别顺手把下面的大图/图库也关了
  window.addEventListener(
    "keydown",
    (e) => {
      if (!peEd.open || e.key !== "Escape") return;
      e.stopPropagation();
      e.preventDefault();
      peClose();
    },
    true
  );

  if (typeof ResizeObserver === "function" && $("peStage")) {
    let lastW = 0;
    let lastH = 0;
    new ResizeObserver(() => {
      const st = $("peStage");
      if (!st || !peEd.open) return;
      if (st.clientWidth === lastW && st.clientHeight === lastH) return;
      lastW = st.clientWidth;
      lastH = st.clientHeight;
      peSchedule(false);
    }).observe($("peStage"));
  }
})();

// 给其它模块用的唯一入口
window.photoEditor = {
  open: peOpen,
  close: peClose,
  isOpen: () => peEd.open,
};
