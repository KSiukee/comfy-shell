// 提示词框内联补全（词库版）：打字时在光标下方给候选，Tab / Enter 上屏，Esc 关掉。
// 只认预置的那 137 条（表情 / 光线 / 构图 / 镜头 / 机位），不做模型续写。
// 中文输入法组合期间不弹、不拦键——那段时间键盘归输入法。

let pbItems = null;
let pbMatches = [];
let pbIndex = 0;
let pbSegment = "";
let pbComposing = false;

const PB_MAX = 8;
const PB_MIN_ASCII = 2;

async function pbLoad() {
  if (pbItems) return pbItems;
  const cats = await adderLoad();
  const out = [];
  for (const c of cats) {
    for (const it of c.items) {
      out.push({ n: it.n, zh: it.zh, en: it.en || it.zh, g: it.g || "", cat: c.key, catLabel: c.label });
    }
  }
  pbItems = out;
  return out;
}

/// 光标前那一段（上一个标点之后），返回 {text, start}。
const PB_SEP = /[，,。.；;、\n\r\t|/（）()【】[\]]/;

function pbSegmentBefore(textarea) {
  const pos = textarea.selectionStart || 0;
  const before = textarea.value.slice(0, pos);
  let cut = 0;
  for (let i = before.length - 1; i >= 0; i--) {
    if (PB_SEP.test(before[i])) {
      cut = i + 1;
      break;
    }
  }
  return { text: before.slice(cut), start: cut };
}

function pbScore(it, seg) {
  const q = seg.toLowerCase();
  const name = it.n.toLowerCase();
  const body = (it.zh + " " + it.en).toLowerCase();
  const grp = (it.g || it.catLabel || "").toLowerCase();
  if (name.startsWith(q)) return 100;
  if (name.includes(q)) return 80;
  if (grp.includes(q)) return 55;
  if (body.includes(q)) return 40;
  return 0;
}

async function pbQuery(seg) {
  const items = await pbLoad();
  const scored = [];
  for (const it of items) {
    const s = pbScore(it, seg);
    if (s > 0) scored.push({ it, s });
  }
  scored.sort((a, b) => b.s - a.s || a.it.n.length - b.it.n.length);
  return scored.slice(0, PB_MAX).map((x) => x.it);
}

/// 光标在 textarea 里的像素位置（镜像 div，标准做法）。
function pbCaretPos(textarea) {
  const div = document.createElement("div");
  const cs = getComputedStyle(textarea);
  const props = [
    "boxSizing", "width", "paddingTop", "paddingRight", "paddingBottom", "paddingLeft",
    "borderTopWidth", "borderLeftWidth", "fontFamily", "fontSize", "fontWeight", "lineHeight",
    "letterSpacing", "textAlign", "whiteSpace", "wordBreak", "overflowWrap",
  ];
  for (const p of props) div.style[p] = cs[p];
  div.style.position = "absolute";
  div.style.visibility = "hidden";
  div.style.whiteSpace = "pre-wrap";
  div.style.wordWrap = "break-word";
  div.style.overflow = "hidden";
  div.style.top = "0";
  div.style.left = "-9999px";
  div.textContent = textarea.value.slice(0, textarea.selectionStart || 0);
  const span = document.createElement("span");
  span.textContent = textarea.value.slice(textarea.selectionStart || 0) || ".";
  div.appendChild(span);
  document.body.appendChild(div);
  const top = span.offsetTop - textarea.scrollTop;
  const left = span.offsetLeft - textarea.scrollLeft;
  const box = textarea.getBoundingClientRect();
  document.body.removeChild(div);
  return {
    left: box.left + left - parseFloat(cs.paddingLeft || 0) + window.scrollX,
    top: box.top + top + parseFloat(cs.lineHeight || "18") + window.scrollY,
  };
}

function pbClose() {
  const box = $("pbPanel");
  if (box) box.hidden = true;
  pbMatches = [];
  pbIndex = 0;
}

