// 提示词辅助器：把散在提示词栏的「姿势 / 去AI感 / 扩写 / 解析图 / 排版 / 助手」收进一张卡片页。
// 只做入口，不搬逻辑——卡片点下去就是点原来那个隐藏按钮，行为一字不差。

const PH_TILES = [
  { icon: "🧩", title: "编译", desc: "一句话 → 按当前模型方言写完整提示词（Qwen 中文长句 / MOSS 排版 / FLUX 自然语言）", onclick: () => openCompile() },
  { src: "btnPoses2", icon: "🕺", title: "姿势库", desc: "白模姿势：点一条抄描述，或直接加参考图" },
  { src: "btnDeAi", icon: "🎞", title: "去 AI 感", desc: "追加自然光 / 颗粒 / 轻微畸变，压掉塑料感" },
  { src: "btnEnhance", icon: "✨", title: "扩写", desc: "一句话扩成完整画面描述（官方增强器）" },
  { src: "btnAnalyze", icon: "🔍", title: "解析图", desc: "丢一张图，拆成分类提示词" },
  { src: "btnLayout", icon: "🎨", title: "排版", desc: "MOSS 专用：拖框定位置、文字物体分开标" },
  { src: "btnAssist", icon: "🤖", title: "助手", desc: "中文需求 → 翻译 / 扩写 → 提示词" },
];

function phBuild() {
  const grid = $("phGrid");
  if (!grid) return;
  grid.textContent = "";
  const tiles = PH_TILES.concat(window.PH_EXTRA || []);
  for (const t of tiles) {
    const src = t.src ? $(t.src) : null;
    const b = document.createElement("button");
    b.type = "button";
    b.className = "ph-tile";
    const ico = document.createElement("span");
    ico.className = "ico";
    ico.textContent = t.icon;
    const txt = document.createElement("span");
    txt.className = "txt";
    const head = document.createElement("b");
    head.textContent = t.title;
    const desc = document.createElement("span");
    desc.textContent = t.desc;
    txt.append(head, desc);
    b.append(ico, txt);
    const off = t.onclick ? false : !src || src.hidden || src.disabled;
    b.disabled = off;
    if (off) b.title = src && src.disabled ? "当前模型/模式下不可用" : "这个模型没有它";
    b.onclick = () => {
      closePh();
      if (t.onclick) t.onclick();
      else if (src) src.click();
    };
    grid.appendChild(b);
  }
}

function openPh() {
  phBuild();
  $("phModal").hidden = false;
  if (typeof updateFxHint === "function") updateFxHint();
}

function closePh() {
  $("phModal").hidden = true;
}

$("phOpen").onclick = openPh;
$("phClose").onclick = closePh;
$("phModal").addEventListener("click", (e) => {
  if (e.target === $("phModal")) closePh();
});
