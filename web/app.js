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

/// 让「比例 / 步数」快捷标签跟着真实数值走（切模型、还原表单、手改数字都要同步）。
function syncRatioChips() {
  const w = Number($("width") && $("width").value) || 0;
  const h = Number($("height") && $("height").value) || 0;
  if (!w || !h) return;
  const r = w / h;
  let best = null;
  let bestD = 0.04;
  for (const [k, [pw, ph]] of Object.entries(RATIOS)) {
    const d = Math.abs(r - pw / ph);
    if (d <= bestD) {
      bestD = d;
      best = k;
    }
  }
  document.querySelectorAll("#ratioChips button").forEach((b) => b.classList.toggle("active", b.dataset.r === best));
}

function syncStepChips() {
  if (!$("steps")) return;
  // train.js 的 syncStepPresets 监听 input，这里补一刀让它按当前值高亮
  $("steps").dispatchEvent(new Event("input", { bubbles: true }));
}

function syncQuickPicks() {
  syncRatioChips();
  syncStepChips();
}

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

// 家族 → 能用这套 LoRA 的模型。漏一个 id 的后果是「这个模型的 LoRA 下拉里看不到自己的 LoRA」，
// 所以按家族列常量，别再在每条规则里手抄一遍。
const MOSS_MODELS = ["moss", "mossbase", "mossimagine", "mossedit", "kromoss"];
const FLUX3_MODELS = ["flux3", "flux3base"];

// 规则**按顺序取第一条命中的**：具体条目一定排在通用规则前面，
// 否则 /moss/i 这种宽规则会把后面所有 MOSS 细分条目全吃掉（备注就变成一句废话）。
const LORA_RULES = [
  // —— 不可用的：命中即藏
  { test: /minimax_h3/i, models: [], note: "MiniMax 视频专用" },
  { test: /flux2|spritesheet/i, models: [], note: "FLUX.2 专用，FLUX.3 不适配" },
  { test: /qwen_image_2\.1_8step/i, models: [], note: "壳里自动挂的 8 步加速，不用手选" },
  // —— Qwen
  { test: /qwen_image_2\.1_turbo/i, models: ["qwen21"], rank: 1, strength: 1.0, note: "加速（配 8 步）" },
  { test: /Qwen-Image-2512.*Pixel-Art/i, models: ["qwen21"], rank: 2, strength: 0.85, note: "像素风" },
  { test: /anything2real|Qwen2\.1_/i, models: ["qwen21"], rank: 2, strength: 0.8, note: "风格转真人（改图）" },
  { test: /bfs_(head|body)/i, models: ["qwen21"], rank: 4, strength: 0.85, note: "换头 / 换身（Qwen2.1 专用）" },
  // —— MOSS：细分条目放在通用 /moss/i 之前
  { test: /MOSS-Image-NSFW/i, models: MOSS_MODELS, rank: 1, strength: 0.85, note: "NSFW 人像 / 场景" },
  { test: /MOSS-Image-Beauty/i, models: MOSS_MODELS, rank: 2, strength: 0.75, note: "人像美化、柔和磨皮" },
  { test: /kromoss.*8step/i, models: MOSS_MODELS, rank: 1, strength: 1.0, note: "Kromoss 8 步加速" },
  { test: /moss_hires/i, models: MOSS_MODELS, rank: 3, strength: 0.8, note: "高清细节（hires）" },
  { test: /charref/i, models: MOSS_MODELS, rank: 3, strength: 0.8, note: "角色参考 / 一致性" },
  { test: /moss_image_edit/i, models: MOSS_MODELS, rank: 3, strength: 0.8, note: "MOSS 改图专用" },
  { test: /ultra_real/i, models: MOSS_MODELS, rank: 3, strength: 0.7, note: "写实向微调（真皮质感）" },
  { test: /moss384/i, models: MOSS_MODELS, rank: 3, strength: 0.8, note: "MOSS384 配套" },
  { test: /moss_kohya_probe/i, models: MOSS_MODELS, rank: 4, strength: 0.9, note: "自训测试（kohya）" },
  { test: /moss/i, models: MOSS_MODELS, rank: 3, strength: 0.8, note: "MOSS 生态 LoRA（未细分）" },
  // —— FLUX.3
  {
    test: /natural facial skin texture/i,
    models: FLUX3_MODELS,
    rank: 2,
    strength: 1.0,
    note: "皮肤纹理（Klein 编辑，无触发词）",
  },
  { test: /folder_probe|ui_probe|low_probe|mirror_probe|crop_probe/i, models: FLUX3_MODELS, rank: 1, strength: 0.9, note: "你自训（FLUX.3 Base）" },
];

