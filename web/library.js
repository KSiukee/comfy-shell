// 图库 / 对比 / 看图 / 翻译（从 app.js 拆出；依赖 app.js 的全局）

// ---------------------------------------------------------------- image library

let libItems = [];
let libRenderLimit = 120;
let libSel = new Set();
let libPrompts = new Map(); // path -> prompt (from generation history)
let libHist = new Map(); // path -> 生成历史条目（详情抽屉在无 PNG 参数时回退用）
let libAltQuery = ""; // english translation of the current query
const libUrlCache = new Map();
let libObserver = null;

function ensureLibObserver() {
  if (libObserver) return;
  libObserver = new IntersectionObserver(
    (entries) => {
      for (const en of entries) {
        if (!en.isIntersecting) continue;
        const img = en.target;
        libObserver.unobserve(img);
        loadLibImage(img);
      }
    },
    { root: $("libGrid"), rootMargin: "400px" }
  );
}

async function loadLibImage(img) {
  const path = img.dataset.path;
  const box = img.parentElement;
  const markLoaded = () => {
    img.classList.add("on");
    const tip = box && box.querySelector(".li-loading");
    if (tip) tip.remove();
  };
  try {
    if (libUrlCache.has(path)) {
      img.src = libUrlCache.get(path);
    } else {
      const url = await api.call("read_output_image", { path });
      libUrlCache.set(path, url);
      img.src = url;
    }
    if (img.complete && img.naturalWidth) markLoaded();
    else img.onload = markLoaded;
  } catch {
    const tip = box && box.querySelector(".li-loading");
    if (tip) tip.textContent = "读取失败";
  }
}

async function openLibrary() {
  $("library").hidden = false;
  libRenderLimit = 120;
  // 老收藏只有 stars.json：第一次开图库时把项目内 _favorites 镜像补齐（幂等）
  if (!openLibrary._favSynced) {
    openLibrary._favSynced = true;
    try {
      await api.call("library_sync_favorites");
    } catch {}
  }
  await loadLibUpscaleModels();
  await loadLibrary();
}

async function loadLibrary() {
  $("libGrid").innerHTML = '<div class="lib-empty">扫描中…</div>';
  try {
    libItems = await api.call("library_scan");
  } catch (e) {
    libItems = [];
    toast(String(e), true);
  }
  try {
    const hist = await api.call("history_list");
    libPrompts = new Map();
    libHist = new Map();
    for (const h of hist) {
      for (const im of h.images || []) {
        libPrompts.set(im.path, h.prompt || "");
        libHist.set(im.path, h);
      }
    }
  } catch {
    libPrompts = new Map();
    libHist = new Map();
  }
  libSel.clear();
  renderLibrary();
  if (typeof refreshLibDetail === "function") refreshLibDetail();
}

function sortedLibrary() {
  const q = $("libSearch").value.trim().toLowerCase();
  const starOnly = $("libStarOnly").checked;
  const alt = libAltQuery.toLowerCase();
  let list = libItems.filter((i) => {
    const prompt = (libPrompts.get(i.path) || "").toLowerCase();
    const hit =
      !q ||
      i.filename.toLowerCase().includes(q) ||
      prompt.includes(q) ||
      (alt && prompt.includes(alt));
    return hit && (!starOnly || i.starred);
  });
  const sort = $("libSort").value;
  if (sort === "old") list = [...list].sort((a, b) => a.mtime - b.mtime);
  else if (sort === "name") list = [...list].sort((a, b) => a.filename.localeCompare(b.filename));
  return list;
}

