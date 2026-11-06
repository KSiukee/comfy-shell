// 看图拆提示词：五块候选 + 点选回填 + 云端/本地双引擎。
// 2026-11-05 从 tools.js 拆出来——那边只留姿势库，别让一个文件同时管两件事。

// ---------------------------------------------------------------- image analyzer
// 五块候选（主体/镜头/光线/色彩/风格），点一下接进提示词，再点一次撤回。
// 只动提示词，不动参数；视觉模型在本地跑（不联网）。

const ANA_BLOCKS = [
  ["subject", "主体"],
  ["camera", "镜头"],
  ["light", "光线"],
  ["color", "色彩"],
  ["style", "风格"],
];
let anaDataUrl = "";
let anaImageName = ""; // 已经在 ComfyUI/input 里的名字（选了新图或用了参考图）
let anaBlocks = []; // [{key, items:[…]}]
let anaPicked = new Set(); // 这一轮点进去的片段（只给按钮状态与撤回用）

function anaLabel(key) {
  const hit = ANA_BLOCKS.find(([k]) => k === key);
  return hit ? hit[1] : key;
}

function paintAna() {
  const box = $("anaBlocks");
  if (!box) return;
  box.textContent = "";
  if (!anaBlocks.length) {
    const hint = document.createElement("div");
    hint.className = "muted";
    hint.textContent = "选一张图 → 解析：会拆成「主体 / 镜头 / 光线 / 色彩 / 风格」五块，每块 2–3 条候选。";
    box.appendChild(hint);
    return;
  }
  for (const b of anaBlocks) {
    const row = document.createElement("div");
    row.className = "ana-block";
    const head = document.createElement("div");
    head.className = "ana-block-head";
    const name = document.createElement("b");
    name.textContent = anaLabel(b.key);
    const count = document.createElement("span");
    count.className = "muted";
    count.textContent = `${b.items.length} 条`;
    head.append(name, count);
    const chips = document.createElement("div");
    chips.className = "chips ana-chips";
    for (const text of b.items) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "ana-chip" + (anaPicked.has(text) ? " on" : "");
      btn.textContent = text;
      btn.title = anaPicked.has(text) ? "再点一次从提示词里撤回" : "接进提示词";
      btn.onclick = () => anaToggle(text);
      chips.appendChild(btn);
    }
    row.append(head, chips);
    box.appendChild(row);
  }
}

function anaSep(text) {
  return /[\u4e00-\u9fff]/.test(text) ? "，" : ", ";
}

function anaAppend(text) {
  const ta = $("prompt");
  const cur = (ta.value || "").trim().replace(/[，,]\s*$/, "");
  ta.value = cur ? cur + anaSep(text) + text : text;
  ta.dispatchEvent(new Event("input", { bubbles: true }));
}

/// 撤回：只在这一条还在末尾时动手（提示词被手动改过就如实说撤不了）。
function anaRemove(text) {
  const ta = $("prompt");
  const cur = (ta.value || "").replace(/\s+$/, "");
  if (!cur.endsWith(text)) return false;
  let head = cur.slice(0, cur.length - text.length).replace(/\s+$/, "");
  head = head.replace(/[，,]$/, "").replace(/\s+$/, "");
  ta.value = head;
  ta.dispatchEvent(new Event("input", { bubbles: true }));
  return true;
}

function anaToggle(text) {
  if (anaPicked.has(text)) {
    if (!anaRemove(text)) return toast("提示词后来被改过，这条没法自动撤回", true);
    anaPicked.delete(text);
  } else {
    anaAppend(text);
    anaPicked.add(text);
  }
  paintAna();
}

async function analyzePicked(file) {
  if (!file) return;
  anaDataUrl = await readFileDataUrl(file);
  anaImageName = "";
  $("anaPreview").src = anaDataUrl;
  $("anaRun").disabled = false;
  $("anaState").textContent = "";
}

async function analyzeUseRef() {
  const name = (state.refs || [])[0];
  if (!name) return toast("当前没有参考图，先拖一张进来", true);
  try {
    const url = await api.call("ref_preview", { name });
    anaDataUrl = "";
    anaImageName = name;
    $("anaPreview").src = url;
    $("anaRun").disabled = false;
    $("anaState").textContent = "用参考图：" + name;
  } catch (e) {
    toast(String(e), true);
  }
}

