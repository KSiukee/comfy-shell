// 剧本设定（#94）：一部戏的「长什么样」——画风、时代地点、光线色彩、镜头语言。
//
// 起因：剧情拆出来的每格只有一句 40~80 字的画面描述，生成时也只多挂了 @角色名，
// 六张图能长出六个派系。这里把设定存进 projects/<id>/story.json，逐格拼在描述前面。
//
// 拼法只有一份实现（Rust 的 story::compose），前端调 story_compose 拿结果——
// 不在 JS 里再抄一遍，免得两边走样。

const storyState = { project: "", data: null, presets: [], saveTimer: 0 };

function storyData() {
  return (
    storyState.data || {
      enabled: true,
      style: "photoreal",
      styleCustom: "",
      era: "",
      light: "",
      camera: "",
      negative: "",
    }
  );
}

async function storyLoad(project) {
  if (!project) return null;
  storyState.project = project;
  try {
    storyState.presets = (await api.call("story_presets")) || [];
  } catch {
    storyState.presets = [];
  }
  try {
    storyState.data = await api.call("story_get", { project });
  } catch {
    storyState.data = null;
  }
  storyRender();
  return storyState.data;
}

function storySaveSoon() {
  clearTimeout(storyState.saveTimer);
  storyState.saveTimer = setTimeout(async () => {
    try {
      storyState.data = await api.call("story_set", { project: storyState.project, story: storyData() });
      await storyPreview();
    } catch (e) {
      toast(`剧本设定没存上：${e}`, true);
    }
  }, 350);
}

async function storyPreview() {
  const el = $("storyPreview");
  if (!el) return;
  if (!storyState.project) {
    el.textContent = "";
    return;
  }
  try {
    const prefix = await api.call("story_compose", { project: storyState.project, text: "" });
    const d = storyData();
    el.textContent = prefix
      ? `每格会拼上：${prefix}`
      : d.enabled
        ? "（画风/时代/光线都空着：等于不拼）"
        : "（已关闭：逐字用每格原文）";
  } catch (e) {
    el.textContent = String(e);
  }
}

function storyField(label, id, value, placeholder) {
  const wrap = document.createElement("div");
  wrap.className = "field";
  const lb = document.createElement("label");
  lb.textContent = label;
  const inp = document.createElement("input");
  inp.id = id;
  inp.value = value || "";
  inp.placeholder = placeholder || "";
  inp.oninput = () => {
    const d = storyData();
    const key = { storyEra: "era", storyLight: "light", storyCam: "camera", storyNeg: "negative", storyCustom: "styleCustom", storyRefText: "styleRefText" }[id];
    if (key) d[key] = inp.value;
    storySaveSoon();
  };
  wrap.append(lb, inp);
  return wrap;
}

