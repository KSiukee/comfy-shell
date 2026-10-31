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
  if ($("inpaintUseRef").checked && state.refs.length) {
    try {
      refImageDataUrl = await api.call("read_input_image", { name: state.refs[0] });
    } catch {}
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
        seed: Number($("seed").value),
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
