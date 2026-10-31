// 配方库（#28）：把当前表单（或某个版本）存成一份可复用快照，一键套回来。
// 全局配方所有项目可见；项目配方只在那个项目里出现。

const RECIPE_FIELDS = [
  "width", "height", "steps", "cfg", "seed", "batch",
  "lora", "loraStrength", "lora2", "lora2Strength", "mossSlg", "slgTier",
];

let recipeCache = [];

function recipeProject() {
  return window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
}

function recipeSnapshot() {
  const f = { mode: state.mode, model: $("model").value };
  for (const id of RECIPE_FIELDS) {
    const el = $(id);
    if (!el) continue;
    f[id] = el.type === "checkbox" ? el.checked : el.value;
  }
  f.presetSel = $("presetSel") ? $("presetSel").value : "";
  f.prompt = $("prompt").value;
  f.negative = $("negative").value;
  f.style = typeof currentStyle === "function" && currentStyle() ? currentStyle().name : "";
  return f;
}

async function saveRecipe(payload) {
  try {
    const r = await api.call("recipe_save", { recipe: JSON.stringify(payload) });
    toast(`已存配方「${r.name}」`);
    recipeCache = await api.call("recipe_list");
    renderRecipes();
    return r;
  } catch (e) {
    toast(String(e), true);
    return null;
  }
}

async function recipeSaveFromForm() {
  const name = ($("recipeName").value || "").trim();
  if (!name) return toast("先给配方起个名字", true);
  const p = recipeProject();
  const scope = $("recipeScope").value === "project" ? (p ? p.id : "") : "";
  if ($("recipeScope").value === "project" && !scope) return toast("当前没有项目", true);
  await saveRecipe({
    name,
    project: scope || null,
    form: recipeSnapshot(),
    layout: state.pendingLayout || null,
    wfParams: typeof wfCollect === "function" ? wfCollect() : null,
  });
  $("recipeName").value = "";
}

async function recipeSaveFromVersion(v) {
  const p = recipeProject();
  const f = {
    mode: "t2i",
    model: v.profile || $("model").value,
    width: v.width,
    height: v.height,
    steps: v.steps,
    cfg: v.cfg,
    seed: v.seed,
    batch: 1,
    lora: v.lora || "",
    loraStrength: v.loraStrength ?? 1,
    lora2: v.lora2 || "",
    lora2Strength: v.lora2Strength ?? 1,
    mossSlg: !!v.slg,
    slgTier: v.slgLayers || "22",
    presetSel: "",
    prompt: v.prompt || "",
    negative: v.negative || "",
    style: "",
  };
  const r = await saveRecipe({
    name: `版本配方 ${new Date(v.createdAt).toLocaleString()}`,
    project: p ? p.id : null,
    form: f,
    layout: v.layout || null,
    wfParams: null,
  });
  if (r) $("recipesModal").hidden = false;
}

function applyRecipe(r) {
  const f = r.form || {};
  const setVal = (id, val) => {
    const el = $(id);
    if (!el || val === undefined || val === null) return;
    if (el.type === "checkbox") el.checked = !!val;
    else el.value = String(val);
  };
  for (const id of RECIPE_FIELDS) setVal(id, f[id]);
  if (f.model) {
    const sel = $("model");
    if ([...sel.options].some((o) => o.value === f.model)) sel.value = f.model;
  }
  if (f.presetSel !== undefined) {
    const s = $("presetSel");
    if (s && [...s.options].some((o) => o.value === f.presetSel)) s.value = f.presetSel;
  }
  if (f.prompt !== undefined) $("prompt").value = f.prompt;
  if (f.negative !== undefined) $("negative").value = f.negative;
  if (f.mode === "t2i" || f.mode === "edit") {
    if (typeof setMode === "function") setMode(f.mode);
  }
  if (typeof setStyleName === "function") setStyleName(f.style || "");
  state.pendingLayout = r.layout || null;
  if (r.wfParams && typeof wfParamName !== "undefined" && wfParamName) {
    try {
      localStorage.setItem(`cs.wfparams.${wfParamName}`, JSON.stringify(r.wfParams));
    } catch {}
    if (typeof wfRefresh === "function") wfRefresh();
  }
  $("recipesModal").hidden = true;
  toast(`已应用配方「${r.name}」${r.layout ? "（含排版）" : ""}`);
}

function recipeVisible(r) {
  if (!r.project) return true;
  const p = recipeProject();
  return !!p && p.id === r.project;
}

function renderRecipes() {
  const box = $("recipeList");
  box.textContent = "";
  const list = recipeCache.filter(recipeVisible);
  if (!list.length) {
    const d = document.createElement("div");
    d.className = "muted";
    d.style.cssText = "padding:16px 2px;font-size:12px";
    d.textContent = "还没有配方。调好左边的参数，取个名字点「保存当前表单」；以后一键就能套回来。";
    box.appendChild(d);
    return;
  }
  for (const r of list) {
    const row = document.createElement("div");
    row.className = "recipe-row";
    const main = document.createElement("div");
    main.className = "recipe-main";
    const name = document.createElement("b");
    name.textContent = r.name;
    const meta = document.createElement("span");
    meta.className = "muted";
    const f = r.form || {};
    const bits = [f.model || "?", `${f.width || "?"}×${f.height || "?"}`, `${f.steps || "?"} 步`];
    if (f.style) bits.push(`风格 ${f.style}`);
    if (r.layout) bits.push("含排版");
    if (r.project) bits.push("本项目");
    meta.textContent = bits.join(" · ");
    main.append(name, meta);
    const use = document.createElement("button");
    use.className = "btn small primary";
    use.textContent = "应用";
    use.onclick = () => applyRecipe(r);
    const del = document.createElement("button");
    del.className = "btn small ghost";
    del.textContent = "删";
    del.onclick = async () => {
      if (!confirm(`删除配方「${r.name}」？`)) return;
      try {
        await api.call("recipe_delete", { id: r.id });
        recipeCache = await api.call("recipe_list");
        renderRecipes();
      } catch (e) {
        toast(String(e), true);
      }
    };
    row.append(main, use, del);
    box.appendChild(row);
  }
}

async function openRecipes() {
  $("recipesModal").hidden = false;
  try {
    recipeCache = (await api.call("recipe_list")) || [];
  } catch (e) {
    recipeCache = [];
    toast(String(e), true);
  }
  renderRecipes();
  $("recipeName").focus();
}

$("recipeSaveBtn").onclick = () => recipeSaveFromForm();
$("recipeClose").onclick = () => ($("recipesModal").hidden = true);
$("recipesModal").addEventListener("click", (e) => {
  if (e.target === $("recipesModal")) $("recipesModal").hidden = true;
});
