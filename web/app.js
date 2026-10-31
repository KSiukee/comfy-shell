const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);

const state = {
  settings: null,
  profiles: [],
  customs: [],
  models: { loras: [] },
  mode: "t2i",
  refs: [],          // filenames inside ComfyUI/input
  results: [],
  stars: new Set(),       // newest first: {path, filename, width, height, label, seconds, ts, dataUrl?}
  running: false,
  multiBusy: false,
  starting: false,
};

const RATIOS = {
  "1:1": [1024, 1024],
  "3:2": [1216, 832],
  "2:3": [832, 1216],
  "16:9": [1344, 768],
  "9:16": [768, 1344],
};

// ---------------------------------------------------------------- utilities

function toast(msg, isErr = false) {
  const el = $("toast");
  el.textContent = msg;
  el.className = "toast" + (isErr ? " err" : "");
  el.hidden = false;
  clearTimeout(toast._t);
  toast._t = setTimeout(() => (el.hidden = true), isErr ? 6000 : 2600);
}

function profile() {
  const id = $("model").value;
  if (id.startsWith("custom:")) {
    return {
      id,
      label: "自定义 · " + id.slice(7),
      steps: 20,
      cfg: 1,
      turboLora: null,
      turboSteps: 20,
      supportsEdit: false,
      defaultNegative: "",
      custom: true,
      name: id.slice(7),
    };
  }
  return state.profiles.find((p) => p.id === id) || null;
}

function multiStart() {
  if (state.multiBusy) {
    toast("已有一个批量任务在跑，等它结束", true);
    return false;
  }
  state.multiBusy = true;
  return true;
}

function multiEnd() {
  state.multiBusy = false;
}

function setBusy(busy) {
  state.running = busy;
  $("btnGenerate").disabled = busy;
  $("btnCancel").hidden = !busy;
  $("progressWrap").hidden = !busy;
}

function fmtTime(s) {
  return s >= 60 ? `${(s / 60).toFixed(1)} 分` : `${s.toFixed(0)} 秒`;
}

// ---------------------------------------------------------------- status

async function refreshStatus() {
  try {
    const st = await invoke("server_status");
    state.healthy = st.healthy;
    const dot = $("statusDot");
    const text = $("statusText");
    if (st.healthy) {
      dot.className = "dot ok";
      text.textContent = st.managed ? "ComfyUI 运行中（本壳托管）" : "ComfyUI 运行中（外部进程）";
    } else if (state.starting) {
      dot.className = "dot warn";
      text.textContent = "正在启动 ComfyUI…";
    } else if (state.settings?.autoStart) {
      dot.className = "dot warn";
      text.textContent = "ComfyUI 未响应…";
    } else {
      dot.className = "dot err";
      text.textContent = "ComfyUI 未运行";
    }
    $("statusPort").textContent = ":" + st.port;
    $("statusVersion").textContent = st.version ? "v" + st.version : "";
    $("btnStart").disabled = st.healthy || state.starting;
    $("btnStop").disabled = false;
  } catch (e) {
    $("statusDot").className = "dot err";
    $("statusText").textContent = "后端异常";
  }
}

// ---------------------------------------------------------------- model UI

function buildModelSelect() {
  const sel = $("model");
  const current = sel.value;
  const prof = state.profiles.map((p) => `<option value="${p.id}">${p.label}</option>`).join("");
  const cust = state.customs.length
    ? `<optgroup label="自定义工作流">` +
      state.customs.map((n) => `<option value="custom:${n}">自定义 · ${n}</option>`).join("") +
      `</optgroup>`
    : "";
  sel.innerHTML = prof + cust;
  if (current && [...sel.options].some((o) => o.value === current)) sel.value = current;
}

// ---------------------------------------------------------------- LoRA registry
// Which LoRAs each image model can actually load, and the order to try them in.
const LORA_DESC = [
  [/ultra_real/i, "去 AI 塑料感，给皮肤/布料加纹理，强度 0.6–0.8"],
  [/beauty/i, "人像美化、柔和磨皮，强度 0.7"],
  [/nsfw/i, "训练者自己的成人影像风格，强度 0.7–0.9"],
  [/anything2real/i, "把任意画风转成真人照片（改图用，强度 0.8）"],
  [/pixel-art/i, "像素画风"],
  [/turbo/i, "加速 LoRA，配合 8 步使用"],
  [/(probe|test\d|ui_probe|folder_probe)/i, "自己训练的测试 LoRA，可删"],
];

function loraTip(name) {
  const hit = LORA_DESC.find(([re]) => re.test(name));
  return hit ? hit[1] : "";
}

const LORA_RULES = [
  { test: /minimax_h3/i, models: [], note: "MiniMax 视频专用" },
  { test: /flux2|spritesheet/i, models: [], note: "FLUX.2 专用，FLUX.3 不适配" },
  { test: /qwen_image_2\.1_turbo/i, models: ["qwen21"], rank: 1, strength: 1.0, note: "加速（配 8 步）" },
  { test: /Qwen-Image-2512.*Pixel-Art/i, models: ["qwen21"], rank: 2, strength: 0.85, note: "像素风" },
  { test: /anything2real|Qwen2\.1_/i, models: ["qwen21"], rank: 2, strength: 0.8, note: "风格转真人（改图）" },
  { test: /moss/i, models: ["moss", "mossbase", "mossimagine"], rank: 3, strength: 0.8, note: "MOSS 生态 LoRA" },
  { test: /MOSS-Image-NSFW/i, models: ["moss", "mossbase", "mossimagine"], rank: 1, strength: 0.85, note: "NSFW 人像/场景" },
  { test: /folder_probe|ui_probe|low_probe|mirror_probe/i, models: ["flux3base", "flux3"], rank: 1, strength: 0.9, note: "你自训（FLUX.3 Base）" },
];

function loraMeta(name, profileId) {
  for (const r of LORA_RULES) {
    if (r.test.test(name)) {
      return { compatible: r.models.includes(profileId), rank: r.rank || 9, strength: r.strength, note: r.note };
    }
  }
  return { compatible: ["flux3", "flux3base"].includes(profileId), rank: 5, note: "未登记" };
}

function rebuildLoraOptions() {
  const sel = $("lora");
  const p = profile();
  const prev = sel.value;
  if (!p || p.custom) {
    sel.innerHTML = `<option value="">（无）</option>`;
    updateLoraUI();
    return;
  }
  const items = (state.models.loras || []).map((n) => ({ n, m: loraMeta(n, p.id) }));
  // incompatible LoRAs (video models, other families) are hidden, not greyed out
  const ok = items.filter((x) => x.m.compatible).sort((a, b) => a.m.rank - b.m.rank || a.n.localeCompare(b.n));
  const hidden = items.length - ok.length;
  const opts =
    `<option value="">（无）</option>` +
    ok.map((x) => `<option value="${x.n}">${x.m.rank === 1 ? "★ " : ""}${x.n}${x.m.note ? " — " + x.m.note : ""}</option>`).join("");
  sel.innerHTML = opts;
  sel.dataset.hiddenCount = String(hidden);
  const sel2 = $("lora2");
  const prev2 = sel2.value;
  sel2.innerHTML = opts;
  if (prev2 && [...sel2.options].some((o) => o.value === prev2)) sel2.value = prev2;
  if (prev && [...sel.options].some((o) => o.value === prev && !o.disabled)) sel.value = prev;
  updateLoraUI();
}

function updateLoraUI() {
  const p = profile();
  const name = $("lora").value;
  $("lora2StrengthRow").hidden = !$("lora2").value;
  const meta = name && p ? loraMeta(name, p.id) : null;
  $("loraStrengthRow").hidden = !name;
  if (!name) {
    $("loraHint").textContent = "选一个 LoRA 才会显示强度（0.6–0.9 更像，1.0 更贴）";
    return;
  }
  const hiddenCount = Number($("lora").dataset.hiddenCount || 0);
  const tail = hiddenCount > 0 ? `（已隐藏 ${hiddenCount} 个不兼容）` : "";
  const tip = loraTip(name);
  const strength = meta?.strength ?? 0.9;
  $("loraHint").textContent = tip
    ? `${tip}｜建议强度 ${strength} ${tail}`
    : meta?.note
    ? `${meta.note}｜建议强度 ${strength} ${tail}`
    : `未登记来源，建议 0.8–0.9 试 ${tail}`;
}

function applyProfileDefaults(keepSize = false) {
  const p = profile();
  if (!p) return;
  const isCustom = !!p.custom;
  $("steps").value = p.steps;
  $("cfg").value = p.cfg;
  $("negative").value = p.defaultNegative || "";

  const turboBox = $("turbo");
  turboBox.checked = false;
  turboBox.disabled = !p.turboLora;
  turboBox.parentElement.hidden = !p.turboLora;
  const mossy = (p.id || "").startsWith("moss");
  $("slgField").hidden = !mossy;
  $("btnMossPreset").hidden = !mossy;
  $("btnLayout").hidden = !mossy;
  if (mossy && !state.slgTouched) $("mossSlg").checked = true;
  if (!mossy) $("mossSlg").checked = false;

  $("lora").disabled = isCustom;
  $("loraStrength").disabled = isCustom;
  $("lora").parentElement.style.opacity = isCustom ? ".45" : "1";
  if (!isCustom) rebuildLoraOptions();

  const editTab = document.querySelector('.tab[data-mode="edit"]');
  editTab.disabled = isCustom ? true : !p.supportsEdit;
  if (editTab.disabled && state.mode === "edit") setMode("t2i");

  $("btnEnhance").disabled = isCustom;
  $("btnEnhance").title = isCustom
    ? "自定义工作流没有配套的扩写器"
    : "用官方提示词增强器把一句话扩写成完整画面描述";

  $("modelHint").textContent = isCustom
    ? "模板可用占位符：{{prompt}} {{negative}} {{seed}} {{width}} {{height}} {{steps}} {{cfg}}"
    : p.supportsEdit
    ? "支持生图 / 改图（参考图）+ 官方扩写"
    : "仅生图 + 官方扩写";

  if (!keepSize) {
    $("width").value = 1024;
    $("height").value = 1024;
    document.querySelectorAll("#ratioChips button").forEach((b) => b.classList.remove("active"));
  }
  updateRefHint();
}

function setMode(mode) {
  state.mode = mode;
  document.querySelectorAll("#modeTabs .tab").forEach((t) => t.classList.toggle("active", t.dataset.mode === mode));
  refreshRefVisibility();
}

// Qwen-Image 2.1 / FLUX.3 can take reference images for plain text-to-image too
// (same weights, reference conditioning). MOSS and custom graphs cannot.
function refreshRefVisibility() {
  const p = profile();
  const allow = !!p && !!p.supportsEdit;
  const show = !!p && (state.mode === "edit" || (allow && state.mode === "t2i"));
  $("refField").hidden = !show;
  const max = p?.id === "qwen21" ? 1 : 10;
  if (state.mode === "edit") {
    $("refHint").textContent = `（必填，最多 ${max} 张）`;
  } else if (allow) {
    $("refHint").textContent = `（可选，最多 ${max} 张，作主体/风格参考）`;
  } else {
    $("refHint").textContent = "";
  }
}

function updateRefHint() {
  refreshRefVisibility();
}

// ---------------------------------------------------------------- refs

