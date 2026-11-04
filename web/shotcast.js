// 镜头挂卡：一镜挂 1-3 个角色卡 → 整话出图。整话体检在 shotcheck.js。
//
// 单独成文件：shots.js 已经 300 行出头，挂卡/整话这块是独立职责（也照顾上帝文件闸）。
// 口径见 设计笔记：每镜挂 1-3 个角色 → 出图时自动带身份 + 底板。
//
// 出图不另写一条通路：把 `@名字` 塞进提示词，交给 characters.js 那一层已经实测过的展开逻辑
// （换模型 / 挂底板 / 出图后还原，全在那边）。这里只负责「塞哪几个名字」和「记账」。

const SHOT_CAST_MAX = 3;
// characters.js 的展开正则只认这些字符：名字带空格/括号就 @ 不到，挂上去也是白挂。
const SHOT_CAST_NAME_RE = /^[\u4e00-\u9fa5A-Za-z0-9_-]{1,20}$/;

const shotRun = { running: false, stop: false };
let shotCastTarget = null;
let shotCastPicked = [];
let shotCastCardsReady = false;

function shotCastCards() {
  return Array.isArray(charList) ? charList : [];
}

function shotCastCard(id) {
  return shotCastCards().find((c) => c.id === id) || null;
}

/// 角色卡列表要按当前项目过滤，所以每次进挂卡窗都重拉一次（force）。
async function shotCastLoadCards(force = false) {
  if (shotCastCardsReady && !force) return shotCastCards();
  try {
    charList = await api.call("character_list", {
      project: (window.projectUI && window.projectUI.current()) || null,
    });
    shotCastCardsReady = true;
  } catch (e) {
    toast(`角色卡读不到：${e}`, true);
  }
  return shotCastCards();
}

/// 这一镜挂的人：{id, name, card}；card 为 null = 角色卡已经不在了。
function shotCastOf(s) {
  return (s.characters || []).map((id) => {
    const card = shotCastCard(id);
    return { id, name: card ? card.name : "（角色已删）", card };
  });
}

/// 出图要用的提示词：原文 + 这一镜挂的 `@名字`（原文里已经写了的就不重复塞）。
async function shotCastPrompt(s) {
  // 先拼剧本设定（#90），再挂 @角色名——两者不冲突：设定管画风/时代/光线，@ 管身份
  const raw = (await storyComposeText((s.prompt || "").trim())).trim();
  const toks = [];
  for (const x of shotCastOf(s)) {
    if (!x.card || !SHOT_CAST_NAME_RE.test(x.card.name)) continue;
    const t = `@${x.card.name}`;
    if (raw.includes(t) || toks.includes(t)) continue;
    toks.push(t);
  }
  if (!toks.length) return raw;
  return raw ? `${raw}，${toks.join(" ")}` : toks.join(" ");
}

/// 镜头行上的角色小块（点一下进挂卡窗）。
function shotCastChips(s) {
  const wrap = document.createElement("span");
  wrap.className = "shot-cast";
  const list = shotCastOf(s);
  if (!list.length) {
    const b = document.createElement("button");
    b.className = "btn small ghost";
    b.textContent = "👤 挂角色";
    b.onclick = () => shotCastOpen(s);
    wrap.appendChild(b);
    return wrap;
  }
  for (const x of list) {
    const chip = document.createElement("button");
    chip.className = "cast-chip" + (x.card ? "" : " missing");
    chip.textContent = x.name;
    chip.title = x.card
      ? `${x.card.name} · ${x.card.identityPlate ? "有底板" : "没底板（只注入文字）"}`
      : "角色卡已经不见了，点开重挂";
    chip.onclick = () => shotCastOpen(s);
    wrap.appendChild(chip);
  }
  return wrap;
}

// ---------------------------------------------------------------- 挂卡窗

