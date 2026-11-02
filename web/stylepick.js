// 风格选择器：分组 + 收藏 + 删除。
// 数据在 promptfx.js 的 stylePacks 里；这里只管怎么挑、怎么收、怎么删。

const STYLE_FAV_KEY = "cs.styleFav";
const STYLE_GROUPS = [
  ["ads-", "广告"],
  ["artstyle-", "艺术流派"],
  ["futuristic-", "未来"],
  ["game-", "游戏"],
  ["misc-", "杂项"],
  ["papercraft-", "纸艺"],
  ["photo-", "摄影"],
  ["sai-", "官方"],
];

let favStyles = new Set(readFavs());
let styleChip = "全部";
let styleQuery = "";
let stylePendingDel = "";

function readFavs() {
  try {
    const v = JSON.parse(localStorage.getItem(STYLE_FAV_KEY) || "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

function saveFavs() {
  localStorage.setItem(STYLE_FAV_KEY, JSON.stringify([...favStyles]));
}

function styleGroupOf(s) {
  const n = String(s.name || "").toLowerCase();
  const hit = STYLE_GROUPS.find(([pre]) => n.startsWith(pre));
  return hit ? hit[1] : "其他";
}

function styleText(s) {
  return s.zh || s.name;
}

function styleGroupsPresent() {
  const seen = [];
  for (const s of stylePacks) {
    const g = styleGroupOf(s);
    if (!seen.includes(g)) seen.push(g);
  }
  const ordered = STYLE_GROUPS.map(([, g]) => g).filter((g) => seen.includes(g));
  if (seen.includes("其他")) ordered.push("其他");
  return ordered;
}

function stylePickRefresh() {
  const label = $("stylePickLabel");
  if (label) {
    const cur = typeof currentStyle === "function" ? currentStyle() : null;
    label.textContent = cur ? styleText(cur) : "（不套风格）";
  }
  if (!$("stylePop") || $("stylePop").hidden) return;
  renderStyleChips();
  renderStyleList();
}

function renderStyleChips() {
  const box = $("styleChips");
  if (!box) return;
  box.textContent = "";
  const chips = ["收藏", "全部", ...styleGroupsPresent()];
  for (const c of chips) {
    const n =
      c === "收藏"
        ? stylePacks.filter((s) => favStyles.has(s.name)).length
        : c === "全部"
          ? stylePacks.length
          : stylePacks.filter((s) => styleGroupOf(s) === c).length;
    const b = document.createElement("button");
    b.type = "button";
    b.className = "style-chip" + (styleChip === c ? " on" : "");
    b.textContent = n ? `${c} ${n}` : c;
    b.onclick = () => {
      styleChip = c;
      renderStyleChips();
      renderStyleList();
    };
    box.appendChild(b);
  }
}

function styleMatches(s) {
  if (styleChip === "收藏" && !favStyles.has(s.name)) return false;
  if (styleChip !== "收藏" && styleChip !== "全部" && styleGroupOf(s) !== styleChip) return false;
  const q = styleQuery.trim().toLowerCase();
  if (!q) return true;
  return String(s.zh || "").toLowerCase().includes(q) || String(s.name || "").toLowerCase().includes(q);
}

function renderStyleList() {
  const box = $("styleList");
  if (!box) return;
  box.textContent = "";
  const cur = typeof currentStyle === "function" ? currentStyle() : null;

  // 第一行永远是「不套风格」：没有它就没法退回裸提示词
  if (!styleQuery.trim() && (styleChip === "全部" || styleChip === "收藏")) {
    const none = document.createElement("div");
    none.className = "style-row" + (cur ? "" : " on");
    const main = document.createElement("button");
    main.type = "button";
    main.className = "style-main";
    const b = document.createElement("b");
    b.textContent = "（不套风格）";
    const d = document.createElement("span");
    d.className = "muted";
    d.textContent = "裸提示词，不加任何风格词";
    main.append(b, d);
    main.onclick = () => {
      setStyleName("");
      closeStylePop();
    };
    none.appendChild(main);
    box.appendChild(none);
  }

  const list = stylePacks.filter(styleMatches);
  if (!list.length) {
    const p = document.createElement("div");
    p.className = "muted style-empty";
    p.textContent = styleChip === "收藏" ? "还没有收藏：点条目右边的 ☆" : "没有匹配的风格";
    box.appendChild(p);
    return;
  }
  for (const s of list) {
    const row = document.createElement("div");
    row.className = "style-row" + (cur && cur.name === s.name ? " on" : "");
    const main = document.createElement("button");
    main.type = "button";
    main.className = "style-main";
    main.title = s.name;
    const zh = document.createElement("b");
    zh.textContent = styleText(s);
    main.appendChild(zh);
    if (s.zh && s.zh !== s.name) {
      const en = document.createElement("span");
      en.className = "muted";
      en.textContent = s.name;
      main.appendChild(en);
    }
    main.onclick = () => {
      setStyleName(s.name);
      closeStylePop();
    };
    row.appendChild(main);

    if (stylePendingDel === s.name) {
      const yes = document.createElement("button");
      yes.type = "button";
      yes.className = "style-icon danger";
      yes.textContent = "删";
      yes.title = "确认删除";
      yes.onclick = () => doStyleDelete(s.name);
      const no = document.createElement("button");
      no.type = "button";
      no.className = "style-icon";
      no.textContent = "否";
      no.onclick = () => {
        stylePendingDel = "";
        renderStyleList();
      };
      row.append(yes, no);
    } else {
      const star = document.createElement("button");
      star.type = "button";
      star.className = "style-icon" + (favStyles.has(s.name) ? " on" : "");
      star.textContent = favStyles.has(s.name) ? "★" : "☆";
      star.title = favStyles.has(s.name) ? "取消收藏" : "收藏";
      star.onclick = () => toggleStyleFav(s.name);
      const trash = document.createElement("button");
      trash.type = "button";
      trash.className = "style-icon";
      trash.textContent = "🗑";
      trash.title = "从磁盘删掉这个风格（改 styles/*.json）";
      trash.onclick = () => {
        stylePendingDel = s.name;
        renderStyleList();
      };
      row.append(star, trash);
    }
    box.appendChild(row);
  }
}

function toggleStyleFav(name) {
  if (favStyles.has(name)) favStyles.delete(name);
  else favStyles.add(name);
  saveFavs();
  renderStyleChips();
  renderStyleList();
}

async function doStyleDelete(name) {
  try {
    const ok = await api.call("style_delete", { name });
    if (!ok) throw new Error("没找到这个风格，可能已经删掉了");
    if (typeof styleName === "string" && styleName === name) setStyleName("");
    stylePendingDel = "";
    await loadStylePacks();
    toast(`已删掉「${name}」`);
  } catch (e) {
    toast(String(e), true);
    stylePendingDel = "";
    renderStyleList();
  }
}

function openStylePop() {
  $("stylePop").hidden = false;
  stylePendingDel = "";
  renderStyleChips();
  renderStyleList();
  const q = $("styleSearch");
  if (q) q.focus();
}

function closeStylePop() {
  $("stylePop").hidden = true;
  stylePendingDel = "";
}

$("stylePickBtn").onclick = () => {
  if ($("stylePop").hidden) openStylePop();
  else closeStylePop();
};
$("stylePopClose").onclick = closeStylePop;
$("styleSearch").addEventListener("input", () => {
  styleQuery = $("styleSearch").value;
  renderStyleList();
});
document.addEventListener("click", (e) => {
  if ($("stylePop").hidden) return;
  const t = e.target;
  // 点收藏/换分组会当场重画，target 已经从文档里摘下来了 —— 那不算「点外面」
  if (!t || !t.isConnected) return;
  if ($("stylePop").contains(t) || $("stylePickBtn").contains(t)) return;
  closeStylePop();
});

stylePickRefresh();
