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
    const key = { storyEra: "era", storyLight: "light", storyCam: "camera", storyNeg: "negative", storyCustom: "styleCustom" }[id];
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

  const prev = document.createElement("div");
  prev.className = "muted mono";
  prev.id = "storyPreview";
  prev.style.cssText = "font-size:11px;margin-top:4px;white-space:normal";
  box.appendChild(prev);
  storyPreview();
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
