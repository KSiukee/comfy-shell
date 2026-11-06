// 顶栏（项目栏那一排）的显示开关。
//
// 背景：功能越加越多，顶栏一排塞满了按钮；有些功能其实只在「提示词辅助」里用
// （镜头 / 角色 / 提示词库）。这里给用户一个自由取舍：勾上的挂顶栏，没勾的收起来，
// 功能本身不受影响——它们都还能从「提示词辅助」进去。
//
// 两组可选：
//   1) 顶栏原有的功能按钮（TOPBAR_ITEMS）；
//   2) 提示词辅助里的工具，钉到顶栏当快捷方式（HELPER_ITEMS，默认一个都不钉）。
//
// 记在本机（localStorage），按 id 存；新增按钮只要往对应表里加一行。

const TOPBAR_KEY = "cs.topbar";

const TOPBAR_ITEMS = [
  { id: "shots", sel: "#shotsBtn", label: "🎬 镜头", def: false },
  { id: "versions", sel: "#versionsBtn", label: "📜 版本", def: true },
  { id: "prompts", sel: "#promptsBtn", label: "📚 提示词库", def: false },
  { id: "chars", sel: "#charBtn", label: "🧑 角色", def: false },
  { id: "ai", sel: "#btnAiTop", label: "🤖 AI 助手", def: true },
  { id: "clear", sel: "#btnClearHistory", label: "清空历史", def: true },
  { id: "out", sel: "#btnOpenOut", label: "打开输出目录", def: true },
  { id: "hidden", sel: "#projShowHidden", label: "显示隐藏", def: true },
];

/** 调一个全局函数；没加载就 toast，不炸。 */
const topbarCall = (name) => () => {
  const f = window[name];
  if (typeof f !== "function") return toast(`「${name}」这会儿用不了`, true);
  try {
    return f();
  } catch (e) {
    toast(String(e), true);
  }
};

/** 点一个已有按钮；当前模型/模式下不可用就说清楚。 */
const topbarClick = (id, what) => () => {
  const el = $(id);
  if (!el || el.disabled || el.hidden) return toast(`当前模型/模式下没有「${what}」`, true);
  el.click();
};

const HELPER_ITEMS = [
  { id: "h_compile", label: "🧩 编译", desc: "一句话 → 完整提示词", run: topbarCall("openCompile") },
  { id: "h_analyze", label: "🔍 解析图", desc: "丢一张图，拆成五块候选", run: topbarClick("btnAnalyze", "解析图") },
  { id: "h_deai", label: "🎞 去 AI 感", desc: "追加自然光 / 颗粒，压塑料感", run: topbarClick("btnDeAi", "去 AI 感") },
  { id: "h_enhance", label: "✨ 扩写", desc: "一句话扩成完整画面描述", run: topbarClick("btnEnhance", "扩写") },
  { id: "h_poses", label: "🕺 姿势库", desc: "白模姿势：点一条抄描述或加参考图", run: topbarClick("btnPoses2", "姿势库") },
  { id: "h_layout", label: "🎨 排版", desc: "MOSS 专用排版画布", run: topbarClick("btnLayout", "排版") },
  { id: "h_assist", label: "🤖 助手", desc: "中文需求 → 翻译 / 扩写 → 提示词", run: topbarClick("btnAssist", "助手") },
  { id: "h_lint", label: "🩺 体检", desc: "扫通配符 / LoRA / 质量词 / 超长", run: topbarCall("openLint") },
  { id: "h_draft", label: "📐 排版草稿", desc: "用本机模型写一份 MOSS 排版 JSON", run: topbarCall("mossDraft") },
  { id: "h_recipes", label: "🧾 配方库", desc: "模型 + LoRA + 参数 + 风格一键套回", run: topbarCall("openRecipes") },
  { id: "h_help", label: "📖 使用说明", desc: "人话版说明书", run: topbarCall("openHelp") },
];

function topbarSaved() {
  try {
    const v = JSON.parse(localStorage.getItem(TOPBAR_KEY) || "{}");
    return v && typeof v === "object" && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
}

function topbarVisible(id) {
  const item = TOPBAR_ITEMS.find((x) => x.id === id) || HELPER_ITEMS.find((x) => x.id === id);
  if (!item) return false;
  const saved = topbarSaved();
  return Object.prototype.hasOwnProperty.call(saved, id) ? !!saved[id] : !!item.def;
}

/** 把钉住的提示词辅助工具画成顶栏按钮（没钉就整条收起）。 */
function topbarPaintPin() {
  const bar = $("pinBar");
  if (!bar) return;
  bar.textContent = "";
  let n = 0;
  for (const it of HELPER_ITEMS) {
    if (!topbarVisible(it.id)) continue;
    const b = document.createElement("button");
    b.type = "button";
    b.className = "btn small ghost";
    b.textContent = it.label;
    b.title = it.desc;
    b.dataset.tool = it.id;
    b.onclick = () => {
      if (typeof closePh === "function") closePh();
      it.run();
    };
    bar.appendChild(b);
    n++;
  }
  bar.hidden = n === 0;
}

function topbarApply() {
  for (const it of TOPBAR_ITEMS) {
    const el = document.querySelector(it.sel);
    if (!el) continue;
    el.hidden = !topbarVisible(it.id);
  }
  topbarPaintPin();
}

function topbarSet(id, on) {
  const saved = topbarSaved();
  saved[id] = !!on;
  localStorage.setItem(TOPBAR_KEY, JSON.stringify(saved));
  topbarApply();
}

function topbarReset() {
  localStorage.removeItem(TOPBAR_KEY);
  topbarApply();
  topbarRender();
  toast("顶栏已恢复默认");
}

function topbarRenderList(boxId, items) {
  const box = $(boxId);
  if (!box) return;
  box.textContent = "";
  for (const it of items) {
    const row = document.createElement("label");
    row.className = "check topbar-item";
    const cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = topbarVisible(it.id);
    cb.onchange = () => topbarSet(it.id, cb.checked);
    const txt = document.createElement("span");
    txt.textContent = it.label;
    row.append(cb, txt);
    box.appendChild(row);
  }
}

function topbarRender() {
  topbarRenderList("topbarList", TOPBAR_ITEMS);
  topbarRenderList("topbarTools", HELPER_ITEMS);
}

function topbarOpen(force) {
  const pop = $("topbarPop");
  if (!pop) return;
  const want = force === undefined ? pop.hidden : force;
  pop.hidden = !want;
  if (want) topbarRender();
}

function topbarInit() {
  topbarApply();
  const btn = $("phTopbar");
  if (btn) btn.onclick = () => topbarOpen();
  const reset = $("topbarReset");
  if (reset) reset.onclick = topbarReset;
  // 点面板外面收起（点勾选框不算外面）
  document.addEventListener("click", (e) => {
    const pop = $("topbarPop");
    if (!pop || pop.hidden) return;
    if (pop.contains(e.target) || (e.target && e.target.id === "phTopbar")) return;
    pop.hidden = true;
  });
}

topbarInit();
