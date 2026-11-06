// 手动画头蒙版：自动蒙版在侧脸/回头时盖不住后脑勺（会出两个头），让人自己框一下。
//
// 交互故意做得很笨：在图上拖一个矩形框住"要换掉的那颗头"，确定就用这块当蒙版。
// 不做画笔/撤销——头就是个方块，拖两下就好了。

const shotMaskState = { shot: null, card: null, img: null, rect: null, drag: null };

function shotMaskCanvas() {
  return $("shotMaskCanvas");
}

async function openShotMask(s) {
  const src = (s.images || []).slice(-1)[0];
  if (!src) return toast("这一格还没有图", true);
  let card = null;
  try {
    card = await shotSwapCard();
  } catch {
    return toast("先建一个带身份底板的角色卡", true);
  }
  const dataUrl = await api.call("image_data_url", { path: src });
  const img = await qsImage(dataUrl);
  shotMaskState.shot = s;
  shotMaskState.card = card;
  shotMaskState.img = img;
  shotMaskState.rect = null;
  shotMaskState.drag = null;

  const c = shotMaskCanvas();
  c.width = img.naturalWidth;
  c.height = img.naturalHeight;
  c.style.maxWidth = "100%";
  $("shotMaskWrap").style.aspectRatio = `${img.naturalWidth} / ${img.naturalHeight}`;
  $("shotMaskImg").src = dataUrl;
  shotMaskDraw();
  $("shotMaskState").textContent = "在图上拖一个框，把要换掉的那颗头（含后脑勺的头发）框进去";
  $("shotMaskModal").hidden = false;
}

function shotMaskDraw() {
  const c = shotMaskCanvas();
  const g = c.getContext("2d");
  g.clearRect(0, 0, c.width, c.height);
  const r = shotMaskState.drag || shotMaskState.rect;
  if (!r) return;
  g.fillStyle = "rgba(255, 64, 96, 0.28)";
  g.strokeStyle = "#ff4060";
  g.lineWidth = Math.max(2, c.width / 400);
  g.fillRect(r.x, r.y, r.w, r.h);
  g.strokeRect(r.x, r.y, r.w, r.h);
}

function shotMaskPos(e) {
  const c = shotMaskCanvas();
  const box = c.getBoundingClientRect();
  const k = c.width / box.width;
  return { x: Math.max(0, Math.min(c.width, (e.clientX - box.left) * k)), y: Math.max(0, Math.min(c.height, (e.clientY - box.top) * k)) };
}

$("shotMaskCanvas").addEventListener("pointerdown", (e) => {
  e.preventDefault();
  const p = shotMaskPos(e);
  shotMaskState.drag = { x: p.x, y: p.y, w: 0, h: 0, x0: p.x, y0: p.y };
  shotMaskCanvas().setPointerCapture(e.pointerId);
  shotMaskDraw();
});

$("shotMaskCanvas").addEventListener("pointermove", (e) => {
  const d = shotMaskState.drag;
  if (!d) return;
  const p = shotMaskPos(e);
  d.x = Math.min(d.x0, p.x);
  d.y = Math.min(d.y0, p.y);
  d.w = Math.abs(p.x - d.x0);
  d.h = Math.abs(p.y - d.y0);
  shotMaskDraw();
});

$("shotMaskCanvas").addEventListener("pointerup", () => {
  const d = shotMaskState.drag;
  shotMaskState.drag = null;
  if (d && d.w > 12 && d.h > 12) {
    shotMaskState.rect = { x: d.x, y: d.y, w: d.w, h: d.h };
    $("shotMaskState").textContent = `已框：${Math.round(d.w)}×${Math.round(d.h)}（不满意就再拖一个）`;
    shotMaskDraw();
  }
});

function shotMaskBuild() {
  const r = shotMaskState.rect;
  const img = shotMaskState.img;
  if (!r || !img) return "";
  const c = document.createElement("canvas");
  c.width = img.naturalWidth;
  c.height = img.naturalHeight;
  const g = c.getContext("2d");
  g.fillStyle = "#000000";
  g.fillRect(0, 0, c.width, c.height);
  g.fillStyle = "#ffffff";
  const rad = Math.min(r.w, r.h) * 0.18;
  g.beginPath();
  if (g.roundRect) g.roundRect(r.x, r.y, r.w, r.h, rad);
  else g.rect(r.x, r.y, r.w, r.h);
  g.fill();
  return c.toDataURL("image/png");
}

$("shotMaskCancel").onclick = () => {
  $("shotMaskModal").hidden = true;
  shotMaskState.rect = null;
};

$("shotMaskGo").addEventListener("click", async () => {
  const btn = $("shotMaskGo");
  if (!shotMaskState.rect) return toast("先在图上拖一个框", true);
  const mask = shotMaskBuild();
  if (!mask) return toast("蒙版没生成", true);
  btn.disabled = true;
  btn.textContent = "换头中…";
  try {
    await shotSwapHeadKeepOne(shotMaskState.shot, shotMaskState.card, mask);
    $("shotMaskModal").hidden = true;
    toast("已按手画蒙版换头");
    await shotsRefresh();
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "用这块换头";
  }
});
