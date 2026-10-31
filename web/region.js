// 区域标记 / 故事拆分 / 简易模式（从 app.js 拆出；依赖 app.js 的全局）

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
    const out = await api.call("translate_text", { text });
    const lines = String(out).split("\n").map((s) => s.trim()).filter(Boolean);
    if (lines.length === fields.length) {
      fields.forEach((f, i) => (f.value = lines[i]));
    } else {
      // the model merged lines: translate one by one instead
      for (const f of fields) {
        try {
          const one = await api.call("translate_text", { text: f.value.trim() });
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
    const panels = await api.call("story_split", {
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
    const s = await api.call("settings_get");
    s.simpleUi = simple;
    await api.call("settings_set", { settings: s });
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

// boot() 在 boot.js 里最后调用
