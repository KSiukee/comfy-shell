// 自定义工作流参数表单（#27）：从工作流 JSON 推断可调的节点输入，画成表单，
// 值按工作流记住；生成时作为 `req.wfParams`（键 `节点|输入`）交给后端覆盖。

let wfParamSpec = [];
let wfParamValues = {};
let wfParamName = "";

const WF_KNOWN = new Set([
  "steps", "cfg", "denoise", "seed", "noise_seed", "width", "height", "batch_size",
  "strength", "strength_model", "strength_clip", "weight", "start_at_step", "end_at_step",
  "shift", "sampler_name", "scheduler", "upscale_method", "grow_mask_by", "expand",
]);

function wfCurrentName() {
  const v = $("model").value || "";
  return v.startsWith("custom:") ? v.slice(7) : "";
}

function wfStoreKey(name) {
  return `cs.wfparams.${name}`;
}

function wfLoadValues(name) {
  try {
    const raw = JSON.parse(localStorage.getItem(wfStoreKey(name)) || "{}");
    return raw && typeof raw === "object" ? raw : {};
  } catch {
    return {};
  }
}

function wfSaveValues() {
  if (!wfParamName) return;
  try {
    localStorage.setItem(wfStoreKey(wfParamName), JSON.stringify(wfParamValues));
  } catch {}
}

function wfCoerce(p, raw) {
  if (p.type === "bool") return !!raw;
  if (p.type === "int") {
    const n = Math.round(Number(raw));
    return Number.isFinite(n) ? n : Number(p.default) || 0;
  }
  if (p.type === "float") {
    const n = Number(raw);
    return Number.isFinite(n) ? n : Number(p.default) || 0;
  }
  return String(raw);
}

function wfValueOf(p) {
  if (Object.prototype.hasOwnProperty.call(wfParamValues, p.key)) return wfParamValues[p.key];
  return p.default;
}

function wfRender() {
  const box = $("wfParams");
  box.textContent = "";
  const field = $("wfParamsField");
  if (!wfParamName) {
    field.hidden = true;
    return;
  }
  field.hidden = false;
  $("wfParamsHint").textContent = wfParamSpec.length
    ? `${wfParamSpec.length} 项 · 改动只影响这个工作流`
    : "这个工作流没有可识别的参数（也可以直接在 JSON 里写 {{steps}} 之类的占位符）";
  for (const p of wfParamSpec) {
    const row = document.createElement("div");
    row.className = "wf-param";
    const lab = document.createElement("label");
    lab.textContent = p.label;
    lab.title = `${p.node} · ${p.input}`;
    let input;
    if (p.type === "bool") {
      input = document.createElement("input");
      input.type = "checkbox";
      input.checked = !!wfValueOf(p);
    } else if (p.type === "enum" && p.options && p.options.length) {
      input = document.createElement("select");
      for (const opt of p.options) input.appendChild(new Option(opt, opt));
      input.value = String(wfValueOf(p));
    } else {
      input = document.createElement("input");
      input.type = "number";
      input.value = String(wfValueOf(p));
      if (p.type === "float") input.step = "0.01";
      if (typeof p.min === "number") input.min = String(p.min);
      if (typeof p.max === "number") input.max = String(p.max);
    }
    input.dataset.key = p.key;
    input.addEventListener("input", () => {
      wfParamValues[p.key] = wfCoerce(p, p.type === "bool" ? input.checked : input.value);
      wfSaveValues();
    });
    if (p.type === "bool") {
      input.addEventListener("change", () => {
        wfParamValues[p.key] = input.checked;
        wfSaveValues();
      });
    }
    row.append(lab, input);
    box.appendChild(row);
  }
}

async function wfRefresh() {
  const name = wfCurrentName();
  if (!name) {
    wfParamName = "";
    wfParamSpec = [];
    wfParamValues = {};
    wfRender();
    return;
  }
  if (name === wfParamName) {
    wfRender();
    return;
  }
  wfParamName = name;
  wfParamValues = wfLoadValues(name);
  try {
    wfParamSpec = (await api.call("custom_params", { name })) || [];
  } catch (e) {
    wfParamSpec = [];
    toast(`读工作流参数失败：${e}`, true);
  }
  wfRender();
}

function wfCollect() {
  if (!wfParamName) return null;
  const out = {};
  for (const p of wfParamSpec) {
    out[p.key] = wfValueOf(p);
  }
  return Object.keys(out).length ? out : null;
}

$("wfParamsReset").onclick = () => {
  wfParamValues = {};
  wfSaveValues();
  wfRender();
  toast("已恢复默认值");
};

// 生成时挂上参数：包一层 currentReq（app.js 里是函数声明，可覆盖）
const wfBaseCurrentReq = currentReq;
currentReq = function () {
  const req = wfBaseCurrentReq();
  const params = wfCollect();
  if (params) req.wfParams = params;
  return req;
};

$("model").addEventListener("change", () => {
  wfRefresh();
});
wfRefresh();
