// 版本 A/B 对比（#25）：版本窗勾两条 → 并排 / 滑块 + prompt·参数 diff + 「采用这版」。
// 依赖 versions.js（verCache/diffInto/fillVersion）与 library.js（libUrl）。

let abA = null;
let abB = null;
let abMode = "side";

function abEl(tag, cls, text) {
  const el = document.createElement(tag);
  if (cls) el.className = cls;
  if (text !== undefined && text !== null) el.textContent = text;
  return el;
}

function abVersionNo(v) {
  const i = verCache.findIndex((x) => x.id === v.id);
  return i >= 0 ? i + 1 : "?";
}

function abCapText(v) {
  if (!v) return "";
  const stars = "★".repeat(v.stars || 0);
  return `#${abVersionNo(v)} · ${stars} · ${new Date(v.createdAt).toLocaleString()}`;
}

function abImageOf(v) {
  if (!v || !v.images || !v.images.length) return "";
  return v.images[v.images.length - 1];
}

async function abLoadPane(side, v) {
  const img = side === "a" ? $("abImgA") : $("abImgB");
  const cap = side === "a" ? $("abCapA") : $("abCapB");
  img.removeAttribute("src");
  img.alt = "";
  cap.textContent = abCapText(v);
  const path = abImageOf(v);
  if (!path) {
    img.alt = v && v.kind === "layout" ? "排版版本没有出图" : "这版没有图";
    return false;
  }
  try {
    img.src = await libUrl(path);
    return true;
  } catch {
    img.alt = "读取失败";
    return false;
  }
}

function abSetPos(pct) {
  const pos = Math.max(0, Math.min(100, Number(pct) || 0));
  $("abStage").style.setProperty("--ab-pos", `${pos}%`);
  $("abRange").value = String(Math.round(pos));
  $("abRangeText").textContent = `${Math.round(pos)}%`;
}

function abApplyMode() {
  const slide = abMode === "slide";
  $("abStage").classList.toggle("slide", slide);
  $("abBar").hidden = !slide;
  $("abDivider").hidden = !slide;
  $("abSide").classList.toggle("ghost", slide);
  $("abSlide").classList.toggle("ghost", !slide);
  abFitStage();
  if (slide) abSetPos($("abRange").value);
}

/// 滑块模式下舞台按 A 图的宽高比收窄：不这样的话图是居中信箱式的，
/// 拉杆百分比落在「舞台」上而不是「画面」上，看着会像只切到边角。
function abFitStage() {
  const stage = $("abStage");
  if (!stage.classList.contains("slide")) {
    stage.style.width = "";
    return;
  }
  const img = $("abImgA");
  const h = stage.clientHeight || Math.round(window.innerHeight * 0.52);
  if (!img || !img.naturalWidth || !img.naturalHeight) {
    stage.style.width = "";
    return;
  }
  const w = Math.round(h * (img.naturalWidth / img.naturalHeight));
  stage.style.width = `${Math.min(w, Math.round(window.innerWidth - 120))}px`;
}

function abParamRows(v) {
  return [
    ["尺寸", v.width || v.height ? `${v.width || 0}×${v.height || 0}` : ""],
    ["步数", v.steps ? String(v.steps) : ""],
    ["CFG", v.cfg ? String(v.cfg) : ""],
    ["种子", v.seed !== undefined && v.seed !== null ? String(v.seed) : ""],
    ["模型", v.profile || ""],
    ["模式", v.mode || ""],
    ["排版", v.layout ? `${String(v.layout).length} 字符` : "无"],
  ];
}

function renderAbDiff() {
  const box = $("abDiff");
  box.textContent = "";
  if (!abA || !abB) return;

  const promptTitle = abEl("div", "ld-label", "提示词（A → B）");
  const promptDiff = abEl("div", "");
  diffInto(promptDiff, abA.prompt || "", abB.prompt || "");
  box.append(promptTitle, promptDiff);

  if ((abA.negative || "") !== (abB.negative || "")) {
    const negTitle = abEl("div", "ld-label", "负面（A → B）");
    const negDiff = abEl("div", "");
    diffInto(negDiff, abA.negative || "", abB.negative || "");
    box.append(negTitle, negDiff);
  }

  const rowsA = abParamRows(abA);
  const rowsB = abParamRows(abB);
  const paramsTitle = abEl("div", "ld-label", "参数");
  box.appendChild(paramsTitle);
  for (let i = 0; i < rowsA.length; i++) {
    const k = rowsA[i][0];
    const va = rowsA[i][1];
    const vb = rowsB[i][1];
    const row = abEl("div", "ab-row");
    row.appendChild(abEl("span", "k", k));
    if (va === vb) {
      row.appendChild(abEl("span", "v", va || "—"));
    } else {
      row.append(abEl("span", "old", va || "（空）"), abEl("span", "v", "→"), abEl("span", "chg", vb || "（空）"));
    }
    box.appendChild(row);
  }
}

function abSyncLabels() {
  $("abHint").textContent = abA && abB ? `#${abVersionNo(abA)} vs #${abVersionNo(abB)}` : "";
  $("abUseA").textContent = abA ? `采用 A（#${abVersionNo(abA)}）的参数` : "采用 A 的参数";
  $("abUseB").textContent = abB ? `采用 B（#${abVersionNo(abB)}）的参数` : "采用 B 的参数";
}

async function openVersionAB(a, b) {
  if (!a || !b) return;
  abA = a;
  abB = b;
  abMode = "side";
  $("abModal").hidden = false;
  abSyncLabels();
  abApplyMode();
  const [okA, okB] = await Promise.all([abLoadPane("a", a), abLoadPane("b", b)]);
  renderAbDiff();
  $("abSlide").disabled = !(okA && okB);
  if (!(okA && okB)) {
    abMode = "side";
    abApplyMode();
  } else {
    abFitStage(); // 图加载完才知道宽高比
  }
}

function abUse(v) {
  if (!v || typeof fillVersion !== "function") return;
  fillVersion(v);
  $("abModal").hidden = true;
  toast(`已采用 #${abVersionNo(v)} 的参数`);
}

$("abClose").onclick = () => ($("abModal").hidden = true);
$("abSide").onclick = () => {
  abMode = "side";
  abApplyMode();
};
$("abSlide").onclick = () => {
  abMode = "slide";
  abApplyMode();
};
$("abRange").oninput = () => abSetPos($("abRange").value);
$("abSwap").onclick = async () => {
  if (!abA || !abB) return;
  [abA, abB] = [abB, abA];
  abSyncLabels();
  const [okA, okB] = await Promise.all([abLoadPane("a", abA), abLoadPane("b", abB)]);
  renderAbDiff();
  $("abSlide").disabled = !(okA && okB);
  if (!(okA && okB) && abMode === "slide") {
    abMode = "side";
    abApplyMode();
  } else {
    abFitStage();
  }
};
$("abUseA").onclick = () => abUse(abA);
$("abUseB").onclick = () => abUse(abB);
