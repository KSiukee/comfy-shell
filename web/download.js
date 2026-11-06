// ComfyUI 下载 / 更新：源与镜像测速、多线程分块下载、可取消。
// 从 app.js 拆出来；行为不变。

// ---------------------------------------------------------------- ComfyUI 下载 / 更新
// 官方源 + 国内镜像（前缀拼官方 URL）；先测速再选最快的，下载走多线程分块，可取消。
const DL_MIRRORS = [
  { id: "official", name: "官方 GitHub", prefix: "" },
  { id: "ghproxy", name: "gh-proxy.com", prefix: "https://gh-proxy.com/" },
  { id: "ghfast", name: "ghfast.top", prefix: "https://ghfast.top/" },
  { id: "gitmirror", name: "hub.gitmirror.com", prefix: "https://hub.gitmirror.com/" },
  { id: "llkk", name: "gh.llkk.cc", prefix: "https://gh.llkk.cc/" },
];
const DL_SOURCES = {
  source: "https://github.com/comfyanonymous/ComfyUI/archive/refs/heads/master.zip",
  portable:
    "https://github.com/Comfy-Org/ComfyUI/releases/latest/download/ComfyUI_windows_portable_nvidia.7z",
};
let dlBusy = false;
let dlProbeCache = {};

function dlMode() {
  return $("dlMode") ? $("dlMode").value : "source";
}
function dlCandidates() {
  const base = DL_SOURCES[dlMode()];
  return DL_MIRRORS.map((m) => ({ ...m, url: m.prefix ? m.prefix + base : base }));
}
function dlFmt(n) {
  if (!n) return "0 B";
  const u = ["B", "KB", "MB", "GB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${u[i]}`;
}
function dlFillMirrors() {
  const sel = $("dlMirror");
  if (!sel) return;
  const cands = dlCandidates();
  sel.innerHTML = "";
  let bestId = null;
  let bestSpeed = -1;
  for (const c of cands) {
    const o = document.createElement("option");
    o.value = c.id;
    const s = dlProbeCache[c.url];
    o.textContent = c.name + (s !== undefined && s > 0 ? ` · ${s.toFixed(1)} MB/s` : s === -1 ? " · 不可用" : "");
    sel.appendChild(o);
    if (s !== undefined && s > bestSpeed) {
      bestSpeed = s;
      bestId = c.id;
    }
  }
  if (bestId) sel.value = bestId;
}
async function dlProbeAll() {
  if (dlBusy) return;
  const cands = dlCandidates();
  const st = $("dlState");
  if (st) st.textContent = "测速中…（每个源拉 256KB）";
  try {
    const res = await api.call("dl_probe", { urls: cands.map((c) => c.url) });
    dlProbeCache = {};
    for (const r of res || []) dlProbeCache[r.url] = r.ok ? r.mbps : -1;
    dlFillMirrors();
    const best = (res || []).filter((r) => r.ok).sort((a, b) => b.mbps - a.mbps)[0];
    if (st) {
      st.textContent = best
        ? `最快：${(cands.find((c) => c.url === best.url) || {}).name} · ${best.mbps.toFixed(1)} MB/s · 首包 ${best.ms}ms`
        : "所有源都不可用（检查网络/代理）";
    }
  } catch (e) {
    if (st) st.textContent = "测速失败：" + e;
  }
}
async function dlStart() {
  if (dlBusy) return;
  const mode = dlMode();
  const sel = $("dlMirror");
  const cands = dlCandidates();
  const picked = cands.find((c) => c.id === (sel ? sel.value : "")) || cands[0];
  const dir = $("dlDir").value.trim();
  if (mode === "portable" && !confirm("完整便携版约 2GB，会解压到目标目录（必须是空目录）。继续？")) return;
  dlBusy = true;
  $("dlStart").disabled = true;
  $("dlStop").disabled = false;
  const bar = $("dlBar");
  const fill = $("dlFill");
  if (bar) bar.hidden = false;
  if (fill) {
    fill.classList.add("pulse");
    fill.style.width = "0%";
  }
  if ($("dlState")) $("dlState").textContent = "准备下载…";
  try {
    const p = await api.call("comfy_install", { mode, url: picked.url, dir: dir || null, threads: 6 });
    if ($("dlState")) $("dlState").textContent = "完成：" + p;
    toast(`ComfyUI 已${mode === "portable" ? "安装" : "更新"}：${p}`);
    if ($("setRoot")) $("setRoot").value = p;
    state.settings = await api.call("settings_get");
    refreshSettingsInfo();
  } catch (e) {
    if ($("dlState")) $("dlState").textContent = "失败：" + e;
    toast(String(e), true);
  } finally {
    dlBusy = false;
    $("dlStart").disabled = false;
    $("dlStop").disabled = true;
    if (fill) fill.classList.remove("pulse");
    if (bar) bar.hidden = true;
  }
}
if ($("dlMode")) {
  $("dlMode").onchange = () => {
    dlProbeCache = {};
    dlFillMirrors();
    if ($("dlState")) $("dlState").textContent = "已切换下载内容；建议先点「测速」。";
  };
  $("dlProbe").onclick = () => dlProbeAll();
  $("dlStart").onclick = () => dlStart();
  $("dlStop").onclick = () => api.call("dl_cancel").catch(() => {});
  $("dlDirPick").onclick = async () => {
    try {
      const d = await api.call("pick_folder");
      if (d) $("dlDir").value = d;
    } catch (e) {
      toast(String(e), true);
    }
  };
}
listen("dl-progress", (ev) => {
  const p = ev.payload || {};
  const fill = $("dlFill");
  const st = $("dlState");
  if (fill) {
    fill.classList.remove("pulse");
    const pct = p.total ? Math.min(100, (p.got / p.total) * 100) : 35;
    fill.style.width = pct.toFixed(1) + "%";
  }
  if (st) {
    const parts = [p.phase || "下载中"];
    if (p.got) parts.push(`${dlFmt(p.got)} / ${p.total ? dlFmt(p.total) : "?"}`);
    if (p.mbps) parts.push(`${p.mbps.toFixed(1)} MB/s`);
    st.textContent = parts.join(" · ");
  }
});

document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === "Enter") generate();
  if (e.key === "Escape") {
    $("lightbox").hidden = true;
    $("settingsModal").hidden = true;
    $("enhanced").hidden = true;
  }
});