function renderRefs() {
  const box = $("thumbs");
  box.innerHTML = "";
  state.refs.forEach((name, i) => {
    const d = document.createElement("div");
    d.className = "thumb";
    d.innerHTML = `<img src="" alt=""><button title="移除">×</button>`;
    setThumbSrc(d.querySelector("img"), name);
    d.querySelector("button").onclick = () => {
      state.refs.splice(i, 1);
      renderRefs();
    };
    box.appendChild(d);
  });
}

const thumbCache = new Map();
async function setThumbSrc(imgEl, name) {
  if (thumbCache.has(name)) {
    imgEl.src = thumbCache.get(name);
    return;
  }
  try {
    const url = await invoke("read_input_image", { name });
    thumbCache.set(name, url);
    imgEl.src = url;
  } catch {
    imgEl.src = "";
  }
}

async function addRefFile(file) {
  const max = profile()?.id === "qwen21" ? 1 : 10;
  if (state.refs.length >= max) {
    toast(`这个模型最多 ${max} 张参考图`, true);
    return;
  }
  const dataUrl = await new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(r.result);
    r.onerror = rej;
    r.readAsDataURL(file);
  });
  const name = await invoke("save_ref_image", { name: file.name, dataUrl });
  state.refs.push(name);
  thumbCache.set(name, dataUrl);
  renderRefs();
}

async function addRefFromDataUrl(dataUrl, label = "result") {
  const max = profile()?.id === "qwen21" ? 1 : 10;
  if (state.refs.length >= max) {
    toast(`这个模型最多 ${max} 张参考图`, true);
    return;
  }
  const name = await invoke("save_ref_image", { name: label + ".png", dataUrl });
  state.refs.push(name);
  thumbCache.set(name, dataUrl);
  if (state.mode !== "edit" && profile()?.supportsEdit) setMode("edit");
  renderRefs();
  toast("已加入参考图");
}

// ---------------------------------------------------------------- results

const observer = new IntersectionObserver(
  (entries) => {
    for (const en of entries) {
      if (!en.isIntersecting) continue;
      const img = en.target;
      observer.unobserve(img);
      loadImageInto(img);
    }
  },
  { root: $("results"), rootMargin: "300px" }
);

async function loadImageInto(img) {
  const path = img.dataset.path;
  const item = state.results.find((r) => r.path === path);
  if (!item) return;
  if (item.dataUrl) {
    img.src = item.dataUrl;
    return;
  }
  try {
    item.dataUrl = await invoke("read_output_image", { path });
    img.src = item.dataUrl;
  } catch {
    img.alt = "读取失败";
  }
}

// ---------------------------------------------------------------- result management

