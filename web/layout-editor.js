// MOSS 排版编辑器（从 app.js 拆出；依赖 app.js 的全局）

// ---------------------------------------------------------------- MOSS layout editor
let layoutItems = [];
let layoutSel = -1;
let layoutDrag = null;


const LAYOUT_PRESETS = {
  ecom_main: [
    { type: "object", desc: "main product, centered on clean background", bbox: [0.2, 0.12, 0.6, 0.56] },
    { type: "text", desc: "product name, bold", text: "PRODUCT NAME", bbox: [0.1, 0.74, 0.8, 0.09] },
    { type: "text", desc: "one selling point line", text: "key selling point", bbox: [0.12, 0.86, 0.76, 0.07] },
  ],
  ecom_poster: [
    { type: "text", desc: "big headline", text: "SUMMER SALE", bbox: [0.06, 0.05, 0.88, 0.12] },
    { type: "object", desc: "product hero shot", bbox: [0.24, 0.24, 0.52, 0.48] },
    { type: "text", desc: "price tag", text: "¥199", bbox: [0.56, 0.78, 0.34, 0.09] },
  ],
  left_right: [
    { type: "object", desc: "product on the left", bbox: [0.05, 0.15, 0.5, 0.7] },
    { type: "text", desc: "headline on the right", text: "HEADLINE", bbox: [0.6, 0.2, 0.36, 0.12] },
    { type: "text", desc: "body copy on the right", text: "short body copy", bbox: [0.6, 0.4, 0.36, 0.28] },
  ],
  three_rows: [
    { type: "object", desc: "top panel scene", bbox: [0.05, 0.05, 0.9, 0.28] },
    { type: "object", desc: "middle panel scene", bbox: [0.05, 0.36, 0.9, 0.28] },
    { type: "object", desc: "bottom panel scene", bbox: [0.05, 0.67, 0.9, 0.28] },
  ],
  grid4: [
    { type: "object", desc: "top-left panel", bbox: [0.04, 0.04, 0.45, 0.45] },
    { type: "object", desc: "top-right panel", bbox: [0.51, 0.04, 0.45, 0.45] },
    { type: "object", desc: "bottom-left panel", bbox: [0.04, 0.51, 0.45, 0.45] },
    { type: "object", desc: "bottom-right panel", bbox: [0.51, 0.51, 0.45, 0.45] },
  ],
};

$("layoutPreset").onchange = async () => {
  const key = $("layoutPreset").value;
  if (!key) return;
  if (key.startsWith("tpl:")) {
    const name = key.slice(4);
    try {
      const items = await api.call("layout_tpl_load", { name });
      layoutItems = items.map((x) => ({ ...x, bbox: [...x.bbox] }));
      layoutSel = 0;
      $("layoutTplName").value = name;
      layoutRender();
      toast(`已载入模板：${name}`);
    } catch (e) {
      toast(String(e), true);
    }
    return;
  }
  layoutItems = LAYOUT_PRESETS[key].map((x) => ({ ...x, bbox: [...x.bbox] }));
  layoutSel = 0;
  layoutRender();
};

// 自动识框：本地 Qwen3-VL 把海报/菜单/截图读成「文字块 + 物体」，框只是草稿，拖一下更准。
let layoutBg = "";

function setLayoutBg(url) {
  layoutBg = url || "";
  const stage = $("layoutStage");
  stage.style.backgroundImage = layoutBg ? `url("${layoutBg}")` : "";
  stage.style.backgroundSize = "cover";
  stage.style.backgroundPosition = "center";
  stage.style.backgroundRepeat = "no-repeat";
  $("layoutBgClear").hidden = !layoutBg;
}

