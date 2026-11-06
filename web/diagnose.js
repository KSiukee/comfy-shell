// 崩图诊断：看一张坏图 + 刚才的参数，说原因、给能直接点「应用」的改法。
//
// 与「看图拆提示词」共用同一条隐私闸：项目没开云端（或私密）就只走本地。
// 应用只改表单里的 cfg / 步数 / 负面词——提示词的改法只当建议显示，不替人改写。

let diagFixes = null;
let diagPath = "";

function diagParams() {
  return JSON.stringify({
    model: $("model") ? $("model").value : "",
    prompt: $("prompt") ? $("prompt").value : "",
    negative: $("negative") ? $("negative").value : "",
    width: Number($("width") ? $("width").value : 0) || null,
    height: Number($("height") ? $("height").value : 0) || null,
    steps: Number($("steps") ? $("steps").value : 0) || null,
    cfg: Number($("cfg") ? $("cfg").value : 0) || null,
    seed: $("seed") ? $("seed").value : "",
  });
}

function diagEngine() {
  const sel = $("diagEngine");
  const want = sel ? sel.value === "cloud" : false;
  // 项目关掉云端时不许选云端（与解析图一致）
  return want && typeof anaCloudAllowed === "function" && anaCloudAllowed();
}

async function diagShowImage(path) {
  diagPath = path || "";
  const img = $("diagImg");
  if (!img) return;
  if (!diagPath) {
    img.removeAttribute("src");
    return;
  }
  try {
    img.src = await api.call("image_data_url", { path: diagPath });
  } catch {
    img.removeAttribute("src");
  }
}

async function openDiagnose() {
  const list = (state.results || []).slice(0, 12);
  const sel = $("diagPick");
  sel.textContent = "";
  if (!list.length) {
    toast("先出几张图——诊断要看图", true);
    return;
  }
  for (const r of list) {
    const o = document.createElement("option");
    o.value = r.path;
    o.textContent = r.filename || r.path;
    sel.appendChild(o);
  }
  // 引擎下拉：项目不许云端时禁用云端
  const eng = $("diagEngine");
  if (eng) {
    const allow = typeof anaCloudAllowed === "function" ? anaCloudAllowed() : false;
    [...eng.options].forEach((o) => {
      if (o.value === "cloud") o.disabled = !allow;
    });
    if (!allow) eng.value = "local";
  }
  $("diagOut").innerHTML = `<div class="muted">点「诊断」，它会看这张图和表单里现在的参数。</div>`;
  $("diagApply").disabled = true;
  $("diagSource").textContent = "";
  diagFixes = null;
  $("diagModal").hidden = false;
  await diagShowImage(list[0].path);
}

function diagCauseHtml(c) {
  const d = document.createElement("div");
  d.style.cssText = "padding:6px 0;border-bottom:1px dashed #252b3a";
  const b = document.createElement("b");
  b.textContent = c.title || "（没写原因）";
  const p = document.createElement("div");
  p.className = "muted";
  p.style.cssText = "font-size:12px;margin-top:2px";
  p.textContent = c.why || "";
  d.append(b, p);
  return d;
}

function diagRender(data) {
  const box = $("diagOut");
  box.textContent = "";
  const causes = (data && data.causes) || [];
  const fixes = (data && data.fixes) || {};
  diagFixes = fixes;

  if (!causes.length) {
    const ok = document.createElement("div");
    ok.className = "muted";
    ok.textContent = (data && data.note) || "没看出明显问题。";
    box.appendChild(ok);
    $("diagApply").disabled = true;
    return;
  }

  for (const c of causes) box.appendChild(diagCauseHtml(c));

  const rows = [];
  const now = {
    cfg: Number($("cfg").value),
    steps: Number($("steps").value),
  };
  if (fixes.cfg != null) rows.push(`CFG：${now.cfg} → ${fixes.cfg}`);
  if (fixes.steps != null) rows.push(`步数：${now.steps} → ${fixes.steps}`);
  if (fixes.negativeAdd) rows.push(`负面词追加：${fixes.negativeAdd}`);
  if (fixes.promptHint) rows.push(`提示词建议：${fixes.promptHint}（只显示，不改你的词）`);

  if (rows.length) {
    const t = document.createElement("div");
    t.style.cssText = "margin-top:10px;font-size:12.5px";
    t.innerHTML = `<b>建议改动</b>` + rows.map((r) => `<div style="margin-top:4px">· ${r}</div>`).join("");
    box.appendChild(t);
  }
  const canApply = fixes.cfg != null || fixes.steps != null || !!fixes.negativeAdd;
  $("diagApply").disabled = !canApply;
}

async function diagRun() {
  if (!diagPath) return toast("先选一张图", true);
  const btn = $("diagRun");
  btn.disabled = true;
  btn.textContent = "诊断中…";
  $("diagOut").innerHTML = `<div class="muted">正在看图…</div>`;
  try {
    const data = await api.call("diagnose_image", {
      image: diagPath,
      project: (window.projectUI && projectUI.current && projectUI.current()) || null,
      cloud: diagEngine(),
      params: diagParams(),
    });
    diagRender(data);
    $("diagSource").textContent = `${data.source === "cloud" ? "云端" : "本地"} · ${data.model || ""} · ${Math.round((data.ms || 0) / 100) / 10} 秒`;
    if ((data.notes || []).length) toast(data.notes.join("；"));
  } catch (e) {
    $("diagOut").innerHTML = "";
    toast(String(e), true);
  } finally {
    btn.disabled = false;
    btn.textContent = "诊断";
  }
}

function diagApply() {
  if (!diagFixes) return;
  const changed = [];
  if (diagFixes.cfg != null) {
    $("cfg").value = String(diagFixes.cfg);
    changed.push(`CFG=${diagFixes.cfg}`);
  }
  if (diagFixes.steps != null) {
    $("steps").value = String(diagFixes.steps);
    if (typeof syncStepChips === "function") syncStepChips();
    changed.push(`步数=${diagFixes.steps}`);
  }
  if (diagFixes.negativeAdd) {
    const neg = $("negative");
    const add = String(diagFixes.negativeAdd);
    if (!neg.value.includes(add)) neg.value = (neg.value ? neg.value.replace(/,\s*$/, "") + ", " : "") + add;
    changed.push("负面词已追加");
  }
  if (!changed.length) return toast("这次没有能直接改的参数", true);
  toast("已应用：" + changed.join(" · "));
  $("diagModal").hidden = true;
}

$("diagPick").onchange = () => diagShowImage($("diagPick").value);
$("diagRun").onclick = diagRun;
$("diagApply").onclick = diagApply;
$("diagClose").onclick = () => ($("diagModal").hidden = true);
$("diagModal").addEventListener("click", (e) => {
  if (e.target === $("diagModal")) $("diagModal").hidden = true;
});