async function deleteResults(items) {
  const paths = items.map((i) => i.path);
  if (!paths.length) return;
  try {
    await invoke("delete_results", { paths });
    state.results = state.results.filter((r) => !paths.includes(r.path));
    renderResults();
    if (!$("library").hidden) await loadLibrary();
    toast(`已删除 ${paths.length} 张（在 output\\_comfyshell_trash 可找回）`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function useAsReference(r) {
  try {
    const dataUrl = r.dataUrl || (await invoke("read_output_image", { path: r.path }));
    r.dataUrl = dataUrl;
    await addRefFromDataUrl(dataUrl, r.filename.replace(/\.[^.]+$/, ""));
    toast("已设为参考图");
  } catch (e) {
    toast(String(e), true);
  }
}

function hideCtxMenu() {
  $("ctxmenu").hidden = true;
}

function showCtxMenu(x, y, items) {
  const menu = $("ctxmenu");
  menu.innerHTML = "";
  for (const item of items) {
    const b = document.createElement("button");
    b.textContent = item.label;
    if (item.danger) b.className = "danger";
    b.onclick = (ev) => {
      ev.stopPropagation();
      hideCtxMenu();
      item.run();
    };
    menu.appendChild(b);
  }
  menu.hidden = false;
  const w = menu.offsetWidth || 180;
  const h = menu.offsetHeight || 120;
  menu.style.left = `${Math.max(6, Math.min(x, window.innerWidth - w - 8))}px`;
  menu.style.top = `${Math.max(6, Math.min(y, window.innerHeight - h - 8))}px`;
}

document.addEventListener("click", hideCtxMenu);
document.addEventListener("contextmenu", (e) => {
  if (!e.target.closest(".card") && !e.target.closest(".lib-item")) hideCtxMenu();
});

function resultMenu(r) {
  return [
    { label: "删除（移到回收站）", danger: true, run: () => deleteResults([r]) },
    { label: "在文件夹中显示", run: () => invoke("reveal_file", { path: r.path }) },
    { label: "用作参考图", run: () => useAsReference(r) },
    {
      label: "复制提示词",
      run: () => {
        navigator.clipboard.writeText(r.prompt || "").then(
          () => toast("提示词已复制"),
          () => toast("复制失败", true),
        );
      },
    },
  ];
}

async function loadStars() {
  try {
    state.stars = new Set(await invoke("stars_list"));
  } catch {
    state.stars = new Set();
  }
}

async function toggleStar(r, btn) {
  const on = !state.stars.has(r.path);
  if (on) state.stars.add(r.path);
  else state.stars.delete(r.path);
  if (btn) {
    btn.classList.toggle("on", on);
    btn.textContent = on ? "★" : "☆";
  }
  try {
    await invoke("library_star", { path: r.path, starred: on });
    toast(on ? "已收藏" : "已取消收藏");
  } catch (e) {
    toast(String(e), true);
  }
}

function renderResults() {
  const box = $("results");
  box.querySelectorAll(".card").forEach((c) => c.remove());
  $("empty").hidden = state.results.length > 0;
  for (const r of state.results) {
    const card = document.createElement("div");
    card.className = "card";
    const img = document.createElement("img");
    img.dataset.path = r.path;
    img.alt = r.filename;
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
      `<span class="time mono">${r.width}×${r.height} · ${fmtTime(r.seconds)}</span>`;
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
    reveal.onclick = () => invoke("reveal_file", { path: r.path });
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

let lightboxItem = null;

async function openLightbox(r) {
  lightboxItem = r;
  $("lightbox").hidden = false;
  await showLightbox(r);
}

async function showLightbox(r) {
  lightboxItem = r;
  const idx = state.results.findIndex((x) => x.path === r.path);
  $("lbPos").textContent = idx >= 0 ? `${idx + 1} / ${state.results.length}` : "";
  $("lightboxMeta").textContent = `${r.filename} · ${r.width}×${r.height} · ${(r.prompt || "").slice(0, 40)}`;
  $("lightboxImg").src = r.dataUrl || "";
  if (!r.dataUrl) {
    try {
      r.dataUrl = await invoke("read_output_image", { path: r.path });
      if (lightboxItem === r) $("lightboxImg").src = r.dataUrl;
    } catch {}
  }
}

async function lightboxStep(delta) {
  const idx = state.results.findIndex((x) => x.path === lightboxItem?.path);
  if (idx < 0 || state.results.length < 2) return;
  const next = (idx + delta + state.results.length) % state.results.length;
  await showLightbox(state.results[next]);
}

$("lbPrev").onclick = () => lightboxStep(-1);
$("lbNext").onclick = () => lightboxStep(1);

$("lightbox").addEventListener("wheel", (e) => {
  if ($("lightbox").hidden) return;
  e.preventDefault();
  if (Math.abs(e.deltaY) < 3) return;
  lightboxStep(e.deltaY > 0 ? 1 : -1);
}, { passive: false });

function mergeResults(items) {
  const seen = new Set(state.results.map((r) => r.path));
  const fresh = items.filter((i) => !seen.has(i.path));
  state.results = fresh.concat(state.results);
  renderResults();
}

function flattenHistory(entries) {
  const out = [];
  for (const h of entries) {
    for (const im of h.images || []) {
      out.push({
        path: im.path,
        filename: im.filename,
        width: im.width,
        height: im.height,
        label: h.label,
        seconds: h.seconds,
        ts: h.ts,
        prompt: h.prompt,
        mode: h.mode,
      });
    }
  }
  return out;
}

// ---------------------------------------------------------------- generate

function currentReq() {
  return {
    profile: $("model").value,
    mode: state.mode,
    prompt: $("prompt").value.trim(),
    negative: $("negative").value,
    width: Number($("width").value),
    height: Number($("height").value),
    steps: Number($("steps").value),
    cfg: Number($("cfg").value),
    seed: Number($("seed").value),
    batch: Number($("batch").value),
    lora: $("lora").value || null,
    loraStrength: Number($("loraStrength").value),
    lora2: $("lora2").value || null,
    lora2Strength: Number($("lora2Strength").value),
    slg: !!$("mossSlg").checked,
    slgLayers: $("slgTier").value,
    layout: state.pendingLayout || null,
    refImages: [...state.refs],
  };
}

async function generate() {
  if (!(await ensureReady("生成"))) return;
  if (state.running) return false;
  if (!$("prompt").value.trim()) {
    toast("先写提示词", true);
    $("prompt").focus();
    return false;
  }
  if (state.mode === "edit" && state.refs.length === 0) {
    toast("改图模式需要先放参考图", true);
    return false;
  }
  if (!state.healthy) {
    toast("ComfyUI 还没起来，先点「启动」", true);
    return false;
  }
  let ok = false;
  setBusy(true);
  $("progressText").textContent = "提交中…";
  const req = currentReq();
  state.pendingLayout = null;
  try {
    const res = await invoke("generate", { req, turbo: $("turbo").checked });
    const items = res.images.map((im) => ({
      path: im.path,
      filename: im.filename,
      width: im.width,
      height: im.height,
      label: res.label,
      seconds: res.seconds,
      ts: Date.now(),
      prompt: req.prompt,
      mode: req.mode,
      dataUrl: im.dataUrl,
    }));
    mergeResults(items);
    toast(`完成：${fmtTime(res.seconds)}`);
    localStorage.setItem("cs.lastReq", JSON.stringify(req));
    ok = true;
  } catch (e) {
    toast(String(e), true);
  } finally {
    setBusy(false);
    $("progressText").textContent = "";
  }
  return ok;
}

// ---------------------------------------------------------------- enhance

async function enhance() {
  const p = profile();
  if (!p || p.custom) return;
  const text = $("prompt").value.trim();
  if (!text) {
    toast("先写一句想画什么", true);
    return;
  }
  if (state.mode === "edit" && p.id === "qwen21" && state.refs.length === 0) {
    toast("改图扩写需要先放参考图", true);
    return;
  }
  const btn = $("btnEnhance");
  btn.disabled = true;
  btn.textContent = "扩写中…";
  try {
    const out = await invoke("enhance_prompt", {
      profile: p.id,
      mode: state.mode,
      prompt: text,
      refImages: [...state.refs],
    });
    $("enhancedText").value = out;
    $("enhanced").hidden = false;
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "✨ 扩写";
  }
}

// ---------------------------------------------------------------- import

async function importWorkflow(file) {
  const text = await file.text();
  const name = file.name.replace(/\.json$/i, "").slice(0, 40);
  try {
    const saved = await invoke("custom_save", { name, jsonText: text });
    state.customs = await invoke("custom_list");
    buildModelSelect();
    $("model").value = "custom:" + saved;
    applyProfileDefaults();
    toast(`已导入工作流：${saved}`);
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- boot

// The shell is usable before ComfyUI is up: wait here instead of freezing.
async function ensureReady(label = "生成") {
  if (state.healthy) return true;
  toast(`等待 ComfyUI 启动，就绪后自动${label}…`);
  for (let i = 0; i < 300; i++) {
    await new Promise((r) => setTimeout(r, 1000));
    try {
      const st = await invoke("server_status");
      if (st.healthy) {
        state.healthy = true;
        refreshStatus();
        return true;
      }
    } catch {}
  }
  toast("ComfyUI 一直没起来，请检查设置里的目录", true);
  return false;
}

async function boot() {
  state.profiles = await invoke("profiles");
  state.customs = await invoke("custom_list");

  state.settings = await invoke("settings_get");
  $("setRoot").value = state.settings.comfyRoot;
  $("setPort").value = state.settings.port;
  $("setAutoStart").checked = state.settings.autoStart;
  $("setStopOnExit").checked = state.settings.stopOnExit;
  $("setFreeEnhance").checked = !!state.settings.freeEnhance;

  buildModelSelect();
  buildSweepModels();
  refreshStatus(); // paint something before the slower queries

  try {
    applyUiMode(state.settings.simpleUi !== false);
  $("simpleQuality").value = "std";
  $("simpleRatio").value = "1:1";
  await loadStars();
  state.models = await invoke("models_list");
  } catch {
    state.models = { loras: [] };
  }
  $("lora").innerHTML =
    `<option value="">（无）</option>` +
    state.models.loras.map((l) => `<option value="${l}">${l}</option>`).join("");

  try {
    const hist = await invoke("history_list");
    state.results = flattenHistory(hist);
  } catch {}
  renderResults();

  applyProfileDefaults();
  restoreLast();
  renderResults();
  setInterval(refreshStatus, 3000);

  await listen("gen-progress", (ev) => {
    const p = ev.payload;
    $("progressText").textContent = p.started
      ? `采样中 · ${p.elapsed.toFixed(0)}s`
      : `排队中 · ${p.elapsed.toFixed(0)}s`;
  });
  await listen("server-status", () => refreshStatus());
}

function restoreLast() {
  try {
    const raw = localStorage.getItem("cs.lastReq");
    if (!raw) return;
    const r = JSON.parse(raw);
    if (r.profile && [...$("model").options].some((o) => o.value === r.profile)) $("model").value = r.profile;
    applyProfileDefaults(true);
    if (r.prompt) $("prompt").value = r.prompt;
    if (r.negative !== undefined) $("negative").value = r.negative;
    if (r.width) $("width").value = r.width;
    if (r.height) $("height").value = r.height;
    if (r.steps) $("steps").value = r.steps;
    if (r.cfg !== undefined) $("cfg").value = r.cfg;
    if (r.seed !== undefined) $("seed").value = r.seed;
    if (r.batch) $("batch").value = r.batch;
    if (r.lora) $("lora").value = r.lora;
    if (r.lora2 && [...$("lora2").options].some((o) => o.value === r.lora2)) $("lora2").value = r.lora2;
    if (r.slg !== undefined && state.slgTouched) $("mossSlg").checked = !!r.slg;
    if (r.lora2Strength !== undefined) {
      $("lora2Strength").value = r.lora2Strength;
      $("lora2StrengthText").textContent = Number(r.lora2Strength).toFixed(2);
    }
    if (r.loraStrength !== undefined) {
      $("loraStrength").value = r.loraStrength;
      $("loraStrengthText").textContent = Number(r.loraStrength).toFixed(2);
    }
  } catch {}
}

// ---------------------------------------------------------------- wiring

$("model").onchange = () => {
  applyProfileDefaults();
  state.refs = [];
  renderRefs();
};

// Workflows can also be dropped into <config>/workflows by hand: refresh on focus.
$("model").onfocus = async () => {
  try {
    const list = await invoke("custom_list");
    if (list.join("|") !== state.customs.join("|")) {
      state.customs = list;
      buildModelSelect();
    }
  } catch {}
};

document.querySelectorAll("#modeTabs .tab").forEach((t) => {
  t.onclick = () => {
    if (t.disabled) return;
    setMode(t.dataset.mode);
  };
});

document.querySelectorAll("#ratioChips button").forEach((b) => {
  b.onclick = () => {
    const [w, h] = RATIOS[b.dataset.r];
    $("width").value = w;
    $("height").value = h;
    document.querySelectorAll("#ratioChips button").forEach((x) => x.classList.toggle("active", x === b));
  };
});

$("btnDice").onclick = () => ($("seed").value = -1);
$("loraStrength").oninput = () => ($("loraStrengthText").textContent = Number($("loraStrength").value).toFixed(2));
$("lora2").onchange = () => {
  $("lora2StrengthRow").hidden = !$("lora2").value;
};
$("lora2Strength").oninput = () => ($("lora2StrengthText").textContent = Number($("lora2Strength").value).toFixed(2));
const A2R_PROMPT =
  "turn this into a realistic photo of a person, natural skin texture, soft window light, detailed clothing";
$("lora").onchange = () => {
  const p = profile();
  const name = $("lora").value;
  const meta = name && p ? loraMeta(name, p.id) : null;
  if (meta && meta.strength) {
    $("loraStrength").value = String(meta.strength);
    $("loraStrengthText").textContent = Number(meta.strength).toFixed(2);
  }
  if (/anything2real/i.test(name) && p && p.id === "qwen21") {
    if (state.mode !== "edit") setMode("edit");
    if (!$("prompt").value.trim()) $("prompt").value = A2R_PROMPT;
    if (!state.refs.length) toast("这是改图 LoRA：先挂一张参考图再生成", true);
  }
  updateLoraUI();
};

$("btnGenerate").onclick = generate;
$("btnCancel").onclick = async () => {
  await invoke("job_cancel");
  toast("已请求取消");
};

// enhance
$("btnTranslateNeg").onclick = async () => {
  const text = $("negative").value.trim();
  if (!text) return toast("先写点中文负面词", true);
  const btn = $("btnTranslateNeg");
  btn.disabled = true;
  btn.textContent = "翻译中…";
  try {
    const out = await invoke("translate_text", { text });
    $("negative").value = String(out).trim();
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "中 → 英";
  }
};
$("mossSlg").onchange = () => (state.slgTouched = true);
$("btnMossPreset").onclick = () => {
  $("steps").value = 50;
  $("cfg").value = 3;
  $("width").value = 1024;
  $("height").value = 1024;
  $("mossSlg").checked = true;
  $("slgTier").value = "22";
  state.slgTouched = true;
  toast("已套用 MOSS 官方采样：50 步 / CFG 3 / 1024² / SLG 22");
};
$("btnEnhance").onclick = enhance;
$("enhUse").onclick = () => {
  $("prompt").value = $("enhancedText").value;
  $("enhanced").hidden = true;
};
$("enhClose").onclick = () => ($("enhanced").hidden = true);

// workflow import
$("btnImportWf").onclick = () => $("wfFile").click();
$("wfFile").onchange = async (e) => {
  const f = e.target.files[0];
  e.target.value = "";
  if (f) await importWorkflow(f);
};

// ref dropzone
const dz = $("dropzone");
dz.onclick = () => $("fileInput").click();
$("fileInput").onchange = async (e) => {
  for (const f of e.target.files) await addRefFile(f);
  e.target.value = "";
};
dz.ondragover = (e) => {
  e.preventDefault();
  dz.classList.add("over");
};
dz.ondragleave = () => dz.classList.remove("over");
dz.ondrop = async (e) => {
  e.preventDefault();
  dz.classList.remove("over");
  for (const f of e.dataTransfer.files) {
    if (f.type.startsWith("image/")) await addRefFile(f);
  }
};

// lightbox
$("lightboxClose").onclick = () => ($("lightbox").hidden = true);
$("lightbox").onclick = (e) => {
  if (e.target.id === "lightbox") $("lightbox").hidden = true;
};
$("lightboxReveal").onclick = () => lightboxItem && invoke("reveal_file", { path: lightboxItem.path });
$("lightboxRef").onclick = async () => {
  if (!lightboxItem) return;
  const dataUrl = lightboxItem.dataUrl || (await invoke("read_output_image", { path: lightboxItem.path }));
  await addRefFromDataUrl(dataUrl, lightboxItem.filename.replace(/\.[^.]+$/, ""));
  $("lightbox").hidden = true;
};

// topbar
$("btnStart").onclick = async () => {
  state.starting = true;
  $("btnStart").disabled = true;
  toast("正在启动 ComfyUI…");
  refreshStatus();
  try {
    await invoke("server_start");
    toast("ComfyUI 已就绪");
  } catch (e) {
    toast(String(e), true);
  } finally {
    state.starting = false;
    refreshStatus();
  }
};
$("btnStop").onclick = async () => {
  try {
    await invoke("server_stop");
    toast("已停止");
  } catch (e) {
    toast(String(e), true);
  }
  refreshStatus();
};
$("btnOpenOut").onclick = async () => {
  const root = state.settings.comfyRoot.replace(/[\\/]+$/, "");
  await invoke("open_folder", { path: `${root}\\ComfyUI\\output` });
};
$("btnClearHistory").onclick = async () => {
  if (!confirm("清空历史记录？（只删列表，不删图片文件）")) return;
  await invoke("history_clear");
  state.results = [];
  renderResults();
  toast("历史已清空");
};

// settings modal
$("btnSettings").onclick = () => ($("settingsModal").hidden = false);
$("btnSettingsCancel").onclick = () => ($("settingsModal").hidden = true);
$("btnSettingsSave").onclick = async () => {
  const next = {
    comfyRoot: $("setRoot").value.trim(),
    port: Number($("setPort").value),
    autoStart: $("setAutoStart").checked,
    stopOnExit: $("setStopOnExit").checked,
    freeEnhance: $("setFreeEnhance").checked,
  };
  try {
    await invoke("settings_set", { settings: next });
    state.settings = await invoke("settings_get");
    $("settingsModal").hidden = true;
    toast("已保存");
    refreshStatus();
  } catch (e) {
    toast(String(e), true);
  }
};

document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key === "Enter") generate();
  if (e.key === "Escape") {
    $("lightbox").hidden = true;
    $("settingsModal").hidden = true;
    $("enhanced").hidden = true;
  }
});

// ---------------------------------------------------------------- batch queue

const queue = { items: [], running: false, stop: false, done: 0 };

function renderQueue() {
  $("queueCount").textContent = queue.items.length ? `${queue.done}/${queue.items.length}` : "0";
  $("btnQueueRun").hidden = queue.running;
  $("btnQueueStop").hidden = !queue.running;
  $("btnQueueRun").disabled = queue.items.length === 0;
  $("queueState").textContent = queue.running ? "运行中…" : "";
}

function queueAdd() {
  const lines = $("queueText")
    .value.split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  if (!lines.length) {
    toast("先写几行提示词", true);
    return;
  }
  queue.items.push(...lines);
  $("queueText").value = "";
  renderQueue();
  toast(`已加入 ${lines.length} 条`);
}

async function queueRun() {
  if (!(await ensureReady("开始队列"))) return;
  if (queue.running || !queue.items.length) return;
  if (!multiStart()) return;
  queue.running = true;
  queue.stop = false;
  queue.done = 0;
  renderQueue();
  const total = queue.items.length;
  while (queue.items.length && !queue.stop) {
    $("prompt").value = queue.items[0];
    const ok = await generate();
    queue.items.shift();
    if (ok) queue.done++;
    renderQueue();
    if (!ok && queue.stop) break;
  }
  queue.running = false;
  queue.stop = false;
  multiEnd();
  renderQueue();
  toast(`队列结束（完成 ${queue.done}/${total}）`);
}

// ---------------------------------------------------------------- image library

let libItems = [];
let libRenderLimit = 120;
let libSel = new Set();
let libPrompts = new Map(); // path -> prompt (from generation history)
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
  if (libUrlCache.has(path)) {
    img.src = libUrlCache.get(path);
    return;
  }
  try {
    const url = await invoke("read_output_image", { path });
    libUrlCache.set(path, url);
    img.src = url;
  } catch {
    img.alt = "?";
  }
}

async function openLibrary() {
  $("library").hidden = false;
  libRenderLimit = 120;
  await loadLibUpscaleModels();
  await loadLibrary();
}

async function loadLibrary() {
  $("libGrid").innerHTML = '<div class="lib-empty">扫描中…</div>';
  try {
    libItems = await invoke("library_scan");
  } catch (e) {
    libItems = [];
    toast(String(e), true);
  }
  try {
    const hist = await invoke("history_list");
    libPrompts = new Map();
    for (const h of hist) for (const im of h.images || []) libPrompts.set(im.path, h.prompt || "");
  } catch {
    libPrompts = new Map();
  }
  libSel.clear();
  renderLibrary();
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
      `<img alt="${it.filename}" data-path="${it.path}">` +
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
      await invoke("library_star", { path: it.path, starred: it.starred });
      e.target.className = "li-star" + (it.starred ? "" : " off");
    };
    img.onclick = () => openViewer(list, list.indexOf(it));
    libObserver.observe(img);
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
  if (!confirm(`删除选中的 ${libSel.size} 张？\n会移到 output\\_comfyshell_trash\\，可手动恢复`)) return;
  try {
    const n = await invoke("library_delete", { paths: [...libSel] });
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
  const url = await invoke("read_output_image", { path });
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

$("btnQueueAdd").onclick = queueAdd;
$("btnQueueRun").onclick = queueRun;
$("btnQueueStop").onclick = () => {
  queue.stop = true;
  invoke("job_cancel");
  toast("队列将在当前这张结束后停止");
};
renderQueue();

$("btnLibrary").onclick = openLibrary;
$("libClose").onclick = () => ($("library").hidden = true);
$("libRefresh").onclick = loadLibrary;
$("libSearch").oninput = renderLibrary;
$("libStarOnly").onchange = renderLibrary;
$("libSort").onchange = renderLibrary;
$("libDelete").onclick = deleteSelected;
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
bindPan("cmpA");
bindPan("cmpB");

// ---------------------------------------------------------------- batch edit from library

const batchEdit = { running: false, stop: false, done: 0, total: 0, items: [] };

function openBatchEdit() {
  const picked = libItems.filter((i) => libSel.has(i.path));
  if (!picked.length) return;
  const p = profile();
  const target = p && p.supportsEdit ? p : state.profiles.find((x) => x.id === "qwen21");
  if (!target) {
    toast("没有支持改图的模型", true);
    return;
  }
  batchEdit.items = picked;
  $("batchEditCount").textContent = `${picked.length} 张`;
  $("batchEditModel").textContent = `模型：${target.label}（不支持改图的模型会自动用 Qwen-Image 2.1）`;
  $("batchEditPrompt").value = $("prompt").value.trim();
  $("batchEditModal").hidden = false;
  $("batchEditPrompt").focus();
}

async function runBatchEdit() {
  const instruction = $("batchEditPrompt").value.trim();
  if (!instruction) {
    toast("写下要改什么", true);
    return;
  }
  if (!multiStart()) return;
  const items = batchEdit.items.slice();
  batchEdit.running = true;
  batchEdit.stop = false;
  batchEdit.done = 0;
  batchEdit.total = items.length;
  $("batchEditStart").disabled = true;
  $("batchEditCancel").textContent = "停止";

  // pick a model that can do edits, keep the current size/seed
  const p = profile();
  const target = p && p.supportsEdit ? p.id : "qwen21";
  const w = Number($("width").value);
  const h = Number($("height").value);
  const seed = Number($("seed").value);
  $("model").value = target;
  $("model").dispatchEvent(new Event("change", { bubbles: true }));
  $("width").value = w;
  $("height").value = h;
  $("seed").value = seed;
  setMode("edit");
  $("libClose").click();

  for (const it of items) {
    if (batchEdit.stop) break;
    try {
      const dataUrl = await libUrl(it.path);
      const name = await invoke("save_ref_image", { name: it.filename, dataUrl });
      state.refs = [name];
      thumbCache.set(name, dataUrl);
      renderRefs();
      $("prompt").value = instruction;
      const ok = await generate();
      if (ok) batchEdit.done++;
    } catch (e) {
      toast(String(e), true);
    }
    $("batchEditCount").textContent = `${batchEdit.done}/${batchEdit.total}`;
  }

  batchEdit.running = false;
  multiEnd();
  $("batchEditStart").disabled = false;
  $("batchEditCancel").textContent = "取消";
  $("batchEditModal").hidden = true;
  toast(`批量改图结束（完成 ${batchEdit.done}/${batchEdit.total}）`);
}

// ---------------------------------------------------------------- model sweep

function buildSweepModels() {
  const box = $("sweepModels");
  box.innerHTML = state.profiles
    .map((p) => `<label><input type="checkbox" value="${p.id}" checked> ${p.label}</label>`)
    .join("");
}

function sweepSelection() {
  return [...$("sweepModels").querySelectorAll("input:checked")].map((x) => x.value);
}

const SWEEP_SIZES = [512, 640, 768, 896, 1024];

async function runSweep() {
  const mode = $("sweepMode").value;
  const sizeMode = $("sweepSize").value;
  const batch = Number($("sweepBatch").value) || 1;
  const items = mode === "presets" ? Object.keys(PORTRAIT_PRESETS) : sweepSelection();
  if (!items.length) {
    toast("至少选一个", true);
    return;
  }
  const prompt = $("prompt").value.trim();
  if (!prompt) {
    toast("先写提示词", true);
    return;
  }
  const w = Number($("width").value);
  const h = Number($("height").value);
  let seed = Number($("seed").value);
  if (seed < 0) seed = Math.floor(Math.random() * 1e9);
  if (!multiStart()) return;

  const sizes = sizeMode === "five" ? SWEEP_SIZES.map((s) => [s, s]) : [[w, h]];
  const total = items.length * sizes.length;
  const btn = $("btnSweepRun");
  btn.disabled = true;
  let done = 0;
  for (const item of items) {
    if (mode === "presets") {
      applyPortraitPreset(item, true);
    } else {
      $("model").value = item;
      $("model").dispatchEvent(new Event("change", { bubbles: true }));
    }
    // a sweep is text-to-image by definition: drop edit leftovers for a fair comparison
    setMode("t2i");
    state.refs = [];
    renderRefs();
    $("turbo").checked = false;
    for (const [sw, sh] of sizes) {
      $("sweepState").textContent = `${done}/${total} · ${sw}px · ×${batch}`;
      $("width").value = sw;
      $("height").value = sh;
      $("seed").value = seed;
      $("batch").value = batch;
      $("prompt").value = prompt;
      const ok = await generate();
      if (ok) done++;
    }
  }
  $("sweepState").textContent = `完成 ${done}/${total}`;
  btn.disabled = false;
  multiEnd();
  toast(`横扫结束：${done}/${total} 组（seed ${seed}，每组 ×${batch}）`);
}

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

    const dataUrl = canvas.toDataURL("image/png");
    const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
    const path = await invoke("save_export", { name: `compare_${stamp}.png`, dataUrl });
    toast(`已导出：${path}`);
    libUrlCache.set(path, dataUrl);
    libItems = await invoke("library_scan");
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- wiring (v0.4)

$("libEdit").onclick = openBatchEdit;
$("batchEditCancel").onclick = () => {
  if (batchEdit.running) {
    batchEdit.stop = true;
    invoke("job_cancel");
    return;
  }
  $("batchEditModal").hidden = true;
};
$("batchEditStart").onclick = runBatchEdit;

$("btnSweepRun").onclick = runSweep;
$("cmpExport").onclick = exportCompare;


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
    const list = await invoke("upscale_models_list");
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

async function viewerUpscale() {
  const it = viewer.list[viewer.idx];
  if (!it) return;
  const btn = $("vwUpscale");
  btn.disabled = true;
  btn.textContent = "超分中…";
  try {
    const img = await invoke("upscale_image", { path: it.path, model: $("vwUpModel").value || null });
    toast(`超分完成：${img.width}×${img.height}`);
    await loadLibrary();
    const list = sortedLibrary();
    const idx = list.findIndex((x) => x.path === img.path);
    if (idx >= 0) {
      viewer.list = list;
      viewer.idx = idx;
      await showViewerItem();
    }
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "超分 4×";
  }
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
    const en = await invoke("translate_text", { text: q });
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
$("vwUpscale").onclick = viewerUpscale;
$("vwReveal").onclick = () => {
  const it = viewer.list[viewer.idx];
  if (it) invoke("reveal_file", { path: it.path });
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
  let sx = 0;
  let sy = 0;
  stage.addEventListener("pointerdown", (e) => {
    dragging = true;
    sx = e.clientX - viewer.pan.x;
    sy = e.clientY - viewer.pan.y;
    stage.classList.add("dragging");
    stage.setPointerCapture(e.pointerId);
  });
  stage.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    viewer.pan = { x: e.clientX - sx, y: e.clientY - sy };
    applyViewer();
  });
  stage.addEventListener("pointerup", () => {
    dragging = false;
    stage.classList.remove("dragging");
  });
  stage.addEventListener("pointercancel", () => {
    dragging = false;
    stage.classList.remove("dragging");
  });
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

$("libTranslate").onclick = translateSearch;
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
      refImageDataUrl = await invoke("read_input_image", { name: state.refs[0] });
    } catch {}
  }
  const btn = $("inpaintGo");
  btn.disabled = true;
  btn.textContent = "重绘中…";
  try {
    const res = await invoke("inpaint_image", {
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


// ---------------------------------------------------------------- batch upscale / external folder / training

async function loadLibUpscaleModels() {
  try {
    const list = await invoke("upscale_models_list");
    const sel = $("libUpModel");
    sel.innerHTML =
      `<option value="__seedvr2__">SeedVR2 3B · 4×</option>` +
      list.map((m) => `<option value="${m}">${m.replace(/\.(pth|safetensors|pt)$/i, "")}</option>`).join("");
  } catch {}
}

async function batchUpscale() {
  const picked = libItems.filter((i) => libSel.has(i.path));
  if (!picked.length) {
    toast("先勾选要超分的图", true);
    return;
  }
  const model = $("libUpModel").value;
  let done = 0;
  for (const it of picked) {
    try {
      toast(`批量超分 ${done + 1}/${picked.length}…`);
      await invoke("upscale_image", { path: it.path, model });
      done++;
    } catch (e) {
      toast(String(e), true);
    }
  }
  await loadLibrary();
  toast(`批量超分完成 ${done}/${picked.length}`);
}

async function externalUpscale(files) {
  const imgs = [...files].filter((f) => /\.(png|jpe?g|webp)$/i.test(f.name));
  if (!imgs.length) {
    toast("这个文件夹里没有图片", true);
    return;
  }
  const model = $("libUpModel").value;
  let done = 0;
  for (const f of imgs) {
    try {
      toast(`外部超分 ${done + 1}/${imgs.length}：${f.name}`);
      const dataUrl = await new Promise((res) => {
        const r = new FileReader();
        r.onload = () => res(r.result);
        r.readAsDataURL(f);
      });
      const name = await invoke("save_ref_image", { name: f.name, dataUrl });
      await invoke("upscale_input", { name, model });
      done++;
    } catch (e) {
      toast(`${f.name}: ${e}`, true);
    }
  }
  await loadLibrary();
  toast(`外部文件夹超分完成 ${done}/${imgs.length}`);
}

const trainState = { src: "library", files: [], busy: false };

async function trainingOpen() {
  const picked = libItems.filter((i) => libSel.has(i.path));
  trainState.files = [];
  $("trainFolder").value = "";
  $("trainResult").hidden = true;
  $("trainLoss").removeAttribute("src");
  $("trainGo").hidden = false;
  $("trainCancel").textContent = "取消";
  $("trainBar").style.width = "0%";
  $("trainProgressText").textContent = "";
  document.querySelector('input[name="trainSrc"][value="library"]').checked = true;
  trainState.src = "library";
  const ex = $("trainExisting");
  ex.innerHTML = `<option value="">（从零开始）</option>` +
    (state.models.loras || []).map((l) => `<option value="${l}">${l}</option>`).join("");
  ex.value = "";
  $("trainCount").textContent = `${picked.length} 张`;
  $("trainHint").textContent =
    "图库模式用每张图的生成提示词当 caption；文件夹模式自动缩放到所选尺寸（中心裁切或等比镜像填充）并用下面的 caption。训练需壳托管的 ComfyUI（已关 pinned memory）。";
  if (!$("trainName").value) {
    $("trainName").value = "lora_" + Date.now().toString(36).slice(-4);
  }
  $("trainModal").hidden = false;
  $("trainName").focus();
}

const trainAnn = [];

function buildAnnRows() {
  const box = $("annList");
  if (!box) return;
  box.innerHTML = "";
  trainAnn.length = 0;
  if (trainState.src !== "folder" || !trainState.files.length) {
    $("annRow").hidden = true;
    return;
  }
  $("annRow").hidden = false;
  trainState.files.forEach((f, i) => {
    const row = document.createElement("div");
    row.className = "ann-row";
    const img = document.createElement("img");
    img.src = URL.createObjectURL(f);
    const want = document.createElement("input");
    want.placeholder = "想要：如 red coat, silver hair";
    const avoid = document.createElement("input");
    avoid.placeholder = "不要：如 background clutter";
    const weight = document.createElement("select");
    weight.innerHTML = `<option value="1">×1</option><option value="2">×2</option><option value="3">×3</option>`;
    const cropBtn = document.createElement("button");
    cropBtn.type = "button";
    cropBtn.className = "btn ghost small";
    cropBtn.textContent = "框";
    cropBtn.title = "在图上标绿（保留）/ 红（抹掉）";
    const entry = { want, avoid, weight, crop: null, erases: [], cropBtn };
    cropBtn.onclick = () => openMark(f, entry);
    img.style.cursor = "zoom-in";
    img.onclick = () => openMark(f, entry);
    row.append(img, want, avoid, weight, cropBtn);
    box.appendChild(row);
    trainAnn.push(entry);
  });
}

function trainRefreshSource() {
  buildAnnRows();
  const folder = trainState.src === "folder";
  $("trainCaptionRow").hidden = !folder;
  $("trainCount").textContent = folder
    ? `${trainState.files.length} 张`
    : `${libItems.filter((i) => libSel.has(i.path)).length} 张`;
}

async function readFileDataUrl(f) {
  return new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(r.result);
    r.onerror = rej;
    r.readAsDataURL(f);
  });
}

const training = { busy: false };

async function trainingStart() {
  if (trainState.busy) return;
  const simple = document.body.classList.contains("mode-simple");
  if (simple) {
    const k = simpleTrainKnobs();
    $("trainSize").value = String(k.size);
    $("trainRank").value = String(k.rank);
    $("trainSteps").value = String(k.steps);
    const pid = profile()?.id || "flux3base";
    $("trainBase").value = pid.startsWith("moss") ? "moss" : "flux3base";
    $("trainEval").value = "0";
    $("trainRestart").checked = true;
  }
  const picked = libItems.filter((i) => libSel.has(i.path));
  const name = $("trainName").value.trim();
  const base = $("trainBase").value;
  if (!/^[A-Za-z0-9_-]{2,40}$/.test(name)) {
    toast("名字只能用字母/数字/下划线，2-40 字", true);
    return;
  }
  if (trainState.src === "library" && !picked.length) {
    toast("先勾选训练用的图", true);
    return;
  }
  if (trainState.src === "folder" && !trainState.files.length) {
    toast("先选一个文件夹", true);
    return;
  }
  trainState.busy = true;
  $("trainGo").disabled = true;
  $("trainGo").textContent = "训练中…";
  $("trainBar").style.width = "0%";
  $("trainProgressText").textContent = "准备数据…";
  const t0 = Date.now();
  const timer = setInterval(() => {
    if (!$("trainProgressText").textContent.includes("/")) {
      $("trainProgressText").textContent = `训练中 ${Math.round((Date.now() - t0) / 1000)}s`;
    }
  }, 1000);
  try {
    let res;
    const engine = $("trainEngine") ? $("trainEngine").value : "auto";
    const useKohya = engine === "kohya" || (engine === "auto" && base.startsWith("moss"));
    if (useKohya && trainState.src !== "folder") {
      toast("kohya 引擎只支持「外部文件夹」数据源", true);
      return;
    }
    if (trainState.src === "folder") {
      const files = [];
      for (const f of trainState.files) {
        files.push({ name: f.name, dataUrl: await readFileDataUrl(f) });
      }
      const filesPayload = {
        name,
        base,
        steps: Number($("trainSteps").value),
        rank: Number($("trainRank").value),
        lr: Number($("trainLr").value),
        files,
        caption: $("trainCaption").value,
        autoCaption: $("trainAutoCaption").checked,
        existingLora: $("trainExisting").value || null,
        size: Number($("trainSize").value),
        keepAspect: $("trainAspect").value === "keep",
        annotations: trainAnn.map((a) => ({
          want: a.want.value,
          avoid: a.avoid.value,
          weight: Number(a.weight.value),
          crop: a.crop || null,
          erases: a.erases || [],
        })),
        evalPrompts: Number($("trainEval").value) || 0,
        batch: Number($("trainBatch").value) || 1,
        gradAccum: Number($("trainGrad").value) || 1,
        depth: Number($("trainDepth").value) || 3,
        offload: $("trainOffload").checked,
        dtype: $("trainDtype").value,
        restart: $("trainRestart").checked,
      };
      res = await invoke(useKohya ? "train_kohya_files" : "train_lora_files", { req: filesPayload });
      res = await invoke("train_lora", {
        req: {
          name,
          base,
          steps: Number($("trainSteps").value),
          rank: Number($("trainRank").value),
          lr: Number($("trainLr").value),
          images: picked.map((p) => p.path),
          captions: picked.map((p) => libPrompts.get(p.path) || ""),
          existingLora: $("trainExisting").value || null,
          size: Number($("trainSize").value),
          keepAspect: $("trainAspect").value === "keep",
          annotations: trainAnn.map((a) => ({
            want: a.want.value,
            avoid: a.avoid.value,
            weight: Number(a.weight.value),
            crop: a.crop || null,
            erases: a.erases || [],
          })),
          evalPrompts: Number($("trainEval").value) || 0,
          batch: Number($("trainBatch").value) || 1,
          gradAccum: Number($("trainGrad").value) || 1,
          depth: Number($("trainDepth").value) || 3,
          offload: $("trainOffload").checked,
          dtype: $("trainDtype").value,
          restart: $("trainRestart").checked,
        },
      });
    }
    // register in the production list and switch to it
    state.models = await invoke("models_list");
    const loraSel = $("lora");
    loraSel.innerHTML =
      `<option value="">（无）</option>` +
      state.models.loras.map((l) => `<option value="${l}">${l}</option>`).join("");
    if (![...loraSel.options].some((o) => o.value === res.lora)) {
      const opt = document.createElement("option");
      opt.value = res.lora;
      opt.textContent = res.lora;
      loraSel.appendChild(opt);
    }
    loraSel.value = res.lora;
    $("trainBar").style.width = "100%";
    $("trainProgressText").textContent = `完成 · ${res.lora}`;
    if (res.lossImage) {
      $("trainLoss").src = res.lossImage;
      $("trainResult").hidden = false;
    }
    const grid = $("trainEvalGrid");
    grid.innerHTML = "";
    const shots = res.evalImages || [];
    $("trainEvalWrap").hidden = shots.length === 0;
    for (const path of shots) {
      const img = document.createElement("img");
      img.alt = "eval";
      grid.appendChild(img);
      invoke("read_output_image", { path }).then((url) => (img.src = url)).catch(() => {});
    }
    if (res.avoid) toast(`建议负面：${res.avoid}`);
    $("trainGo").hidden = true;
    $("trainCancel").textContent = "关闭";
    toast(`训练完成：${res.lora}（已切到该 LoRA）`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    clearInterval(timer);
    trainState.busy = false;
    $("trainGo").disabled = false;
    $("trainGo").textContent = "开始训练";
  }
}

$("libBatchUp").onclick = batchUpscale;
$("libExternal").onclick = () => $("extFolder").click();
$("extFolder").onchange = async (e) => {
  const files = e.target.files;
  e.target.value = "";
  if (files && files.length) await externalUpscale(files);
};
$("libTrain").onclick = trainingOpen;
$("trainExisting").onchange = () => {
  const v = $("trainExisting").value;
  if (v) {
    const base = v.replace(/\.safetensors$/i, "");
    $("trainName").value = `${base}_v2`;
  }
};
$("trainBase").onchange = () => {
  if ($("trainBase").value === "moss") {
    $("trainHint").textContent =
      "MOSS bf16（9.5GB）在本机 16G 卡上实测会 OOM：开 offload 报 aimdo 错误，不开直接崩。建议用 FLUX.3 Base；MOSS 需要 24G 以上。";
  } else {
    $("trainHint").textContent =
      "图库模式用每张图的生成提示词当 caption；文件夹模式自动缩放到所选尺寸（中心裁切或等比镜像填充）并用下面的 caption。训练需壳托管的 ComfyUI（已关 pinned memory）。";
  }
};
$("trainCancel").onclick = () => ($("trainModal").hidden = true);
$("trainGo").onclick = trainingStart;


for (const r of document.querySelectorAll('input[name="trainSrc"]')) {
  r.onchange = () => {
    trainState.src = r.value;
    trainRefreshSource();
  };
}
$("trainFolderPick").onclick = () => $("trainFolder").click();
$("trainFolder").onchange = (e) => {
  trainState.files = [...e.target.files].filter((f) => /\.(png|jpe?g|webp)$/i.test(f.name));
  if (trainState.src !== "folder") {
    document.querySelector('input[name="trainSrc"][value="folder"]').checked = true;
    trainState.src = "folder";
  }
  trainRefreshSource();
};
listen("caption-progress", (e) => {
  const { done, total } = e.payload || {};
  if (!total) return;
  $("trainProgressText").textContent = `自动 caption ${done} / ${total}`;
}).catch(() => {});

listen("train-progress", (e) => {
  const { value, max } = e.payload || {};
  if (!max) return;
  $("trainBar").style.width = `${Math.min(100, (value / max) * 100)}%`;
  $("trainProgressText").textContent = `${value} / ${max} 步`;
}).catch(() => {});

// quick step presets (chips) for the main panel and the inpaint dialog
const STEP_PRESETS = [30, 36, 42, 49, 25, 50];

function syncStepPresets(container, input, values) {
  const current = Number(input.value);
  for (const b of container.querySelectorAll("button")) {
    b.classList.toggle("on", Number(b.dataset.steps) === current);
  }
}

function buildStepPresets(container, input, values = STEP_PRESETS) {
  container.innerHTML = "";
  for (const v of values) {
    const b = document.createElement("button");
    b.type = "button";
    b.dataset.steps = String(v);
    b.textContent = String(v);
    b.onclick = () => {
      input.value = String(v);
      input.dispatchEvent(new Event("input", { bubbles: true }));
      syncStepPresets(container, input);
    };
    container.appendChild(b);
  }
  input.addEventListener("input", () => syncStepPresets(container, input));
  syncStepPresets(container, input);
}

buildStepPresets($("stepPresets"), $("steps"));
buildStepPresets($("inpaintStepPresets"), $("inpaintSteps"), [20, 25, 30, 36, 42, 49]);

// training resource presets: size / rank / depth / steps / offload
const TRAIN_PRESETS = [
  { label: "低配 384", size: 384, rank: 8, depth: 3, steps: 120, offload: true },
  { label: "标准 512", size: 512, rank: 16, depth: 3, steps: 200, offload: true },
  { label: "细节 640", size: 640, rank: 16, depth: 3, steps: 200, offload: true },
];
(() => {
  const box = $("trainPresets");
  if (!box) return;
  for (const p of TRAIN_PRESETS) {
    const b = document.createElement("button");
    b.type = "button";
    b.textContent = p.label;
    b.onclick = () => {
      $("trainSize").value = String(p.size);
      $("trainRank").value = String(p.rank);
      $("trainDepth").value = String(p.depth);
      if ($("trainRestart")) $("trainRestart").checked = true;
      $("trainSteps").value = String(p.steps);
      $("trainOffload").checked = p.offload;
      toast(`已套用 ${p.label}`);
    };
    box.appendChild(b);
  }
})();


// ---------------------------------------------------------------- portrait presets
const PORTRAIT_PRESETS = {
  real: {
    model: "mossimagine",
    lora: "ultra_real_moss_v2.safetensors",
    strength: 0.7,
    lora2: "",
    lora2Strength: 0.8,
    steps: 50,
    cfg: 3,
    slg: true,
  },
  natural: {
    model: "moss",
    lora: "ultra_real_moss_v2.safetensors",
    strength: 0.7,
    lora2: "MOSS-Image-Beauty-LoRA.safetensors",
    lora2Strength: 0.5,
    steps: 50,
    cfg: 3,
    slg: true,
  },
  raw: {
    model: "mossbase",
    lora: "ultra_real_moss_v2.safetensors",
    strength: 0.7,
    lora2: "",
    lora2Strength: 0.8,
    steps: 50,
    cfg: 3,
    slg: true,
  },
  polished: {
    model: "moss",
    lora: "MOSS-Image-Beauty-LoRA.safetensors",
    strength: 0.7,
    lora2: "",
    lora2Strength: 0.8,
    steps: 50,
    cfg: 3,
    slg: true,
  },
};

function applyPortraitPreset(key, silent) {
  const preset = PORTRAIT_PRESETS[key];
  if (!preset) return false;
  $("model").value = preset.model;
  $("model").dispatchEvent(new Event("change"));
  $("lora").value = preset.lora;
  $("lora").dispatchEvent(new Event("change"));
  $("loraStrength").value = String(preset.strength);
  $("loraStrengthText").textContent = Number(preset.strength).toFixed(2);
  $("lora2").value = preset.lora2 || "";
  $("lora2").dispatchEvent(new Event("change"));
  $("lora2Strength").value = String(preset.lora2Strength);
  $("lora2StrengthText").textContent = Number(preset.lora2Strength).toFixed(2);
  $("steps").value = String(preset.steps);
  $("cfg").value = String(preset.cfg);
  $("mossSlg").checked = !!preset.slg;
  state.slgTouched = true;
  $("presetSel").value = key;
  if (!silent) toast(`已套用预设：${$("presetSel").selectedOptions[0].textContent}`);
  return true;
}

$("presetSel").onchange = () => applyPortraitPreset($("presetSel").value, false);

// ---------------------------------------------------------------- local assistant
$("btnAssist").onclick = () => {
  $("assistModal").hidden = false;
};
$("assistClose").onclick = () => ($("assistModal").hidden = true);
$("assistFill").onclick = () => {
  const text = $("assistOut").value.trim();
  if (!text) return toast("还没有结果", true);
  $("prompt").value = text;
  $("assistModal").hidden = true;
  toast("已填入提示词");
};
$("assistRun").onclick = async () => {
  const request = $("assistInput").value.trim();
  if (!request) return toast("先写需求", true);
  const btn = $("assistRun");
  btn.disabled = true;
  $("assistState").textContent = "思考中…";
  $("assistSteps").innerHTML = "";
  try {
    const res = await invoke("assist", { request, image: state.refs[0] || null });
    const box = $("assistSteps");
    for (const s of res.steps || []) {
      const d = document.createElement("div");
      d.className = "astep";
      d.innerHTML = `<b>${s.tool}</b>${s.output}`;
      box.appendChild(d);
    }
    if (res.note) {
      const d = document.createElement("div");
      d.className = "astep";
      d.innerHTML = `<b>note</b>${res.note}`;
      box.appendChild(d);
    }
    $("assistOut").value = res.prompt || "";
    $("assistState").textContent = `完成 · ${(res.steps || []).length} 步`;
  } catch (e) {
    $("assistState").textContent = "";
    toast(String(e), true);
  } finally {
    btn.disabled = false;
  }
};

// ---------------------------------------------------------------- MOSS layout editor
let layoutItems = [];
let layoutSel = -1;
let layoutDrag = null;


const LAYOUT_PRESETS = {
  ecom_main: [
    { type: "object", desc: "main product, centered on clean background", bbox: [0.2, 0.12, 0.6, 0.56] },
    { type: "text", desc: "product name, bold", text: "PRODUCT NAME", bbox: [0.1, 0.74, 0.8, 0.09] },
    { type: "text", desc: "one selling point line", text: "key selling point", bbox: [0.12, 0.86, 0.76, 0.07] },
  ],
  ecom_poster: [
    { type: "text", desc: "big headline", text: "SUMMER SALE", bbox: [0.06, 0.05, 0.88, 0.12] },
    { type: "object", desc: "product hero shot", bbox: [0.24, 0.24, 0.52, 0.48] },
    { type: "text", desc: "price tag", text: "¥199", bbox: [0.56, 0.78, 0.34, 0.09] },
  ],
  left_right: [
    { type: "object", desc: "product on the left", bbox: [0.05, 0.15, 0.5, 0.7] },
    { type: "text", desc: "headline on the right", text: "HEADLINE", bbox: [0.6, 0.2, 0.36, 0.12] },
    { type: "text", desc: "body copy on the right", text: "short body copy", bbox: [0.6, 0.4, 0.36, 0.28] },
  ],
  three_rows: [
    { type: "object", desc: "top panel scene", bbox: [0.05, 0.05, 0.9, 0.28] },
    { type: "object", desc: "middle panel scene", bbox: [0.05, 0.36, 0.9, 0.28] },
    { type: "object", desc: "bottom panel scene", bbox: [0.05, 0.67, 0.9, 0.28] },
  ],
  grid4: [
    { type: "object", desc: "top-left panel", bbox: [0.04, 0.04, 0.45, 0.45] },
    { type: "object", desc: "top-right panel", bbox: [0.51, 0.04, 0.45, 0.45] },
    { type: "object", desc: "bottom-left panel", bbox: [0.04, 0.51, 0.45, 0.45] },
    { type: "object", desc: "bottom-right panel", bbox: [0.51, 0.51, 0.45, 0.45] },
  ],
};

$("layoutPreset").onchange = () => {
  const key = $("layoutPreset").value;
  if (!key) return;
  layoutItems = LAYOUT_PRESETS[key].map((x) => ({ ...x, bbox: [...x.bbox] }));
  layoutSel = 0;
  layoutRender();
};

function layoutOpen() {
  if (!layoutItems.length) layoutAdd("text");
  $("layoutModal").hidden = false;
  layoutRender();
}

function layoutAdd(type) {
  const box = type === "text"
    ? { type: "text", desc: "large title text", text: "TEXT", bbox: [0.1, 0.08, 0.8, 0.14] }
    : { type: "object", desc: "an object", text: "", bbox: [0.35, 0.45, 0.3, 0.3] };
  layoutItems.push(box);
  layoutSel = layoutItems.length - 1;
  layoutRender();
}

function layoutRender() {
  const boxes = $("layoutBoxes");
  boxes.innerHTML = "";
  layoutItems.forEach((it, i) => {
    const d = document.createElement("div");
    d.className = "lbox" + (i === layoutSel ? " sel" : "");
    d.style.left = `${it.bbox[0] * 100}%`;
    d.style.top = `${it.bbox[1] * 100}%`;
    d.style.width = `${it.bbox[2] * 100}%`;
    d.style.height = `${it.bbox[3] * 100}%`;
    d.innerHTML = `<span>${it.type === "text" ? "文字" : "物体"}${i + 1}</span>` +
      ["nw", "ne", "sw", "se"].map((c) => `<i data-corner="${c}" style="position:absolute;width:10px;height:10px;background:#5d7bf0;border-radius:2px;${c.includes("n") ? "top:-5px" : "bottom:-5px"};${c.includes("w") ? "left:-5px" : "right:-5px"};cursor:${c === "nw" || c === "se" ? "nwse" : "nesw"}-resize"></i>`).join("");
    d.onpointerdown = (e) => {
      e.stopPropagation();
      layoutSel = i;
      const corner = e.target.dataset && e.target.dataset.corner;
      layoutDrag = corner
        ? { mode: "resize", i, corner, bbox: [...it.bbox] }
        : { mode: "move", i, ox: e.clientX, oy: e.clientY, bbox: [...it.bbox] };
      layoutRender();
    };
    boxes.appendChild(d);
  });

  const list = $("layoutList");
  list.innerHTML = "";
  layoutItems.forEach((it, i) => {
    const row = document.createElement("div");
    row.className = "lrow";
    row.style.background = i === layoutSel ? "#1b2130" : "transparent";
    row.style.borderRadius = "6px";
    row.style.padding = "2px";
    const tag = document.createElement("span");
    tag.className = "tag";
    tag.textContent = it.type === "text" ? "文字" : "物体";
    const desc = document.createElement("input");
    desc.value = it.desc;
    desc.placeholder = "描述/文字内容";
    desc.oninput = () => { it.desc = desc.value; };
    if (it.type === "text") {
      const txt = document.createElement("input");
      txt.value = it.text;
      txt.placeholder = "要渲染的文字";
      txt.oninput = () => { it.text = txt.value; };
      row.append(tag, wrapTwo(desc, txt), delBtn(i));
    } else {
      row.append(tag, desc, delBtn(i));
    }
    row.onclick = () => { layoutSel = i; layoutRender(); };
    list.appendChild(row);
  });
  $("layoutHint").textContent = `${layoutItems.length} 个元素 · 在画布上拖出方框`;
}

function wrapTwo(a, b) {
  const div = document.createElement("div");
  div.style.display = "flex";
  div.style.gap = "4px";
  div.style.flexDirection = "column";
  div.append(a, b);
  return div;
}

function delBtn(i) {
  const b = document.createElement("button");
  b.textContent = "×";
  b.className = "btn small ghost";
  b.onclick = (e) => {
    e.stopPropagation();
    layoutItems.splice(i, 1);
    if (layoutSel >= layoutItems.length) layoutSel = layoutItems.length - 1;
    layoutRender();
  };
  return b;
}

(() => {
  const stage = $("layoutStage");
  stage.addEventListener("pointerdown", (e) => {
    if (e.target.closest(".lbox")) return;
    const r = stage.getBoundingClientRect();
    layoutDrag = { mode: "new", sx: (e.clientX - r.left) / r.width, sy: (e.clientY - r.top) / r.height };
    stage.setPointerCapture(e.pointerId);
  });
  stage.addEventListener("pointermove", (e) => {
    if (!layoutDrag) return;
    const r = stage.getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const y = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    if (layoutDrag.mode === "new") {
      if (layoutSel < 0) return;
      layoutItems[layoutSel].bbox = [
        Number(Math.min(layoutDrag.sx, x).toFixed(4)),
        Number(Math.min(layoutDrag.sy, y).toFixed(4)),
        Number(Math.abs(x - layoutDrag.sx).toFixed(4)),
        Number(Math.abs(y - layoutDrag.sy).toFixed(4)),
      ];
      layoutRender();
    } else if (layoutDrag.mode === "resize") {
      const b = [...layoutDrag.bbox];
      const c = layoutDrag.corner;
      if (c.includes("e")) b[2] = Math.max(0.03, x - b[0]);
      if (c.includes("s")) b[3] = Math.max(0.03, y - b[1]);
      if (c.includes("w")) { const nx = Math.min(x, b[0] + b[2] - 0.03); b[2] = b[0] + b[2] - nx; b[0] = nx; }
      if (c.includes("n")) { const ny = Math.min(y, b[1] + b[3] - 0.03); b[3] = b[1] + b[3] - ny; b[1] = ny; }
      layoutItems[layoutDrag.i].bbox = b.map((v) => Number(Math.max(0, Math.min(1, v)).toFixed(4)));
      layoutRender();
    } else {
      const dx = (e.clientX - layoutDrag.ox) / r.width;
      const dy = (e.clientY - layoutDrag.oy) / r.height;
      const b = layoutDrag.bbox;
      layoutItems[layoutDrag.i].bbox = [
        Number(Math.min(0.99, Math.max(0, b[0] + dx)).toFixed(4)),
        Number(Math.min(0.99, Math.max(0, b[1] + dy)).toFixed(4)),
        b[2], b[3],
      ];
      layoutRender();
    }
  });
  stage.addEventListener("pointerup", () => (layoutDrag = null));
})();

function buildLayoutJson() {
  const w = Number($("width").value);
  const h = Number($("height").value);
  const summary = ($("layoutSummary").value || $("prompt").value || "").trim();
  const items = [];
  for (const it of layoutItems) {
    const x0 = it.bbox[0] * 1000, y0 = it.bbox[1] * 1000;
    const x1 = (it.bbox[0] + it.bbox[2]) * 1000, y1 = (it.bbox[1] + it.bbox[3]) * 1000;
    const box = [
      Math.floor((y0 * h) / 1000) - Math.floor(h / 2),
      Math.floor((x0 * w) / 1000) - Math.floor(w / 2),
      Math.ceil((y1 * h) / 1000) - Math.floor(h / 2),
      Math.ceil((x1 * w) / 1000) - Math.floor(w / 2),
    ];
    const item = { label: it.type === "text" ? "text" : "object", caption: it.desc || "", bbox: box };
    if (it.type === "text") item.text = it.text || "";
    items.push(item);
  }
  return JSON.stringify({
    summary: { caption: summary },
    items,
    bbox_coord_space: "abs_centralized",
    bbox_format: "yxyx",
  });
}

$("btnLayout").onclick = () => { $("anaModal").hidden = true; $("poseModal").hidden = true; layoutOpen(); };
$("layoutClose").onclick = () => ($("layoutModal").hidden = true);
$("layoutAddText").onclick = () => layoutAdd("text");
$("layoutAddObj").onclick = () => layoutAdd("object");
$("layoutRun").onclick = async () => {
  const summary = ($("layoutSummary").value || $("prompt").value || "").trim();
  if (!summary) return toast("先写整张图的描述", true);
  state.pendingLayout = buildLayoutJson();
  if (!$("prompt").value.trim()) $("prompt").value = summary;
  $("layoutModal").hidden = true;
  await generate();
};

// ---------------------------------------------------------------- image analyzer

const ANA_FIELDS = [
  ["pose", "姿势"],
  ["expression", "表情"],
  ["clothing", "衣着"],
  ["accessories", "配饰"],
  ["background", "背景"],
  ["style", "风格"],
  ["colors", "色彩"],
  ["lighting", "光线"],
  ["mood", "氛围"],
];
let anaDataUrl = "";

function buildAnaFields() {
  const box = $("anaFields");
  box.innerHTML = "";
  for (const [key, label] of ANA_FIELDS) {
    const row = document.createElement("label");
    row.className = "ana-row";
    row.innerHTML = `<span>${label}</span><input id="ana_${key}" placeholder="" />`;
    box.appendChild(row);
  }
}

async function analyzePicked(file) {
  if (!file) return;
  anaDataUrl = await readFileDataUrl(file);
  $("anaPreview").src = anaDataUrl;
  $("anaRun").disabled = false;
  $("anaState").textContent = "";
}

let anaExtras = [];

function renderExtras(list) {
  anaExtras = (list || []).map((x) => ({ label: x.label || "", value: x.value || "" }));
  paintExtras();
}

function paintExtras() {
  const box = $("anaExtras");
  box.innerHTML = "";
  anaExtras.forEach((ex, i) => {
    const row = document.createElement("div");
    row.className = "xrow";
    const label = document.createElement("input");
    label.value = ex.label;
    label.placeholder = "分类（如 weather）";
    label.oninput = () => (ex.label = label.value);
    const value = document.createElement("input");
    value.value = ex.value;
    value.placeholder = "内容（如 light rain）";
    value.oninput = () => (ex.value = value.value);
    const del = document.createElement("button");
    del.className = "btn small ghost";
    del.textContent = "×";
    del.onclick = () => {
      anaExtras.splice(i, 1);
      paintExtras();
    };
    row.append(label, value, del);
    box.appendChild(row);
  });
}

$("anaAddExtra").onclick = () => {
  anaExtras.push({ label: "", value: "" });
  paintExtras();
};

async function runAnalyze() {
  if (!$("ana_pose")) buildAnaFields();
  if (!anaDataUrl) return toast("先选一张图", true);
  const btn = $("anaRun");
  btn.disabled = true;
  $("anaState").textContent = "解析中…（首次加载视觉模型约 20–60 秒）";
  try {
    const name = await invoke("save_ref_image", { name: "ana_input.png", dataUrl: anaDataUrl });
    const raw = await invoke("analyze_image", { image: name, twoPass: $("anaTwoPass").checked });
    $("anaRaw").value = raw;
    let data = {};
    try {
      data = JSON.parse(raw);
    } catch {
      $("anaState").textContent = "模型没返回标准 JSON，看下面原始输出";
    }
    for (const [key] of ANA_FIELDS) {
      const el = $("ana_" + key);
      if (el) el.value = data[key] || "";
    }
    renderExtras(data.extras || []);
    $("anaPrompt").value = data.prompt || "";
    $("anaRawBox").open = !Object.keys(data).length;
    $("anaState").textContent = Object.keys(data).length ? "解析完成" : "解析完成（格式异常）";
  } catch (e) {
    $("anaState").textContent = "";
    toast(String(e), true);
  } finally {
    btn.disabled = false;
  }
}

$("btnAnalyze").onclick = () => {
  $("layoutModal").hidden = true; $("poseModal").hidden = true;
  buildAnaFields();
  $("anaModal").hidden = false;
};
$("anaClose").onclick = () => ($("anaModal").hidden = true);
$("anaPick").onclick = () => $("anaFile").click();
$("anaFile").onchange = (e) => analyzePicked(e.target.files[0]);
$("anaRun").onclick = runAnalyze;
$("anaFillPrompt").onclick = () => {
  const parts = ANA_FIELDS.map(([k]) => ($("ana_" + k)?.value || "").trim()).filter(Boolean);
  for (const ex of anaExtras) {
    const v = (ex.value || "").trim();
    if (v) parts.push(v);
  }
  if (!parts.length) return toast("还没有解析结果", true);
  const text = parts.join(", ");
  $("prompt").value = text;
  $("anaModal").hidden = true;
  toast("已填入提示词");
};
$("anaCopy").onclick = async () => {
  const raw = $("anaRaw").value.trim();
  if (!raw) return toast("还没有解析结果", true);
  await navigator.clipboard.writeText(raw);
  toast("已复制 JSON");
};

// ---------------------------------------------------------------- pose library
let poseCatalog = null;
let poseShown = 0;
const poseCache = new Map();

async function openPoses() {
  if (!poseCatalog) {
    try {
      poseCatalog = JSON.parse(await invoke("poses_catalog"));
    } catch (e) {
      toast(String(e), true);
      return;
    }
    const classes = [...new Set(poseCatalog.map((p) => p.class_cn).filter(Boolean))].sort();
    $("poseClass").innerHTML =
      `<option value="">全部类别</option>` + classes.map((c) => `<option value="${c}">${c}</option>`).join("");
  }
  poseShown = 0;
  $("poseGrid").innerHTML = "";
  $("poseModal").hidden = false;
  renderPoses();
}

function poseFiltered() {
  const q = $("poseSearch").value.trim().toLowerCase();
  const cls = $("poseClass").value;
  const angle = $("poseAngle").value;
  return poseCatalog.filter((p) => {
    if (cls && p.class_cn !== cls) return false;
    // entries without this camera angle would show a broken/duplicated tile
    if (!(p.views || {})[angle]) return false;
    if (!q) return true;
    const hay = `${p.id} ${p.name_cn || ""} ${p.name_en || ""} ${p.class_cn || ""} ${p.desc_en || ""}`.toLowerCase();
    return hay.includes(q);
  });
}

function renderPoses() {
  const list = poseFiltered();
  const grid = $("poseGrid");
  const endCount = Math.min(list.length, poseShown || 80);
  $("poseCount").textContent = `${endCount} / ${list.length}`;
  if (!list.length) {
    grid.innerHTML = '<div class="lib-empty">没有匹配的姿势</div>';
    return;
  }
  const angle = $("poseAngle").value;
  const end = Math.min(list.length, poseShown || 80);
  poseShown = end;
  grid.innerHTML = "";
  for (const p of list.slice(0, end)) {
    const file = (p.views || {})[angle] || (p.views || {}).front || Object.values(p.views || {})[0];
    if (!file) continue;
    const d = document.createElement("div");
    d.className = "pose-card";
    d.innerHTML = `<img data-file="${file}" alt="${p.id}"><div class="pc-name">${p.id} · ${p.name_cn || p.name_en || ""}</div><span class="pc-word" title="复制英文描述">词</span><span class="pc-word pc-fill" title="把英文描述填进提示词">填</span>`;
    const img = d.querySelector("img");
    img.onerror = () => d.remove();
    d.querySelector(".pc-fill").onclick = (e) => {
      e.stopPropagation();
      const t = p.desc_en || p.name_en || "";
      if (!t) return;
      const box = $("prompt");
      box.value = box.value.trim() ? `${box.value.trim()}, ${t}` : t;
      toast("已填入姿势描述");
    };
    d.querySelector(".pc-word").onclick = (e) => {
      e.stopPropagation();
      navigator.clipboard.writeText(p.desc_en || p.name_en || "").then(() => toast("已复制姿势描述"));
    };
    d.onclick = async () => {
      if (!profile()?.supportsEdit) {
        const t = p.desc_en || p.name_en || "";
        const box = $("prompt");
        if (t) box.value = box.value.trim() ? `${box.value.trim()}, ${t}` : t;
        toast("这个模型不支持参考图，已把姿势描述填进提示词");
        $("poseModal").hidden = true;
        return;
      }
      try {
        let url = poseCache.get(file);
        if (!url) {
          url = await invoke("read_input_image", { name: `shisei/line/${file}` });
          poseCache.set(file, url);
        }
        await addRefFromDataUrl(url, `pose_${p.id}_${angle}`);
        $("poseModal").hidden = true;
      } catch (e) {
        toast(String(e), true);
      }
    };
    grid.appendChild(d);
  }
  lazyLoadPoses();
}

let poseObserver = null;
function lazyLoadPoses() {
  if (!poseObserver) {
    poseObserver = new IntersectionObserver((entries) => {
      for (const en of entries) {
        if (!en.isIntersecting) continue;
        const img = en.target;
        const file = img.dataset.file;
        if (!file || img.src) continue;
        invoke("read_input_image", { name: `shisei/line/${file}` })
          .then((url) => {
            img.src = url;
            poseCache.set(file, url);
          })
          .catch(() => {});
        poseObserver.unobserve(img);
      }
    }, { root: $("poseGrid"), rootMargin: "300px" });
  }
  document.querySelectorAll("#poseGrid img[data-file]").forEach((img) => {
    if (!img.src) poseObserver.observe(img);
  });
}

$("btnPoses").onclick = () => { $("anaModal").hidden = true; $("layoutModal").hidden = true; openPoses(); };
$("btnPoses2").onclick = () => { $("anaModal").hidden = true; $("layoutModal").hidden = true; openPoses(); };
$("poseClose").onclick = () => ($("poseModal").hidden = true);
$("poseSearch").oninput = () => { poseShown = 0; renderPoses(); };
$("poseClass").onchange = () => { poseShown = 0; renderPoses(); };
$("poseAngle").onchange = () => { poseShown = 0; renderPoses(); };
$("poseGrid").addEventListener("scroll", () => {
  const g = $("poseGrid");
  if (g.scrollTop + g.clientHeight < g.scrollHeight - 200) return;
  if (poseShown < poseFiltered().length) {
    poseShown += 80;
    renderPoses();
  }
});

// ---------------------------------------------------------------- region marker
let markTarget = null;
let markKeep = null;
let markErases = [];
let markMode = "keep";
let markDrag = null;
let markFile = null;

function openMark(file, entry) {
  markTarget = entry;
  markFile = file;
  markKeep = entry.crop ? [...entry.crop] : null;
  markErases = (entry.erases || []).map((r) => [...r]);
  markMode = "keep";
  markSetMode("keep");
  $("cropModal").hidden = false;
  const img = $("cropImg");
  img.src = URL.createObjectURL(file);
  img.onload = renderMark;
}

function markSetMode(mode) {
  markMode = mode;
  $("markKeep").classList.toggle("primary", mode === "keep");
  $("markErase").classList.toggle("primary", mode === "erase");
}

function renderMark() {
  const wrap = $("cropOverlay");
  wrap.innerHTML = "";
  const img = $("cropImg");
  const w = img.clientWidth;
  const h = img.clientHeight;
  const add = (r, cls) => {
    const d = document.createElement("div");
    d.className = `rect ${cls}`;
    d.style.left = `${r[0] * w}px`;
    d.style.top = `${r[1] * h}px`;
    d.style.width = `${r[2] * w}px`;
    d.style.height = `${r[3] * h}px`;
    wrap.appendChild(d);
  };
  if (markKeep) add(markKeep, "keep");
  for (const r of markErases) add(r, "erase");
  $("markState").textContent = `${markErases.length} 个红区${markKeep ? " · 已框绿区" : ""}`;
}

(() => {
  const stage = $("cropStage");
  const img = $("cropImg");
  stage.addEventListener("pointerdown", (e) => {
    const r = img.getBoundingClientRect();
    markDrag = { x: (e.clientX - r.left) / r.width, y: (e.clientY - r.top) / r.height };
    stage.setPointerCapture(e.pointerId);
  });
  stage.addEventListener("pointermove", (e) => {
    if (!markDrag) return;
    const r = img.getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const y = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    const rect = [
      Math.min(markDrag.x, x),
      Math.min(markDrag.y, y),
      Math.abs(x - markDrag.x),
      Math.abs(y - markDrag.y),
    ].map((v) => Number(v.toFixed(4)));
    if (markMode === "keep") markKeep = rect;
    else markErases = markErases.slice(0, -1).concat([rect]);
    renderMark();
  });
  stage.addEventListener("pointerup", () => (markDrag = null));
})();

$("markKeep").onclick = () => markSetMode("keep");
$("markErase").onclick = () => markSetMode("erase");
$("markUndo").onclick = () => {
  if (markErases.length) markErases.pop();
  else markKeep = null;
  renderMark();
};
$("cropClear").onclick = () => {
  markKeep = null;
  markErases = [];
  renderMark();
};
$("cropCancel").onclick = () => ($("cropModal").hidden = true);
$("cropOk").onclick = () => {
  markTarget.crop = markKeep && markKeep[2] > 0.02 && markKeep[3] > 0.02 ? markKeep : null;
  markTarget.erases = markErases.filter((r) => r[2] > 0.01 && r[3] > 0.01);
  const n = markTarget.erases.length + (markTarget.crop ? 1 : 0);
  markTarget.cropBtn.textContent = n ? `标${n}` : "框";
  markTarget.cropBtn.classList.toggle("primary", n > 0);
  $("cropModal").hidden = true;
};

// translate Chinese annotations to English with the local model
$("annTranslate").onclick = async () => {
  const hasCjk = (s) => /[\u4e00-\u9fff]/.test(s);
  const fields = [];
  for (const a of trainAnn) {
    if (hasCjk(a.want.value)) fields.push(a.want);
    if (hasCjk(a.avoid.value)) fields.push(a.avoid);
  }
  if (!fields.length) return toast("没有中文标注要翻译", true);
  const btn = $("annTranslate");
  btn.disabled = true;
  btn.textContent = "翻译中…";
  try {
    const text = fields.map((f) => f.value.trim()).join("\n");
    const out = await invoke("translate_text", { text });
    const lines = String(out).split("\n").map((s) => s.trim()).filter(Boolean);
    if (lines.length === fields.length) {
      fields.forEach((f, i) => (f.value = lines[i]));
    } else {
      // the model merged lines: translate one by one instead
      for (const f of fields) {
        try {
          const one = await invoke("translate_text", { text: f.value.trim() });
          f.value = String(one).trim();
        } catch {}
      }
    }
    toast(`已翻译 ${fields.length} 条`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "中→英 翻译标注";
  }
};

// ---------------------------------------------------------------- story split
$("btnStorySplit").onclick = () => {
  $("storyModal").hidden = false;
  $("storyText").focus();
};
$("storyCancel").onclick = () => ($("storyModal").hidden = true);
$("storyGo").onclick = async () => {
  const text = $("storyText").value.trim();
  if (!text) return toast("先贴一段剧情", true);
  const btn = $("storyGo");
  btn.disabled = true;
  btn.textContent = "拆分中…";
  try {
    const panels = await invoke("story_split", {
      text,
      maxPanels: Number($("storyMax").value) || 12,
    });
    $("storyPanels").value = (panels || []).join("\n");
    toast(`拆出 ${(panels || []).length} 格`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "拆分";
  }
};
$("storyPush").onclick = () => {
  const lines = $("storyPanels")
    .value.split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  if (!lines.length) return toast("还没有分镜", true);
  $("queueText").value = lines.join("\n");
  queueAdd();
  $("storyModal").hidden = true;
};

// ---------------------------------------------------------------- simple / pro mode
function applyUiMode(simple) {
  document.body.classList.toggle("mode-simple", simple);
  document.body.classList.toggle("mode-pro", !simple);
  $("btnUiMode").textContent = simple ? "简单" : "专业";
  $("btnUiMode").title = simple ? "当前：简单模式，点一下切到专业" : "当前：专业模式，点一下切回简单";
}

$("btnUiMode").onclick = async () => {
  const simple = !document.body.classList.contains("mode-simple");
  applyUiMode(simple);
  try {
    const s = await invoke("settings_get");
    s.simpleUi = simple;
    await invoke("settings_set", { settings: s });
    state.settings = s;
  } catch {}
};

// quality / ratio presets used by simple mode
function applyQuality(q) {
  const p = profile();
  if (!p) return;
  if (q === "fast") {
    $("steps").value = String(Math.min(16, Number(p.steps) || 16));
    $("cfg").value = String(p.cfg);
  } else if (q === "fine") {
    $("steps").value = String(Math.max(40, Number(p.steps) || 25));
    $("cfg").value = String(p.cfg);
  } else {
    $("steps").value = String(p.steps);
    $("cfg").value = String(p.cfg);
  }
  syncStepPresets($("stepPresets"), $("steps"));
}

const SIMPLE_RATIOS = {
  "1:1": [1024, 1024],
  "3:2": [1216, 832],
  "2:3": [832, 1216],
  "9:16": [768, 1344],
};

$("simpleQuality").onchange = () => applyQuality($("simpleQuality").value);
$("simpleRatio").onchange = () => {
  const w = SIMPLE_RATIOS[$("simpleRatio").value] || SIMPLE_RATIOS["1:1"];
  $("width").value = String(w[0]);
  $("height").value = String(w[1]);
};

// kind + strength -> hidden training knobs
function simpleTrainKnobs() {
  const table = {
    character: { size: 512, rank: 16, base: 200 },
    style: { size: 512, rank: 16, base: 160 },
    object: { size: 512, rank: 8, base: 120 },
    scene: { size: 640, rank: 16, base: 200 },
  };
  const t = table[$("simpleKind").value] || table.character;
  const mult = $("simpleStrength").value === "light" ? 0.6 : $("simpleStrength").value === "heavy" ? 1.5 : 1;
  return { size: t.size, rank: t.rank, steps: Math.round(t.base * mult) };
}

(() => {
  const grid = $("libGrid");
  grid.addEventListener("scroll", () => {
    if (grid.scrollTop + grid.clientHeight < grid.scrollHeight - 200) return;
    const total = sortedLibrary().length;
    if (libRenderLimit < total) {
      libRenderLimit += 200;
      renderLibrary();
    }
  });
})();

boot().catch((e) => toast("初始化失败：" + e, true));


















