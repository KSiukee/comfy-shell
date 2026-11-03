// 项目工作区：表单草稿 / 布局 / 面板宽 / 图库筛选，按项目存。
// 私密项目不保存（workspace.json 是明文）。
const _wsGet = (id) => {
  const e = document.getElementById(id);
  return e ? e.value : undefined;
};
const _wsChk = (id) => {
  const e = document.getElementById(id);
  return e ? !!e.checked : undefined;
};

let wsLoadedProject = "";
let wsSaving = false;
let wsTimer = null;
let wsApplying = false;

function wsCollect() {
  const layoutEl = document.querySelector(".layout");
  const panel = layoutEl
    ? parseInt(getComputedStyle(layoutEl).getPropertyValue("--panel-w"), 10) || undefined
    : undefined;
  return {
    schema: 1,
    savedAt: Date.now(),
    form: {
      prompt: _wsGet("prompt"),
      negative: _wsGet("negative"),
      width: _wsGet("width"),
      height: _wsGet("height"),
      steps: _wsGet("steps"),
      cfg: _wsGet("cfg"),
      seed: _wsGet("seed"),
      model: _wsGet("model"),
      lora: _wsGet("lora"),
      loraStrength: _wsGet("loraStrength"),
      lora2: _wsGet("lora2"),
      lora2Strength: _wsGet("lora2Strength"),
      slg: _wsChk("mossSlg"),
      slgLayers: _wsGet("slgTier"),
      mode: state.mode,
    },
    layout: state.pendingLayout || null,
    panel,
    resultSize: _wsGet("resSize"),
    gallery: {
      scope: typeof libScope !== "undefined" ? libScope : undefined,
      search: _wsGet("libSearch"),
      sort: _wsGet("libSort"),
      starOnly: _wsChk("libStarOnly"),
    },
  };
}

async function wsSaveFor(project) {
  if (!project || wsSaving) return;
  const p = (window.projectUI.list() || []).find((x) => x.id === project);
  if (p && p.private) return;
  wsSaving = true;
  try {
    await api.call("workspace_save", { project, data: wsCollect() });
  } catch {
    // 项目可能刚被删/私密；静默
  } finally {
    wsSaving = false;
  }
}

function wsScheduleSave() {
  if (wsApplying) return;
  clearTimeout(wsTimer);
  wsTimer = setTimeout(() => {
    const id = projectUI.current();
    if (id) wsSaveFor(id);
  }, 900);
}

function wsSetVal(id, v) {
  if (v === undefined || v === null) return;
  const el = document.getElementById(id);
  if (el) el.value = v;
}

