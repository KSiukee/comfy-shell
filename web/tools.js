// 看图分析 / 姿势库（从 app.js 拆出；依赖 app.js 的全局）

// ---------------------------------------------------------------- image analyzer（#82）
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

async function runAnalyze() {
  if (!anaDataUrl && !anaImageName) return toast("先选一张图", true);
  const attempts = 2;
  let lastErr = "";
  for (let attempt = 0; attempt < attempts; attempt++) {
    const btn = $("anaRun");
    btn.disabled = true;
    $("anaState").textContent =
      attempt === 0 ? "看图拆五块…（首次加载视觉模型约 20–60 秒）" : "模型没按五块回来，再试一次…";
    try {
      let name = anaImageName;
      if (!name) {
        name = await api.call("save_ref_image", { name: "ana_input.png", dataUrl: anaDataUrl });
        anaImageName = name;
      }
      const res = await api.call("analyze_blocks", { image: name });
      anaBlocks = (res.blocks || []).map((b) => ({ key: b.key, items: b.items || [] }));
      anaPicked.clear();
      $("anaRaw").value = res.raw || "";
      paintAna();
      $("anaState").textContent = `完成 · ${((res.ms || 0) / 1000).toFixed(1)} 秒`;
      return;
    } catch (e) {
      lastErr = String(e);
    } finally {
      btn.disabled = false;
    }
  }
  $("anaState").textContent = "";
  toast(lastErr || "解析失败", true);
}

