// 镜头：一话 = 一组镜头；单格可填入/重画/删除。
// 依赖 app.js 的 generate()/state 与 projects.js 的 projectUI。
let shotsCache = [];

function shotProject() {
  return window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
}

// #90 分镜连续性：每格写「这一格结束时」的状态，下一格拼成「接上一格：…」。
function continuityRender() {
  const box = $("continuityBody");
  if (!box) return;
  box.textContent = "";
  if (!shotsCache.length) {
    const d = document.createElement("div");
    d.className = "muted";
    d.textContent = "还没有镜头";
    box.appendChild(d);
    return;
  }
  for (const s of shotsCache) {
    const row = document.createElement("div");
    row.className = "row tight";
    const idx = document.createElement("span");
    idx.className = "mono muted";
    idx.style.cssText = "flex:0 0 34px;font-size:11px";
    idx.textContent = "#" + s.index;
    const inp = document.createElement("input");
    inp.value = s.continuity || "";
    inp.style.flex = "1";
    inp.placeholder = s.index === 1 ? "第一格不用填（它没有上一格）" : "例：退到檐下，右肩湿透，攥着断掉的伞骨";
    inp.onchange = async () => {
      const p = shotProject();
      if (!p) return;
      try {
        const r = await api.call("shot_set_continuity", { project: p.id, id: s.id, text: inp.value });
        s.continuity = r.continuity;
        toast("第 " + s.index + " 格落点已存");
      } catch (e) {
        toast(String(e), true);
      }
    };
    row.append(idx, inp);
    box.appendChild(row);
  }
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
  fill.onclick = async () => {
    await fillShot(s);
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
  row.append(idx, prompt, shotCastChips(s), count, fill, again, del);
  return row;
}

async function shotsRefresh() {
  const box = $("shotsList");
  box.innerHTML = "";
  const p = shotProject();
  if (!p) {
    box.textContent = "先在项目栏选一个项目";
    if ($("shotsSummary")) $("shotsSummary").textContent = "";
    if ($("shotsCheckBox")) $("shotsCheckBox").hidden = true;
    return;
  }
  if (p.private) {
    box.textContent = "私密项目暂不支持镜头（先移出隐私区）";
    if ($("shotsSummary")) $("shotsSummary").textContent = "";
    if ($("shotsCheckBox")) $("shotsCheckBox").hidden = true;
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
    shotSummaryUpdate();
    continuityRender();
    return;
  }
  for (const s of shotsCache) box.appendChild(shotRow(s));
  shotSummaryUpdate();
  continuityRender();
}

async function fillShot(s) {
  // 最终提示词由后端拼（#94 画风前缀 + #90 上一格落点 + 本格原文），前端不重复拼
  let prompt = s.prompt || "";
  let negative = s.negative || "";
  try {
    const p = shotProject();
    if (p) {
      const r = await api.call("shot_prompt", { project: p.id, id: s.id });
      prompt = r.prompt;
      negative = r.negative;
    }
  } catch {}
  $("prompt").value = prompt;
  $("negative").value = negative;
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
  $("shotsModal").hidden = true;
  const ok = await shotGenerate(s); // 挂的角色会一起带上（shotcast.js）
  await shotsRefresh();
  $("shotsModal").hidden = false;
  toast(ok ? `第 ${s.index} 格重画完成` : "没有新图（生成失败？）", !ok);
}

$("shotsBtn").onclick = async () => {
  $("shotsModal").hidden = false;
  const p0 = shotProject();
  if (p0 && !p0.private && typeof storyLoad === "function") await storyLoad(p0.id);
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
$("continuityAuto").onclick = async () => {
  const p = shotProject();
  if (!p) return toast("先选一个项目", true);
  if (!shotsCache.length) return toast("还没有镜头", true);
  const btn = $("continuityAuto");
  btn.disabled = true;
  btn.textContent = "补落点中…";
  $("continuityState").textContent = "本地模型在想…";
  try {
    const r = await api.call("shots_continuity", { project: p.id });
    $("continuityState").textContent = `补了 ${r.written}/${r.total} 格`;
    await shotsRefresh();
    toast(`已补 ${r.written} 格的落点`);
  } catch (e) {
    $("continuityState").textContent = "";
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "自动补落点（本地模型）";
  }
};

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
    const pj = shotProject();
    const panels = await api.call("story_split", {
      text,
      maxPanels: Number($("shotsMax").value) || 12,
      project: pj ? pj.id : null,
    });
    $("shotsPanels").value = (panels || []).join("\n");
    $("shotsSplitState").textContent = `拆出 ${(panels || []).length} 格（${pj ? "已按剧本设定" : "未带设定"}），可先改再存`;
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

// ---------------------------------------------------------------- 单张图存为镜头
let shotPickSource = null;

async function attachImageToShot(id) {
  const p = shotProject();
  try {
    await api.call("shot_add_image", { project: p.id, id, path: shotPickSource.path });
    $("shotPickModal").hidden = true;
    toast("已存进这一格");
    if (!$("shotsModal").hidden) await shotsRefresh();
  } catch (e) {
    toast(String(e), true);
  }
}

async function openShotPick(r) {
  const p = shotProject();
  if (!p) return toast("先选一个项目（镜头按项目存）", true);
  if (p.private) return toast("私密项目暂不支持镜头", true);
  if (!r || !r.path) return toast("这张图没有本地路径", true);
  shotPickSource = r;
  const box = $("shotPickList");
  box.textContent = "";
  let shots = [];
  try {
    shots = await api.call("shots_list", { project: p.id });
  } catch (e) {
    toast(String(e), true);
  }
  if (!shots.length) {
    const p2 = document.createElement("div");
    p2.className = "muted";
    p2.style.cssText = "padding:10px 2px;font-size:12px";
    p2.textContent = "这个项目还没有镜头。点下面「新建一格」，用这张图的提示词生成第 1 格。";
    box.appendChild(p2);
  }
  for (const s of shots) {
    const row = document.createElement("button");
    row.type = "button";
    row.className = "shot-pick-row";
    const idx = document.createElement("b");
    idx.textContent = `#${s.index}`;
    const txt = document.createElement("span");
    txt.textContent = s.prompt;
    const cnt = document.createElement("span");
    cnt.className = "muted mono";
    cnt.textContent = `🖼 ${s.images.length}`;
    row.append(idx, txt, cnt);
    row.onclick = () => attachImageToShot(s.id);
    box.appendChild(row);
  }
  $("shotPickModal").hidden = false;
}

$("lbShot").onclick = () => {
  if (typeof lightboxItem === "undefined" || !lightboxItem) return toast("先点开一张图", true);
  openShotPick(lightboxItem);
};
$("shotPickCancel").onclick = () => ($("shotPickModal").hidden = true);
$("shotPickModal").addEventListener("click", (e) => {
  if (e.target === $("shotPickModal")) $("shotPickModal").hidden = true;
});
$("shotPickNew").onclick = async () => {
  const p = shotProject();
  if (!p) return toast("先选一个项目", true);
  const prompt = (shotPickSource.prompt || "").trim() || "（这张图）";
  try {
    const shots = await api.call("shots_add", {
      project: p.id,
      prompts: [prompt],
      defaults: shotDefaults(),
    });
    const created = shots[shots.length - 1];
    if (!created) throw new Error("新建镜头失败");
    await attachImageToShot(created.id);
  } catch (e) {
    toast(String(e), true);
  }
};