function wsApply(data) {
  if (!data || data.schema !== 1) return;
  wsApplying = true;
  try {
    const f = data.form || {};
    wsSetVal("prompt", f.prompt);
    wsSetVal("negative", f.negative);
    wsSetVal("width", f.width);
    wsSetVal("height", f.height);
    wsSetVal("steps", f.steps);
    wsSetVal("cfg", f.cfg);
    wsSetVal("seed", f.seed);
    if (f.model) {
      const sel = document.getElementById("model");
      if (sel && [...sel.options].some((o) => o.value === f.model)) sel.value = f.model;
    }
    if (f.lora !== undefined) {
      const sel = document.getElementById("lora");
      if (sel && [...sel.options].some((o) => o.value === (f.lora || ""))) sel.value = f.lora || "";
    }
    if (f.lora2 !== undefined) {
      const sel = document.getElementById("lora2");
      if (sel && [...sel.options].some((o) => o.value === (f.lora2 || ""))) sel.value = f.lora2 || "";
    }
    wsSetVal("loraStrength", f.loraStrength);
    wsSetVal("lora2Strength", f.lora2Strength);
    if (f.loraStrength !== undefined && document.getElementById("loraStrengthText")) {
      document.getElementById("loraStrengthText").textContent = Number(f.loraStrength).toFixed(2);
    }
    if (f.lora2Strength !== undefined && document.getElementById("lora2StrengthText")) {
      document.getElementById("lora2StrengthText").textContent = Number(f.lora2Strength).toFixed(2);
    }
    if (f.slg !== undefined) {
      const cb = document.getElementById("mossSlg");
      if (cb) cb.checked = !!f.slg;
    }
    wsSetVal("slgTier", f.slgLayers);
    state.pendingLayout = data.layout || null;
    if (typeof setMode === "function") setMode(f.mode === "edit" ? "t2i" : f.mode || "t2i");

    if (data.panel) {
      const layoutEl = document.querySelector(".layout");
      if (layoutEl) layoutEl.style.setProperty("--panel-w", `${data.panel}px`);
    }
    if (data.resultSize) {
      const n = Number(data.resultSize);
      const results = document.getElementById("results");
      const input = document.getElementById("resSize");
      if (results && input) {
        results.style.setProperty("--res-thumb", `${n}px`);
        results.classList.toggle("dense", n < 260);
        input.value = String(n);
      }
    }
    const g = data.gallery || {};
    if (g.scope && typeof libScope !== "undefined") {
      // 老工作区里可能存着 "all"：现在只有本项目/私密两档，其余一律按本项目
      libScope = g.scope === "private" ? "private" : "project";
      const sel = document.getElementById("libScope");
      if (sel) sel.value = libScope;
    }
    wsSetVal("libSearch", g.search);
    wsSetVal("libSort", g.sort);
    if (g.starOnly !== undefined) {
      const cb = document.getElementById("libStarOnly");
      if (cb) cb.checked = !!g.starOnly;
    }
    if (typeof refreshLibraryIfOpen === "function") refreshLibraryIfOpen();
  } finally {
    wsApplying = false;
  }
}

async function wsLoadFor(project) {
  const p = (window.projectUI.list() || []).find((x) => x.id === project);
  if (p && p.private) return;
  try {
    const data = await api.call("workspace_get", { project });
    if (data) wsApply(data);
    else wsResetForm();
  } catch {}
}

/// 目标项目还没有草稿：把表单清回默认，别把上一个项目（可能是隐藏项目）的提示词带过来。
function wsResetForm() {
  wsApplying = true;
  try {
    if ($("prompt")) $("prompt").value = "";
    if ($("negative")) $("negative").value = "";
    if ($("seed")) $("seed").value = "-1";
    if (typeof applyProfileDefaults === "function") applyProfileDefaults();
  } finally {
    wsApplying = false;
  }
}

async function wsCheck() {
  const id = projectUI.current();
  if (!id) {
    if (wsLoadedProject) {
      await wsSaveFor(wsLoadedProject);
      wsLoadedProject = "";
    }
    return;
  }
  if (id === wsLoadedProject) return;
  if (wsLoadedProject) await wsSaveFor(wsLoadedProject);
  wsLoadedProject = id;
  await wsLoadFor(id);
}

window.workspaceRestore = async () => {
  wsLoadedProject = "";
  await wsCheck();
};

// 切项目前先把旧项目存下来（capture 在 projects.js 的 change 之前）
const _wsSel = document.getElementById("projSel");
if (_wsSel) {
  _wsSel.addEventListener(
    "change",
    () => {
      const old = wsLoadedProject || projectUI.current();
      if (old) wsSaveFor(old);
      wsLoadedProject = "";
    },
    true
  );
}

document.addEventListener(
  "input",
  (e) => {
    if (e.target && (e.target.id === "projSel" || e.target.id === "projShowHidden")) return;
    wsScheduleSave();
  },
  true
);
document.addEventListener(
  "change",
  (e) => {
    if (e.target && (e.target.id === "projSel" || e.target.id === "projShowHidden")) return;
    wsScheduleSave();
  },
  true
);

window.addEventListener("beforeunload", () => {
  const id = projectUI.current();
  if (id) wsSaveFor(id);
});

setInterval(wsCheck, 700);
