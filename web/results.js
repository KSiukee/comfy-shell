// 首页图区（结果网格 + 大图翻页）。

// 缩略图并发上限：超分后的图动辄 4000×6000（base64 40MB+），同时读几张会把 WebView 内存打爆
const imgQueue = [];
let imgRunning = 0;

function enqueueImage(img) {
  imgQueue.push(img);
  pumpImages();
}

function pumpImages() {
  while (imgRunning < 2 && imgQueue.length) {
    const img = imgQueue.shift();
    imgRunning++;
    loadImageInto(img).finally(() => {
      imgRunning--;
      pumpImages();
    });
  }
}

// 从 app.js 拆出；app.js 只保留生成与装配。
// v0.95.1：只显示**当前项目**的图——避免切项目时看到别的项目的图。

/** 图片路径 → 所属项目 id（`output/projects/<id>/…`）；根目录的图返回空串（临时）。 */
function normWinPath(p) {
  return String(p || "").replace(/^\\\\\?\\/, "");
}

function resultProjectId(path) {
  const m = /[\\/]projects[\\/]([^\\/]+)[\\/]/.exec(String(path || ""));
  return m ? m[1] : "";
}

function resultScopeWant() {
  const p = window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
  return p ? p.id : "";
}

/** 当前项目下的条目（不改 state.results 本身）。没有项目就什么都不显示。 */
function resultsInScope() {
  const want = resultScopeWant();
  if (!want) return [];
  // 历史里的路径可能带 \\?\ 前缀（Windows canonicalize），统一去掉再比，避免同一张图重复
  const seen = new Set();
  const base = [];
  for (const r of state.results) {
    if (resultProjectId(r.path) !== want) continue;
    const p = normWinPath(r.path);
    if (seen.has(p)) continue;
    seen.add(p);
    base.push(r.path === p ? r : { ...r, path: p });
  }
  const extra = (state.libExtra || []).filter((r) => !seen.has(normWinPath(r.path)));
  return base.concat(extra).sort((a, b) => (b.ts || 0) - (a.ts || 0));
}

/**
 * 图区 = 历史 + 当前项目磁盘上的图。
 * 历史被「清空」或超分这类没进历史的产物，也不会从图区消失（图库本来就是扫磁盘）。
 */
async function refreshLibExtra() {
  const want = resultScopeWant();
  state.gridCleared = false;
  if (!want) {
    state.libExtra = [];
    renderResults();
    return;
  }
  try {
    const items = (await api.call("library_scan")) || [];
    state.libExtra = items
      .filter((i) => resultProjectId(i.path) === want)
      .map((i) => ({
        path: i.path,
        filename: i.filename,
        width: i.width,
        height: i.height,
        ts: Math.floor((i.mtime || 0) / 1e6),
        label: "",
        prompt: "",
        mode: "",
        seconds: 0,
        fromLib: true,
      }));
  } catch {
    state.libExtra = [];
  }
  renderResults();
}

function emptyHint() {
  const p = $("empty") && $("empty").querySelector("p");
  if (!p) return;
  if (state.gridCleared) {
    p.textContent = "图区已清空；磁盘上的图还在，切项目或重开后会重新扫出来。";
    return;
  }
  p.textContent = !resultScopeWant()
    ? "先在左上角项目栏选一个项目（图区只显示当前项目的图）。"
    : "本项目还没有图。左边写好提示词，点「生成」。";
}

function renderResults() {
  const box = $("results");
  box.querySelectorAll(".card").forEach((c) => c.remove());
  const list = resultsInScope().filter((x) => !x.missing);
  if (list.length) state.gridCleared = false;
  $("empty").hidden = list.length > 0;
  emptyHint();
  for (const r of list) {
    const card = document.createElement("div");
    card.className = "card";
    const img = document.createElement("img");
    img.dataset.path = r.path;
    img.__item = r;
    img.alt = "";
    if (r.dataUrl) img.src = r.dataUrl;
    else observer.observe(img);
    img.onclick = () => openLightbox(r);
    card.oncontextmenu = (e) => {
      e.preventDefault();
      showCtxMenu(e.clientX, e.clientY, resultMenu(r));
    };
    const meta = document.createElement("div");
    meta.className = "meta";
    meta.innerHTML =
      `<span class="label" title="${r.filename}">${r.label}</span>` +
      `<span class="time mono">${r.width}×${r.height}${r.seconds ? " · " + fmtTime(r.seconds) : ""}</span>`;
    const star = document.createElement("button");
    star.className = "btn ghost small star" + (state.stars.has(r.path) ? " on" : "");
    star.title = "收藏";
    star.textContent = state.stars.has(r.path) ? "★" : "☆";
    star.onclick = (e) => {
      e.stopPropagation();
      toggleStar(r, star);
    };
    meta.appendChild(star);
    const reveal = document.createElement("button");
    reveal.className = "btn ghost small";
    reveal.title = "在文件夹中显示";
    reveal.textContent = "📁";
    reveal.onclick = () => api.call("reveal_file", { path: r.path });
    meta.appendChild(reveal);
    const del = document.createElement("button");
    del.className = "btn ghost small";
    del.title = "删除（移到回收站）";
    del.textContent = "🗑";
    del.onclick = (e) => {
      e.stopPropagation();
      deleteResults([r]);
    };
    meta.appendChild(del);
    const more = document.createElement("button");
    more.className = "btn ghost small";
    more.title = "更多";
    more.textContent = "⋯";
    more.onclick = (e) => {
      e.stopPropagation();
      const rect = more.getBoundingClientRect();
      showCtxMenu(rect.left, rect.bottom + 4, resultMenu(r));
    };
    meta.appendChild(more);
    card.appendChild(img);
    card.appendChild(meta);
    box.appendChild(card);
  }
}