$("btnQueueAdd").onclick = () => queueAdd();
$("btnQueueRun").onclick = () => queueRun();
$("btnQueueStop").onclick = () => {
  queue.stop = true;
  api.call("job_cancel");
  toast("队列将在当前这张结束后停止");
};
$("btnLibrary").onclick = () => openLibrary();
$("libClose").onclick = () => ($("library").hidden = true);
$("libRefresh").onclick = () => loadLibrary();
$("libSearch").oninput = () => renderLibrary();
$("libStarOnly").onchange = () => renderLibrary();
$("libSort").onchange = () => renderLibrary();
$("libDelete").onclick = () => deleteSelected();
$("libCompare").onclick = async () => {
  const picked = libItems.filter((i) => libSel.has(i.path));
  if (picked.length !== 2) return;
  await openCompare(picked[0], picked[1]);
};

$("cmpClose").onclick = () => ($("compare").hidden = true);
$("cmpSwap").onclick = async () => {
  if (cmpPaths.length !== 2) return;
  const tmp = cmpPaths[0];
  cmpPaths[0] = cmpPaths[1];
  cmpPaths[1] = tmp;
  $("cmpALabel").textContent = cmpPaths[0].filename;
  $("cmpBLabel").textContent = cmpPaths[1].filename;
  $("cmpA").src = await libUrl(cmpPaths[0].path);
  $("cmpB").src = await libUrl(cmpPaths[1].path);
};
$("cmpZoom").oninput = () => {
  cmpZoom = Number($("cmpZoom").value);
  applyCmp();
};


$("libEdit").onclick = () => openBatchEdit();
$("batchEditCancel").onclick = () => {
  if (batchEdit.running) {
    batchEdit.stop = true;
    api.call("job_cancel");
    return;
  }
  $("batchEditModal").hidden = true;
};
$("batchEditStart").onclick = () => runBatchEdit();

$("cmpExport").onclick = () => exportCompare();