function loraMeta(name, profileId) {
  for (const r of LORA_RULES) {
    if (r.test.test(name)) {
      return { compatible: r.models.includes(profileId), rank: r.rank || 9, strength: r.strength, note: r.note };
    }
  }
  return { compatible: FLUX3_MODELS.includes(profileId), rank: 5, note: "未登记" };
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
  const off = typeof disabledLoras !== "undefined" ? disabledLoras : new Set();
  const items = (state.models.loras || [])
    .filter((n) => !off.has(n))
    .map((n) => ({ n, m: loraMeta(n, p.id) }));
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
  syncStepChips();
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
  }
  syncRatioChips();
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
      enqueueImage(img);
    }
  },
  { root: $("results"), rootMargin: "300px" }
);

async function loadImageInto(img) {
  const path = img.dataset.path;
  // 图区现在也会显示「磁盘上但不在历史里」的图，优先用渲染时挂的条目
  const item = img.__item || state.results.find((r) => r.path === path);
  if (!item) return;
  if (item.dataUrl) {
    img.src = item.dataUrl;
    return;
  }
  try {
    item.dataUrl = await api.call("read_output_image", { path });
    img.src = item.dataUrl;
  } catch {
    // 一次失败不等于「这张图不存在」（超分大图会因内存/并发瞬时失败）：
    // 先把卡片留着画成坏图占位，点一下重试，别把整张卡从图区删掉。
    img.src = "";
    img.classList.add("broken");
    img.alt = "打不开，点一下重试";
    img.title = "打不开，点一下重试";
    img.onclick = () => {
      img.classList.remove("broken");
      img.onclick = () => img.__item && openLightbox(img.__item);
      enqueueImage(img);
    };
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
    seed: seedArg(),
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
    // 按项目存：不然在隐藏项目里生成一次，切到别的项目再启动就会把它的提示词带回表单（实测泄漏）
    localStorage.setItem("cs.lastReq." + ((window.projectUI && projectUI.current && projectUI.current()) || "none"), JSON.stringify(req));
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
  if ($("setReplaceOriginal")) $("setReplaceOriginal").checked = !!state.settings.replaceOriginal;

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
  if (typeof refreshLibExtra === "function") refreshLibExtra();

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
    // 旧版本是全局键（cs.lastReq），会跨项目把提示词带回来——启动时直接清掉
    localStorage.removeItem("cs.lastReq");
    const pid = (window.projectUI && projectUI.current && projectUI.current()) || "none";
    const raw = localStorage.getItem("cs.lastReq." + pid);
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
    syncQuickPicks();
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

// 手改宽高时，比例快捷标签跟着变（MOSS 预设/切模型已在 applyProfileDefaults 里同步）
["width", "height"].forEach((id) => {
  if ($(id)) $(id).addEventListener("input", syncRatioChips);
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
  syncQuickPicks();
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
  if (!confirm("清空历史记录？\n\n图区会立刻清空；磁盘上的图片不删，切项目或重开后会重新扫出来。")) return;
  await api.call("history_clear");
  state.results = [];
  // v0.109.1 起图区 = 历史 + 当前项目磁盘上的图：不清这一份，点了看着像没反应
  state.libExtra = [];
  state.gridCleared = true;
  renderResults();
  toast("历史已清空（磁盘图未删，刷新后会再扫出来）");
};

// 设置面板已移到 web/settingspanel.js

// 反馈 / 爱发电已移到 web/feedback.js

// ComfyUI 下载 / 更新已移到 web/download.js

// 人像预设 + 去 AI 感已移到 web/presets.js

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
