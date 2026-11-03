// 蒙版 / 局部重绘（从 app.js 拆出；依赖 app.js 的全局）

// ---------------------------------------------------------------- mask painting / inpaint

const maskState = { active: false, strokes: [], brush: 48, erase: false, drawing: false, w: 0, h: 0 };

function maskCanvas() {
  return $("maskCanvas");
}

function positionMaskCanvas() {
  const img = $("viewerImg");
  const stage = $("viewerStage");
  const c = maskCanvas();
  const ir = img.getBoundingClientRect();
  const sr = stage.getBoundingClientRect();
  c.style.left = `${ir.left - sr.left}px`;
  c.style.top = `${ir.top - sr.top}px`;
  c.style.width = `${ir.width}px`;
  c.style.height = `${ir.height}px`;
}

function enterMaskMode() {
  const it = viewer.list[viewer.idx];
  if (!it) return;
  const img = $("viewerImg");
  if (!img.naturalWidth) return;
  viewer.zoom = 100;
  viewer.pan = { x: 0, y: 0 };
  applyViewer();
  maskState.active = true;
  maskState.strokes = [];
  maskState.erase = false;
  $("maskErase").classList.remove("primary");
  const c = maskCanvas();
  maskState.w = img.naturalWidth;
  maskState.h = img.naturalHeight;
  c.width = maskState.w;
  c.height = maskState.h;
  c.hidden = false;
  $("maskBar").hidden = false;
  positionMaskCanvas();
  redrawMask();
}

function exitMaskMode() {
  maskState.active = false;
  maskState.drawing = false;
  const c = maskCanvas();
  c.hidden = true;
  $("maskBar").hidden = true;
  $("inpaintModal").hidden = true;
}

function drawStroke(ctx, stroke, color = "rgba(255,70,70,0.5)") {
  if (stroke.ellipse) {
    // 「自动圈脸」用的一次性椭圆（不可擦、不可拖，撤销就没了）
    const e = stroke.ellipse;
    ctx.globalCompositeOperation = "source-over";
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.ellipse(e.cx, e.cy, e.rx, e.ry, 0, 0, Math.PI * 2);
    ctx.fill();
    return;
  }
  const pts = stroke.points;
  if (!pts.length) return;
  ctx.globalCompositeOperation = stroke.erase ? "destination-out" : "source-over";
  ctx.fillStyle = color;
  ctx.strokeStyle = color;
  ctx.lineWidth = stroke.brush;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  if (pts.length === 1) {
    ctx.beginPath();
    ctx.arc(pts[0].x, pts[0].y, stroke.brush / 2, 0, Math.PI * 2);
    ctx.fill();
    return;
  }
  ctx.beginPath();
  ctx.moveTo(pts[0].x, pts[0].y);
  for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y);
  ctx.stroke();
}

// On screen the strokes are translucent red so the photo stays visible.
function redrawMask() {
  const c = maskCanvas();
  const ctx = c.getContext("2d");
  ctx.globalCompositeOperation = "source-over";
  ctx.clearRect(0, 0, c.width, c.height);
  for (const stroke of maskState.strokes) drawStroke(ctx, stroke);
}

// The exported mask must be opaque black with solid white paint.
function exportMaskDataUrl() {
  const c = document.createElement("canvas");
  c.width = maskState.w;
  c.height = maskState.h;
  const ctx = c.getContext("2d");
  ctx.clearRect(0, 0, c.width, c.height);
  for (const stroke of maskState.strokes) drawStroke(ctx, stroke, "#ffffff");
  ctx.globalCompositeOperation = "destination-over";
  ctx.fillStyle = "#000000";
  ctx.fillRect(0, 0, c.width, c.height);
  return c.toDataURL("image/png");
}

