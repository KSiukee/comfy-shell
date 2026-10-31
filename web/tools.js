// 看图分析 / 姿势库（从 app.js 拆出；依赖 app.js 的全局）

// ---------------------------------------------------------------- image analyzer

const ANA_FIELDS = [
  ["pose", "姿势"],
  ["expression", "表情"],
  ["clothing", "衣着"],
  ["accessories", "配饰"],
  ["background", "背景"],
  ["style", "风格"],
  ["colors", "色彩"],
  ["lighting", "光线"],
  ["mood", "氛围"],
];
let anaDataUrl = "";

function buildAnaFields() {
  const box = $("anaFields");
  box.innerHTML = "";
  for (const [key, label] of ANA_FIELDS) {
    const row = document.createElement("label");
    row.className = "ana-row";
    row.innerHTML = `<span>${label}</span><input id="ana_${key}" placeholder="" />`;
    box.appendChild(row);
  }
}

async function analyzePicked(file) {
  if (!file) return;
  anaDataUrl = await readFileDataUrl(file);
  $("anaPreview").src = anaDataUrl;
  $("anaRun").disabled = false;
  $("anaState").textContent = "";
}

let anaExtras = [];

function renderExtras(list) {
  anaExtras = (list || []).map((x) => ({ label: x.label || "", value: x.value || "" }));
  paintExtras();
}

function paintExtras() {
  const box = $("anaExtras");
  box.innerHTML = "";
  anaExtras.forEach((ex, i) => {
    const row = document.createElement("div");
    row.className = "xrow";
    const label = document.createElement("input");
    label.value = ex.label;
    label.placeholder = "分类（如 weather）";
    label.oninput = () => (ex.label = label.value);
    const value = document.createElement("input");
    value.value = ex.value;
    value.placeholder = "内容（如 light rain）";
    value.oninput = () => (ex.value = value.value);
    const del = document.createElement("button");
    del.className = "btn small ghost";
    del.textContent = "×";
    del.onclick = () => {
      anaExtras.splice(i, 1);
      paintExtras();
    };
    row.append(label, value, del);
    box.appendChild(row);
  });
}

$("anaAddExtra").onclick = () => {
  anaExtras.push({ label: "", value: "" });
  paintExtras();
};

async function runAnalyze() {
  if (!$("ana_pose")) buildAnaFields();
  if (!anaDataUrl) return toast("先选一张图", true);
  const btn = $("anaRun");
  btn.disabled = true;
  $("anaState").textContent = "解析中…（首次加载视觉模型约 20–60 秒）";
  try {
    const name = await api.call("save_ref_image", { name: "ana_input.png", dataUrl: anaDataUrl });
    const raw = await api.call("analyze_image", { image: name, twoPass: $("anaTwoPass").checked });
    $("anaRaw").value = raw;
    let data = {};
    try {
      data = JSON.parse(raw);
    } catch {
      $("anaState").textContent = "模型没返回标准 JSON，看下面原始输出";
    }
    for (const [key] of ANA_FIELDS) {
      const el = $("ana_" + key);
      if (el) el.value = data[key] || "";
    }
    renderExtras(data.extras || []);
    $("anaPrompt").value = data.prompt || "";
    $("anaRawBox").open = !Object.keys(data).length;
    $("anaState").textContent = Object.keys(data).length ? "解析完成" : "解析完成（格式异常）";
  } catch (e) {
    $("anaState").textContent = "";
    toast(String(e), true);
  } finally {
    btn.disabled = false;
  }
}

$("btnAnalyze").onclick = () => {
  $("layoutModal").hidden = true; $("poseModal").hidden = true;
  buildAnaFields();
  $("anaModal").hidden = false;
};
$("anaClose").onclick = () => ($("anaModal").hidden = true);
$("anaPick").onclick = () => $("anaFile").click();
$("anaFile").onchange = (e) => analyzePicked(e.target.files[0]);
$("anaRun").onclick = runAnalyze;
$("anaFillPrompt").onclick = () => {
  const parts = ANA_FIELDS.map(([k]) => ($("ana_" + k)?.value || "").trim()).filter(Boolean);
  for (const ex of anaExtras) {
    const v = (ex.value || "").trim();
    if (v) parts.push(v);
  }
  if (!parts.length) return toast("还没有解析结果", true);
  const text = parts.join(", ");
  $("prompt").value = text;
  $("anaModal").hidden = true;
  toast("已填入提示词");
};
$("anaCopy").onclick = async () => {
  const raw = $("anaRaw").value.trim();
  if (!raw) return toast("还没有解析结果", true);
  await navigator.clipboard.writeText(raw);
  toast("已复制 JSON");
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
