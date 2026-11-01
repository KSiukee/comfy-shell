// 添加器的两排标签：上面一排是分组（视觉 / 人物 / 环境 / 风格），下面一排是该分组里的类别。
// 单独放一个文件，是为了让 expressions.js 不用为了标签变长而膨胀（上帝文件闸 400 行）。

const ADDER_GROUPS = ["视觉", "人物", "环境", "风格"];

function adderGroupOf(cats, key) {
  const c = cats.find((x) => x.key === key);
  return c && c.group ? c.group : ADDER_GROUPS[0];
}

/// 画两排标签并接好点击。onPick(key) 由调用方决定（这里不碰渲染逻辑）。
function adderBuildTabs(host, cats, opts) {
  const { cat, lang, favCount, onPick, onLang } = opts;
  host.textContent = "";

  const group = cat === "fav" || cat === "mine" ? "" : adderGroupOf(cats, cat);
  const groupRow = document.createElement("div");
  groupRow.className = "row tight adder-row";
  for (const g of ADDER_GROUPS) {
    if (!cats.some((c) => c.group === g)) continue;
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "style-chip adder-group" + (group === g ? " on" : "");
    btn.textContent = g;
    btn.onclick = () => {
      const first = cats.find((c) => c.group === g);
      if (first) onPick(first.key);
      else onPick(g);
    };
    groupRow.appendChild(btn);
  }
  const favBtn = document.createElement("button");
  favBtn.type = "button";
  favBtn.className = "style-chip" + (cat === "fav" ? " on" : "");
  favBtn.textContent = `★ 收藏 ${favCount}`;
  favBtn.onclick = () => onPick("fav");
  groupRow.appendChild(favBtn);

  const mineBtn = document.createElement("button");
  mineBtn.type = "button";
  mineBtn.className = "style-chip" + (cat === "mine" ? " on" : "");
  mineBtn.textContent = "我的用语";
  mineBtn.onclick = () => onPick("mine");
  groupRow.appendChild(mineBtn);

  for (const l of ["zh", "en"]) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "style-chip expr-lang" + (lang === l ? " on" : "");
    btn.textContent = l === "zh" ? "中文" : "EN";
    btn.onclick = () => onLang(l);
    groupRow.appendChild(btn);
  }
  host.appendChild(groupRow);

  // 只有在选具体类别时，才出第二排（收藏/我的用语不需要类别）。
  if (cat === "fav" || cat === "mine") return;
  const catRow = document.createElement("div");
  catRow.className = "row tight adder-row";
  for (const c of cats.filter((x) => x.group === group)) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "style-chip" + (cat === c.key ? " on" : "");
    btn.textContent = `${c.label} ${c.items.length}`;
    btn.onclick = () => onPick(c.key);
    catRow.appendChild(btn);
  }
  host.appendChild(catRow);
}