// ---------------------------------------------------------------- 自动圈脸（只盖脸的椭圆）
//
// 为什么要有这个：局部重绘的效果八成取决于蒙版（docs/57 那次的教训是蒙版贪大，
// 把头发和发簪一起画了）。这里用本机视觉模型找一个脸框，再按框画椭圆——
// 比手涂稳定，也比「按比例猜」靠谱。模型会晚一点点（几秒），失败就让人自己涂。
async function autoFaceMask() {
  const it = viewer.list[viewer.idx];
  if (!it || !maskState.active) return;
  const btn = $("maskFace");
  const old = btn.textContent;
  btn.disabled = true;
  btn.textContent = "找脸中…";
  try {
    const dataUrl = await api.call("image_data_url", { path: it.path });
    const name = await api.call("save_ref_image", { name: "face_detect_src.png", dataUrl });
    const box = await api.call("face_detect", { image: name });
    const [x, y, w, h] = box.bbox;
    const W = maskState.w;
    const H = maskState.h;
    // 模型给的是「整个头」的框（含头发），要往里收成脸：
    // 水平取中间 60%，垂直从 42% 到 100%（发际线以下到下巴）——不这么收会把发簪一起重画。
    const cx = (x + w / 2) * W;
    const top = (y + h * 0.42) * H;
    const bottom = (y + h) * H;
    const rx = w * 0.30 * W;
    const ry = ((bottom - top) / 2) * 1.05;
    const cy = (top + bottom) / 2;
    maskState.strokes.push({ ellipse: { cx, cy, rx, ry } });
    redrawMask();
    toast("已圈脸——大小不合适就撤销后手涂");
  } catch (e) {
    toast(`没圈上：${e}`, true);
  } finally {
    btn.disabled = false;
    btn.textContent = old;
  }
}

function maskPoint(e) {
  const c = maskCanvas();
  const r = c.getBoundingClientRect();
  return {
    x: ((e.clientX - r.left) / Math.max(1, r.width)) * c.width,
    y: ((e.clientY - r.top) / Math.max(1, r.height)) * c.height,
  };
}

function hasMask() {
  return maskState.strokes.some((st) => !st.erase);
}

async function startInpaint() {
  const it = viewer.list[viewer.idx];
  if (!it) return;
  if (!hasMask()) {
    toast("先在图上涂一块要重绘的区域", true);
    return;
  }
  // mask must be fully black apart from the painted region
  const p = profile();
  const target = p && p.supportsEdit ? p : state.profiles.find((x) => x.id === "qwen21");
  if (!target) {
    toast("没有支持局部重绘的模型", true);
    return;
  }
  $("inpaintInfo").textContent = `${target.label} · ${maskState.w}×${maskState.h}`;
  $("inpaintPrompt").value = $("prompt").value.trim();
  $("inpaintSteps").value = target.steps;
  $("inpaintModal").hidden = false;
  $("inpaintPrompt").focus();
}

// ---------------------------------------------------------------- 角色底板当参考（自动裁脸）
//
// 为什么要在前端裁一刀：整身底板里脸只有 ~100px，当参考时身份信号太弱，
// 重绘出来的脸会「像但不真像」；裁成 512 的大头再喂，效果明显更稳（实测见 docs/57）。
// 裁剪比例按 3:4 全身底板（人基本居中）估的：水平 32%-70%、垂直 6%-30%。
let inpaintRefName = "";

async function inpaintPlateFaceDataUrl(platePath) {
  const dataUrl = await api.call("image_data_url", { path: platePath });
  const img = new Image();
  await new Promise((res, rej) => {
    img.onload = res;
    img.onerror = rej;
    img.src = dataUrl;
  });
  const w = img.naturalWidth;
  const h = img.naturalHeight;
  const box = { x: 0.32, y: 0.06, w: 0.38, h: 0.24 };
  const c = document.createElement("canvas");
  c.width = 512;
  c.height = 512;
  c.getContext("2d").drawImage(
    img,
    Math.round(w * box.x),
    Math.round(h * box.y),
    Math.round(w * box.w),
    Math.round(h * box.h),
    0,
    0,
    512,
    512,
  );
  return c.toDataURL("image/png");
}

async function pickCharacterPlate() {
  try {
    const list = await api.call("character_list", { project: charProjectId() || null });
    if (!list.length) return null;
    const cur = window.charCurrent && charCurrent && charCurrent.id ? charCurrent : null;
    const card = (cur && list.find((c) => c.id === cur.id)) || list[0];
    return card.identityPlate ? card : null;
  } catch {
    return null;
  }
}