// ---------------------------------------------------------------- lightbox
// v0.96.1：图区灯箱也支持「点图放大 / 拖动看细节」，点图外关闭（跟图库大图一致）。

let lightboxItem = null;
const lbView = { zoom: 100, pan: { x: 0, y: 0 } };

function applyLbView() {
  const img = $("lightboxImg");
  if (!img) return;
  img.style.transform = `translate(${lbView.pan.x}px, ${lbView.pan.y}px) scale(${lbView.zoom / 100})`;
  img.classList.toggle("zoomed", lbView.zoom > 105);
}

function resetLbView() {
  lbView.zoom = 100;
  lbView.pan = { x: 0, y: 0 };
  applyLbView();
}

/// 以光标为锚点缩放（100% ⇄ 260%）。
function lbZoomAt(clientX, clientY) {
  const img = $("lightboxImg");
  if (!img) return;
  const r = img.getBoundingClientRect();
  const cx = r.left + r.width / 2;
  const cy = r.top + r.height / 2;
  const z0 = lbView.zoom;
  const z1 = z0 < 140 ? 260 : 100;
  if (z1 === 100) {
    lbView.pan = { x: 0, y: 0 };
  } else {
    const k = z1 / z0;
    const px = clientX - cx;
    const py = clientY - cy;
    lbView.pan = { x: px - (px - lbView.pan.x) * k, y: py - (py - lbView.pan.y) * k };
  }
  lbView.zoom = z1;
  applyLbView();
}

async function openLightbox(r) {
  lightboxItem = r;
  $("lightbox").hidden = false;
  resetLbView();
  await showLightbox(r);
}

async function showLightbox(r) {
  lightboxItem = r;
  const list = resultsInScope();
  const idx = list.findIndex((x) => x.path === r.path);
  $("lbPos").textContent = idx >= 0 ? `${idx + 1} / ${list.length}` : "";
  $("lightboxMeta").textContent = `${r.filename} · ${r.width}×${r.height} · ${(r.prompt || "").slice(0, 40)}`;
  resetLbView();
  $("lightboxImg").src = r.dataUrl || "";
  if (!r.dataUrl) {
    try {
      r.dataUrl = await api.call("read_output_image", { path: r.path });
      if (lightboxItem === r) $("lightboxImg").src = r.dataUrl;
    } catch {}
  }
}

async function lightboxStep(delta) {
  const list = resultsInScope();
  const idx = list.findIndex((x) => x.path === lightboxItem?.path);
  if (idx < 0 || list.length < 2) return;
  const next = (idx + delta + list.length) % list.length;
  await showLightbox(list[next]);
}

$("lbPrev").onclick = () => lightboxStep(-1);
$("lbNext").onclick = () => lightboxStep(1);

// 单击图 = 定点放大 / 再点复位；按住拖 = 平移；点图外 = 关闭
(() => {
  const lb = $("lightbox");
  let drag = null;
  lb.addEventListener("pointerdown", (e) => {
    if (lb.hidden || e.button !== 0) return;
    // 底部工具条上的按钮不算「背景点击」，否则点「下一张」会把灯箱关掉
    if (e.target.closest && e.target.closest(".lightbox-bar")) return;
    const img = $("lightboxImg");
    const onImg = e.target === img && img.complete;
    if (onImg) e.preventDefault(); // 别让浏览器把图片当成 HTML5 拖拽（会让拖动只走几像素）
    drag = {
      onImg: !!onImg,
      moved: false,
      sx: e.clientX,
      sy: e.clientY,
      px: e.clientX - lbView.pan.x,
      py: e.clientY - lbView.pan.y,
    };
    if (onImg) {
      try {
        img.setPointerCapture(e.pointerId);
      } catch {}
    }
  });
  lb.addEventListener("pointermove", (e) => {
    if (!drag || !drag.onImg) return;
    if (!drag.moved && Math.abs(e.clientX - drag.sx) + Math.abs(e.clientY - drag.sy) > 5) drag.moved = true;
    if (!drag.moved) return;
    lbView.pan = { x: e.clientX - drag.px, y: e.clientY - drag.py };
    applyLbView();
  });
  const end = (e) => {
    if (!drag) return;
    const d = drag;
    drag = null;
    if (!d.onImg) {
      // 点在图上之外（背景）：关闭
      lb.hidden = true;
      return;
    }
    if (!d.moved && e && e.type === "pointerup") lbZoomAt(e.clientX, e.clientY);
  };
  lb.addEventListener("pointerup", end);
  lb.addEventListener("pointercancel", () => (drag = null));
})();

$("lightbox").addEventListener(
  "wheel",
  (e) => {
    if ($("lightbox").hidden) return;
    e.preventDefault();
    if (Math.abs(e.deltaY) < 3) return;
    lightboxStep(e.deltaY > 0 ? 1 : -1);
  },
  { passive: false }
);

// ---------------------------------------------------------------- 图区范围
// v0.95.1：只有「本项目」一档，不再提供「全部」（防止跨项目泄漏）。
function applyResScope() {
  renderResults();
}

applyResScope();
