// 提示词添加器：表情 / 光线 / 构图 / 镜头 / 机位，点一下接进提示词。
// 数据在 web/expressions.json（表情 80 条，含分组）与 web/adders.json（其余四类）。
// 中英双语；★ 收藏存 localStorage。

let adderData = null; // [{key,label,items:[{n,zh,en,g}]}]
let adderCat = "";
let adderQuery = "";
let adderLang = localStorage.getItem("cs.adderLang") || "zh";
const ADDER_FAV = "cs.adderFav";

function adderFavs() {
  try {
    const v = JSON.parse(localStorage.getItem(ADDER_FAV) || "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

function adderFavKey(cat, it) {
  return `${cat}|${it.n}`;
}

function adderIsFav(cat, it) {
  return adderFavs().includes(adderFavKey(cat, it));
}

function adderToggleFav(cat, it) {
  const key = adderFavKey(cat, it);
  const list = adderFavs();
  const i = list.indexOf(key);
  if (i >= 0) list.splice(i, 1);
  else list.push(key);
  localStorage.setItem(ADDER_FAV, JSON.stringify(list));
}

async function adderLoad() {
  if (adderData) return adderData;
  const [expr, adders] = await Promise.all([
    fetch(encodeURI("expressions.json")).then((r) => r.json()),
    fetch(encodeURI("adders.json")).then((r) => r.json()),
  ]);
  const cats = [
    {
      key: "expr",
      label: "表情",
      items: expr.items.map((it) => ({ n: it.n, zh: it.zh, en: it.en || it.zh, g: it.g })),
    },
  ];
  for (const c of adders.categories) {
    cats.push({ key: c.key, label: c.label, items: c.items.map((it) => ({ n: it.n, zh: it.zh, en: it.en || it.zh, g: "" })) });
  }
  adderData = cats;
  return cats;
}

function adderText(it) {
  return adderLang === "en" ? it.en || it.zh : it.zh;
}

function adderMatches(it) {
  const q = adderQuery.trim().toLowerCase();
  if (!q) return true;
  return (
    it.n.toLowerCase().includes(q) ||
    it.zh.toLowerCase().includes(q) ||
    (it.en || "").toLowerCase().includes(q) ||
    (it.g || "").includes(q)
  );
}

function adderApply(it) {
  const box = $("prompt");
  const text = adderText(it);
  const cur = box.value.trim();
  if (cur.includes(text)) {
    toast(`「${it.n}」已经在提示词里了`);
    return;
  }
  box.value = cur ? `${cur}，${text}` : text;
  box.focus();
  box.setSelectionRange(box.value.length, box.value.length);
  toast(`已加入「${it.n}」`);
}

function adderItems() {
  if (!adderData) return [];
  if (adderCat === "fav") {
    const keys = new Set(adderFavs());
    const out = [];
    for (const c of adderData) {
      for (const it of c.items) {
        if (keys.has(adderFavKey(c.key, it))) out.push({ ...it, cat: c.key });
      }
    }
    return out.filter(adderMatches);
  }
  const cat = adderData.find((c) => c.key === adderCat) || adderData[0];
  return cat.items.filter(adderMatches).map((it) => ({ ...it, cat: cat.key }));
}

async function renderAdders() {
  const tabs = $("exprTabs");
  let cats;
  try {
    cats = await adderLoad();
  } catch (e) {
    $("exprList").textContent = `提示词数据没读到：${e}`;
    return;
  }
  if (!adderCat) adderCat = "expr";
  tabs.textContent = "";
  const favN = adderFavs().length;
  const buttons = [{ key: "fav", label: `★ 收藏 ${favN}` }, ...cats.map((c) => ({ key: c.key, label: `${c.label} ${c.items.length}` }))];
  for (const b of buttons) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "style-chip" + ((adderCat || "expr") === b.key ? " on" : "");
    btn.textContent = b.label;
    btn.onclick = () => {
      adderCat = b.key;
      renderAdders();
    };
    tabs.appendChild(btn);
  }
  for (const lang of ["zh", "en"]) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "style-chip expr-lang" + (adderLang === lang ? " on" : "");
    btn.textContent = lang === "zh" ? "中文" : "EN";
    btn.onclick = () => {
      adderLang = lang;
      localStorage.setItem("cs.adderLang", lang);
      renderAdders();
    };
    tabs.appendChild(btn);
  }

  const items = adderItems();
  const box = $("exprList");
  box.textContent = "";
  $("exprCount").textContent = `${items.length} 个`;
  if (!items.length) {
    const d = document.createElement("div");
    d.className = "muted";
    d.style.cssText = "padding:14px 2px;font-size:12px";
    d.textContent = adderCat === "fav" ? "还没有收藏：点卡片右上角的 ☆" : "没有匹配的条目";
    box.appendChild(d);
    return;
  }
  for (const it of items) {
    const card = document.createElement("div");
    card.className = "expr-card";
    card.title = it.g ? `${it.g} · ${it.zh}` : it.zh;
    const name = document.createElement("b");
    name.textContent = it.n;
    const desc = document.createElement("span");
    desc.textContent = adderText(it);
    const star = document.createElement("button");
    star.type = "button";
    star.className = "expr-star" + (adderIsFav(it.cat, it) ? " on" : "");
    star.textContent = adderIsFav(it.cat, it) ? "★" : "☆";
    star.title = "收藏";
    star.onclick = (e) => {
      e.stopPropagation();
      adderToggleFav(it.cat, it);
      renderAdders();
    };
    card.append(name, desc, star);
    card.onclick = () => adderApply(it);
    box.appendChild(card);
  }
}

async function exprOpen() {
  $("exprModal").hidden = false;
  await renderAdders();
  $("exprSearch").focus();
}

$("exprBtn").onclick = () => exprOpen();
$("exprClose").onclick = () => ($("exprModal").hidden = true);
$("exprModal").addEventListener("click", (e) => {
  if (e.target === $("exprModal")) $("exprModal").hidden = true;
});
$("exprSearch").oninput = () => {
  adderQuery = $("exprSearch").value;
  renderAdders();
};