const ANA_ENGINE_KEY = "cs.anaEngine";

function anaProject() {
  return (window.projectUI && projectUI.current && projectUI.current()) || null;
}

function anaProjObj() {
  return (window.projectUI && projectUI.currentObj && projectUI.currentObj()) || null;
}

/// 这个项目允许把图发去云端吗。没归项目的「临时」按允许算（否则每次都要先建项目）。
function anaCloudAllowed() {
  const p = anaProjObj();
  if (!p) return true;
  // 私密 / 隐藏都算锁定：没显式开过就只许本地
  const locked = !!(p.private || p.hidden);
  return p.cloudVision === null || p.cloudVision === undefined ? !locked : !!p.cloudVision;
}

/// 项目开关一变，引擎选择要跟着收：不许云端的项目只有本地一条路，并且说清为什么。
function anaSyncEngine() {
  const sel = $("anaEngine");
  if (!sel) return;
  const allow = anaCloudAllowed();
  const p = anaProjObj();
  const opt = [...sel.options].find((o) => o.value === "cloud");
  if (opt) opt.disabled = !allow;
  if (!allow && sel.value === "cloud") sel.value = "local";
  // 项目级开关就摆在这里（不挤项目栏）；「临时」没有项目，不显示
  const wrap = $("anaCloudWrap");
  const box = $("anaProjectCloud");
  if (wrap) wrap.hidden = !p;
  if (box) {
    box.checked = allow;
    box.onchange = anaToggleProjectCloud;
  }
  const hint = $("anaEngineHint");
  if (!hint) return;
  if (allow) {
    hint.textContent = "";
  } else {
    hint.textContent =
      p && p.private
        ? "这个私密项目没开云端——勾上「本项目允许云端」才能发（要再确认一次）"
        : "这个项目没开云端（隐藏项目也算锁定）——勾上「本项目允许云端」才能发";
  }
}

/// 在用到它的地方直接开关，不去挤顶栏。
async function anaToggleProjectCloud() {
  const p = anaProjObj();
  const box = $("anaProjectCloud");
  if (!p || !box) return;
  const on = !!box.checked;
  if (on && p.private) {
    const yes = confirm(
      `「${p.name}」在锁定区里（私密项目）。\n\n` +
        "打开云端看图后，每次「解析图」选云端，这张图都会被发到视觉 API（离开本机）。\n" +
        "默认只用本地 Qwen3-VL。\n\n确定要为这个项目打开云端看图吗？"
    );
    if (!yes) {
      box.checked = false;
      return;
    }
  }
  try {
    await api.call("project_set_cloud_vision", { id: p.id, on, confirm: on && p.private });
    if (window.projectUI && projectUI.refresh) await projectUI.refresh(p.id);
    anaSyncEngine();
    toast(`「${p.name}」云端看图已${on ? "打开" : "关掉"}`);
  } catch (e) {
    box.checked = !on;
    toast(String(e), true);
  }
}

/// 引擎选择：记住上次用的；默认云端（有 key 时更快，也不吃显卡）。
function anaInitEngine() {
  const sel = $("anaEngine");
  if (!sel) return;
  sel.value = localStorage.getItem(ANA_ENGINE_KEY) === "local" ? "local" : "cloud";
  sel.onchange = () => {
    localStorage.setItem(ANA_ENGINE_KEY, sel.value);
    if (sel.value === "local") $("anaState").textContent = "";
  };
}

