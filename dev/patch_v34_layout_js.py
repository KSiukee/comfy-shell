proj = r"D:\deepseek-files\ai-tools\comfy-shell"
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")

# payload: include the layout JSON when the editor produced one
j = j.replace('''    slgLayers: $("slgTier").value,
    refImages: [...state.refs],
  };''', '''    slgLayers: $("slgTier").value,
    layout: state.pendingLayout || null,
    refImages: [...state.refs],
  };''', 1)

# clear it once the job is submitted (success or failure)
j = j.replace('''  try {
    const res = await invoke("generate", { req, turbo: $("turbo").checked });''', '''  state.pendingLayout = null;
  try {
    const res = await invoke("generate", { req, turbo: $("turbo").checked });''', 1)

code = '''// ---------------------------------------------------------------- MOSS layout editor
let layoutItems = [];
let layoutSel = -1;
let layoutDrag = null;

function layoutOpen() {
  if (!layoutItems.length) layoutAdd("text");
  $("layoutModal").hidden = false;
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
    d.innerHTML = `<span>${it.type === "text" ? "文字" : "物体"}${i + 1}</span>`;
    d.onpointerdown = (e) => {
      e.stopPropagation();
      layoutSel = i;
      layoutDrag = { mode: "move", i, ox: e.clientX, oy: e.clientY, bbox: [...it.bbox] };
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

$("btnLayout").onclick = layoutOpen;
$("layoutClose").onclick = () => ($("layoutModal").hidden = true);
$("layoutAddText").onclick = () => layoutAdd("text");
$("layoutAddObj").onclick = () => layoutAdd("object");
$("layoutRun").onclick = async () => {
  const summary = ($("layoutSummary").value || $("prompt").value || "").trim();
  if (!summary) return toast("先写整张图的描述", true);
  state.pendingLayout = buildLayoutJson();
  if (!$("prompt").value.trim()) $("prompt").value = summary;
  $("layoutModal").hidden = true;
  await generate();
};

// ---------------------------------------------------------------- image analyzer
'''
j = j.replace("// ---------------------------------------------------------------- image analyzer", code, 1)
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("layout js ok")
