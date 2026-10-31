proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- HTML: preset selector ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''        <div class="hint" id="modelHint"></div>
      </div>''', '''        <div class="hint" id="modelHint"></div>
        <div class="row tight pro-only">
          <span class="muted" style="font-size:11.5px;white-space:nowrap">人像预设</span>
          <select id="presetSel">
            <option value="">自定义</option>
            <option value="real">真实感（Imagine + UltraReal 0.7）</option>
            <option value="natural">自然耐看（SFT + Ultra 0.7 + Beauty 0.5）</option>
            <option value="polished">精致人像（SFT + Beauty 0.7）</option>
          </select>
        </div>
      </div>''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html ok")

# ---------------- JS: presets ----------------
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")

preset_code = '''// ---------------------------------------------------------------- portrait presets
const PORTRAIT_PRESETS = {
  real: {
    model: "mossimagine",
    lora: "ultra_real_moss_v2.safetensors",
    strength: 0.7,
    lora2: "",
    lora2Strength: 0.8,
    steps: 50,
    cfg: 3,
    slg: true,
  },
  natural: {
    model: "moss",
    lora: "ultra_real_moss_v2.safetensors",
    strength: 0.7,
    lora2: "MOSS-Image-Beauty-LoRA.safetensors",
    lora2Strength: 0.5,
    steps: 50,
    cfg: 3,
    slg: true,
  },
  polished: {
    model: "moss",
    lora: "MOSS-Image-Beauty-LoRA.safetensors",
    strength: 0.7,
    lora2: "",
    lora2Strength: 0.8,
    steps: 50,
    cfg: 3,
    slg: true,
  },
};

$("presetSel").onchange = () => {
  const key = $("presetSel").value;
  const preset = PORTRAIT_PRESETS[key];
  if (!preset) return;
  $("model").value = preset.model;
  $("model").dispatchEvent(new Event("change"));
  $("lora").value = preset.lora;
  $("lora").dispatchEvent(new Event("change"));
  $("loraStrength").value = String(preset.strength);
  $("loraStrengthText").textContent = Number(preset.strength).toFixed(2);
  $("lora2").value = preset.lora2 || "";
  $("lora2").dispatchEvent(new Event("change"));
  $("lora2Strength").value = String(preset.lora2Strength);
  $("lora2StrengthText").textContent = Number(preset.lora2Strength).toFixed(2);
  $("steps").value = String(preset.steps);
  $("cfg").value = String(preset.cfg);
  $("mossSlg").checked = !!preset.slg;
  state.slgTouched = true;
  toast(`已套用预设：${$("presetSel").selectedOptions[0].textContent}`);
};

'''
j = j.replace("// ---------------------------------------------------------------- crop", preset_code + "// ---------------------------------------------------------------- crop", 1)
if "PORTRAIT_PRESETS" not in j:
    # fallback: insert before region marker
    j = j.replace("// ---------------------------------------------------------------- region marker", preset_code + "// ---------------------------------------------------------------- region marker", 1)

open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok", "PORTRAIT_PRESETS" in j)
