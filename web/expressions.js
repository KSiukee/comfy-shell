// 表情辅助器：一键挑一个人物表情，接在提示词后面。
// 预设是通用的（去掉性别/镜头/画质词），中英文都认。

let exprData = null;
let exprGroup = "";
let exprQuery = "";

async function exprLoad() {
  if (exprData) return exprData;
  const res = await fetch(encodeURI("expressions.json"));
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  exprData = await res.json();
  return exprData;
}

function exprMatches(it) {
  const q = exprQuery.trim().toLowerCase();
  if (!q) return true;
  return it.n.toLowerCase().includes(q) || it.zh.toLowerCase().includes(q) || it.g.includes(q);
}

function exprApply(it) {
  const box = $("prompt");
  const cur = box.value.trim();
  if (cur.includes(it.zh)) {
    toast(`「${it.n}」已经在提示词里了`);
    return;
  }
  box.value = cur ? `${cur}，${it.zh}` : it.zh;
  box.focus();
  box.setSelectionRange(box.value.length, box.value.length);
  toast(`已加入「${it.n}」（${it.zh.length} 字）`);
}

async function renderExpressions() {
  const tabs = $("exprTabs");
  const list = $("exprList");
  let data;
  try {
    data = await exprLoad();
  } catch (e) {
    list.textContent = `表情预设没读到（web/expressions.json）：${e}`;
    return;
  }
  tabs.textContent = "";
  const all = document.createElement("button");
  all.type = "button";
  all.className = "style-chip" + (exprGroup ? "" : " on");
  all.textContent = `全部 ${data.items.length}`;
  all.onclick = () => {
    exprGroup = "";
    renderExpressions();
  };
  tabs.appendChild(all);
  for (const g of data.groups) {
    const n = data.items.filter((i) => i.g === g).length;
    if (!n) continue;
    const b = document.createElement("button");
    b.type = "button";
    b.className = "style-chip" + (exprGroup === g ? " on" : "");
    b.textContent = `${g} ${n}`;
    b.onclick = () => {
      exprGroup = g;
      renderExpressions();
    };
    tabs.appendChild(b);
  }
  const items = data.items.filter((i) => (!exprGroup || i.g === exprGroup) && exprMatches(i));
  list.textContent = "";
  if (!items.length) {
    const d = document.createElement("div");
    d.className = "muted";
    d.style.cssText = "padding:14px 2px;font-size:12px";
    d.textContent = "没有匹配的表情";
    list.appendChild(d);
    return;
  }
  for (const it of items) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "expr-card";
    b.title = `${it.g} · ${it.zh}`;
    const name = document.createElement("b");
    name.textContent = it.n;
    const desc = document.createElement("span");
    desc.textContent = it.zh;
    b.append(name, desc);
    b.onclick = () => exprApply(it);
    list.appendChild(b);
  }
  $("exprCount").textContent = `${items.length}/${data.items.length} 个`;
}

async function exprOpen() {
  $("exprModal").hidden = false;
  await renderExpressions();
  $("exprSearch").focus();
}

$("exprBtn").onclick = () => exprOpen();
$("exprClose").onclick = () => ($("exprModal").hidden = true);
$("exprModal").addEventListener("click", (e) => {
  if (e.target === $("exprModal")) $("exprModal").hidden = true;
});
$("exprSearch").oninput = () => {
  exprQuery = $("exprSearch").value;
  renderExpressions();
};