async function inpaintUsePlate() {
  const card = await pickCharacterPlate();
  if (!card) {
    toast("还没有带底板的角色卡：先去「角色卡」建一个并点「出底板」", true);
    return;
  }
  const btn = $("inpaintUsePlate");
  btn.disabled = true;
  const old = btn.textContent;
  btn.textContent = "裁脸中…";
  try {
    const dataUrl = await inpaintPlateFaceDataUrl(card.identityPlate);
    inpaintRefName = await api.call("save_ref_image", { name: "plate_face_ref.png", dataUrl });
    $("inpaintRefState").textContent = `参考：${card.name} 的底板（已裁脸）`;
    $("inpaintUseRef").checked = true;
    toast(`已用「${card.name}」的底板裁出脸当参考`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = old;
  }
}

async function runInpaint() {
  const it = viewer.list[viewer.idx];
  const prompt = $("inpaintPrompt").value.trim();
  if (!it || !prompt) {
    toast("写一下这块要画成什么", true);
    return;
  }
  const p = profile();
  const target = p && p.supportsEdit ? p : state.profiles.find((x) => x.id === "qwen21");
  const maskDataUrl = exportMaskDataUrl();
  let refImageDataUrl = null;
  if ($("inpaintUseRef").checked) {
    // 优先用「自动裁脸」那张（身份信号强得多），没有再退回当前参考图
    if (inpaintRefName) {
      try {
        refImageDataUrl = await api.call("read_input_image", { name: inpaintRefName });
      } catch {}
    }
    if (!refImageDataUrl && state.refs.length) {
      try {
        refImageDataUrl = await api.call("read_input_image", { name: state.refs[0] });
      } catch {}
    }
  }
  const btn = $("inpaintGo");
  btn.disabled = true;
  btn.textContent = "重绘中…";
  try {
    const res = await api.call("inpaint_image", {
      req: {
        profile: target.id,
        prompt,
        negative: $("negative").value,
        source: it.path,
        maskDataUrl,
        steps: Number($("inpaintSteps").value),
        cfg: target.cfg,
        seed: seedArg(),
        denoise: Number($("inpaintDenoise").value),
        feather: Number($("inpaintFeather").value),
        refImageDataUrl,
      },
    });
    toast(`局部重绘完成：${fmtTime(res.seconds)}`);
    exitMaskMode();
    await loadLibrary();
    const list = sortedLibrary();
    const idx = list.findIndex((x) => x.path === res.images[0].path);
    if (idx >= 0) {
      viewer.list = list;
      viewer.idx = idx;
      await showViewerItem();
    }
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "开始";
  }
}

$("vwInpaint").onclick = () => {
  if (maskState.active) exitMaskMode();
  else enterMaskMode();
};
$("maskBrush").oninput = () => (maskState.brush = Number($("maskBrush").value));
$("maskErase").onclick = () => {
  maskState.erase = !maskState.erase;
  $("maskErase").classList.toggle("primary", maskState.erase);
};
$("maskFace").onclick = () => autoFaceMask();
$("maskUndo").onclick = () => {
  maskState.strokes.pop();
  redrawMask();
};
$("maskClear").onclick = () => {
  maskState.strokes = [];
  redrawMask();
};
$("maskCancel").onclick = exitMaskMode;
$("maskStart").onclick = startInpaint;
$("inpaintUsePlate").onclick = () => inpaintUsePlate();
$("inpaintCancel").onclick = () => ($("inpaintModal").hidden = true);
$("inpaintGo").onclick = runInpaint;

(() => {
  const c = maskCanvas();
  c.addEventListener("pointerdown", (e) => {
    if (!maskState.active) return;
    e.preventDefault();
    try { c.setPointerCapture(e.pointerId); } catch {}
    maskState.drawing = true;
    const stroke = { points: [maskPoint(e)], brush: maskState.brush, erase: maskState.erase };
    maskState.strokes.push(stroke);
    drawStroke(c.getContext("2d"), stroke);
  });
  c.addEventListener("pointermove", (e) => {
    if (!maskState.drawing) return;
    const stroke = maskState.strokes[maskState.strokes.length - 1];
    if (!stroke) return;
    const pt = maskPoint(e);
    const last = stroke.points[stroke.points.length - 1];
    stroke.points.push(pt);
    const ctx = c.getContext("2d");
    drawStroke(ctx, { points: [last, pt], brush: stroke.brush, erase: stroke.erase });
  });
  c.addEventListener("pointerup", () => (maskState.drawing = false));
  c.addEventListener("pointercancel", () => (maskState.drawing = false));
})();

window.addEventListener("resize", () => {
  if (maskState.active) {
    positionMaskCanvas();
    redrawMask();
  }
});
