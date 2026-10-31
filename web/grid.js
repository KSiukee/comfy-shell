// 参数网格（#29）：横扫的通用版。选两维、各填几个值，跑成一张带标签的表，点一格采用。
// 旧「横扫」的用法（模型 × 尺寸、预设 × 尺寸）留成两个模板按钮，位置不变。

const GRID_CAP = 16;
const GRID_KEY = "cs.gridLast";

const GRID_AXES = {
  model: { label: "模型", hint: "全部模型", fill: () => state.profiles.map((p) => p.id).join(",") },
  preset: { label: "人像预设", hint: "", fill: () => Object.keys(PORTRAIT_PRESETS).join(",") },
  size: { label: "尺寸", hint: "1024 或 832x1216", fill: () => "768,896,1024" },
  seed: { label: "种子", hint: "-1 表示随机", fill: () => "" },
  cfg: { label: "CFG", hint: "3,5,7", fill: () => "" },
  steps: { label: "步数", hint: "20,30,50", fill: () => "" },
  lora: { label: "LoRA 强度", hint: "0.4,0.7,1.0", fill: () => "" },
  style: { label: "风格", hint: "留空 = 不套风格", fill: () => stylePacks.slice(0, 3).map((s) => s.name).join(",") },
};

let gridRun = null; // {x:{axis,values}, y:{axis,values}, cells:[], cursor}

function gridLabelFor(axis, value) {
  if (axis === "model") {
    const p = state.profiles.find((x) => x.id === value);
    return p ? p.label : value;
  }
  if (axis === "preset") {
    const p = PORTRAIT_PRESETS[value];
    return p ? p.label || value : value;
  }
  if (axis === "style") {
    const s = (typeof stylePacks !== "undefined" ? stylePacks : []).find((x) => x.name === value);
    return s ? styleText(s) : value;
  }
  return value;
}

function gridParseValues(axis, raw) {
  return String(raw || "")
    .split(/[,，;；]+/)
    .map((s) => s.trim())
    .filter(Boolean)
    .map((v) => ({ value: v, label: gridLabelFor(axis, v) }));
}

function gridFillAxisSelects() {
  for (const id of ["gridXAxis", "gridYAxis"]) {
    const sel = $(id);
    if (!sel) continue;
    sel.innerHTML = "";
    for (const [key, cfg] of Object.entries(GRID_AXES)) sel.appendChild(new Option(cfg.label, key));
  }
  $("gridXAxis").value = "seed";
  $("gridYAxis").value = "cfg";
  $("gridXValues").value = "1,2,3";
  $("gridYValues").value = "3,5";
}

function gridSnapshot() {
  return {
    model: $("model").value,
    preset: $("presetSel") ? $("presetSel").value : "",
    width: Number($("width").value) || 1024,
    height: Number($("height").value) || 1024,
    seed: Number($("seed").value),
    cfg: Number($("cfg").value) || 4,
    steps: Number($("steps").value) || 30,
    loraStrength: Number($("loraStrength").value) || 1,
    lora2Strength: Number($("lora2Strength").value) || 1,
    style: typeof currentStyle === "function" && currentStyle() ? currentStyle().name : "",
    prompt: $("prompt").value,
    negative: $("negative").value,
  };
}

function gridSizeOf(value, base) {
  const m = /^(\d+)\s*[x×*]\s*(\d+)$/i.exec(String(value).trim());
  if (m) return [Number(m[1]), Number(m[2])];
  const n = Number(value);
  if (!Number.isFinite(n) || n < 64) return [base.width, base.height];
  return [n, n];
}

function gridCellParams(base, xAxis, xVal, yAxis, yVal) {
  const p = { ...base, xAxis, xVal, yAxis, yVal };
  const set = (axis, val) => {
    if (axis === "model") p.model = val;
    else if (axis === "preset") p.preset = val;
    else if (axis === "size") [p.width, p.height] = gridSizeOf(val, base);
    else if (axis === "seed") p.seed = Number(val);
    else if (axis === "cfg") p.cfg = Number(val);
    else if (axis === "steps") p.steps = Number(val);
    else if (axis === "lora") {
      p.loraStrength = Number(val);
      p.lora2Strength = Number(val);
    } else if (axis === "style") p.style = val === "（不套风格）" ? "" : val;
  };
  set(xAxis, xVal);
  set(yAxis, yVal);
  if (!Number.isFinite(p.seed)) p.seed = -1;
  return p;
}

function gridBuild() {
  const xAxis = $("gridXAxis").value;
  const yAxis = $("gridYAxis").value;
  const xs = gridParseValues(xAxis, $("gridXValues").value);
  const ys = gridParseValues(yAxis, $("gridYValues").value);
  if (!xs.length || !ys.length) {
    toast("两轴都要填至少一个值", true);
    return null;
  }
  const total = xs.length * ys.length;
  if (total > GRID_CAP) {
    toast(`一共 ${total} 张，超过上限 ${GRID_CAP}：少填几个值`, true);
    return null;
  }
  const base = gridSnapshot();
  const cells = [];
  for (const y of ys) {
    for (const x of xs) {
      cells.push({
        x: x.label,
        y: y.label,
        xVal: x.value,
        yVal: y.value,
        params: gridCellParams(base, xAxis, x.value, yAxis, y.value),
        status: "wait",
        path: "",
        filename: "",
      });
    }
  }
  return { x: { axis: xAxis, values: xs.map((v) => v.label) }, y: { axis: yAxis, values: ys.map((v) => v.label) }, cols: xs.length, base, cells, cursor: 0 };
}

