proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- HTML: preset select ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''        <input id="layoutSummary" placeholder="整张图的描述（summary）" style="flex: 2 1 0" />''',
'''        <select id="layoutPreset" style="flex: 0 0 190px">
          <option value="">布局模板…</option>
          <option value="ecom_main">电商主图（产品 + 底部文案）</option>
          <option value="ecom_poster">电商海报（标题 + 产品 + 价格）</option>
          <option value="left_right">左图右文</option>
          <option value="three_rows">三分屏</option>
          <option value="grid4">四宫格</option>
        </select>
        <input id="layoutSummary" placeholder="整张图的描述（summary）" style="flex: 2 1 0" />''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html ok")

# ---------------- JS ----------------
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")

# resize handles inside each box
j = j.replace('''    d.innerHTML = `<span>${it.type === "text" ? "文字" : "物体"}${i + 1}</span>`;''',
'''    d.innerHTML = `<span>${it.type === "text" ? "文字" : "物体"}${i + 1}</span>` +
      ["nw", "ne", "sw", "se"].map((c) => `<i data-corner="${c}" style="position:absolute;width:10px;height:10px;background:#5d7bf0;border-radius:2px;${c.includes("n") ? "top:-5px" : "bottom:-5px"};${c.includes("w") ? "left:-5px" : "right:-5px"};cursor:${c === "nw" || c === "se" ? "nwse" : "nesw"}-resize"></i>`).join("");''', 1)

j = j.replace('''    d.onpointerdown = (e) => {
      e.stopPropagation();
      layoutSel = i;
      layoutDrag = { mode: "move", i, ox: e.clientX, oy: e.clientY, bbox: [...it.bbox] };
      layoutRender();
    };''', '''    d.onpointerdown = (e) => {
      e.stopPropagation();
      layoutSel = i;
      const corner = e.target.dataset && e.target.dataset.corner;
      layoutDrag = corner
        ? { mode: "resize", i, corner, bbox: [...it.bbox] }
        : { mode: "move", i, ox: e.clientX, oy: e.clientY, bbox: [...it.bbox] };
      layoutRender();
    };''', 1)

# resize math in pointermove
j = j.replace('''    } else {
      const dx = (e.clientX - layoutDrag.ox) / r.width;
      const dy = (e.clientY - layoutDrag.oy) / r.height;
      const b = layoutDrag.bbox;''', '''    } else if (layoutDrag.mode === "resize") {
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
      const b = layoutDrag.bbox;''', 1)

presets = '''
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

$("layoutPreset").onchange = () => {
  const key = $("layoutPreset").value;
  if (!key) return;
  layoutItems = LAYOUT_PRESETS[key].map((x) => ({ ...x, bbox: [...x.bbox] }));
  layoutSel = 0;
  layoutRender();
};
'''
j = j.replace("function layoutOpen() {", presets + "\nfunction layoutOpen() {", 1)

open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok")