function renderLibrary() {
  ensureLibObserver();
  const grid = $("libGrid");
  const list = sortedLibrary();
  $("libCount").textContent = `${list.length} / ${libItems.length}`;
  $("libCompare").disabled = libSel.size !== 2;
  $("libDelete").disabled = libSel.size === 0;
  $("libEdit").disabled = libSel.size === 0;
  $("libBatchUp").disabled = libSel.size === 0;
  grid.innerHTML = "";
  if (!list.length) {
    grid.innerHTML = '<div class="lib-empty">没有图片（生成几张再来）</div>';
    return;
  }
  const slice = list.slice(0, libRenderLimit);
  for (const it of slice) {
    const d = document.createElement("div");
    d.className = "lib-item" + (libSel.has(it.path) ? " sel" : "");
    const promptText = libPrompts.get(it.path) || "";
    d.title = promptText || it.filename;
    d.innerHTML =
      `<div class="li-thumb"><span class="li-loading">载入中…</span><img alt="" data-path="${it.path}"></div>` +
      `<button class="li-star ${it.starred ? "" : "off"}" title="收藏">★</button>` +
      `<div class="li-meta"><span class="label" title="${it.filename}">${it.filename}</span>` +
      `<span class="mono">${it.width}×${it.height}</span></div>`;
    const img = d.querySelector("img");
    img.onerror = () => d.remove();
    // selection checkbox sits top-left so a plain click can still open the image
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = libSel.has(it.path);
    box.style.cssText =
      "position:absolute;top:6px;left:6px;z-index:2;width:16px;height:16px;accent-color:#5d7bf0;";
    box.onclick = (e) => {
      e.stopPropagation();
      if (box.checked) libSel.add(it.path);
      else libSel.delete(it.path);
      d.classList.toggle("sel", box.checked);
      $("libCompare").disabled = libSel.size !== 2;
      $("libDelete").disabled = libSel.size === 0;
      $("libEdit").disabled = libSel.size === 0;
      $("libBatchUp").disabled = libSel.size === 0;
        };
    d.appendChild(box);
    d.querySelector(".li-star").onclick = async (e) => {
      e.stopPropagation();
      it.starred = !it.starred;
      await api.call("library_star", { path: it.path, starred: it.starred });
      e.target.className = "li-star" + (it.starred ? "" : " off");
    };
    if (typeof attachLibItemDetail === "function") attachLibItemDetail(d, it);
    img.onclick = () => openViewer(list, list.indexOf(it));    libObserver.observe(img);
    grid.appendChild(d);
  }
  if (list.length > slice.length) {
    const more = document.createElement("div");
    more.className = "lib-empty";
    more.style.gridColumn = "1 / -1";
    more.textContent = `已显示 ${slice.length} / ${list.length}，点这里加载更多`;
    more.onclick = () => {
      libRenderLimit += 200;
      renderLibrary();
    };
    grid.appendChild(more);
  }
}

async function deleteSelected() {
  if (!libSel.size) return;
  if (!confirm(`删除选中的 ${libSel.size} 张？\n会移到各自项目文件夹的 _trash\\ 里，可手动恢复`)) return;
  try {
    const n = await api.call("library_delete", { paths: [...libSel] });
    toast(`已移走 ${n} 张`);
    await loadLibrary();
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- compare

let cmpPaths = [];
let cmpZoom = 100;
let cmpPan = { x: 0, y: 0 };

async function libUrl(path) {
  if (libUrlCache.has(path)) return libUrlCache.get(path);
  const url = await api.call("read_output_image", { path });
  libUrlCache.set(path, url);
  return url;
}

async function openCompare(a, b) {
  cmpPaths = [a, b];
  $("compare").hidden = false;
  $("cmpALabel").textContent = a.filename;
  $("cmpBLabel").textContent = b.filename;
  $("cmpA").src = await libUrl(a.path);
  $("cmpB").src = await libUrl(b.path);
  cmpZoom = 100;
  cmpPan = { x: 0, y: 0 };
  $("cmpZoom").value = 100;
  applyCmp();
}

function applyCmp() {
  const t = `translate(${cmpPan.x}px, ${cmpPan.y}px) scale(${cmpZoom / 100})`;
  $("cmpA").style.transform = t;
  $("cmpB").style.transform = t;
  $("cmpZoomText").textContent = `${cmpZoom}%`;
}

function bindPan(paneId) {
  const pane = $(paneId);
  let dragging = false;
  let sx = 0;
  let sy = 0;
  pane.addEventListener("pointerdown", (e) => {
    dragging = true;
    sx = e.clientX - cmpPan.x;
    sy = e.clientY - cmpPan.y;
    pane.setPointerCapture(e.pointerId);
  });
  pane.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    cmpPan = { x: e.clientX - sx, y: e.clientY - sy };
    applyCmp();
  });
  pane.addEventListener("pointerup", () => (dragging = false));
  pane.addEventListener("pointercancel", () => (dragging = false));
}

// ---------------------------------------------------------------- wiring (v0.3)