async function runLayoutDetect(imageName, previewUrl) {
  const btn = $("layoutDetect");
  const label = btn.textContent;
  btn.disabled = true;
  btn.textContent = "识别中…";
  try {
    const items = await api.call("layout_detect", { image: imageName });
    layoutItems = items.map((x) => ({ ...x, bbox: [...x.bbox] }));
    layoutSel = layoutItems.length ? 0 : -1;
    if (previewUrl) setLayoutBg(previewUrl);
    layoutRender();
    toast(`已识出 ${layoutItems.length} 个框，拖动微调后再生成`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = label;
    $("layoutImg").value = "";
  }
}

$("layoutDetect").onclick = () => $("layoutImg").click();
$("layoutImg").onchange = async () => {
  const file = $("layoutImg").files && $("layoutImg").files[0];
  if (!file) return;
  try {
    const dataUrl = await readFileDataUrl(file);
    const name = await api.call("save_ref_image", { name: file.name || "layout_ref.png", dataUrl });
    await runLayoutDetect(name, dataUrl);
  } catch (e) {
    toast(String(e), true);
  }
};
$("layoutBgClear").onclick = () => setLayoutBg("");

// 自定义模板：文件落在 <config>\layouts\<name>.json，一模板一文件，可直接拷给别人。
async function refreshLayoutTpls() {
  try {
    state.layoutTpls = await api.call("layout_tpl_list");
  } catch (e) {
    state.layoutTpls = [];
  }
  const sel = $("layoutPreset");
  const keep = sel.value;
  sel.querySelectorAll("optgroup").forEach((g) => g.remove());
  if (state.layoutTpls.length) {
    const og = document.createElement("optgroup");
    og.label = "我的模板";
    for (const name of state.layoutTpls) {
      const o = document.createElement("option");
      o.value = "tpl:" + name;
      o.textContent = name;
      og.appendChild(o);
    }
    sel.appendChild(og);
  }
  if ([...sel.options].some((o) => o.value === keep)) sel.value = keep;
}

$("layoutTplSave").onclick = async () => {
  const name = ($("layoutTplName").value || "").trim();
  if (!name) return toast("先给模板起个名字", true);
  if (!layoutItems.length) return toast("模板里还没有元素", true);
  const existed = state.layoutTpls.includes(name);
  try {
    const saved = await api.call("layout_tpl_save", {
      name,
      items: JSON.stringify(layoutItems),
    });
    await refreshLayoutTpls();
    $("layoutPreset").value = "tpl:" + saved;
    $("layoutTplName").value = saved;
    toast(`${existed ? "已覆盖模板" : "已存模板"}：${saved}`);
  } catch (e) {
    toast(String(e), true);
  }
};

$("layoutVersionSave").onclick = async () => {
  const p = window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
  if (!p) return toast("先选一个项目", true);
  if (p.private) return toast("私密项目不记录排版版本", true);
  const items = buildLayoutJson();
  const name = ($("layoutTplName").value || "").trim() || `排版 ${new Date().toLocaleString()}`;
  try {
    const v = await api.call("layout_version_save", { project: p.id, name, items });
    state.pendingLayout = items;
    toast(`已存排版版本：${v.prompt}`);
  } catch (e) {
    toast(String(e), true);
  }
};

$("layoutTplDel").onclick = async () => {
  const v = $("layoutPreset").value;
  if (!v.startsWith("tpl:")) return toast("先在下拉里选中「我的模板」里的一个", true);
  const name = v.slice(4);
  if (!confirm(`删除模板「${name}」？\n只删模板文件，不动任何图片。`)) return;
  try {
    await api.call("layout_tpl_delete", { name });
    await refreshLayoutTpls();
    $("layoutPreset").value = "";
    toast(`已删除模板：${name}`);
  } catch (e) {
    toast(String(e), true);
  }
};

function layoutOpen() {
  if (!layoutItems.length) layoutAdd("text");
  $("layoutModal").hidden = false;
  refreshLayoutTpls();
  layoutRender();
}

function layoutAdd(type) {
  const box = type === "text"
    ? { type: "text", desc: "large title text", text: "TEXT", bbox: [0.1, 0.08, 0.8, 0.14] }
    : { type: "object", desc: "an object", text: "", bbox: [0.35, 0.45, 0.3, 0.3] };
  layoutItems.push(box);
  layoutSel = layoutItems.length - 1;
  layoutRender();
}

function layoutRender() {
  const boxes = $("layoutBoxes");
  boxes.innerHTML = "";
  layoutItems.forEach((it, i) => {
    const d = document.createElement("div");
    d.className = "lbox" + (i === layoutSel ? " sel" : "");
    d.style.left = `${it.bbox[0] * 100}%`;
    d.style.top = `${it.bbox[1] * 100}%`;
    d.style.width = `${it.bbox[2] * 100}%`;
    d.style.height = `${it.bbox[3] * 100}%`;
    d.innerHTML = `<span>${it.type === "text" ? "文字" : "物体"}${i + 1}</span>` +
      ["nw", "ne", "sw", "se"].map((c) => `<i data-corner="${c}" style="position:absolute;width:10px;height:10px;background:#5d7bf0;border-radius:2px;${c.includes("n") ? "top:-5px" : "bottom:-5px"};${c.includes("w") ? "left:-5px" : "right:-5px"};cursor:${c === "nw" || c === "se" ? "nwse" : "nesw"}-resize"></i>`).join("");
    d.onpointerdown = (e) => {
      e.stopPropagation();
      layoutSel = i;
      const corner = e.target.dataset && e.target.dataset.corner;
      layoutDrag = corner
        ? { mode: "resize", i, corner, bbox: [...it.bbox] }
        : { mode: "move", i, ox: e.clientX, oy: e.clientY, bbox: [...it.bbox] };
      layoutRender();
    };
    boxes.appendChild(d);
  });

  const list = $("layoutList");
  list.innerHTML = "";
  layoutItems.forEach((it, i) => {
    const row = document.createElement("div");
    row.className = "lrow";
    row.style.background = i === layoutSel ? "#1b2130" : "transparent";
    row.style.borderRadius = "6px";
    row.style.padding = "2px";
    const tag = document.createElement("span");
    tag.className = "tag";
    tag.textContent = it.type === "text" ? "文字" : "物体";
    const desc = document.createElement("input");
    desc.value = it.desc;
    desc.placeholder = "描述/文字内容";
    desc.oninput = () => { it.desc = desc.value; };
    if (it.type === "text") {
      const txt = document.createElement("input");
      txt.value = it.text;
      txt.placeholder = "要渲染的文字";
      txt.oninput = () => { it.text = txt.value; };
      row.append(tag, wrapTwo(desc, txt), delBtn(i));
    } else {
      row.append(tag, desc, delBtn(i));
    }
    row.onclick = () => { layoutSel = i; layoutRender(); };
    list.appendChild(row);
  });
  $("layoutHint").textContent = `${layoutItems.length} 个元素 · 在画布上拖出方框`;
}

function wrapTwo(a, b) {
  const div = document.createElement("div");
  div.style.display = "flex";
  div.style.gap = "4px";
  div.style.flexDirection = "column";
  div.append(a, b);
  return div;
}

function delBtn(i) {
  const b = document.createElement("button");
  b.textContent = "×";
  b.className = "btn small ghost";
  b.onclick = (e) => {
    e.stopPropagation();
    layoutItems.splice(i, 1);
    if (layoutSel >= layoutItems.length) layoutSel = layoutItems.length - 1;
    layoutRender();
  };
  return b;
}

(() => {
  const stage = $("layoutStage");
  stage.addEventListener("pointerdown", (e) => {
    if (e.target.closest(".lbox")) return;
    const r = stage.getBoundingClientRect();
    layoutDrag = { mode: "new", sx: (e.clientX - r.left) / r.width, sy: (e.clientY - r.top) / r.height };
    stage.setPointerCapture(e.pointerId);
  });
  stage.addEventListener("pointermove", (e) => {
    if (!layoutDrag) return;
    const r = stage.getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
    const y = Math.min(1, Math.max(0, (e.clientY - r.top) / r.height));
    if (layoutDrag.mode === "new") {
      if (layoutSel < 0) return;
      layoutItems[layoutSel].bbox = [
        Number(Math.min(layoutDrag.sx, x).toFixed(4)),
        Number(Math.min(layoutDrag.sy, y).toFixed(4)),
        Number(Math.abs(x - layoutDrag.sx).toFixed(4)),
        Number(Math.abs(y - layoutDrag.sy).toFixed(4)),
      ];
      layoutRender();
    } else if (layoutDrag.mode === "resize") {
      const b = [...layoutDrag.bbox];
      const c = layoutDrag.corner;
      if (c.includes("e")) b[2] = Math.max(0.03, x - b[0]);
      if (c.includes("s")) b[3] = Math.max(0.03, y - b[1]);
      if (c.includes("w")) { const nx = Math.min(x, b[0] + b[2] - 0.03); b[2] = b[0] + b[2] - nx; b[0] = nx; }
      if (c.includes("n")) { const ny = Math.min(y, b[1] + b[3] - 0.03); b[3] = b[1] + b[3] - ny; b[1] = ny; }
      layoutItems[layoutDrag.i].bbox = b.map((v) => Number(Math.max(0, Math.min(1, v)).toFixed(4)));
      layoutRender();
    } else {
      const dx = (e.clientX - layoutDrag.ox) / r.width;
      const dy = (e.clientY - layoutDrag.oy) / r.height;
      const b = layoutDrag.bbox;
      layoutItems[layoutDrag.i].bbox = [
        Number(Math.min(0.99, Math.max(0, b[0] + dx)).toFixed(4)),
        Number(Math.min(0.99, Math.max(0, b[1] + dy)).toFixed(4)),
        b[2], b[3],
      ];
      layoutRender();
    }
  });
  stage.addEventListener("pointerup", () => (layoutDrag = null));
})();

function buildLayoutJson() {
  const w = Number($("width").value);
  const h = Number($("height").value);
  const summary = ($("layoutSummary").value || $("prompt").value || "").trim();
  const items = [];
  for (const it of layoutItems) {
    const x0 = it.bbox[0] * 1000, y0 = it.bbox[1] * 1000;
    const x1 = (it.bbox[0] + it.bbox[2]) * 1000, y1 = (it.bbox[1] + it.bbox[3]) * 1000;
    const box = [
      Math.floor((y0 * h) / 1000) - Math.floor(h / 2),
      Math.floor((x0 * w) / 1000) - Math.floor(w / 2),
      Math.ceil((y1 * h) / 1000) - Math.floor(h / 2),
      Math.ceil((x1 * w) / 1000) - Math.floor(w / 2),
    ];
    const item = { label: it.type === "text" ? "text" : "object", caption: it.desc || "", bbox: box };
    if (it.type === "text") item.text = it.text || "";
    items.push(item);
  }
  return JSON.stringify({
    summary: { caption: summary },
    items,
    bbox_coord_space: "abs_centralized",
    bbox_format: "yxyx",
  });
}

async function layoutLoadJson(text) {
  let data;
  try {
    data = typeof text === "string" ? JSON.parse(text) : text;
  } catch (e) {
    toast(`排版 JSON 解析失败：${e}`, true);
    return 0;
  }
  const errs = (await layoutCheckJson(data)) || [];
  if (errs.length) {
    toast(`排版没载入：${errs[0]}${errs.length > 1 ? `（共 ${errs.length} 条）` : ""}`, true);
    if (typeof problemsAdd === "function") problemsAdd("layout", errs.join("\n"), "");
    return 0;
  }
  const norm = normalizeLayoutDraft(data, Number($("width").value) || 1024, Number($("height").value) || 1024);
  if (!norm.items.length) {
    toast("这份排版里没有元素", true);
    return 0;
  }
  if (!norm.compiled) {
    $("width").value = norm.width;
    $("height").value = norm.height;
  }
  layoutItems = norm.items;
  layoutSel = 0;
  if (data.summary && data.summary.caption) {
    $("layoutSummary").value = data.summary.caption;
  }
  layoutRender();
  state.pendingLayout = buildLayoutJson();
  layoutOpen();
  return layoutItems.length;
}

$("btnLayout").onclick = () => { $("anaModal").hidden = true; $("poseModal").hidden = true; layoutOpen(); };
$("layoutClose").onclick = () => ($("layoutModal").hidden = true);
$("layoutAddText").onclick = () => layoutAdd("text");
$("layoutAddObj").onclick = () => layoutAdd("object");
$("layoutRun").onclick = async () => {
  const summary = ($("layoutSummary").value || $("prompt").value || "").trim();
  if (!summary) return toast("先写整张图的描述", true);
  const built = buildLayoutJson();
  const errs = (await layoutCheckJson(built)) || [];
  if (errs.length) {
    toast(`排版有问题：${errs[0]}${errs.length > 1 ? `（共 ${errs.length} 条）` : ""}`, true);
    if (typeof problemsAdd === "function") problemsAdd("layout", errs.join("\n"), "");
    return;
  }
  state.pendingLayout = built;
  if (!$("prompt").value.trim()) $("prompt").value = summary;
  $("layoutModal").hidden = true;
  await generate();
};