function storyRender() {
  const box = $("storyBody");
  if (!box) return;
  const d = storyData();
  box.textContent = "";

  const head = document.createElement("div");
  head.className = "row tight";
  const on = document.createElement("label");
  on.className = "check";
  const cb = document.createElement("input");
  cb.type = "checkbox";
  cb.id = "storyOn";
  cb.checked = !!d.enabled;
  cb.onchange = () => {
    d.enabled = cb.checked;
    storySaveSoon();
    storyPreview();
  };
  on.append(cb, document.createTextNode(" 启用（关掉 = 逐字回到旧行为）"));
  head.appendChild(on);
  box.appendChild(head);

  const row1 = document.createElement("div");
  row1.className = "row two";
  const styleWrap = document.createElement("div");
  styleWrap.className = "field";
  const lb = document.createElement("label");
  lb.textContent = "画风";
  const sel = document.createElement("select");
  sel.id = "storyStyle";
  for (const p of storyState.presets) {
    const o = document.createElement("option");
    o.value = p.id;
    o.textContent = p.label;
    sel.appendChild(o);
  }
  if (!storyState.presets.length) {
    const o = document.createElement("option");
    o.value = d.style;
    o.textContent = d.style;
    sel.appendChild(o);
  }
  sel.value = d.style;
  sel.onchange = () => {
    d.style = sel.value;
    storySaveSoon();
    storyRender();
  };
  styleWrap.append(lb, sel);
  row1.append(styleWrap, storyField("时代 / 地点", "storyEra", d.era, "宋代江南，雨夜青石板街"));
  box.appendChild(row1);

  if (d.style === "custom") {
    const row = document.createElement("div");
    row.className = "row two";
    row.appendChild(storyField("自定义画风", "storyCustom", d.styleCustom, "像素画，16 色，粗描边"));
    box.appendChild(row);
  }

  const row2 = document.createElement("div");
  row2.className = "row two";
  row2.append(
    storyField("光线 / 色彩", "storyLight", d.light, "低饱和冷调，暖黄灯笼光，湿润反光"),
    storyField("镜头语言", "storyCam", d.camera, "35mm 电影感，浅景深，中近景为主")
  );
  box.appendChild(row2);

  box.appendChild(storyField("统一负面词（格子自己没填时用它）", "storyNeg", d.negative, "低清，畸形手，多余手指，文字水印"));

  // #92 风格参考图：只读它的画风（不当生成参考图，免得模型抄内容）
  const refRow = document.createElement("div");
  refRow.className = "row tight";
  const refBtn = document.createElement("button");
  refBtn.className = "btn small";
  refBtn.id = "storyRefPick";
  refBtn.textContent = "🖼 用当前大图提取画风";
  refBtn.title = "用本机视觉模型看当前这张图，抽出色彩/光线/质感，拼进每格提示词；不把图片当参考图直接喂给模型";
  refBtn.onclick = () => storyPickStyleRef(refBtn);
  const refName = document.createElement("span");
  refName.className = "muted mono";
  refName.id = "storyRefName";
  refName.style.fontSize = "11px";
  refName.textContent = d.styleRef ? String(d.styleRef).split(/[\\/]/).pop() : "参考图：未选";
  const refClear = document.createElement("button");
  refClear.className = "btn small ghost";
  refClear.id = "storyRefClear";
  refClear.textContent = "清除";
  refClear.onclick = () => {
    const dd = storyData();
    dd.styleRef = "";
    dd.styleRefText = "";
    storySaveSoon();
    storyRender();
  };
  refRow.append(refBtn, refName, refClear);
  box.appendChild(refRow);
  box.appendChild(
    storyField("从参考图读出的画风（可改）", "storyRefText", d.styleRefText, "例：暖黄低饱和，窗边侧光，细腻颗粒")
  );

  const prev = document.createElement("div");
  prev.className = "muted mono";
  prev.id = "storyPreview";
  prev.style.cssText = "font-size:11px;margin-top:4px;white-space:normal";
  box.appendChild(prev);
  storyPreview();
}

function storyParseAnalyze(raw) {
  const t = String(raw || "");
  const m = t.match(/\{[\s\S]*\}/);
  if (!m) return null;
  try {
    return JSON.parse(m[0]);
  } catch {
    return null;
  }
}

/// 用当前大图当画风参考：视觉模型只读 style/colors/lighting 三段，拼成一句中文可改。
async function storyPickStyleRef(btn) {
  const it = viewer.list[viewer.idx];
  if (!it) return toast("先在大图里打开一张想当画风参考的图", true);
  const old = btn.textContent;
  btn.disabled = true;
  btn.textContent = "读图中…";
  try {
    const dataUrl = await api.call("image_data_url", { path: it.path });
    const name = await api.call("save_ref_image", { name: "story_style_ref.png", dataUrl });
    const raw = await api.call("analyze_image", { image: name });
    const obj = storyParseAnalyze(raw);
    const d = storyData();
    d.styleRef = it.path;
    // style 里常混进景别词（medium shot / close-up）——那是每格自己的事，不能锁死全片
    const styleClean = String(obj?.style || "")
      .split(/[,，]/)
      .map((x) => x.trim())
      .filter((x) => x && !/(shot|view|angle|close-?up|frame|perspective|camera|portrait)/i.test(x))
      .join(", ");
    d.styleRefText = obj
      ? [styleClean, obj.colors, obj.lighting].filter((x) => x && String(x).trim()).join("，")
      : String(raw || "").trim().slice(0, 120);
    storySaveSoon();
    storyRender();
    toast(obj ? "已从这张图读出画风（可手改）" : "模型没给结构化结果，已把原文塞进去，建议手改");
  } catch (e) {
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = old;
  }
}

/// 把设定拼到一段画面描述前面（Rust 算，避免两份实现走样）。
async function storyComposeText(raw) {
  if (!storyState.project) return raw;
  try {
    return await api.call("story_compose", { project: storyState.project, text: raw || "" });
  } catch {
    return raw;
  }
}

function storyNegative() {
  const d = storyState.data;
  return d && d.enabled && d.negative ? d.negative : "";
}