// ---------------------------------------------------------------- export comparison

function loadImg(src) {
  return new Promise((res, rej) => {
    const img = new Image();
    img.onload = () => res(img);
    img.onerror = rej;
    img.src = src;
  });
}

async function exportCompare() {
  if (cmpPaths.length !== 2) return;
  try {
    const [ua, ub] = [await libUrl(cmpPaths[0].path), await libUrl(cmpPaths[1].path)];
    const [ia, ib] = [await loadImg(ua), await loadImg(ub)];
    const H = 1024;
    const wa = Math.round((ia.width / ia.height) * H);
    const wb = Math.round((ib.width / ib.height) * H);
    const pad = 12;
    const caption = 34;
    const canvas = document.createElement("canvas");
    canvas.width = wa + wb + pad * 3;
    canvas.height = H + pad * 2 + caption;
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#0d0f14";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(ia, pad, pad, wa, H);
    ctx.drawImage(ib, pad * 2 + wa, pad, wb, H);
    ctx.fillStyle = "#c8cee0";
    ctx.font = "16px 'Segoe UI', 'Microsoft YaHei UI', sans-serif";
    ctx.textBaseline = "middle";
    const y = pad + H + caption / 2;
    ctx.fillText(`${cmpPaths[0].filename}  ${ia.width}×${ia.height}`, pad, y);
    const labelB = `${cmpPaths[1].filename}  ${ib.width}×${ib.height}`;
    ctx.fillText(labelB, pad * 2 + wa, y);

    const warn = typeof poseLicenseWarn === "function" ? poseLicenseWarn() : "";
    if (warn && !confirm(`本项目用过仅本地使用的姿势库：\n${warn}\n\n继续导出？`)) return;
    const dataUrl = canvas.toDataURL("image/png");
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    const path = await api.call("save_export", { name: `compare_${stamp}.png`, dataUrl });
    toast(`已导出：${path}`);
    libUrlCache.set(path, dataUrl);
    libItems = await api.call("library_scan");
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- wiring (v0.4)

// ---------------------------------------------------------------- image viewer

const viewer = { list: [], idx: 0, zoom: 100, pan: { x: 0, y: 0 } };

async function openViewer(list, idx) {
  if (!list || !list.length) return;
  viewer.list = list;
  viewer.idx = Math.max(0, Math.min(idx, list.length - 1));
  $("viewer").hidden = false;
  await loadUpscaleModels();
  await showViewerItem();
}

async function loadUpscaleModels() {
  try {
    const list = await api.call("upscale_models_list");
    const sel = $("vwUpModel");
    const cur = sel.value;
    sel.innerHTML =
      `<option value="__seedvr2__">SeedVR2 3B · 4×（AI 修复，慢但更真）</option>` +
      list.map((m) => `<option value="${m}">${m.replace(/\.(pth|safetensors|pt)$/i, "")}</option>`).join("");
    if (cur && [...sel.options].some((o) => o.value === cur)) sel.value = cur;
    sel.hidden = list.length === 0;
  } catch {}
}

async function showViewerItem() {
  const it = viewer.list[viewer.idx];
  if (!it) return;
  $("vwName").textContent = `${it.filename} · ${it.width}×${it.height}`;
  $("vwPos").textContent = `${viewer.idx + 1} / ${viewer.list.length}`;
  viewer.zoom = 100;
  viewer.pan = { x: 0, y: 0 };
  $("viewerImg").src = await libUrl(it.path);
  applyViewer();
}

function applyViewer() {
  const t = `translate(${viewer.pan.x}px, ${viewer.pan.y}px) scale(${viewer.zoom / 100})`;
  $("viewerImg").style.transform = t;
  $("vwZoom").textContent = `${Math.round(viewer.zoom)}%`;
  const stage = $("viewerStage");
  if (stage) stage.classList.toggle("zoomed", viewer.zoom > 105);
}

function viewerStep(delta) {
  if (!viewer.list.length) return;
  viewer.idx = (viewer.idx + delta + viewer.list.length) % viewer.list.length;
  showViewerItem();
}

function viewerZoom(mult) {
  viewer.zoom = Math.max(10, Math.min(800, viewer.zoom * mult));
  applyViewer();
}

/// 以光标位置为锚点缩放：图上的那个点留在光标下面（100% ⇄ 260% 来回切）。
function viewerZoomAt(clientX, clientY) {
  const stage = $("viewerStage");
  if (!stage) return;
  const r = stage.getBoundingClientRect();
  const z0 = viewer.zoom;
  const z1 = z0 < 140 ? 260 : 100;
  const px = clientX - (r.left + r.width / 2);
  const py = clientY - (r.top + r.height / 2);
  if (z1 === 100) {
    // 回到适应：居中才看得全
    viewer.pan = { x: 0, y: 0 };
  } else {
    const k = z1 / z0;
    viewer.pan = { x: px - (px - viewer.pan.x) * k, y: py - (py - viewer.pan.y) * k };
  }
  viewer.zoom = z1;
  applyViewer();
}

// ---------------------------------------------------------------- 超分队列
// 生成/超分在跑的时候不硬插队：排进队列，等空闲自动跑（生成结束时 app.js 会调 drainUpscale）。

const upQueue = [];
let upBusy = false;

function upscaleQueueLen() {
  return upQueue.length;
}

async function runUpscale(item) {
  if (!item) return;
  upBusy = true;
  const btn = $("vwUpscale");
  if (btn) {
    btn.disabled = true;
    btn.textContent = "超分中…";
  }
  try {
    const model = $("vwUpModel") && $("vwUpModel").value ? $("vwUpModel").value : null;
    const img = await api.call("upscale_image", { path: item.path, model });
    toast(`超分完成：${img.width}×${img.height}`);
    if (typeof loadLibrary === "function" && !$("library").hidden) {
      await loadLibrary();
      if (!$("viewer").hidden && viewer.list.length) {
        const list = sortedLibrary();
        const idx = list.findIndex((x) => x.path === img.path);
        if (idx >= 0) {
          viewer.list = list;
          viewer.idx = idx;
          await showViewerItem();
        }
      }
    }
    // 首页图区也补一条，省得切过去找不到
    if (typeof mergeResults === "function") {
      const filename = String(img.path || "").split(/[\\/]/).pop();
      mergeResults([
        {
          path: img.path,
          filename,
          width: img.width,
          height: img.height,
          label: "超分",
          seconds: 0,
          ts: Date.now() / 1000,
          prompt: "",
        },
      ]);
    }
  } catch (e) {
    toast(`超分失败：${e}`, true);
  } finally {
    upBusy = false;
    if (btn) {
      btn.disabled = false;
      btn.textContent = "超分 4×";
    }
    setTimeout(() => drainUpscale(), 200);
  }
}

/// 空闲才跑；生成中或正在超分就排队。
function queueUpscale(item) {
  if (!item || !item.path) return;
  if (String(item.path).startsWith("vault://")) {
    toast("私密图的超分还没做", true);
    return;
  }
  if (upBusy || (typeof state !== "undefined" && state.running)) {
    upQueue.push({ path: item.path, filename: item.filename });
    toast(`已排队：${item.filename || "当前图"}（前面还有${upBusy ? "超分" : "生成"}任务）`);
    return;
  }
  runUpscale(item);
}

function drainUpscale() {
  if (upBusy || !upQueue.length) return;
  if (typeof state !== "undefined" && state.running) return;
  const next = upQueue.shift();
  runUpscale(next);
}

// 首页灯箱里的「超分 4×」
(() => {
  const b = $("lbUpscale");
  if (!b) return;
  b.onclick = () => {
    const it = typeof lightboxItem !== "undefined" ? lightboxItem : null;
    if (it) queueUpscale(it);
  };
})();

async function viewerUpscale() {
  const it = viewer.list[viewer.idx];
  if (!it) return;
  queueUpscale(it);
}

// ---------------------------------------------------------------- query translation

async function translateSearch() {
  const q = $("libSearch").value.trim();
  if (!q) {
    toast("先输入要搜的词", true);
    return;
  }
  const btn = $("libTranslate");
  btn.disabled = true;
  btn.textContent = "翻译中…";
  try {
    const en = await api.call("translate_text", { text: q });
    libAltQuery = en.trim();
    $("libAltText").textContent = libAltQuery;
    $("libAltChip").hidden = false;
    renderLibrary();
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "中→英";
  }
}

// ---------------------------------------------------------------- wiring (v0.5)