function shotCastRenderPick() {
  const box = $("shotCastList");
  box.textContent = "";
  const cards = shotCastCards();
  const known = new Set(cards.map((c) => c.id));
  for (const id of shotCastPicked) {
    if (known.has(id)) continue;
    const row = document.createElement("div");
    row.className = "cast-pick-row missing";
    const nm = document.createElement("b");
    nm.textContent = "角色卡已删除";
    const sub = document.createElement("span");
    sub.textContent = id;
    const off = document.createElement("button");
    off.className = "btn small ghost";
    off.textContent = "摘掉";
    off.onclick = () => {
      shotCastPicked = shotCastPicked.filter((x) => x !== id);
      shotCastRenderPick();
    };
    row.append(nm, sub, off);
    box.appendChild(row);
  }
  if (!cards.length) {
    const d = document.createElement("div");
    d.className = "muted";
    d.style.cssText = "padding:10px 2px;font-size:12px";
    d.textContent = "这个项目里还没有角色卡。先去「角色卡」建一个（记得点「出底板」）。";
    box.appendChild(d);
  }
  for (const c of cards) {
    const on = shotCastPicked.includes(c.id);
    const bad = !SHOT_CAST_NAME_RE.test(c.name || "");
    const row = document.createElement("button");
    row.type = "button";
    row.className = `cast-pick-row${on ? " on" : ""}${bad ? " bad" : ""}`;
    const nm = document.createElement("b");
    nm.textContent = `${on ? "✓ " : ""}${c.name}`;
    const sub = document.createElement("span");
    const bits = [c.project ? "本项目" : "全局", c.identityPlate ? "有底板" : "没底板"];
    if (c.graded) bits.push(`盲测 ${c.graded}`);
    if (bad) bits.push("名字带空格/符号，@ 引用不了");
    sub.textContent = bits.join(" · ");
    row.append(nm, sub);
    row.onclick = () => {
      if (on) shotCastPicked = shotCastPicked.filter((x) => x !== c.id);
      else if (bad) return toast(`「${c.name}」这个名字 @ 不到（不能有空格/括号），先在角色卡里改个名`, true);
      else if (shotCastPicked.length >= SHOT_CAST_MAX) return toast(`一镜最多挂 ${SHOT_CAST_MAX} 个角色`, true);
      else shotCastPicked = shotCastPicked.concat(c.id);
      shotCastRenderPick();
    };
    box.appendChild(row);
  }
  $("shotCastHint").textContent = `已选 ${shotCastPicked.length}/${SHOT_CAST_MAX}（顺序＝挂底板的顺序）`;
}

async function shotCastOpen(s) {
  shotCastTarget = s;
  shotCastPicked = (s.characters || []).slice();
  $("shotCastTitle").textContent = `第 ${s.index} 格 · 挂角色`;
  $("shotCastModal").hidden = false;
  $("shotCastList").textContent = "读角色卡…";
  await shotCastLoadCards(true);
  shotCastRenderPick();
}

async function shotCastSave(all) {
  const s = shotCastTarget;
  const p = shotProject();
  if (!s || !p) return;
  try {
    await api.call("shot_set_characters", { project: p.id, id: s.id, characters: shotCastPicked });
    let touched = 1;
    if (all) {
      for (const x of shotsCache) {
        if (x.id === s.id) continue;
        await api.call("shot_set_characters", { project: p.id, id: x.id, characters: shotCastPicked });
        touched++;
      }
    }
    $("shotCastModal").hidden = true;
    await shotsRefresh();
    toast(all ? `已挂到 ${touched} 格` : `第 ${s.index} 格：${shotCastPicked.length} 个角色`);
  } catch (e) {
    toast(String(e), true);
  }
}

// ---------------------------------------------------------------- 出图（单格 / 整话）

