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
