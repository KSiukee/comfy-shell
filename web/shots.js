// 镜头：一话 = 一组镜头；单格可填入/重画/删除。
// 依赖 app.js 的 generate()/state 与 projects.js 的 projectUI。
let shotsCache = [];

function shotProject() {
  return window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
}

function shotRow(s) {
  const row = document.createElement("div");
  row.className = "row tight";
  row.style.cssText = "border:1px solid var(--line);border-radius:8px;padding:8px;align-items:center";
  const idx = document.createElement("span");
  idx.className = "mono muted";
  idx.style.cssText = "flex:0 0 34px;font-size:11px";
  idx.textContent = `#${s.index}`;
  const prompt = document.createElement("span");
  prompt.textContent = s.prompt;
  prompt.title = s.prompt;
  prompt.style.cssText = "flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap";
  const count = document.createElement("span");
  count.className = "muted mono";
  count.style.fontSize = "11px";
  count.textContent = `🖼 ${s.images.length}`;
  const fill = document.createElement("button");
  fill.className = "btn small";
  fill.textContent = "填入";
  fill.onclick = () => {
    fillShot(s);
    $("shotsModal").hidden = true;
    toast(`已填入第 ${s.index} 格`);
  };
  const again = document.createElement("button");
  again.className = "btn small";
  again.textContent = "重画";
  again.onclick = () => regenerateShot(s);
  const del = document.createElement("button");
  del.className = "btn small ghost";
  del.textContent = "删";
  del.onclick = async () => {
    if (!confirm(`删除第 ${s.index} 格？`)) return;
    try {
      await api.call("shot_delete", { project: shotProject().id, id: s.id });
      await shotsRefresh();
    } catch (e) {
      toast(String(e), true);
    }
  };
  row.append(idx, prompt, count, fill, again, del);
  return row;
}

async function shotsRefresh() {
  const box = $("shotsList");
  box.innerHTML = "";
  const p = shotProject();
  if (!p) {
    box.textContent = "先在项目栏选一个项目";
    return;
  }
  if (p.private) {
    box.textContent = "私密项目暂不支持镜头（先移出隐私区）";
    return;
  }
  try {
    shotsCache = await api.call("shots_list", { project: p.id });
  } catch (e) {
    box.textContent = String(e);
    return;
  }
  if (!shotsCache.length) {
    const empty = document.createElement("div");
    empty.className = "shots-empty";
    const t = document.createElement("div");
    t.textContent = "还没有镜头。可以直接贴一段剧情拆成镜头，也可以先生成单张再存。";
    const b = document.createElement("button");
    b.className = "btn small primary";
    b.textContent = "✂️ 贴一段剧情拆镜头";
    b.onclick = () => {
      $("shotsSplitBox").open = true;
      $("shotsStory").focus();
    };
    empty.append(t, b);
    box.appendChild(empty);
    return;
  }
  for (const s of shotsCache) box.appendChild(shotRow(s));
}

function fillShot(s) {
  $("prompt").value = s.prompt || "";
  $("negative").value = s.negative || "";
  $("width").value = s.width;
  $("height").value = s.height;
  $("steps").value = s.steps;
  $("cfg").value = s.cfg;
  $("seed").value = s.seed;
  $("batch").value = "1";
  $("mossSlg").checked = !!s.slg;
  $("slgTier").value = s.slgLayers || "22";
  if (s.profile) {
    const sel = $("model");
    if ([...sel.options].some((o) => o.value === s.profile)) sel.value = s.profile;
  }
  state.pendingLayout = s.layout || null;
}

async function regenerateShot(s) {
  const p = shotProject();
  if (!p) return;
  fillShot(s);
  // 重画换种子，否则 ComfyUI 会直接回缓存里的同一张图
  $("seed").value = Math.floor(Math.random() * 1000000000);
  $("shotsModal").hidden = true;
  const before = state.results.length;
  try {
    await generate();
  } catch {}
  const added = state.results.slice(0, Math.max(0, state.results.length - before));
  for (const r of added) {
    try {
      await api.call("shot_add_image", { project: p.id, id: s.id, path: r.path });
    } catch {}
  }
  await shotsRefresh();
  $("shotsModal").hidden = false;
  toast(added.length ? `第 ${s.index} 格重画完成` : "没有新图（生成失败？）", !added.length);
}

$("shotsBtn").onclick = async () => {
  $("shotsModal").hidden = false;
  await shotsRefresh();
};
$("shotsExport").onclick = async () => {
  const p = shotProject();
  if (!p) return toast("先选一个项目", true);
  if (p.private) return toast("私密项目暂不支持整话导出", true);
  const warn = typeof poseLicenseWarn === "function" ? poseLicenseWarn() : "";
  if (warn && !confirm(`本项目用过仅本地使用的姿势库：\n${warn}\n\n继续导出？`)) return;
  const dest = await api.call("pick_folder");
  if (!dest) return;
  try {
    const dir = await api.call("shots_export", { project: p.id, dest });
    toast(`已导出：${dir}`);
    await api.call("open_folder", { path: dir });
  } catch (e) {
    toast(String(e), true);
  }
};
$("shotsClose").onclick = () => ($("shotsModal").hidden = true);

function shotDefaults() {
  return {
    profile: $("model").value,
    negative: $("negative").value,
    width: Number($("width").value),
    height: Number($("height").value),
    steps: Number($("steps").value),
    cfg: Number($("cfg").value),
    seed: Number($("seed").value),
    slg: !!$("mossSlg").checked,
    slgLayers: $("slgTier").value,
    layout: state.pendingLayout || null,
  };
}

function storyPanelLines() {
  return ($("shotsPanels").value || "")
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
}

$("shotsGoSplit").onclick = async () => {
  const text = ($("shotsStory").value || "").trim();
  if (!text) return toast("先贴一段剧情", true);
  const btn = $("shotsGoSplit");
  btn.disabled = true;
  btn.textContent = "拆分中…";
  $("shotsSplitState").textContent = "思考中…";
  try {
    const panels = await api.call("story_split", {
      text,
      maxPanels: Number($("shotsMax").value) || 12,
    });
    $("shotsPanels").value = (panels || []).join("\n");
    $("shotsSplitState").textContent = `拆出 ${(panels || []).length} 格，可先改再存`;
  } catch (e) {
    $("shotsSplitState").textContent = "";
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "拆分";
  }
};

$("shotsSavePanels").onclick = async () => {
  const p = shotProject();
  if (!p) return toast("先选一个项目", true);
  if (p.private) return toast("私密项目暂不支持镜头", true);
  const prompts = storyPanelLines();
  if (!prompts.length) return toast("还没有分镜：先贴剧情点「拆分」", true);
  try {
    const shots = await api.call("shots_add", {
      project: p.id,
      prompts,
      defaults: shotDefaults(),
    });
    toast(`已存，共 ${shots.length} 个镜头`);
    await shotsRefresh();
  } catch (e) {
    toast(String(e), true);
  }
};

$("shotsPushPanels").onclick = () => {
  const lines = storyPanelLines();
  if (!lines.length) return toast("还没有分镜：先贴剧情点「拆分」", true);
  $("queueText").value = lines.join("\n");
  queueAdd();
  toast(`已推 ${lines.length} 条到队列（点左侧「批量队列」开始）`);
};
