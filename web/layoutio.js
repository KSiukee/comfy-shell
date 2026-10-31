// 排版 JSON 的两种形态 + 校验（MOSS 官方 references/layout.md）：
//   草稿：width/height（16 的倍数）+ items[].bbox 是像素 xyxy
//   编译后：bbox_coord_space=abs_centralized + bbox_format=yxyx（没有画布字段）
// 编辑器内部统一用 0..1 的 [x, y, w, h]，转换和校验只在边界发生。

function normalizeLayoutDraft(data, fallbackW, fallbackH) {
  const items = data && Array.isArray(data.items) ? data.items : [];
  const compiled = !!data && (data.bbox_coord_space === "abs_centralized" || data.bbox_format === "yxyx");
  const dw = Number(data && data.width) || 0;
  const dh = Number(data && data.height) || 0;
  const w = dw > 0 ? dw : fallbackW || 1024;
  const h = dh > 0 ? dh : fallbackH || 1024;
  const clamp01 = (v) => Math.max(0, Math.min(1, Number(v) || 0));
  const out = [];
  for (const it of items) {
    const b = Array.isArray(it.bbox) ? it.bbox.map(Number) : [0, 0, 0, 0];
    let x, y, bw, bh;
    if (compiled) {
      // yxyx，相对画布中心
      const top = b[0] + h / 2;
      const left = b[1] + w / 2;
      const bottom = b[2] + h / 2;
      const right = b[3] + w / 2;
      x = left / w;
      y = top / h;
      bw = (right - left) / w;
      bh = (bottom - top) / h;
    } else {
      // xyxy，像素
      x = b[0] / w;
      y = b[1] / h;
      bw = (b[2] - b[0]) / w;
      bh = (b[3] - b[1]) / h;
    }
    out.push({
      type: it.label === "text" ? "text" : "object",
      desc: it.caption || "",
      text: it.text || "",
      bbox: [clamp01(x), clamp01(y), clamp01(bw), clamp01(bh)],
    });
  }
  return { items: out, width: w, height: h, compiled };
}

async function layoutCheckJson(data) {
  try {
    const text = typeof data === "string" ? data : JSON.stringify(data);
    const errs = await api.call("layout_check", { text });
    return Array.isArray(errs) ? errs : [];
  } catch (e) {
    return [String(e)];
  }
}
