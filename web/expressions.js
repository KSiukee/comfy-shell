// 提示词添加器：表情 / 光线 / 构图 / 镜头 / 机位，点一下接进提示词。
// 数据在 web/expressions.json（表情 80 条，含分组）与 web/adders.json（其余四类）。
// 中英双语；★ 收藏存 localStorage。

let adderData = null; // [{key,label,items:[{n,zh,en,g}]}]
let adderCat = "";
let adderQuery = "";
let adderLang = localStorage.getItem("cs.adderLang") || "zh";
let adderMineAll = false;
const ADDER_FAV = "cs.adderFav";
const PHRASE_HIDE = "cs.phraseHide";

function phraseHidden() {
  try {
    const v = JSON.parse(localStorage.getItem(PHRASE_HIDE) || "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

function phraseHide(text) {
  const list = phraseHidden();
  if (!list.includes(text)) list.push(text);
  localStorage.setItem(PHRASE_HIDE, JSON.stringify(list));
}

function adderProjectId() {
  return window.projectUI && window.projectUI.current ? window.projectUI.current() : "";
}

const PHRASE_EXCLUDE = "cs.phraseExcludeProjects";

function phraseExcluded() {
  try {
    const v = JSON.parse(localStorage.getItem(PHRASE_EXCLUDE) || "[]");
    return Array.isArray(v) ? v : [];
  } catch {
    return [];
  }
}

function phraseExclude(id) {
  const list = phraseExcluded();
  if (!id || list.includes(id)) return;
  list.push(id);
  localStorage.setItem(PHRASE_EXCLUDE, JSON.stringify(list));
}

function phraseRestore(id) {
  const list = phraseExcluded().filter((x) => x !== id);
  localStorage.setItem(PHRASE_EXCLUDE, JSON.stringify(list));
}

/// 项目 id → 名字；老记录没有项目（id 空串）显示成「早先的记录」。
function phraseProjectName(id) {
  if (!id) return "早先的记录";
  const list = (window.projectUI && window.projectUI.list ? window.projectUI.list() : []) || [];
  const p = list.find((x) => x.id === id);
  return p ? p.name || p.id : id;
}

function phraseSourceText(it) {
  const parts = (it.projects || []).slice(0, 4).map((p) => phraseProjectName(p.id));
  const more = (it.projects || []).length > 4 ? ` 等 ${it.projects.length} 个` : "";
  return parts.length ? `来自 ${parts.join("、")}${more}` : "";
}

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
  const buttons = [
    { key: "fav", label: `★ 收藏 ${favN}` },
    { key: "mine", label: "我的用语" },
    ...cats.map((c) => ({ key: c.key, label: `${c.label} ${c.items.length}` })),
  ];
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

  let items;
  if (adderCat === "mine") {
    try {
      const rows = await api.call("phrase_mine", {
        project: adderMineAll ? null : adderProjectId() || null,
        hidden: phraseHidden(),
        excluded: phraseExcluded(),
      });
      items = (rows || [])
        .map((r) => ({
          n: r.text,
          zh: r.text,
          en: r.text,
          g: "",
          cat: "mine",
          mine: true,
          count: r.count,
          sources: r.sources,
          projects: r.projects || [],
        }))
        .filter(adderMatches);
    } catch (e) {
      items = [];
      toast(String(e), true);
    }
  } else {
    items = adderItems();
  }
  const box = $("exprList");
  box.textContent = "";
  $("exprCount").textContent = `${items.length} 个`;
  if (!items.length) {
    const d = document.createElement("div");
    d.className = "muted";
    d.style.cssText = "padding:14px 2px;font-size:12px";
    if (adderCat === "fav") d.textContent = "还没有收藏：点卡片右上角的 ☆";
    else if (adderCat === "mine" && !adderMineAll) d.textContent = "这个项目里还没有重复出现 3 次以上的短语。多写几条提示词，或者先看看全部项目。";
    else if (adderCat === "mine") d.textContent = "还挖不到常用短语：同一个小片段至少要在 2 条提示词里出现 3 次。";
    else d.textContent = "没有匹配的条目";
    box.appendChild(d);
    if (adderCat === "mine" && !adderMineAll) {
      const b = document.createElement("button");
      b.type = "button";
      b.className = "btn small ghost";
      b.style.marginTop = "8px";
      b.textContent = "看看全部项目";
      b.onclick = () => {
        adderMineAll = true;
        renderAdders();
      };
      box.appendChild(b);
    }
    return;
  }
  if (adderCat === "mine") {
    const ex = phraseExcluded();
    if (ex.length) {
      const row = document.createElement("div");
      row.className = "row tight";
      row.style.gridColumn = "1 / -1";
      row.style.flexWrap = "wrap";
      const lab = document.createElement("span");
      lab.className = "muted";
      lab.style.fontSize = "11.5px";
      lab.textContent = "已排除的项目：";
      row.appendChild(lab);
      for (const id of ex) {
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "style-chip on";
        chip.textContent = `${phraseProjectName(id)} ×`;
        chip.title = "点一下恢复这个项目的挖掘";
        chip.onclick = () => {
          phraseRestore(id);
          renderAdders();
        };
        row.appendChild(chip);
      }
      box.appendChild(row);
    }
  }
  for (const it of items) {
    const card = document.createElement("div");
    card.className = "expr-card";
    card.title = it.g ? `${it.g} · ${it.zh}` : it.zh;
    const name = document.createElement("b");
    name.textContent = it.n;
    const desc = document.createElement("span");
    if (it.mine) {
      const src = phraseSourceText(it);
      desc.textContent = `出现 ${it.count} 次 · 跨 ${it.sources} 条${src ? " · " + src : ""}`;
    } else {
      desc.textContent = adderText(it);
    }
    const star = document.createElement("button");
    star.type = "button";
    if (it.mine) {
      star.className = "expr-star";
      star.textContent = "×";
      star.title = "不是我的词：不再出现";
      star.onclick = (e) => {
        e.stopPropagation();
        phraseHide(it.n);
        renderAdders();
      };
    } else {
      star.className = "expr-star" + (adderIsFav(it.cat, it) ? " on" : "");
      star.textContent = adderIsFav(it.cat, it) ? "★" : "☆";
      star.title = "收藏";
      star.onclick = (e) => {
        e.stopPropagation();
        adderToggleFav(it.cat, it);
        renderAdders();
      };
    }
    card.append(name, desc, star);
    if (it.mine && (it.projects || []).length) {
      const srcRow = document.createElement("div");
      srcRow.className = "phrase-src";
      for (const p of it.projects.slice(0, 4)) {
        const label = `${phraseProjectName(p.id)} ${p.count}`;
        if (!p.id) {
          // 老记录没有项目信息，排除不了，就不做成可点的
          const span = document.createElement("span");
          span.className = "phrase-src-chip";
          span.style.cursor = "default";
          span.textContent = label;
          span.title = "早先的记录没有项目信息，没法按项目排除";
          srcRow.appendChild(span);
          continue;
        }
        const chip = document.createElement("button");
        chip.type = "button";
        chip.className = "phrase-src-chip";
        chip.textContent = label;
        chip.title = "点一下：把这个项目从挖掘里排除";
        chip.onclick = (e) => {
          e.stopPropagation();
          phraseExclude(p.id);
          toast(`已排除「${phraseProjectName(p.id)}」的挖掘`);
          renderAdders();
        };
        srcRow.appendChild(chip);
      }
      card.appendChild(srcRow);
    }
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