function gridSetModel(id) {
  const sel = $("model");
  if (![...sel.options].some((o) => o.value === id)) return false;
  sel.value = id;
  sel.dispatchEvent(new Event("change", { bubbles: true }));
  return true;
}

function gridApplyCell(p) {
  gridSetModel(p.model);
  if ($("presetSel") && [...$("presetSel").options].some((o) => o.value === p.preset)) $("presetSel").value = p.preset;
  $("width").value = p.width;
  $("height").value = p.height;
  if (Number($("seed").value) !== p.seed) $("seed").value = p.seed;
  $("cfg").value = p.cfg;
  $("steps").value = p.steps;
  $("loraStrength").value = p.loraStrength;
  $("lora2Strength").value = p.lora2Strength;
  $("prompt").value = p.prompt;
  $("negative").value = p.negative;
  $("batch").value = 1;
  $("turbo").checked = false;
  setMode("t2i");
  state.refs = [];
  renderRefs();
  if (typeof setStyleName === "function") setStyleName(p.style || "");
}

function gridRestore(base) {
  gridApplyCell(base);
}

function gridSaveRun() {
  if (!gridRun) return;
  try {
    const slim = {
      x: gridRun.x,
      y: gridRun.y,
      cols: gridRun.cols,
      base: gridRun.base,
      cursor: gridRun.cursor,
      cells: gridRun.cells.map((c) => ({ ...c, dataUrl: "" })),
    };
    localStorage.setItem(GRID_KEY, JSON.stringify(slim));
  } catch {}
}

function gridProgressText() {
  if (!gridRun) return "";
  const done = gridRun.cells.filter((c) => c.status === "ok" || c.status === "fail").length;
  return `${done}/${gridRun.cells.length} · ${gridRun.x.axis} × ${gridRun.y.axis}`;
}

async function gridRunNow() {
  if (!gridRun) gridRun = gridBuild();
  if (!gridRun) return;
  if (state.running) return toast("正在生成，等这张跑完", true);
  if (!multiStart()) return;
  const cells = gridRun.cells;
  const base = gridRun.base;
  $("gridRun").disabled = true;
  $("gridStop").hidden = false;
  $("gridStop").disabled = false;
  gridRun.stop = false;
  for (let i = gridRun.cursor; i < cells.length; i++) {
    if (gridRun.stop) break;
    const cell = cells[i];
    if (cell.status === "ok") continue;
    cell.status = "run";
    $("gridState").textContent = `${i + 1}/${cells.length} · ${cell.x} / ${cell.y}`;
    gridApplyCell(cell.params);
    const before = state.results.length;
    let ok = false;
    try {
      ok = await generate();
    } catch (e) {
      toast(String(e), true);
    }
    if (ok) {
      const fresh = state.results.length > before ? state.results[0] : null;
      cell.status = "ok";
      cell.path = fresh ? fresh.path : "";
      cell.filename = fresh ? fresh.filename : "";
      cell.dataUrl = fresh && fresh.dataUrl ? fresh.dataUrl : "";
    } else {
      cell.status = "fail";
    }
    gridRun.cursor = i + 1;
    gridSaveRun();
    gridRenderTable();
  }
  $("gridRun").disabled = false;
  $("gridStop").hidden = true;
  gridRestore(base);
  multiEnd();
  const okCount = cells.filter((c) => c.status === "ok").length;
  $("gridState").textContent = gridRun.stop ? `已停止（${okCount}/${cells.length}）` : `完成 ${okCount}/${cells.length}`;
  toast(`网格结束：${okCount}/${cells.length} 张`);
  gridSaveRun();
  gridOpenTable();
}

function gridStopNow() {
  if (!gridRun) return;
  gridRun.stop = true;
  $("gridStop").disabled = true;
  $("gridState").textContent = "正在停…";
}

async function gridFillCellImage(cell) {
  if (cell.dataUrl || !cell.path) return;
  try {
    cell.dataUrl = await libUrl(cell.path);
  } catch {
    cell.dataUrl = "";
  }
}

let gridRenderSeq = 0;

