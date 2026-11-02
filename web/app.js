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
  layoutTpls: [],         // saved layout template names (under <config>/layouts)
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
  // 生成结束后，把排队中的超分继续跑掉
  if (!busy && typeof drainUpscale === "function") setTimeout(() => drainUpscale(), 300);
}

function fmtTime(s) {
  return s >= 60 ? `${(s / 60).toFixed(1)} 分` : `${s.toFixed(0)} 秒`;
}

// ---------------------------------------------------------------- status

async function refreshStatus() {
  try {
    const st = await api.call("server_status");
    state.healthy = st.healthy;
    const dot = $("statusDot");
    const text = $("statusText");
    if (st.healthy) {
      dot.className = "dot ok";
      text.textContent = st.managed ? "本壳托管" : "外部进程";
      $("status").title = st.managed ? "ComfyUI 运行中（本壳托管）" : "ComfyUI 运行中（外部进程）";
    } else if (state.starting) {
      dot.className = "dot warn";
      text.textContent = "启动中…";
      $("status").title = "正在启动 ComfyUI…";
    } else if (state.settings?.autoStart) {
      dot.className = "dot warn";
      text.textContent = "未响应…";
      $("status").title = "ComfyUI 未响应（会自动重试）";
    } else {
      dot.className = "dot err";
      text.textContent = "未运行";
      $("status").title = "ComfyUI 未运行";
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
  [/natural facial skin texture|skin texture|lora_skin/i, "真实皮肤纹理（补毛孔/绒毛，改图用），强度 1.0"],
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
  {
    test: /natural facial skin texture/i,
    models: ["flux3", "flux3base"],
    rank: 2,
    strength: 1.0,
    note: "皮肤纹理（FLUX.3 Klein 编辑，无触发词）",
  },
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
    const url = await api.call("read_input_image", { name });
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
  const name = await api.call("save_ref_image", { name: file.name, dataUrl });
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
  const name = await api.call("save_ref_image", { name: label + ".png", dataUrl });
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
    item.dataUrl = await api.call("read_output_image", { path });
    img.src = item.dataUrl;
  } catch {
    item.missing = true; img.closest(".card")?.remove(); if (!$("results").querySelector(".card")) $("empty").hidden = false;
  }
}

// ---------------------------------------------------------------- result management

async function deleteResults(items) {
  const paths = items.map((i) => i.path);
  if (!paths.length) return;
  try {
    await api.call("delete_results", { paths });
    state.results = state.results.filter((r) => !paths.includes(r.path));
    renderResults();
    if (!$("library").hidden) await loadLibrary();
    toast(`已删除 ${paths.length} 张（在项目文件夹的 _trash 里可找回）`);
  } catch (e) {
    toast(String(e), true);
  }
}

async function useAsReference(r) {
  try {
    const dataUrl = r.dataUrl || (await api.call("read_output_image", { path: r.path }));
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
    {
      label: "调色 / 裁切…",
      run: () =>
        window.photoEditor &&
        window.photoEditor.open({ path: r.path, filename: r.filename, dataUrl: r.dataUrl }),
    },
    {
      label: "超分 4×（有任务在跑就排队）",
      run: () => {
        if (typeof queueUpscale === "function") queueUpscale(r);
      },
    },
    { label: "删除（移到回收站）", danger: true, run: () => deleteResults([r]) },
    { label: "在文件夹中显示", run: () => api.call("reveal_file", { path: r.path }) },
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
    state.stars = new Set(await api.call("stars_list"));
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
    await api.call("library_star", { path: r.path, starred: on });
    toast(on ? "已收藏" : "已取消收藏");
  } catch (e) {
    toast(String(e), true);
  }
}

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
    layout: state.pendingLayout || null, project: (window.projectUI && window.projectUI.current()) || null, versionParent: state.versionBranch || null,
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
  state.pendingLayout = null; state.versionBranch = null;
  try {
    const res = await api.call("generate", { req, turbo: $("turbo").checked });
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
    const out = await api.call("enhance_prompt", {
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
    const saved = await api.call("custom_save", { name, jsonText: text });
    state.customs = await api.call("custom_list");
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
      const st = await api.call("server_status");
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
  state.profiles = await api.call("profiles");
  state.customs = await api.call("custom_list");
  state.layoutTpls = await api.call("layout_tpl_list").catch(() => []);

  state.settings = await api.call("settings_get");
  $("setRoot").value = state.settings.comfyRoot;
  $("setPort").value = state.settings.port;
  if ($("setOutputDir")) $("setOutputDir").value = state.settings.outputDir || "";
  $("setAutoStart").checked = state.settings.autoStart;
  $("setStopOnExit").checked = state.settings.stopOnExit;
  $("setFreeEnhance").checked = !!state.settings.freeEnhance;

  buildModelSelect();
  refreshStatus(); // paint something before the slower queries

  try {
    applyUiMode(state.settings.simpleUi !== false);
  $("simpleQuality").value = "std";
  $("simpleRatio").value = "1:1";
  await loadStars();
  state.models = await api.call("models_list");
  } catch {
    state.models = { loras: [] };
  }
  $("lora").innerHTML =
    `<option value="">（无）</option>` +
    state.models.loras.map((l) => `<option value="${l}">${l}</option>`).join("");

  try {
    const hist = await api.call("history_list");
    state.results = flattenHistory(hist);
  } catch {}
  renderResults();

  applyProfileDefaults();
  restoreLast();
  renderResults();
  setInterval(() => {
    // 设置/助手弹窗开着时，别再刷新底下的状态条：少一次 IPC + 重绘，弹窗更跟手
    if ($("settingsModal") && !$("settingsModal").hidden) return;
    if ($("aiModal") && !$("aiModal").hidden) return;
    refreshStatus();
  }, 3000);

  await listen("gen-progress", (ev) => {
    if ($("settingsModal") && !$("settingsModal").hidden) return;
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
    const list = await api.call("custom_list");
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
  await api.call("job_cancel");
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
    const out = await api.call("translate_text", { text });
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
  // 背景点击的关闭交给 results.js 的 pointer 逻辑（它要区分「点图放大」和「点背景关闭」）；
  // 这里只兜住键盘/程序化 click，避免重复处理导致拖拽松手时误关。
  if (e.target.id === "lightbox" && e.detail === 0) $("lightbox").hidden = true;
};
$("lightboxReveal").onclick = () => lightboxItem && api.call("reveal_file", { path: lightboxItem.path });
$("lightboxRef").onclick = async () => {
  if (!lightboxItem) return;
  const dataUrl = lightboxItem.dataUrl || (await api.call("read_output_image", { path: lightboxItem.path }));
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
    await api.call("server_start");
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
    await api.call("server_stop");
    toast("已停止");
  } catch (e) {
    toast(String(e), true);
  }
  refreshStatus();
};
$("btnOpenOut").onclick = async () => {
  const root = state.settings.comfyRoot.replace(/[\\/]+$/, "");
  await api.call("open_folder", { path: `${root}\\ComfyUI\\output` });
};
$("btnClearHistory").onclick = async () => {
  if (!confirm("清空历史记录？（只删列表，不删图片文件）")) return;
  await api.call("history_clear");
  state.results = [];
  renderResults();
  toast("历史已清空");
};

// settings modal（v0.97：左侧分类 + 右侧内容）
function setSettingsTab(tab) {
  document.querySelectorAll("#setNav .set-tab").forEach((b) => b.classList.toggle("active", b.dataset.tab === tab));
  document.querySelectorAll("#settingsModal .set-pane").forEach((p) => p.classList.toggle("active", p.dataset.pane === tab));
  if (tab === "ai" && window.aiAgent) window.aiAgent.refresh();
  if (tab === "workflows" && typeof wfDirty !== "undefined" && wfDirty) renderWorkflowPresets();
  if (tab === "about") refreshFeedbackSmtp();
}
document.querySelectorAll("#setNav .set-tab").forEach((b) => {
  b.onclick = () => setSettingsTab(b.dataset.tab);
});

function collectSettings() {
  const prevOut = (state.settings && state.settings.outputDir) || "";
  // 从现有设置铺开再覆盖这几个字段：别把 simpleUi / kohya 路径之类的旧字段冲掉
  return Object.assign({}, state.settings || {}, {
    comfyRoot: $("setRoot").value.trim(),
    port: Number($("setPort").value),
    outputDir: $("setOutputDir") ? $("setOutputDir").value.trim() : prevOut,
    autoStart: $("setAutoStart").checked,
    stopOnExit: $("setStopOnExit").checked,
    freeEnhance: $("setFreeEnhance").checked,
  });
}

async function saveSettings(closeModal) {
  const next = collectSettings();
  const prevOut = (state.settings && state.settings.outputDir) || "";
  await api.call("settings_set", { settings: next });
  state.settings = await api.call("settings_get");
  if (closeModal) $("settingsModal").hidden = true;
  if (next.outputDir !== prevOut) toast("出图目录已保存：重启 ComfyUI 后生效");
  else if (closeModal) toast("已保存");
  refreshStatus();
}

async function refreshSettingsInfo() {
  try {
    const st = await api.call("server_status");
    const el = $("setComfyVersion");
    if (el) {
      el.textContent = st.healthy
        ? `ComfyUI ${st.version || ""} · 运行中${st.managed ? "（壳托管）" : "（外部进程）"}`
        : "未运行";
    }
  } catch {}
  try {
    const v = await api.call("app_version");
    if ($("setAppVersion")) $("setAppVersion").textContent = "v" + v;
  } catch {}
  if (typeof markWorkflowsDirty === "function") markWorkflowsDirty();
}

/// 工作流依赖检查：内置的每个档位需要哪几个模型、本机缺哪个。
let wfDirty = true;

function markWorkflowsDirty() {
  wfDirty = true;
  const pane = document.querySelector('#settingsModal .set-pane[data-pane="workflows"]');
  if (pane && pane.classList.contains("active")) renderWorkflowPresets();
}

async function renderWorkflowPresets() {
  wfDirty = false;
  const box = $("setWorkflows");
  if (!box) return;
  box.innerHTML = '<div class="muted" style="font-size:12px">检查依赖中…</div>';
  try {
    const [profs, models, man] = await Promise.all([
      api.call("profiles"),
      api.call("models_scan"),
      api.call("models_manifest"),
    ]);
    const have = new Set((models || []).map((m) => String(m.name || "").split("/").pop().toLowerCase()));
    const byTarget = new Map((man || []).map((m) => [m.target.toLowerCase(), m]));
    box.innerHTML = "";
    for (const p of profs || []) {
      const req = [p.unet, p.clip, p.vae].filter(Boolean);
      const missing = req.filter((f) => !have.has(String(f).toLowerCase()));
      const item = document.createElement("div");
      item.className = "set-wf-item";
      const head = document.createElement("div");
      head.className = "set-wf-head";
      head.innerHTML = `<i class="set-wf-dot ${missing.length ? "miss" : "ok"}"></i><b></b><span class="muted" style="font-size:11.5px"></span>`;
      head.querySelector("b").textContent = p.label || p.id;
      head.querySelector("span").textContent = `${p.steps} 步 · CFG ${p.cfg}${p.supportsEdit ? " · 支持改图" : ""}`;
      item.appendChild(head);

      if (!missing.length) {
        const ok = document.createElement("div");
        ok.className = "set-wf-sub";
        ok.textContent = "依赖齐全，可直接开跑";
        item.appendChild(ok);
      } else {
        for (const f of missing) {
          const m = byTarget.get(String(f).toLowerCase());
          const row = document.createElement("div");
          row.className = "set-wf-sub";
          const name = document.createElement("span");
          name.textContent = f;
          row.appendChild(name);
          if (m && m.size) {
            const sz = document.createElement("span");
            sz.className = "muted";
            sz.textContent = " · " + dlFmt(m.size);
            row.appendChild(sz);
          }
          if (m && (m.sources || []).length) {
            const btn = document.createElement("button");
            btn.className = "btn small ghost";
            btn.type = "button";
            btn.style.marginLeft = "8px";
            btn.textContent = "下载";
            btn.onclick = () => downloadModel(m, btn);
            row.appendChild(btn);
            if (m.note) {
              const note = document.createElement("div");
              note.className = "muted";
              note.style.fontSize = "11px";
              note.textContent = m.note;
              row.appendChild(note);
            }
          } else {
            const none = document.createElement("span");
            none.className = "muted";
            none.textContent = " · 无公开源（本机定制）";
            row.appendChild(none);
          }
          item.appendChild(row);
        }
      }
      box.appendChild(item);
    }
  } catch (e) {
    box.textContent = "检查失败：" + e;
  }
}

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
if ($("setOutputPick")) {
  $("setOutputPick").onclick = async () => {
    try {
      const dir = await api.call("pick_folder");
      if (dir) $("setOutputDir").value = dir;
    } catch (e) {
      toast(String(e), true);
    }
  };
}
$("btnSettingsSave").onclick = async () => {
  try {
    await saveSettings(true);
  } catch (e) {
    toast(String(e), true);
  }
};

// ---------------------------------------------------------------- 关于页：反馈 / 爱发电
async function sendFeedback() {
  const kind = $("fbKind") ? $("fbKind").value : "bug";
  const text = $("fbText") ? $("fbText").value.trim() : "";
  if (!text) return toast("先写点内容", true);
  const btn = $("fbSend");
  if (btn) btn.disabled = true;
  if ($("fbState")) $("fbState").textContent = "发送中…";
  try {
    await api.call("feedback_send", {
      kind,
      text,
      contact: $("fbContact") ? $("fbContact").value.trim() : "",
    });
    if ($("fbState")) $("fbState").textContent = "已发送，谢谢！";
    if ($("fbText")) $("fbText").value = "";
    toast("反馈已发送");
  } catch (e) {
    // 表单服务不通时兜底：复制内容 + 打开邮件客户端
    const ver = $("setAppVersion") ? $("setAppVersion").textContent : "";
    const body = `[${kind}] ${text}\n\n版本 ${ver}`;
    try {
      await navigator.clipboard.writeText(body);
    } catch {}
    const mailto =
      "mailto:ksiukee@qq.com?subject=" +
      encodeURIComponent("[显影反馈]") +
      "&body=" +
      encodeURIComponent(body);
    await api.call("open_url", { url: mailto }).catch(() => {});
    if ($("fbState")) $("fbState").textContent = "表单发送失败，已复制内容并打开邮件客户端";
    toast("已复制反馈内容，可直接粘贴发送", true);
  } finally {
    if (btn) btn.disabled = false;
  }
}
if ($("fbSend")) $("fbSend").onclick = sendFeedback;

async function refreshFeedbackSmtp() {
  const ok = await api.call("feedback_smtp_status").catch(() => false);
  if ($("fbSmtpState")) {
    $("fbSmtpState").textContent = ok
      ? "已配置：一键发信可用（授权码本机加密保存）"
      : "未配置：填 QQ 邮箱「设置 → 账户 → 授权码」后，才能一键发信";
  }
  if ($("fbSmtp")) $("fbSmtp").value = "";
}

if ($("fbSmtpSave")) {
  $("fbSmtpSave").onclick = async () => {
    const code = ($("fbSmtp") ? $("fbSmtp").value : "").trim();
    if (!code) return toast("先填授权码", true);
    try {
      await api.call("feedback_smtp_set", { code });
      toast("授权码已保存（加密）");
      await refreshFeedbackSmtp();
    } catch (e) {
      toast(String(e), true);
    }
  };
}
if ($("fbSmtpClear")) {
  $("fbSmtpClear").onclick = async () => {
    if (!confirm("清除已保存的发信授权码？")) return;
    await api.call("feedback_smtp_clear").catch(() => {});
    toast("已清除");
    await refreshFeedbackSmtp();
  };
}
if ($("afdianBtn")) {
  $("afdianBtn").onclick = () =>
    api.call("open_url", { url: "https://afdian.com/a/siukee" }).catch((e) => toast(String(e), true));
}

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


// ---------------------------------------------------------------- de-AI preset
const DEAI_PROMPT =
  "neutral white balance, no text or logos, candid expression not looking at camera, single window light with soft shadows, f1.8 shallow depth of field, subtle film grain, slight lens distortion";
const DEAI_NEG =
  "yellow cast, oversaturated, glow, bloom, text, watermark, plastic skin, over-smoothed";
// 按逗号片段去重后追加：用户词里已有的（如 watermark）不再重复，点两次等于一次。
function appendTokens(value, extra) {
  const seen = new Set(
    value.toLowerCase().split(",").map((s) => s.trim()).filter(Boolean)
  );
  const add = [];
  for (const raw of extra.split(",")) {
    const token = raw.trim();
    if (token && !seen.has(token.toLowerCase())) {
      seen.add(token.toLowerCase());
      add.push(token);
    }
  }
  if (!add.length) return value;
  const base = value.trim().replace(/,\s*$/, "");
  return base ? `${base}, ${add.join(", ")}` : add.join(", ");
}
$("btnDeAi").onclick = () => {
  const box = $("prompt");
  const neg = $("negative");
  const before = `${box.value}\n${neg.value}`;
  box.value = appendTokens(box.value, DEAI_PROMPT);
  neg.value = appendTokens(neg.value, DEAI_NEG);
  const changed = `${box.value}\n${neg.value}` !== before;
  toast(
    changed
      ? "已加：去 AI 感后缀 + 负面词（噪点/轻微畸变；已有的不重复）"
      : "去 AI 感后缀和负面词都已经在了"
  );
};


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
    const res = await api.call("assist", { request, image: state.refs[0] || null });
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