$("btnAnalyze").onclick = () => {
  $("layoutModal").hidden = true;
  $("poseModal").hidden = true;
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

// ---------------------------------------------------------------- pose library
let poseCatalog = null;
let poseShown = 0;
const poseCache = new Map();

let posePackId = "";
let posePackInfo = new Map();

async function openPoses() {
  try {
    if (!$("posePack").options.length) {
      const packs = await api.call("pose_pack_list");
      if (!packs.length) throw "没有可用的姿势库";
      posePackInfo = new Map(packs.map((p) => [p.id, p]));
      $("posePack").innerHTML = packs
        .map((p) => `<option value="${p.id}">${p.name} · ${p.count}${p.publishable ? "" : "（仅本地）"}</option>`)
        .join("");
      posePackId = localStorage.getItem("cs.posePack") || packs[0].id;
      if (!packs.some((p) => p.id === posePackId)) posePackId = packs[0].id;
      $("posePack").value = posePackId;
    }
    await loadPosePack($("posePack").value || posePackId);
  } catch (e) {
    toast(String(e), true);
    return;
  }
  $("poseModal").hidden = false;
}

async function loadPosePack(id) {
  posePackId = id;
  poseCatalog = JSON.parse(await api.call("poses_catalog", { pack: id }));
  const classes = [...new Set(poseCatalog.map((p) => p.class_cn).filter(Boolean))].sort();
  $("poseClass").innerHTML =
    `<option value="">全部类别</option>` + classes.map((c) => `<option value="${c}">${c}</option>`).join("");
  poseShown = 0;
  $("poseGrid").innerHTML = "";
  const info = posePackInfo.get(id) || { name: id, license: "未标注", publishable: false, note: "" };
  $("poseMeta").textContent = `${info.name} · ${poseCatalog.length} 组 · ${info.publishable ? "可发布" : "仅本地"}`;
  const hint = $("poseLicenseHint");
  if (info.publishable) {
    hint.hidden = true;
    hint.textContent = "";
  } else {
    hint.hidden = false;
    hint.textContent = `⚠ ${info.name}：${info.license}${info.note ? " · " + info.note : ""} —— 生成图发布/商用前请确认版权`;
    hint.style.color = "#d9a441";
  }
  renderPoses();
}

function poseLicenseWarn() {
  const p = window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
  if (!p) return "";
  return localStorage.getItem(`cs.poseWarn.${p.id}`) || "";
}

$("posePack").onchange = async () => {
  const id = $("posePack").value;
  localStorage.setItem("cs.posePack", id);
  try {
    await loadPosePack(id);
  } catch (e) {
    toast(String(e), true);
  }
};

function poseView(p, angle) {
  const v = p.views || {};
  return v[angle] || v.front || Object.values(v)[0] || "";
}

function poseFiltered() {
  const q = $("poseSearch").value.trim().toLowerCase();
  const cls = $("poseClass").value;
  const angle = $("poseAngle").value;
  return poseCatalog.filter((p) => {
    if (cls && p.class_cn !== cls) return false;
    // 缺这个机位就回退到正面/任意可用机位，别把整组姿势藏掉
    if (!poseView(p, angle)) return false;
    if (!q) return true;
    const hay = `${p.id} ${p.name_cn || ""} ${p.name_en || ""} ${p.class_cn || ""} ${p.desc_en || ""}`.toLowerCase();
    return hay.includes(q);
  });
}

function renderPoses() {
  const list = poseFiltered();
  const grid = $("poseGrid");
  const endCount = Math.min(list.length, poseShown || 80);
  $("poseCount").textContent = `${endCount} / ${list.length}`;
  if (!list.length) {
    grid.innerHTML = '<div class="lib-empty">没有匹配的姿势</div>';
    return;
  }
  const angle = $("poseAngle").value;
  const end = Math.min(list.length, poseShown || 80);
  poseShown = end;
  grid.innerHTML = "";
  for (const p of list.slice(0, end)) {
    const file = poseView(p, angle);
    if (!file) continue;
    const thumb = (p.thumbs || {})[angle] || file;
    const d = document.createElement("div");
    d.className = "pose-card";
    d.innerHTML = `<img data-file="${thumb}" alt="${p.id}"><div class="pc-name">${p.id} · ${p.name_cn || p.name_en || ""}</div><span class="pc-word" title="复制英文描述">词</span><span class="pc-word pc-fill" title="把英文描述填进提示词">填</span>`;
    const img = d.querySelector("img");
    img.onerror = () => d.remove();
    d.querySelector(".pc-fill").onclick = (e) => {
      e.stopPropagation();
      const t = p.desc_en || p.name_en || "";
      if (!t) return;
      const box = $("prompt");
      box.value = box.value.trim() ? `${box.value.trim()}, ${t}` : t;
      toast("已填入姿势描述");
    };
    d.querySelector(".pc-word").onclick = (e) => {
      e.stopPropagation();
      navigator.clipboard.writeText(p.desc_en || p.name_en || "").then(() => toast("已复制姿势描述"));
    };
    d.onclick = async () => {
      if (!profile()?.supportsEdit) {
        const t = p.desc_en || p.name_en || "";
        const box = $("prompt");
        if (t) box.value = box.value.trim() ? `${box.value.trim()}, ${t}` : t;
        toast("这个模型不支持参考图，已把姿势描述填进提示词");
        $("poseModal").hidden = true;
        return;
      }
      try {
        let url = poseCache.get(file);
        if (!url) {
          url = await api.call("read_input_image", { name: file });
          poseCache.set(file, url);
        }
        await addRefFromDataUrl(url, `pose_${p.id}_${angle}`);
        const info = posePackInfo.get(posePackId);
        const proj = window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
        if (info && !info.publishable && proj) {
          localStorage.setItem(`cs.poseWarn.${proj.id}`, `${info.name}｜${info.license}`);
        }
        $("poseModal").hidden = true;
      } catch (e) {
        toast(String(e), true);
      }
    };
    grid.appendChild(d);
  }
  lazyLoadPoses();
}

let poseObserver = null;
function lazyLoadPoses() {
  if (!poseObserver) {
    poseObserver = new IntersectionObserver((entries) => {
      for (const en of entries) {
        if (!en.isIntersecting) continue;
        const img = en.target;
        const file = img.dataset.file;
        if (!file || img.src) continue;
        api.call("read_input_image", { name: file })
          .then((url) => {
            img.src = url;
            poseCache.set(file, url);
          })
          .catch(() => {});
        poseObserver.unobserve(img);
      }
    }, { root: $("poseGrid"), rootMargin: "300px" });
  }
  document.querySelectorAll("#poseGrid img[data-file]").forEach((img) => {
    if (!img.src) poseObserver.observe(img);
  });
}

$("btnPoses").onclick = () => { $("anaModal").hidden = true; $("layoutModal").hidden = true; openPoses(); };
$("btnPoses2").onclick = () => { $("anaModal").hidden = true; $("layoutModal").hidden = true; openPoses(); };
$("poseClose").onclick = () => ($("poseModal").hidden = true);
$("poseSearch").oninput = () => { poseShown = 0; renderPoses(); };
$("poseClass").onchange = () => { poseShown = 0; renderPoses(); };
$("poseAngle").onchange = () => { poseShown = 0; renderPoses(); };
$("poseGrid").addEventListener("scroll", () => {
  const g = $("poseGrid");
  if (g.scrollTop + g.clientHeight < g.scrollHeight - 200) return;
  if (poseShown < poseFiltered().length) {
    poseShown += 80;
    renderPoses();
  }
});