async function gridRenderTable() {
  const box = $("gridTable");
  if (!box || !gridRun) return;
  const seq = ++gridRenderSeq;
  box.textContent = "";
  box.style.gridTemplateColumns = `86px repeat(${gridRun.cols}, minmax(120px, 1fr))`;
  box.appendChild(document.createElement("div"));
  for (const x of gridRun.x.values) {
    const h = document.createElement("div");
    h.className = "grid-h";
    h.textContent = x;
    box.appendChild(h);
  }
  for (let r = 0; r < gridRun.y.values.length; r++) {
    const yh = document.createElement("div");
    yh.className = "grid-h";
    yh.textContent = gridRun.y.values[r];
    box.appendChild(yh);
    for (let c = 0; c < gridRun.cols; c++) {
      const cell = gridRun.cells[r * gridRun.cols + c];
      const d = document.createElement("button");
      d.type = "button";
      d.className = "grid-cell" + (cell.status === "fail" ? " fail" : "");
      if (cell.status === "ok") {
        await gridFillCellImage(cell);
        if (seq !== gridRenderSeq) return; // 有更新的渲染在跑，别往旧表里塞
        if (cell.dataUrl) {
          const img = document.createElement("img");
          img.src = cell.dataUrl;
          img.alt = cell.filename;
          d.appendChild(img);
        }
      } else {
        const s = document.createElement("span");
        s.className = "muted";
        s.textContent = cell.status === "run" ? "生成中…" : cell.status === "fail" ? "失败" : "未跑";
        d.appendChild(s);
      }
      d.onclick = () => gridAdopt(cell);
      box.appendChild(d);
    }
  }
}

function gridAdopt(cell) {
  if (!cell || cell.status !== "ok") return;
  gridApplyCell(cell.params);
  $("gridModal").hidden = true;
  toast(`已采用：${cell.x} / ${cell.y}`);
}

function gridOpenTable() {
  if (!gridRun) {
    toast("还没有网格结果", true);
    return;
  }
  $("gridModal").hidden = false;
  $("gridModalMeta").textContent = `${gridRun.x.axis} × ${gridRun.y.axis} · ${gridRun.cells.filter((c) => c.status === "ok").length}/${gridRun.cells.length} 张 · 点一格采用`;
  gridRenderTable();
}

function gridRestoreSaved() {
  let saved = null;
  try {
    saved = JSON.parse(localStorage.getItem(GRID_KEY) || "null");
  } catch {}
  if (!saved || !Array.isArray(saved.cells) || !saved.cells.length) return;
  gridRun = saved;
  $("gridXAxis").value = saved.x.axis;
  $("gridYAxis").value = saved.y.axis;
  $("gridXValues").value = saved.x.values.join(",");
  $("gridYValues").value = saved.y.values.join(",");
  const pending = saved.cells.some((c) => c.status === "wait" || c.status === "run");
  $("gridResume").hidden = !pending;
  $("gridState").textContent = pending ? `上次没跑完（${saved.cursor}/${saved.cells.length}）` : `上次完成（${saved.cells.filter((c) => c.status === "ok").length}/${saved.cells.length}）`;
}

function gridTemplate(models) {
  if (models) {
    $("gridXAxis").value = "model";
    $("gridXValues").value = GRID_AXES.model.fill();
  } else {
    $("gridXAxis").value = "preset";
    $("gridXValues").value = GRID_AXES.preset.fill();
  }
  $("gridYAxis").value = "size";
  $("gridYValues").value = "1024,896,768";
  gridUpdateCount();
}

function gridUpdateCount() {
  const xs = gridParseValues($("gridXAxis").value, $("gridXValues").value);
  const ys = gridParseValues($("gridYAxis").value, $("gridYValues").value);
  const n = xs.length * ys.length;
  $("gridCount").textContent = n ? `${xs.length} × ${ys.length} = ${n} 张${n > GRID_CAP ? `（上限 ${GRID_CAP}）` : ""}` : "";
  $("gridCount").classList.toggle("danger-text", n > GRID_CAP);
}

gridFillAxisSelects();
gridRestoreSaved();
gridUpdateCount();

$("gridXValues").oninput = gridUpdateCount;
$("gridYValues").oninput = gridUpdateCount;
$("gridXAxis").onchange = () => {
  $("gridXValues").placeholder = GRID_AXES[$("gridXAxis").value].hint;
  gridUpdateCount();
};
$("gridYAxis").onchange = () => {
  $("gridYValues").placeholder = GRID_AXES[$("gridYAxis").value].hint;
  gridUpdateCount();
};
$("gridXValues").placeholder = GRID_AXES[$("gridXAxis").value].hint;
$("gridYValues").placeholder = GRID_AXES[$("gridYAxis").value].hint;
$("gridTplModels").onclick = () => gridTemplate(true);
$("gridTplPresets").onclick = () => gridTemplate(false);
$("gridRun").onclick = () => {
  gridRun = gridBuild();
  if (!gridRun) return;
  localStorage.removeItem(GRID_KEY);
  $("gridResume").hidden = true;
  gridRunNow();
};
$("gridStop").onclick = () => gridStopNow();
$("gridResume").onclick = () => {
  $("gridResume").hidden = true;
  gridRunNow();
};
$("gridOpen").onclick = () => gridOpenTable();
$("gridModalClose").onclick = () => ($("gridModal").hidden = true);
$("gridModal").addEventListener("click", (e) => {
  if (e.target === $("gridModal")) $("gridModal").hidden = true;
});
