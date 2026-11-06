// 顶栏（项目栏那一排）的显示开关。
//
// 背景：功能越加越多，顶栏一排塞满了按钮；有些功能其实只在「提示词辅助」里用
// （镜头 / 角色 / 提示词库）。这里给用户一个自由取舍：勾上的挂顶栏，没勾的收起来，
// 功能本身不受影响——它们都还能从「提示词辅助」进去。
//
// 记在本机（localStorage），按 id 存；新增按钮只要往 TOPBAR_ITEMS 里加一行。
// 默认值就是「推荐的清爽布局」：镜头 / 角色 / 提示词库默认不占顶栏。

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

function topbarSaved() {
  try {
    const v = JSON.parse(localStorage.getItem(TOPBAR_KEY) || "{}");
    return v && typeof v === "object" && !Array.isArray(v) ? v : {};
  } catch {
    return {};
  }
}

function topbarVisible(id) {
  const item = TOPBAR_ITEMS.find((x) => x.id === id);
  if (!item) return false;
  const saved = topbarSaved();
  return Object.prototype.hasOwnProperty.call(saved, id) ? !!saved[id] : !!item.def;
}

function topbarApply() {
  for (const it of TOPBAR_ITEMS) {
    const el = document.querySelector(it.sel);
    if (!el) continue;
    el.hidden = !topbarVisible(it.id);
  }
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

function topbarRender() {
  const box = $("topbarList");
  if (!box) return;
  box.textContent = "";
  for (const it of TOPBAR_ITEMS) {
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