/// 发送前把「这次会发什么」摊开——图 + 端点 + 那段提示词，可取消。
function askCloudSend() {
  return new Promise((resolve) => {
    const box = $("cloudSend");
    $("cloudSendImg").src = $("anaPreview").src || "";
    $("cloudSendPrompt").value = "…";
    $("cloudSendTo").textContent = "…";
    const done = (ok) => {
      box.hidden = true;
      box.onclick = null;
      document.removeEventListener("keydown", onKey);
      resolve(ok);
    };
    const onKey = (e) => {
      if (e.key === "Escape") done(false);
    };
    $("cloudSendNo").onclick = () => done(false);
    $("cloudSendYes").onclick = () => done(true);
    box.onclick = (e) => {
      if (e.target === box) done(false);
    };
    document.addEventListener("keydown", onKey);
    box.hidden = false;
    api.call("vision_info")
      .then((info) => {
        $("cloudSendTo").textContent = `${info.base} · ${info.model}`;
        $("cloudSendPrompt").value = info.prompt || "";
      })
      .catch(() => {
        $("cloudSendTo").textContent = "（读不到云端设置）";
      });
  });
}

async function runAnalyze() {
  if (!anaDataUrl && !anaImageName) return toast("先选一张图", true);
  if ($("anaEngine").value === "cloud") {
    if (!anaCloudAllowed()) {
      anaSyncEngine();
      return toast("这个项目没开云端看图，只能用本地", true);
    }
    // 每次发云端都摊开给人看：发去哪、发什么
    if (!(await askCloudSend())) return;
  }
  const btn = $("anaRun");
  btn.disabled = true;
  let lastErr = "";
  for (let attempt = 0; attempt < 2; attempt++) {
    const engine = $("anaEngine").value;
    $("anaState").textContent =
      engine === "cloud"
        ? "云端看图中…（推理模型，约 5–20 秒）"
        : "本地看图拆五块…（首次加载视觉模型约 20–60 秒）";
    try {
      let name = anaImageName;
      if (!name) {
        name = await api.call("save_ref_image", { name: "ana_input.png", dataUrl: anaDataUrl });
        anaImageName = name;
      }
      const res = await api.call("analyze_blocks", {
        image: name,
        project: anaProject(),
        cloud: engine === "cloud",
      });
      anaBlocks = (res.blocks || []).map((b) => ({ key: b.key, items: b.items || [] }));
      anaPicked.clear();
      $("anaRaw").value = res.raw || "";
      paintAna();
      const who = res.source === "cloud" ? `云端 ${res.model || ""}` : `本地 ${res.model || "Qwen3-VL"}`;
      $("anaState").textContent = `完成 · ${who} · ${((res.ms || 0) / 1000).toFixed(1)} 秒`;
      if (res.notes && res.notes.length) toast(res.notes.join("；"));
      btn.disabled = false;
      return;
    } catch (e) {
      lastErr = String(e);
      if (engine === "cloud" && /(没开云端|关掉了云端|私密项目)/.test(lastErr)) {
        $("anaEngine").value = "local";
        anaSyncEngine();
        toast("这个项目不许云端，已切回本地", true);
        continue;
      }
      break;
    }
  }
  btn.disabled = false;
  $("anaState").textContent = "";
  toast(lastErr || "解析失败", true);
}

anaInitEngine();

$("btnAnalyze").onclick = () => {
  $("layoutModal").hidden = true;
  $("poseModal").hidden = true;
  anaSyncEngine();
  paintAna();
  $("anaModal").hidden = false;
};
$("anaClose").onclick = () => ($("anaModal").hidden = true);
$("anaPick").onclick = () => $("anaFile").click();
$("anaFile").onchange = (e) => analyzePicked(e.target.files[0]);
$("anaUseRef").onclick = analyzeUseRef;
$("anaRun").onclick = runAnalyze;
$("anaAll").onclick = () => {
  if (!anaBlocks.length) return toast("还没有解析结果", true);
  let n = 0;
  for (const b of anaBlocks) {
    for (const text of b.items) {
      if (anaPicked.has(text)) continue;
      anaAppend(text);
      anaPicked.add(text);
      n += 1;
    }
  }
  paintAna();
  toast(n ? `已接进 ${n} 条` : "五块已经全接进去了");
};
$("anaCopy").onclick = async () => {
  const raw = $("anaRaw").value.trim();
  if (!raw) return toast("还没有解析结果", true);
  try {
    await navigator.clipboard.writeText(raw);
    toast("已复制 JSON");
  } catch {
    toast("复制失败，手动选一下原始输出", true);
  }
};
