// 图片编辑器（v0.95）：裁切 / 套 LUT / ACR 式调色（亮·颜色·效果·曲线·细节）。
// 入口：图库卡片与详情、大图查看器、首页结果菜单与灯箱。
// 口径：默认另存新图（放原图同目录）；炸图可「覆盖原图」。
// 依赖：app.js 的 $/api/toast、gallery.rs 的 read_output_image / save_export / save_over_image、luts.rs 的命令。

const PE_SLIDERS = [
  { key: "exposure", label: "曝光", min: -2, max: 2, step: 0.05, digits: 2 },
  { key: "contrast", label: "对比度", min: -100, max: 100, step: 1 },
  { key: "highlights", label: "高光", min: -100, max: 100, step: 1 },
  { key: "shadows", label: "阴影", min: -100, max: 100, step: 1 },
  { key: "whites", label: "白色", min: -100, max: 100, step: 1 },
  { key: "blacks", label: "黑色", min: -100, max: 100, step: 1 },
  { key: "temp", label: "色温", min: -100, max: 100, step: 1 },
  { key: "tint", label: "色调", min: -100, max: 100, step: 1 },
  { key: "vibrance", label: "自然饱和度", min: -100, max: 100, step: 1 },
  { key: "saturation", label: "饱和度", min: -100, max: 100, step: 1 },
  { key: "texture", label: "纹理", min: 0, max: 100, step: 1 },
  { key: "clarity", label: "清晰度", min: 0, max: 100, step: 1 },
  { key: "dehaze", label: "去除薄雾", min: -100, max: 100, step: 1 },
  { key: "vignette", label: "晕影", min: -100, max: 100, step: 1 },
  { key: "grain", label: "颗粒", min: 0, max: 100, step: 1 },
  { key: "curveHi", label: "高光", min: -100, max: 100, step: 1 },
  { key: "curveLi", label: "亮调", min: -100, max: 100, step: 1 },
  { key: "curveDk", label: "暗调", min: -100, max: 100, step: 1 },
  { key: "curveSh", label: "阴影", min: -100, max: 100, step: 1 },
  { key: "sharpen", label: "锐化", min: 0, max: 150, step: 1 },
  { key: "nr", label: "减少杂色", min: 0, max: 100, step: 1 },
  { key: "chromaNr", label: "杂色深度", min: 0, max: 100, step: 1 },
];

/// 面板分组照 ACR：亮 / 颜色 / 效果 / 曲线 / 细节
const PE_SECTIONS = [
  { id: "light", title: "亮", keys: ["exposure", "contrast", "highlights", "shadows", "whites", "blacks"] },
  { id: "color", title: "颜色", keys: ["temp", "tint", "vibrance", "saturation"], wb: true },
  { id: "fx", title: "效果", keys: ["texture", "clarity", "dehaze", "vignette", "grain"] },
  { id: "curve", title: "曲线", keys: ["curveHi", "curveLi", "curveDk", "curveSh"], curve: true },
  { id: "detail", title: "细节", keys: ["sharpen", "nr", "chromaNr"] },
];

