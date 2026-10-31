import re
proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- HTML: sweep controls + LoRA hint ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''        <summary>模型横扫 <span class="muted">同一提示词，每个模型各出一张</span></summary>
        <div class="sweep-models" id="sweepModels"></div>
        <div class="row tight">''', '''        <summary>横扫对比 <span class="muted">同一提示词，按模型 / 人像预设批量出图</span></summary>
        <div class="row tight">
          <select id="sweepMode" style="flex: 1 1 0">
            <option value="presets">按人像预设</option>
            <option value="models">按模型</option>
          </select>
          <select id="sweepSize" style="flex: 1 1 0">
            <option value="one">当前尺寸</option>
            <option value="five">5 个尺寸（512→1024）</option>
          </select>
          <select id="sweepBatch" style="flex: 0 0 90px">
            <option value="1">1 张</option>
            <option value="2">2 张</option>
            <option value="3">3 张</option>
          </select>
        </div>
        <div class="sweep-models" id="sweepModels"></div>
        <div class="row tight">''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html ok")

# ---------------- JS ----------------
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")

# 1) hide poses without the selected view
j = j.replace('''function poseFiltered() {
  const q = $("poseSearch").value.trim().toLowerCase();
  const cls = $("poseClass").value;
  return poseCatalog.filter((p) => {
    if (cls && p.class_cn !== cls) return false;''', '''function poseFiltered() {
  const q = $("poseSearch").value.trim().toLowerCase();
  const cls = $("poseClass").value;
  const angle = $("poseAngle").value;
  return poseCatalog.filter((p) => {
    if (cls && p.class_cn !== cls) return false;
    // entries without this camera angle would show a broken/duplicated tile
    if (!(p.views || {})[angle]) return false;''', 1)

# 2) LoRA human descriptions
desc_block = '''const LORA_DESC = [
  [/ultra_real/i, "去 AI 塑料感，给皮肤/布料加纹理，强度 0.6–0.8"],
  [/beauty/i, "人像美化、柔和磨皮，强度 0.7"],
  [/nsfw/i, "训练者自己的成人影像风格，强度 0.7–0.9"],
  [/anything2real/i, "把任意画风转成真人照片（改图用，强度 0.8）"],
  [/pixel-art/i, "像素画风"],
  [/turbo/i, "加速 LoRA，配合 8 步使用"],
  [/(probe|test\\d|ui_probe|folder_probe)/i, "自己训练的测试 LoRA，可删"],
];

function loraTip(name) {
  const hit = LORA_DESC.find(([re]) => re.test(name));
  return hit ? hit[1] : "";
}

'''
j = j.replace("const LORA_RULES = [", desc_block + "const LORA_RULES = [", 1)

# show the tip in the hint line under the LoRA picker
j = j.replace('''  $("loraHint").textContent = name
    ? `强度 ${Number($("loraStrength").value).toFixed(2)}`''', '''  const tip = name ? loraTip(name) : "";
  $("loraHint").textContent = name
    ? `${tip ? tip + " · " : ""}强度 ${Number($("loraStrength").value).toFixed(2)}`''', 1)

# 3) extract preset application so the sweep can reuse it
j = j.replace('''$("presetSel").onchange = () => {
  const key = $("presetSel").value;
  const preset = PORTRAIT_PRESETS[key];
  if (!preset) return;''', '''function applyPortraitPreset(key, silent) {
  const preset = PORTRAIT_PRESETS[key];
  if (!preset) return false;''', 1)
j = j.replace('''  $("mossSlg").checked = !!preset.slg;
  state.slgTouched = true;
  toast(`已套用预设：${$("presetSel").selectedOptions[0].textContent}`);
};''', '''  $("mossSlg").checked = !!preset.slg;
  state.slgTouched = true;
  $("presetSel").value = key;
  if (!silent) toast(`已套用预设：${$("presetSel").selectedOptions[0].textContent}`);
  return true;
}

$("presetSel").onchange = () => applyPortraitPreset($("presetSel").value, false);''', 1)

# 4) sweep: presets / 5 sizes / batch
old_start = j.index("async function runSweep() {")
old_end = j.index("// ---------------------------------------------------------------- export comparison", old_start)
new_sweep = '''const SWEEP_SIZES = [512, 640, 768, 896, 1024];

async function runSweep() {
  const mode = $("sweepMode").value;
  const sizeMode = $("sweepSize").value;
  const batch = Number($("sweepBatch").value) || 1;
  const items = mode === "presets" ? Object.keys(PORTRAIT_PRESETS) : sweepSelection();
  if (!items.length) {
    toast("至少选一个", true);
    return;
  }
  const prompt = $("prompt").value.trim();
  if (!prompt) {
    toast("先写提示词", true);
    return;
  }
  const w = Number($("width").value);
  const h = Number($("height").value);
  let seed = Number($("seed").value);
  if (seed < 0) seed = Math.floor(Math.random() * 1e9);
  if (!multiStart()) return;

  const sizes = sizeMode === "five" ? SWEEP_SIZES.map((s) => [s, s]) : [[w, h]];
  const total = items.length * sizes.length;
  const btn = $("btnSweepRun");
  btn.disabled = true;
  let done = 0;
  for (const item of items) {
    if (mode === "presets") {
      applyPortraitPreset(item, true);
    } else {
      $("model").value = item;
      $("model").dispatchEvent(new Event("change", { bubbles: true }));
    }
    // a sweep is text-to-image by definition: drop edit leftovers for a fair comparison
    setMode("t2i");
    state.refs = [];
    renderRefs();
    $("turbo").checked = false;
    for (const [sw, sh] of sizes) {
      $("sweepState").textContent = `${done}/${total} · ${sw}px · ×${batch}`;
      $("width").value = sw;
      $("height").value = sh;
      $("seed").value = seed;
      $("batch").value = batch;
      $("prompt").value = prompt;
      const ok = await generate();
      if (ok) done++;
    }
  }
  $("sweepState").textContent = `完成 ${done}/${total}`;
  btn.disabled = false;
  multiEnd();
  toast(`横扫结束：${done}/${total} 组（seed ${seed}，每组 ×${batch}）`);
}

'''
j = j[:old_start] + new_sweep + j[old_end:]

open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok")
