// 参考图预处理：把参考图补白到目标比例再送编辑。
//
// 起因：FLUX.3/Qwen 的编辑模式「输出比例照抄参考图」——参考图是 226×1250 的细条，
// 出来就是 224×1248 的细条，人看着头大身窄长。补白到 3:4 之后比例才正常。
//
// 不改 app.js：包一层 generate()，在提交前把参考图换成补白版本。

const REFPREP_KEY = "cs.refPrep";
const REFPREP_MIN = 512;

function refPrepOn() {
  const v = localStorage.getItem(REFPREP_KEY);
  return v !== "0"; // 默认开
}

function refPrepSet(on) {
  localStorage.setItem(REFPREP_KEY, on ? "1" : "0");
}

function refPrepLoadImg(src) {
  return new Promise((res, rej) => {
    const im = new Image();
    im.onload = () => res(im);
    im.onerror = () => rej(new Error("参考图读不出来"));
    im.src = src;
  });
}

/// 把一张图居中放进 w×h 的白底画布（等比缩放，不裁切）。
/// 先裁掉四周的白边：参考图常带大片留白，直接补白会把人缩得更小。
async function refPrepPad(src, w, h) {
  const im = await refPrepLoadImg(src);
  const c = document.createElement("canvas");
  c.width = Math.max(REFPREP_MIN, Math.round(w));
  c.height = Math.max(REFPREP_MIN, Math.round(h));
  const g = c.getContext("2d");
  g.fillStyle = "#ffffff";
  g.fillRect(0, 0, c.width, c.height);

  // 找非白像素的包围盒（阈值 246，容忍轻微灰底）
  const probe = document.createElement("canvas");
  probe.width = im.width;
  probe.height = im.height;
  const pg = probe.getContext("2d");
  pg.drawImage(im, 0, 0);
  let box = { x: 0, y: 0, w: im.width, h: im.height };
  try {
    const data = pg.getImageData(0, 0, im.width, im.height).data;
    let minX = im.width, minY = im.height, maxX = -1, maxY = -1;
    for (let y = 0; y < im.height; y += 2) {
      for (let x = 0; x < im.width; x += 2) {
        const i = (y * im.width + x) * 4;
        if (data[i] < 246 || data[i + 1] < 246 || data[i + 2] < 246) {
          if (x < minX) minX = x;
          if (y < minY) minY = y;
          if (x > maxX) maxX = x;
          if (y > maxY) maxY = y;
        }
      }
    }
    if (maxX > minX && maxY > minY) {
      const pad = Math.round(Math.max(maxX - minX, maxY - minY) * 0.03);
      minX = Math.max(0, minX - pad);
      minY = Math.max(0, minY - pad);
      maxX = Math.min(im.width - 1, maxX + pad);
      maxY = Math.min(im.height - 1, maxY + pad);
      box = { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
    }
  } catch {
    /* 取不到像素就整张用 */
  }

  const scale = Math.min(c.width / box.w, c.height / box.h);
  const dw = Math.round(box.w * scale);
  const dh = Math.round(box.h * scale);
  const dx = Math.round((c.width - dw) / 2);
  const dy = Math.round((c.height - dh) / 2);
  g.imageSmoothingQuality = "high";
  g.drawImage(im, box.x, box.y, box.w, box.h, dx, dy, dw, dh);
  return c.toDataURL("image/png");
}

/// 图片文件名 → dataURL（走壳里的预览命令，和参考图缩略图同一条路）。
async function refPrepSrcDataUrl(name) {
  if (thumbCache.has(name)) return thumbCache.get(name);
  const url = await api.call("ref_preview", { name });
  thumbCache.set(name, url);
  return url;
}

/// 把 state.refs 里的参考图换成补白版；失败就原样返回（绝不挡住生成）。
async function refPrepRefs(refs, w, h) {
  if (!refPrepOn() || !refs.length) return refs;
  let warned = false;
  const out = [];
  for (const name of refs) {
    try {
      const src = await refPrepSrcDataUrl(name);
      const im = await refPrepLoadImg(src);
      // 太窄的参考图（从拼好的设定板里切出来的那种）补白也救不了比例，直接提示。
      const ratio = im.width / im.height;
      if (!warned && ratio < 0.4) {
        warned = true;
        toast("参考图太窄：建议先出一张 3:4 的全身「身份底板」，再从它换装/换景", true);
      }
      const padded = await refPrepPad(src, w, h);
      const newName = await api.call("save_ref_image", { name: "prep_" + name, dataUrl: padded });
      out.push(newName);
    } catch (e) {
      toast(`参考图补白失败，按原图继续：${e}`, true);
      out.push(name);
    }
  }
  return out;
}

// ---- 包一层 generate：编辑模式下先把参考图补白 ----
const baseGenerateRefPrep = generate;
generate = async function refPrepGenerate() {
  try {
    if (state.mode === "edit" && state.refs.length) {
      const w = parseInt($("width").value, 10) || 1024;
      const h = parseInt($("height").value, 10) || 1365;
      const original = state.refs.slice();
      const prepped = await refPrepRefs(original, w, h);
      state.refs = prepped;
      try {
        return await baseGenerateRefPrep();
      } finally {
        state.refs = original;
        renderRefs();
      }
    }
  } catch (e) {
    toast(`参考图预处理出错，按原流程继续：${e}`, true);
  }
  return baseGenerateRefPrep();
};