/// 按这一镜自己的设置出一张图，并把新图 + 这次用的角色/底板记回镜头。
async function shotGenerate(s) {
  const p = shotProject();
  if (!p) return false;
  await fillShot(s);
  const prompt = await shotCastPrompt(s);
  $("prompt").value = prompt;
  if (!prompt) {
    toast(`第 ${s.index} 格没有提示词`, true);
    return false;
  }
  // 有卡片要带底板 → 先回生图模式，由 characters.js 挂上底板后再切编辑；
  // 不然上一格留下的编辑模式会让「没挂卡」的格报「缺参考图」。
  setMode("t2i");
  if ((s.images || []).length) $("seed").value = Math.floor(Math.random() * 1e9); // 重画必须换种子
  const cast = shotCastOf(s)
    .filter((x) => x.card)
    .map((x) => ({ id: x.id, name: x.card.name, plate: x.card.identityPlate || null }));

  const before = (state.results || []).length;
  let ok = false;
  try {
    ok = await generate();
  } catch (e) {
    toast(String(e), true);
  }
  const added = (state.results || []).slice(0, Math.max(0, (state.results || []).length - before));
  if (!ok || !added.length) return false;
  for (const r of added) {
    try {
      await api.call("shot_add_image", { project: p.id, id: s.id, path: r.path });
    } catch {}
  }
  if (cast.length) {
    try {
      await api.call("shot_set_cast", { project: p.id, id: s.id, cast });
    } catch {}
  }
  return true;
}

async function shotRunEpisode() {
  if (shotRun.running) return;
  const p = shotProject();
  if (!p) return toast("先选一个项目", true);
  if (p.private) return toast("私密项目暂不支持镜头", true);
  if (!shotsCache.length) return toast("还没有镜头", true);
  const missing = shotsCache.filter((s) => !(s.images || []).length);
  const targets = missing.length ? missing : shotsCache.slice();
  if (!missing.length && !confirm(`这一话 ${shotsCache.length} 格都已有图。要按每格自己的设置全部重画吗？`)) return;
  if (!(await ensureReady("整话出图"))) return;
  await shotCastLoadCards();
  const lost = [...new Set(shotsCache.filter((s) => shotCastOf(s).some((x) => !x.card)).map((s) => s.index))];
  if (lost.length && !confirm(`第 ${lost.join("、")} 格挂的角色卡已经不在，那几格只会用文字提示词。继续？`)) return;

  shotRun.running = true;
  shotRun.stop = false;
  const btn = $("shotsRun");
  btn.disabled = true;
  let ok = 0;
  let fail = 0;
  try {
    for (const s of targets) {
      if (shotRun.stop) break;
      btn.textContent = `出图 ${ok + fail + 1}/${targets.length}（点一下停）`;
      const good = await shotGenerate(s);
      if (good) ok++;
      else fail++;
      await shotsRefresh();
    }
  } finally {
    shotRun.running = false;
    shotRun.stop = false;
    btn.disabled = false;
    btn.textContent = "整话出图";
    await shotsRefresh();
    shotCheckRender();
    toast(`整话出图：成功 ${ok} · 失败 ${fail}`, fail > 0);
  }
}

// ---------------------------------------------------------------- 接线

$("shotsRun").onclick = () => {
  if (shotRun.running) {
    shotRun.stop = true;
    return;
  }
  shotRunEpisode();
};

$("shotsCheck").onclick = () => {
  const box = $("shotsCheckBox");
  if (!box.hidden) {
    box.hidden = true;
    return;
  }
  shotCheckRender();
  box.scrollIntoView({ block: "nearest" });
};

$("shotCastSave").onclick = () => shotCastSave(false);
$("shotCastClear").onclick = () => {
  shotCastPicked = [];
  shotCastRenderPick();
};
$("shotCastAll").onclick = () => {
  const n = shotsCache.filter((x) => !shotCastTarget || x.id !== shotCastTarget.id).length;
  if (!n) return toast("这一话只有这一格", true);
  if (!confirm(`把这 ${shotCastPicked.length} 个角色挂到其余 ${n} 格？`)) return;
  shotCastSave(true);
};
$("shotCastCancel").onclick = () => ($("shotCastModal").hidden = true);
$("shotCastModal").addEventListener("click", (e) => {
  if (e.target === $("shotCastModal")) $("shotCastModal").hidden = true;
});