const PE_LOOKS = [
  { name: "原图", adj: {} },
  { name: "清透", adj: { exposure: 0.1, contrast: 8, highlights: -12, shadows: 16, whites: 6, vibrance: 8 } },
  { name: "柔和", adj: { exposure: 0.05, contrast: -6, highlights: -14, shadows: 12, vibrance: -4, curveSh: 8 } },
  { name: "胶片", adj: { contrast: 6, highlights: -16, shadows: 12, vibrance: -10, temp: 8, curveSh: 10, grain: 12 } },
  { name: "暖阳", adj: { exposure: 0.08, temp: 22, vibrance: 10, shadows: 6 } },
  { name: "冷夜", adj: { exposure: -0.05, temp: -20, contrast: 6, vibrance: -6, shadows: -8 } },
  { name: "黑白", adj: { saturation: -100, contrast: 12 } },
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
  return {
    exposure: 0,
    contrast: 0,
    highlights: 0,
    shadows: 0,
    whites: 0,
    blacks: 0,
    temp: 0,
    tint: 0,
    vibrance: 0,
    saturation: 0,
    texture: 0,
    clarity: 0,
    dehaze: 0,
    vignette: 0,
    grain: 0,
    curveHi: 0,
    curveLi: 0,
    curveDk: 0,
    curveSh: 0,
    sharpen: 0,
    nr: 0,
    chromaNr: 0,
  };
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
  curvePts: [
    { x: 0.25, y: 0.25 },
    { x: 0.5, y: 0.5 },
    { x: 0.75, y: 0.75 },
  ],
  hist: null,
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

// ---------------------------------------------------------------- 颜色管线（ACR 口径）

/// 单调三次插值（Fritsch–Carlson）：给曲线用，保证不会过冲出现反相。
function peMonotoneCurve(pts) {
  const P = [{ x: 0, y: 0 }].concat(
    pts.map((p) => ({ x: p.x, y: p.y })),
    [{ x: 1, y: 1 }]
  );
  const n = P.length;
  const d = [];
  const m = [];
  for (let i = 0; i < n - 1; i++) d.push((P[i + 1].y - P[i].y) / Math.max(1e-6, P[i + 1].x - P[i].x));
  m[0] = d[0];
  for (let i = 1; i < n - 1; i++) m[i] = d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2;
  m[n - 1] = d[n - 2];
  for (let i = 0; i < n - 1; i++) {
    if (Math.abs(d[i]) < 1e-9) {
      m[i] = 0;
      m[i + 1] = 0;
      continue;
    }
    const a = m[i] / d[i];
    const b = m[i + 1] / d[i];
    const s = a * a + b * b;
    if (s > 9) {
      const t = 3 / Math.sqrt(s);
      m[i] = t * a * d[i];
      m[i + 1] = t * b * d[i];
    }
  }
  return (x) => {
    if (x <= 0) return P[0].y;
    if (x >= 1) return P[n - 1].y;
    let i = 0;
    while (i < n - 2 && x > P[i + 1].x) i++;
    const h = P[i + 1].x - P[i].x;
    const t = (x - P[i].x) / h;
    const t2 = t * t;
    const t3 = t2 * t;
    return (
      (2 * t3 - 3 * t2 + 1) * P[i].y +
      (t3 - 2 * t2 + t) * h * m[i] +
      (-2 * t3 + 3 * t2) * P[i + 1].y +
      (t3 - t2) * h * m[i + 1]
    );
  };
}

/// 曲线查找表：参数曲线（高光/亮调/暗调/阴影）叠加点曲线，输出 256 项。
function peCurveLut(adj) {
  const lut = new Float32Array(256);
  const hs = (adj.curveHi || 0) / 100;
  const li = (adj.curveLi || 0) / 100;
  const dk = (adj.curveDk || 0) / 100;
  const sh = (adj.curveSh || 0) / 100;
  const flat = !hs && !li && !dk && !sh;
  const pts = (peEd && peEd.curvePts) || [];
  const curve = peMonotoneCurve(pts);
  for (let i = 0; i < 256; i++) {
    let x = i / 255;
    if (!flat) {
      const ws = Math.pow(Math.max(0, 1 - x / 0.42), 2);
      const wd = Math.exp(-Math.pow((x - 0.3) / 0.2, 2));
      const wl = Math.exp(-Math.pow((x - 0.7) / 0.2, 2));
      const wh = Math.pow(Math.max(0, (x - 0.62) / 0.38), 2);
      x += sh * 0.22 * ws + dk * 0.22 * wd + li * 0.22 * wl + hs * 0.22 * wh;
      x = x < 0 ? 0 : x > 1 ? 1 : x;
    }
    lut[i] = Math.min(1, Math.max(0, curve(x)));
  }
  return lut;
}

/// 逐像素调色：
/// 白平衡/曝光 → 对比度 → 高光/阴影/白色/黑色 → 自然饱和度/饱和度 → 曲线
/// → 去除薄雾 → 文件 LUT → （细节）→ 颗粒 → 晕影。
/// 点运算主循环 + 一个可选的细节（模糊）通道，预览与导出同一套。
function peApplyPixels(data, w, h, adj, lut, lutStrength) {
  const exposure = Math.pow(2, adj.exposure || 0);
  const con = 1 + (adj.contrast || 0) / 100;
  const sat = 1 + (adj.saturation || 0) / 100;
  const vib = (adj.vibrance || 0) / 100;
  const t = (adj.temp || 0) / 100;
  const tn = (adj.tint || 0) / 100;
  const rWB = (1 + t * 0.24) * (1 + tn * 0.05) * exposure;
  const gWB = (1 - tn * 0.16) * exposure;
  const bWB = (1 - t * 0.24) * (1 + tn * 0.05) * exposure;
  const hi = (adj.highlights || 0) / 100;
  const sh = (adj.shadows || 0) / 100;
  const wh = (adj.whites || 0) / 100;
  const bl = (adj.blacks || 0) / 100;
  const dz = (adj.dehaze || 0) / 100;

  const useLut = !!(lut && lut.size > 1 && lutStrength > 0.5);
  const N = useLut ? lut.size : 0;
  const LD = useLut ? lut.data : null;
  const lm1 = N - 1;
  const N3 = N * 3;
  const N3N = N * N * 3;
  const ls = useLut ? lutStrength / 100 : 0;

  const curve = peCurveLut(adj);
  const doTone = con !== 1 || !!hi || !!sh || !!wh || !!bl;
  const doSat = sat !== 1 || !!vib;
  const doDehaze = !!dz;

  for (let i = 0, n = w * h * 4; i < n; i += 4) {
    let r = data[i] / 255;
    let g = data[i + 1] / 255;
    let b = data[i + 2] / 255;

    // 白平衡 + 曝光
    r *= rWB;
    g *= gWB;
    b *= bWB;

    if (doTone) {
      if (con !== 1) {
        r = (r - 0.5) * con + 0.5;
        g = (g - 0.5) * con + 0.5;
        b = (b - 0.5) * con + 0.5;
      }
      if (hi || sh || wh || bl) {
        const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let dl = 0;
        if (sh) {
          const k = 1 - l * 1.55;
          if (k > 0) dl += sh * 0.42 * k;
        }
        if (hi) {
          const k = (l - 0.32) * 1.47;
          if (k > 0) dl += hi * 0.42 * Math.min(1, k);
        }
        if (wh) {
          const k = (l - 0.58) / 0.42;
          if (k > 0) dl += wh * 0.34 * k * k;
        }
        if (bl) {
          const k = 1 - l / 0.38;
          if (k > 0) dl += bl * 0.34 * k * k;
        }
        r += dl;
        g += dl;
        b += dl;
      }
    }

    if (doSat) {
      if (vib) {
        const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        const mx = Math.max(r, g, b);
        const mn = Math.min(r, g, b);
        const f = 1 + vib * 0.9 * (1 - (mx - mn));
        r = l + (r - l) * f;
        g = l + (g - l) * f;
        b = l + (b - l) * f;
      }
      if (sat !== 1) {
        const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        r = l + (r - l) * sat;
        g = l + (g - l) * sat;
        b = l + (b - l) * sat;
      }
    }

    // 夹到 0..1 再进曲线 / LUT
    r = r < 0 ? 0 : r > 1 ? 1 : r;
    g = g < 0 ? 0 : g > 1 ? 1 : g;
    b = b < 0 ? 0 : b > 1 ? 1 : b;

    r = curve[(r * 255) | 0];
    g = curve[(g * 255) | 0];
    b = curve[(b * 255) | 0];

    if (doDehaze) {
      // 正=去雾（压黑场 + 提对比），负=加雾（抬黑场）
      const k = 1 + 0.42 * dz;
      const lift = -0.035 * dz;
      r = (r + lift) * k;
      g = (g + lift) * k;
      b = (b + lift) * k;
    }

    if (useLut) {
      const fr = Math.min(1, Math.max(0, r)) * lm1;
      const fg = Math.min(1, Math.max(0, g)) * lm1;
      const fb = Math.min(1, Math.max(0, b)) * lm1;
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

    data[i] = r * 255;
    data[i + 1] = g * 255;
    data[i + 2] = b * 255;
  }

  // 细节（空间运算）：预览与导出都走这一条
  peDetailPass(data, w, h, adj);

  // 颗粒 + 晕影（放最后：颗粒要被晕影压暗才自然）
  const grain = (adj.grain || 0) / 100;
  const vig = (adj.vignette || 0) / 100;
  if (grain || vig) {
    const cx = w / 2;
    const cy = h / 2;
    const invD = 1 / Math.hypot(cx, cy);
    const seed = (w * 73856093) ^ (h * 19349663);
    for (let y = 0, i = 0; y < h; y++) {
      const dy = (y - cy) * invD;
      for (let x = 0; x < w; x++, i += 4) {
        let r = data[i];
        let g = data[i + 1];
        let b = data[i + 2];
        if (grain) {
          // 位置哈希：同一张图每次导出颗粒位置一致，不会闪
          let hsh = (x * 374761393 + y * 668265263 + seed) | 0;
          hsh = Math.imul(hsh ^ (hsh >>> 13), 1274126177);
          const nz = (((hsh ^ (hsh >>> 16)) >>> 0) % 1024) / 1023 - 0.5;
          const amt = nz * grain * 42;
          r += amt;
          g += amt;
          b += amt;
        }
        if (vig) {
          const dx = (x - cx) * invD;
          const d2 = dx * dx + dy * dy;
          const k = 1 - vig * 0.72 * Math.max(0, d2 - 0.18) / 0.82;
          r *= k;
          g *= k;
          b *= k;
        }
        data[i] = r;
        data[i + 1] = g;
        data[i + 2] = b;
      }
    }
  }
}

/// 可分离盒式模糊（滑动窗口，O(n)）。给锐化/纹理/清晰度/降噪用。
function peBoxBlur(src, w, h, r) {
  const tmp = new Float32Array(w * h);
  const out = new Float32Array(w * h);
  const div = 2 * r + 1;
  for (let y = 0; y < h; y++) {
    const off = y * w;
    let sum = 0;
    for (let i = -r; i <= r; i++) sum += src[off + Math.min(w - 1, Math.max(0, i))];
    for (let x = 0; x < w; x++) {
      tmp[off + x] = sum / div;
      sum += src[off + Math.min(w - 1, x + r + 1)] - src[off + Math.max(0, x - r)];
    }
  }
  for (let x = 0; x < w; x++) {
    let sum = 0;
    for (let i = -r; i <= r; i++) sum += tmp[Math.min(h - 1, Math.max(0, i)) * w + x];
    for (let y = 0; y < h; y++) {
      out[y * w + x] = sum / div;
      sum += tmp[Math.min(h - 1, y + r + 1) * w + x] - tmp[Math.max(0, y - r) * w + x];
    }
  }
  return out;
}

/// 细节通道：锐化 / 纹理 / 清晰度 / 减少杂色（亮度）+ 杂色深度减低（色度）。
/// 半径随分辨率缩放，保证 1K 预览与 4K 导出的观感一致。
function peDetailPass(data, w, h, adj) {
  const tex = (adj.texture || 0) / 100;
  const cla = (adj.clarity || 0) / 100;
  const shp = (adj.sharpen || 0) / 100;
  const nr = (adj.nr || 0) / 100;
  const cnr = (adj.chromaNr || 0) / 100;
  if (tex <= 0 && cla <= 0 && shp <= 0 && nr <= 0 && cnr <= 0) return;

  const n = w * h;
  const Y = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    const p = i * 4;
    Y[i] = 0.2126 * data[p] + 0.7152 * data[p + 1] + 0.0722 * data[p + 2];
  }
  const s = Math.max(1, Math.min(w, h) / 900);
  const r1 = Math.max(1, Math.round(s));
  const r3 = Math.max(1, Math.round(2.6 * s));
  const rc = Math.max(4, Math.round(16 * s));

  const b1 = shp || nr ? peBoxBlur(Y, w, h, r1) : null;
  const b3 = tex > 0 ? peBoxBlur(Y, w, h, r3) : null;
  const bc = cla > 0 ? peBoxBlur(Y, w, h, rc) : null;

  for (let i = 0; i < n; i++) {
    const y0 = Y[i];
    let y = y0;
    if (nr && b1) y = y0 + (b1[i] - y0) * nr * 0.85;
    if (shp && b1) y += (y0 - b1[i]) * shp * 1.25;
    if (tex && b3) y += (y0 - b3[i]) * tex * 1.0;
    if (cla && bc) y += (y0 - bc[i]) * cla * 0.9;
    const k = y / Math.max(1e-4, y0);
    const p = i * 4;
    data[p] = data[p] * k;
    data[p + 1] = data[p + 1] * k;
    data[p + 2] = data[p + 2] * k;
  }

  if (cnr > 0) {
    const cb = new Float32Array(n);
    const cr = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const p = i * 4;
      const y = 0.2126 * data[p] + 0.7152 * data[p + 1] + 0.0722 * data[p + 2];
      cb[i] = data[p + 2] - y;
      cr[i] = data[p] - y;
    }
    const cb2 = peBoxBlur(cb, w, h, 1);
    const cr2 = peBoxBlur(cr, w, h, 1);
    for (let i = 0; i < n; i++) {
      const p = i * 4;
      const y = 0.2126 * data[p] + 0.7152 * data[p + 1] + 0.0722 * data[p + 2];
      const nb = y + cb[i] * (1 - cnr) + cb2[i] * cnr;
      const nr2 = y + cr[i] * (1 - cnr) + cr2[i] * cnr;
      const ng = (y - 0.2126 * nr2 - 0.0722 * nb) / 0.7152;
      data[p] = nr2;
      data[p + 1] = ng;
      data[p + 2] = nb;
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
  peBuildHist();
  peDrawCurve();
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
  peEd.curvePts = [
    { x: 0.25, y: 0.25 },
    { x: 0.5, y: 0.5 },
    { x: 0.75, y: 0.75 },
  ];
  peSyncSliders();
  peSchedule(false);
}

function peResetAll() {
  peBaseFromImage();
  peEd.crop = null;
  peEd.adj = peAdjZero();
  peEd.curvePts = [
    { x: 0.25, y: 0.25 },
    { x: 0.5, y: 0.5 },
    { x: 0.75, y: 0.75 },
  ];
  peEd.lut = null;
  peEd.lutName = "";
  peEd.lutStrength = 100;
  peSetAspect(0);
  peBuildHist();
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
  peDrawCurve();
  document.querySelectorAll("#pePresets button").forEach((b) => b.classList.remove("active"));
}

function peResetSection(sec) {
  for (const k of sec.keys) peEd.adj[k] = 0;
  if (sec.curve) {
    peEd.curvePts = [
      { x: 0.25, y: 0.25 },
      { x: 0.5, y: 0.5 },
      { x: 0.75, y: 0.75 },
    ];
  }
  peSyncSliders();
  peSchedule(false);
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

/// 按原分辨率渲染当前编辑结果（裁切 + 调色 + LUT）。
function peRenderFull() {
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
  return c;
}

function peDataUrl(canvas, fmt) {
  if (fmt === "jpg") return canvas.toDataURL("image/jpeg", 0.94);
  if (fmt === "webp") return canvas.toDataURL("image/webp", 0.94);
  return canvas.toDataURL("image/png");
}

/// 显示「导出中…」并让浏览器先画一帧：逐像素循环会占住主线程。
async function peBeginExport() {
  peEd.saving = true;
  const btn = $("peSave");
  const over = $("peSaveOver");
  if (btn) {
    btn.disabled = true;
    btn.textContent = "导出中…";
  }
  if (over) over.disabled = true;
  const busy = $("peBusy");
  if (busy) {
    busy.textContent = "导出中…";
    busy.hidden = false;
  }
  await new Promise((r) => requestAnimationFrame(() => setTimeout(r, 0)));
}

function peEndExport() {
  peEd.saving = false;
  const btn = $("peSave");
  const over = $("peSaveOver");
  if (btn) {
    btn.disabled = false;
    btn.textContent = "另存为新图";
  }
  if (over) over.disabled = false;
  const busy = $("peBusy");
  if (busy) busy.hidden = true;
}

async function peSave() {
  if (peEd.saving || !peEd.base) return;
  await peBeginExport();
  try {
    const fmt = ($("peFormat") && $("peFormat").value) === "jpg" ? "jpg" : "png";
    const c = peRenderFull();
    const dataUrl = peDataUrl(c, fmt);
    const name = peOutName(fmt);
    const path = await api.call("save_export", { name, dataUrl, subdir: peSubdir() });
    toast(`已另存：${path}`);
    await peAfterSave({ path, filename: name, width: c.width, height: c.height, dataUrl });
  } catch (e) {
    toast(String(e), true);
  } finally {
    peEndExport();
  }
}

/// 覆盖原图：炸图只留一小格时用（原图留着没意义）。
/// 格式跟原文件走——PNG 原图就写 PNG，JPEG 原图就写 JPEG，不搞假扩展名。
async function peSaveOver() {
  if (peEd.saving || !peEd.base) return;
  if (String(peEd.path).startsWith("vault://")) {
    toast("私密图不支持覆盖", true);
    return;
  }
  const ext = ((peEd.filename || "").match(/\.[^.]+$/) || [".png"])[0].toLowerCase();
  const fmt = ext === ".jpg" || ext === ".jpeg" ? "jpg" : ext === ".webp" ? "webp" : "png";
  if (!confirm(`覆盖原图？\n\n${peEd.filename}\n\n原图会被直接替换（PNG 里的提示词参数会丢），不能撤销。`)) return;
  await peBeginExport();
  try {
    const c = peRenderFull();
    const dataUrl = peDataUrl(c, fmt);
    const path = await api.call("save_over_image", { path: peEd.path, dataUrl });
    toast(`已覆盖：${path}`);
    await peAfterSave({
      path,
      filename: peEd.filename,
      width: c.width,
      height: c.height,
      dataUrl,
      over: true,
    });
  } catch (e) {
    toast(String(e), true);
  } finally {
    peEndExport();
  }
}

async function peAfterSave(saved) {
  peClose();
  const lib = typeof $ === "function" ? $("library") : null;
  if (lib && !lib.hidden && typeof loadLibrary === "function") {
    try {
      await loadLibrary();
    } catch {}
  }
  if (saved.over) {
    // 覆盖：同一个 path 换掉缩略图与尺寸，别让界面显示旧图
    if (typeof libUrlCache !== "undefined" && libUrlCache) libUrlCache.set(saved.path, saved.dataUrl);
    const item = (typeof state !== "undefined" && state.results || []).find((r) => r.path === saved.path);
    if (item) {
      item.width = saved.width;
      item.height = saved.height;
      item.dataUrl = saved.dataUrl;
      item.missing = false;
    }
    if (
      typeof lightboxItem !== "undefined" &&
      lightboxItem &&
      lightboxItem.path === saved.path
    ) {
      lightboxItem.dataUrl = saved.dataUrl;
      if ($("lightboxImg")) $("lightboxImg").src = saved.dataUrl;
    }
    if (typeof renderResults === "function") renderResults();
    return;
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
}

// ---------------------------------------------------------------- 开关

function peSetTab(tab) {
  peEd.tab = tab;
  document.querySelectorAll("#peTabs .tab").forEach((b) => b.classList.toggle("active", b.dataset.tab === tab));
  document.querySelectorAll(".pe-pane").forEach((p) => (p.hidden = p.dataset.pane !== tab));
  peSchedule(false);
}

async function peOpen(item, startTab) {
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
  peEd.curvePts = [
    { x: 0.25, y: 0.25 },
    { x: 0.5, y: 0.5 },
    { x: 0.75, y: 0.75 },
  ];
  peEd.hist = null;
  peEd.lut = null;
  peEd.lutName = "";
  peEd.lutStrength = 100;
  peEd.saving = false;
  peEd.tab = startTab || "light";

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
  peSetTab(peEd.tab);

  try {
    const url = item.dataUrl || (await api.call("read_output_image", { path: item.path }));
    const img = await peLoadImage(url);
    peEd.img = img;
    peBaseFromImage();
    peBuildHist();
    peDrawCurve();
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

function peSliderRow(s) {
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
    if (s.key.startsWith("curve")) peDrawCurve();
    peSchedule(true);
    document.querySelectorAll("#pePresets button").forEach((b) => b.classList.remove("active"));
  };
  inp.onchange = () => peSchedule(false);
  inp.ondblclick = () => {
    peEd.adj[s.key] = 0;
    inp.value = 0;
    peSyncSliderLabels();
    if (s.key.startsWith("curve")) peDrawCurve();
    peSchedule(false);
  };
  const val = document.createElement("span");
  val.className = "pe-val mono";
  row.append(lab, inp, val);
  return row;
}

/// 白平衡一行：ACR 里是「原照设置」下拉；这里只有原照设置，给一个复位。
function peWbRow() {
  const row = document.createElement("div");
  row.className = "pe-row pe-wb";
  const lab = document.createElement("label");
  lab.textContent = "白平衡";
  const val = document.createElement("span");
  val.className = "pe-wb-val";
  val.textContent = "原照设置";
  const btn = document.createElement("button");
  btn.className = "pe-mini";
  btn.title = "复位白平衡";
  btn.textContent = "↺";
  btn.onclick = () => {
    peEd.adj.temp = 0;
    peEd.adj.tint = 0;
    peSyncSliders();
    peSchedule(false);
  };
  row.append(lab, val, btn);
  return row;
}

/// 曲线面板：直方图 + 对角线 + 3 个可拖控制点（ACR 那种）。
function peCurveBox() {
  const box = document.createElement("div");
  box.className = "pe-curve";
  const cv = document.createElement("canvas");
  cv.id = "peCurveCanvas";
  cv.width = 272;
  cv.height = 150;
  box.appendChild(cv);

  let drag = -1;
  const local = (e) => {
    const r = cv.getBoundingClientRect();
    return { x: (e.clientX - r.left) / r.width, y: 1 - (e.clientY - r.top) / r.height };
  };
  cv.addEventListener("pointerdown", (e) => {
    const p = local(e);
    let best = -1;
    let bd = 0.09;
    peEd.curvePts.forEach((pt, i) => {
      const d = Math.abs(pt.x - p.x);
      if (d < bd) {
        bd = d;
        best = i;
      }
    });
    if (best < 0) return;
    drag = best;
    cv.classList.add("drag");
    try {
      cv.setPointerCapture(e.pointerId);
    } catch {}
  });
  cv.addEventListener("pointermove", (e) => {
    if (drag < 0) return;
    const p = local(e);
    peEd.curvePts[drag].y = Math.max(0, Math.min(1, p.y));
    peDrawCurve();
    peSchedule(true);
  });
  const end = () => {
    if (drag < 0) return;
    drag = -1;
    cv.classList.remove("drag");
    peSchedule(false);
  };
  cv.addEventListener("pointerup", end);
  cv.addEventListener("pointercancel", end);
  return box;
}

function peDrawCurve() {
  const cv = $("peCurveCanvas");
  if (!cv) return;
  const ctx = cv.getContext("2d");
  const W = cv.width;
  const H = cv.height;
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = "#0b0e14";
  ctx.fillRect(0, 0, W, H);

  ctx.strokeStyle = "#222a38";
  ctx.lineWidth = 1;
  for (let i = 1; i < 4; i++) {
    const x = (W * i) / 4;
    const y = (H * i) / 4;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, H);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(W, y);
    ctx.stroke();
  }

  const hist = peEd.hist;
  if (hist) {
    let max = 0;
    for (let i = 0; i < 256; i++) if (hist[i] > max) max = hist[i];
    if (max > 0) {
      ctx.fillStyle = "rgba(140,160,200,.22)";
      const bw = W / 256;
      for (let i = 0; i < 256; i++) {
        const h = (hist[i] / max) * H;
        ctx.fillRect(i * bw, H - h, Math.max(1, bw), h);
      }
    }
  }

  const lut = peCurveLut(peEd.adj);
  ctx.strokeStyle = "#c8d2ea";
  ctx.lineWidth = 1.6;
  ctx.beginPath();
  for (let i = 0; i < 256; i++) {
    const x = (i / 255) * W;
    const y = H - lut[i] * H;
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }
  ctx.stroke();

  for (const pt of peEd.curvePts) {
    ctx.beginPath();
    ctx.arc(pt.x * W, H - pt.y * H, 4.5, 0, Math.PI * 2);
    ctx.fillStyle = "#5d7bf0";
    ctx.fill();
    ctx.strokeStyle = "#dfe6f3";
    ctx.lineWidth = 1.4;
    ctx.stroke();
  }
}

/// 曲线背景的直方图：把原图缩到 256 宽统计亮度，够看形状。
function peBuildHist() {
  if (!peEd.base) return;
  const h = Math.max(1, Math.min(256, Math.round((peEd.base.height / peEd.base.width) * 256)));
  const c = document.createElement("canvas");
  c.width = 256;
  c.height = h;
  const ctx = c.getContext("2d", { willReadFrequently: true });
  ctx.drawImage(peEd.base, 0, 0, 256, h);
  const d = ctx.getImageData(0, 0, 256, h).data;
  const hist = new Float32Array(256);
  for (let i = 0; i < d.length; i += 4) {
    const l = (0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2]) | 0;
    hist[l < 0 ? 0 : l > 255 ? 255 : l]++;
  }
  peEd.hist = hist;
}

function peBuildSliders() {
  const box = $("peSliders");
  if (!box || box.childElementCount) return;
  for (const sec of PE_SECTIONS) {
    const wrap = document.createElement("div");
    wrap.className = "pe-sec" + (sec.id === "light" || sec.id === "color" ? " open" : "");
    const head = document.createElement("div");
    head.className = "pe-sec-head";
    const title = document.createElement("span");
    title.className = "pe-sec-title";
    title.textContent = sec.title;
    const reset = document.createElement("button");
    reset.className = "pe-sec-reset";
    reset.title = "复位这一组";
    reset.textContent = "↺";
    reset.onclick = (e) => {
      e.stopPropagation();
      peResetSection(sec);
    };
    const chev = document.createElement("span");
    chev.className = "pe-sec-chev";
    chev.textContent = "▾";
    head.append(title, reset, chev);
    head.onclick = () => wrap.classList.toggle("open");
    const body = document.createElement("div");
    body.className = "pe-sec-body";
    if (sec.wb) body.appendChild(peWbRow());
    if (sec.curve) body.appendChild(peCurveBox());
    for (const k of sec.keys) {
      const spec = PE_SLIDERS.find((x) => x.key === k);
      if (spec) body.appendChild(peSliderRow(spec));
    }
    wrap.append(head, body);
    box.appendChild(wrap);
  }
  peDrawCurve();
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
  bind("peSaveOver", peSaveOver);

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
