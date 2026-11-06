// 模型管理器：扫 ComfyUI/models/**，看预览/触发词、启用/禁用。
// 禁用只写 `.disabled` 标记，不动模型文件；壳里的 LoRA 下拉会跳过它们。

let modelCache = [];
let modelKind = "";
let modelQuery = "";
let disabledLoras = new Set();

function modelSize(bytes) {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(0)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function modelDate(sec) {
  if (!sec) return "";
  const d = new Date(sec * 1000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function modelVisible(m) {
  if (modelKind && m.kind !== modelKind) return false;
  const q = modelQuery.trim().toLowerCase();
  if (!q) return true;
  return (
    m.name.toLowerCase().includes(q) ||
    (m.baseModel || "").toLowerCase().includes(q) ||
    m.triggers.some((t) => t.toLowerCase().includes(q))
  );
}

function modelKinds() {
  const map = new Map();
  for (const m of modelCache) map.set(m.kind, (map.get(m.kind) || 0) + 1);
  return [...map.entries()].sort((a, b) => b[1] - a[1]);
}

function renderModelChips() {
  const box = $("modelKinds");
  box.textContent = "";
  const all = document.createElement("button");
  all.type = "button";
  all.className = "style-chip" + (modelKind ? "" : " on");
  all.textContent = `全部 ${modelCache.length}`;
  all.onclick = () => {
    modelKind = "";
    renderModels();
  };
  box.appendChild(all);
  for (const [kind, n] of modelKinds()) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "style-chip" + (modelKind === kind ? " on" : "");
    b.textContent = `${kind} ${n}`;
    b.onclick = () => {
      modelKind = kind;
      renderModels();
    };
    box.appendChild(b);
  }
}

async function renderModels() {
  renderModelChips();
  const box = $("modelList");
  box.textContent = "";
  const list = modelCache.filter(modelVisible);
  if (!list.length) {
    const d = document.createElement("div");
    d.className = "muted";
    d.style.cssText = "padding:16px 2px;font-size:12px";
    d.textContent = modelCache.length
      ? "没有匹配的模型"
      : "没扫到模型。检查「设置 → ComfyUI 目录」是否指向 ComfyUI 根目录。";
    box.appendChild(d);
    return;
  }
  for (const m of list) {
    const row = document.createElement("div");
    row.className = "model-row" + (m.disabled ? " off" : "");
    const thumb = document.createElement("div");
    thumb.className = "model-thumb";
    if (m.preview) {
      api
        .call("model_preview", { name: m.preview })
        .then((url) => {
          const img = document.createElement("img");
          img.src = url;
          thumb.textContent = "";
          thumb.appendChild(img);
        })
        .catch(() => {});
    } else {
      const ico = document.createElement("span");
      ico.className = "model-thumb-ico";
      ico.textContent = m.kind.slice(0, 2);
      thumb.appendChild(ico);
    }
    const main = document.createElement("div");
    main.className = "model-main";
    const head = document.createElement("div");
    head.className = "model-name";
    head.textContent = m.name;
    const meta = document.createElement("div");
    meta.className = "muted model-meta";
    const bits = [m.kind, modelSize(m.size), modelDate(m.mtime)];
    if (m.baseModel) bits.push(m.baseModel);
    if (m.hash) bits.push(`#${m.hash.slice(0, 8)}`);
    meta.textContent = bits.join(" · ");
    main.append(head, meta);
    if (m.triggers.length) {
      const tw = document.createElement("div");
      tw.className = "model-triggers";
      for (const t of m.triggers.slice(0, 8)) {
        const chip = document.createElement("span");
        chip.className = "model-trigger";
        chip.textContent = t;
        chip.title = "点一下复制";
        chip.onclick = () => {
          navigator.clipboard?.writeText(t);
          toast(`已复制触发词：${t}`);
        };
        tw.appendChild(chip);
      }
      main.appendChild(tw);
    }
    const toggle = document.createElement("button");
    toggle.className = "btn small " + (m.disabled ? "primary" : "ghost");
    toggle.textContent = m.disabled ? "启用" : "禁用";
    toggle.onclick = async () => {
      try {
        await api.call("model_toggle", { name: m.name, enabled: m.disabled });
        m.disabled = !m.disabled;
        syncDisabledLoras();
        renderModels();
      } catch (e) {
        toast(String(e), true);
      }
    };
    row.append(thumb, main, toggle);
    box.appendChild(row);
  }
  $("modelCount").textContent = `${list.length}/${modelCache.length} 个`;
}

function syncDisabledLoras() {
  disabledLoras = new Set(
    modelCache.filter((m) => m.disabled && m.kind === "loras").map((m) => m.name.replace(/^loras\//, "")),
  );
  applyLoraFilter();
}

function applyLoraFilter() {
  // 只负责「禁用标记」这件事：模型过滤、注释、lora2 全部由 rebuildLoraOptions 一处出。
  // 以前这里自己拼一份**不分模型**的全量表塞进 #lora（还不管 #lora2），
  // 于是启动/扫完模型后两个下拉各说各话：lora 全量混着别的家族，lora2 停在旧模型的表上。
  if (!state.models || typeof rebuildLoraOptions !== "function") return;
  rebuildLoraOptions();
}

async function modelsOpen() {
  $("modelsModal").hidden = false;
  $("modelList").textContent = "";
  $("modelCount").textContent = "扫描中…";
  try {
    modelCache = (await api.call("models_scan")) || [];
  } catch (e) {
    modelCache = [];
    toast(String(e), true);
  }
  await renderModels();
}

async function modelFilterInit() {
  try {
    modelCache = (await api.call("models_scan")) || [];
    syncDisabledLoras();
  } catch {}
}

$("modelsBtn").onclick = () => modelsOpen();
$("modelsClose").onclick = () => ($("modelsModal").hidden = true);
$("modelsModal").addEventListener("click", (e) => {
  if (e.target === $("modelsModal")) $("modelsModal").hidden = true;
});
$("modelSearch").oninput = () => {
  modelQuery = $("modelSearch").value;
  renderModels();
};
$("modelRescan").onclick = () => modelsOpen();

// ---------------------------------------------------------------- 模型下载
// 从 app.js 搬来（模型管理里「下载」按钮用）；行为不变。
async function downloadModel(entry, btn) {
  if (!confirm(`下载 ${entry.target}？\n约 ${dlFmt(entry.size)}，下到 ${entry.kind}/\n来源：hf-mirror（失败自动回退 HF）`)) return;
  btn.disabled = true;
  btn.textContent = "下载中…";
  const bar = $("wfBar");
  const fill = $("wfFill");
  const st = $("wfState");
  if (bar) bar.hidden = false;
  if (fill) fill.style.width = "0%";
  if (st) {
    st.hidden = false;
    st.textContent = `下载 ${entry.target}…`;
  }
  try {
    await api.call("model_download", {
      target: entry.target,
      kind: entry.kind,
      urls: entry.sources || [],
    });
    toast(`已下载：${entry.target}`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    if (bar) bar.hidden = true;
    if (st) st.hidden = true;
    renderWorkflowPresets();
  }
}

listen("model-progress", (ev) => {
  const p = ev.payload || {};
  const st = $("wfState");
  const fill = $("wfFill");
  if (st) {
    st.hidden = false;
    const parts = [p.phase || "下载中", p.target || ""];
    if (p.got) parts.push(`${dlFmt(p.got)} / ${p.total ? dlFmt(p.total) : "?"}`);
    if (p.mbps) parts.push(`${p.mbps.toFixed(1)} MB/s`);
    if (p.error) parts.push(p.error.slice(0, 160));
    st.textContent = parts.filter(Boolean).join(" · ");
  }
  if (fill && p.total) fill.style.width = Math.min(100, (p.got / p.total) * 100).toFixed(1) + "%";
  if (p.phase === "完成") {
    setTimeout(() => {
      if ($("wfState")) $("wfState").hidden = true;
      if ($("wfBar")) $("wfBar").hidden = true;
    }, 1800);
  }
});

$("btnSettings").onclick = () => {
  // 先把弹窗画出来，再做填充/扫描这些后台活，避免「点了没反应」
  setSettingsTab("general");
  $("settingsModal").hidden = false;
  setTimeout(() => {
    if (window.shortcuts) window.shortcuts.build();
    if ($("dlDir")) $("dlDir").value = (state.settings && state.settings.comfyRoot) || "";
    if (typeof dlFillMirrors === "function") dlFillMirrors();
    if (window.aiAgent) window.aiAgent.refresh();
    refreshSettingsInfo();
  }, 16);
};
$("btnSettingsCancel").onclick = () => ($("settingsModal").hidden = true);
if ($("btnSettingsCloseX")) $("btnSettingsCloseX").onclick = () => ($("settingsModal").hidden = true);
if ($("setOpenCfg")) {
  $("setOpenCfg").onclick = async () => {
    try {
      const dir = await api.call("config_dir_path");
      await api.call("open_folder", { path: dir });
    } catch (e) {
      toast(String(e), true);
    }
  };
}
if ($("setOpenComfy")) {
  $("setOpenComfy").onclick = () => {
    const p = $("setRoot").value.trim();
    if (p) api.call("open_folder", { path: p });
  };
}
if ($("setOpenModels")) {
  $("setOpenModels").onclick = () => {
    const p = $("setRoot").value.trim();
    if (p) api.call("open_folder", { path: `${p}\\ComfyUI\\models` });
  };
}
if ($("setRestartComfy")) {
  $("setRestartComfy").onclick = async () => {
    try {
      await saveSettings(false);
      toast("正在重启 ComfyUI…");
      await api.call("server_stop");
      await api.call("server_start");
      toast("ComfyUI 已重启");
    } catch (e) {
      toast(String(e), true);
    } finally {
      refreshSettingsInfo();
    }
  };
}