$("vwClose").onclick = () => ($("viewer").hidden = true);
$("vwPrev").onclick = () => viewerStep(-1);
$("vwNext").onclick = () => viewerStep(1);
$("vwZoomIn").onclick = () => viewerZoom(1.25);
$("vwZoomOut").onclick = () => viewerZoom(0.8);
$("vwFit").onclick = () => {
  viewer.zoom = 100;
  viewer.pan = { x: 0, y: 0 };
  applyViewer();
};
$("vwUpscale").onclick = () => viewerUpscale();
$("vwReveal").onclick = () => {
  const it = viewer.list[viewer.idx];
  if (it) api.call("reveal_file", { path: it.path });
};
$("vwRef").onclick = async () => {
  const it = viewer.list[viewer.idx];
  if (!it) return;
  const dataUrl = await libUrl(it.path);
  await addRefFromDataUrl(dataUrl, it.filename.replace(/\.[^.]+$/, ""));
  $("viewer").hidden = true;
};

$("viewerStage").addEventListener("wheel", (e) => {
  if ($("viewer").hidden || maskState.active) return;
  e.preventDefault();
  if (e.ctrlKey || e.metaKey) {
    viewerZoom(e.deltaY < 0 ? 1.12 : 0.89);
    return;
  }
  if (Math.abs(e.deltaY) < 3) return;
  viewerStep(e.deltaY > 0 ? 1 : -1);
}, { passive: false });

(() => {
  const stage = $("viewerStage");
  let dragging = false;
  let moved = false;
  let outside = false;
  let sx = 0;
  let sy = 0;
  let startX = 0;
  let startY = 0;
  stage.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    if (typeof maskState !== "undefined" && maskState.active) return;
    // 点在图外 = 关掉查看器（跟灯箱一致）；点在图内才进入拖拽/放大
    const img = $("viewerImg");
    const r = img ? img.getBoundingClientRect() : null;
    const onImg =
      r && e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom;
    outside = !onImg;
    dragging = !!onImg;
    moved = false;
    if (!onImg) return;
    e.preventDefault(); // 关掉浏览器原生图片拖拽，否则拖两下就断
    startX = e.clientX;
    startY = e.clientY;
    sx = e.clientX - viewer.pan.x;
    sy = e.clientY - viewer.pan.y;
    try {
      stage.setPointerCapture(e.pointerId);
    } catch {}
  });
  stage.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    if (!moved && Math.abs(e.clientX - startX) + Math.abs(e.clientY - startY) > 5) {
      moved = true;
      stage.classList.add("dragging");
    }
    if (!moved) return;
    viewer.pan = { x: e.clientX - sx, y: e.clientY - sy };
    applyViewer();
  });
  const end = (e) => {
    if (outside) {
      outside = false;
      dragging = false;
      moved = false;
      if (e && e.type === "pointerup") $("viewer").hidden = true;
      return;
    }
    if (!dragging) return;
    const wasMoved = moved;
    dragging = false;
    moved = false;
    stage.classList.remove("dragging");
    // 没拖动 = 单击：以点击处为锚放大 / 再点回到适应
    if (!wasMoved && e && e.type === "pointerup") viewerZoomAt(e.clientX, e.clientY);
  };
  stage.addEventListener("pointerup", end);
  stage.addEventListener("pointercancel", end);
})();

document.addEventListener("keydown", (e) => {
  if (!$("lightbox").hidden) {
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") { e.preventDefault(); lightboxStep(-1); }
    else if (e.key === "ArrowRight" || e.key === "ArrowDown") { e.preventDefault(); lightboxStep(1); }
    else if (e.key === "Escape") { $("lightbox").hidden = true; }
    return;
  }
  if (!$("viewer").hidden) {
    if (e.key === "ArrowLeft" || e.key === "ArrowUp") { e.preventDefault(); viewerStep(-1); }
    else if (e.key === "ArrowRight" || e.key === "ArrowDown") { e.preventDefault(); viewerStep(1); }
    else if (e.key === "+" || e.key === "=") { viewerZoom(1.25); }
    else if (e.key === "-") { viewerZoom(0.8); }
    else if (e.key === "Escape") { $("viewer").hidden = true; }
    return;
  }
  if (!$("library").hidden && e.key === "Escape") $("library").hidden = true;
});

$("libTranslate").onclick = () => translateSearch();
$("libAltClear").onclick = () => {
  libAltQuery = "";
  $("libAltChip").hidden = true;
  renderLibrary();
};
$("libSearch").addEventListener("input", () => {
  libRenderLimit = 120;
  if (libAltQuery) {
    libAltQuery = "";
    $("libAltChip").hidden = true;
  }
});