function pbRender() {
  const box = $("pbPanel");
  if (!box) return;
  if (!pbMatches.length) {
    pbClose();
    return;
  }
  box.textContent = "";
  pbMatches.forEach((it, i) => {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "pb-row" + (i === pbIndex ? " on" : "");
    const name = document.createElement("b");
    name.textContent = it.n;
    const kind = document.createElement("span");
    kind.className = "pb-kind";
    kind.textContent = it.catLabel || it.cat;
    const text = document.createElement("span");
    text.className = "pb-text";
    text.textContent = pbLangFor(pbSegment) === "en" ? it.en : it.zh;
    row.append(name, kind, text);
    row.onmousedown = (e) => {
      e.preventDefault();
      pbAccept(i);
    };
    box.appendChild(row);
  });
  box.hidden = false;
  const ta = $("prompt");
  const pos = pbCaretPos(ta);
  const boxW = box.offsetWidth || 360;
  const maxLeft = window.scrollX + document.documentElement.clientWidth - boxW - 12;
  box.style.left = Math.max(12, Math.min(pos.left, maxLeft)) + "px";
  box.style.top = pos.top + "px";
}

/// 上屏用哪种语言：你打的这段是中文就上中文，是英文就上英文（而不是跟着「我的用语」那个开关）。
function pbLangFor(seg) {
  return /[\u4e00-\u9fff]/.test(seg) ? "zh" : "en";
}

function pbAccept(i) {
  const it = pbMatches[i];
  if (!it) return;
  const ta = $("prompt");
  const pos = ta.selectionStart || 0;
  const text = ta.value;
  const start = pos - pbSegment.length;
  const lang = pbLangFor(pbSegment);
  const insert = (lang === "en" ? it.en : it.zh) || it.zh;
  const lead = (pbSegment.match(/^\s*/) || [""])[0];
  ta.value = text.slice(0, start) + lead + insert + text.slice(pos);
  const caret = start + lead.length + insert.length;
  ta.focus();
  ta.setSelectionRange(caret, caret);
  pbClose();
}

async function pbUpdate() {
  const ta = $("prompt");
  if (!ta || pbComposing || document.activeElement !== ta) {
    pbClose();
    return;
  }
  const { text } = pbSegmentBefore(ta);
  const seg = text.trim();
  const ascii = /^[\x00-\x7F]+$/.test(seg);
  if (!seg || seg.length > 20 || (ascii && seg.length < PB_MIN_ASCII)) {
    pbClose();
    return;
  }
  const matches = await pbQuery(seg);
  pbSegment = text;
  pbMatches = matches;
  if (pbIndex >= pbMatches.length) pbIndex = 0;
  pbRender();
}

function pbKey(e) {
  if (!pbMatches.length) return false;
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    pbIndex = (pbIndex + (e.key === "ArrowDown" ? 1 : -1) + pbMatches.length) % pbMatches.length;
    pbRender();
    return true;
  }
  if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey && !e.ctrlKey && !e.metaKey)) {
    e.preventDefault();
    pbAccept(pbIndex);
    return true;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    pbClose();
    return true;
  }
  return false;
}

function pbInit() {
  const ta = $("prompt");
  if (!ta) return;
  ta.addEventListener("compositionstart", () => {
    pbComposing = true;
    pbClose();
  });
  ta.addEventListener("compositionend", () => {
    pbComposing = false;
    pbUpdate();
  });
  ta.addEventListener("input", () => {
    if (!pbComposing) pbUpdate();
  });
  ta.addEventListener("keydown", (e) => {
    if (pbKey(e)) return;
  });
  ta.addEventListener("blur", () => setTimeout(pbClose, 120));
  ta.addEventListener("scroll", () => pbClose());
  window.addEventListener("resize", () => pbClose());
  document.addEventListener("mousedown", (e) => {
    if (e.target !== ta && !$("pbPanel").contains(e.target)) pbClose();
  });
}

pbInit();
