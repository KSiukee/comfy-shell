proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- HTML ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''          <button class="btn small ghost" id="btnPoses2" title="白模姿势库">🕺 姿势</button>''',
'''          <button class="btn small ghost" id="btnPoses2" title="白模姿势库">🕺 姿势</button>
          <button class="btn small ghost" id="btnAnalyze" title="丢一张图，让本地视觉模型拆成分类提示词">🔍 解析图</button>''', 1)

modal = '''
  <!-- image analyzer -->
  <div class="modal" id="anaModal" hidden>
    <div class="modal-card" style="width: 860px">
      <h3>看图拆提示词 <span class="muted" style="font-size:12px">本地 Qwen3-VL · 分类整理 · 不联网</span></h3>
      <div class="row tight">
        <input type="file" id="anaFile" accept="image/png,image/jpeg,image/webp" hidden />
        <button class="btn small" id="anaPick">选择图片</button>
        <button class="btn small primary" id="anaRun" disabled>解析</button>
        <span class="muted mono" id="anaState"></span>
        <span class="spacer"></span>
        <button class="btn ghost small" id="anaClose">关闭</button>
      </div>
      <div class="row tight" style="align-items:flex-start">
        <img id="anaPreview" alt="" style="width:220px;max-height:300px;object-fit:contain;border-radius:8px;background:#0f1116" />
        <div style="flex:1;display:flex;flex-direction:column;gap:6px" id="anaFields"></div>
      </div>
      <details id="anaRawBox">
        <summary>完整提示词 / 原始 JSON</summary>
        <textarea id="anaPrompt" rows="3" style="width:100%"></textarea>
        <textarea id="anaRaw" rows="5" style="width:100%;font-size:11px"></textarea>
      </details>
      <div class="row tight">
        <button class="btn small primary" id="anaFillPrompt">组合成提示词</button>
        <button class="btn small ghost" id="anaCopy">复制 JSON</button>
      </div>
    </div>
  </div>
'''
h = h.replace("  <!-- pose library -->", modal + "\n  <!-- pose library -->", 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html ok")

# ---------------- CSS ----------------
p = proj + r"\web\style.css"
c = open(p, encoding="utf-8").read()
c += '''
#anaFields .ana-row { display: grid; grid-template-columns: 64px 1fr; gap: 8px; align-items: center; }
#anaFields .ana-row span { font-size: 12px; color: #8e97a8; }
#anaFields input { font-size: 12.5px; }
'''
open(p, "w", encoding="utf-8", newline="\n").write(c)
print("css ok")

# ---------------- JS ----------------
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")

code = '''// ---------------------------------------------------------------- image analyzer
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

async function runAnalyze() {
  if (!anaDataUrl) return toast("先选一张图", true);
  const btn = $("anaRun");
  btn.disabled = true;
  $("anaState").textContent = "解析中…（首次加载视觉模型约 20–60 秒）";
  try {
    const name = await invoke("save_ref_image", { name: "ana_input.png", dataUrl: anaDataUrl });
    const raw = await invoke("analyze_image", { image: name });
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
  if (!anaDataUrl) buildAnaFields();
  $("anaModal").hidden = false;
};
$("anaClose").onclick = () => ($("anaModal").hidden = true);
$("anaPick").onclick = () => $("anaFile").click();
$("anaFile").onchange = (e) => analyzePicked(e.target.files[0]);
$("anaRun").onclick = runAnalyze;
$("anaFillPrompt").onclick = () => {
  const parts = ANA_FIELDS.map(([k]) => ($("ana_" + k)?.value || "").trim()).filter(Boolean);
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

'''
j = j.replace("// ---------------------------------------------------------------- pose library", code + "// ---------------------------------------------------------------- pose library", 1)
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok")
