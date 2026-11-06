// 人像预设（MOSS/Qwen 的几张常用配比）+ 去 AI 感后缀/负面词。
// 从 app.js 拆出来；行为不变。

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
  raw: {
    model: "mossbase",
    lora: "ultra_real_moss_v2.safetensors",
    strength: 0.7,
    lora2: "",
    lora2Strength: 0.8,
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

function applyPortraitPreset(key, silent) {
  const preset = PORTRAIT_PRESETS[key];
  if (!preset) return false;
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
  $("presetSel").value = key;
  if (!silent) toast(`已套用预设：${$("presetSel").selectedOptions[0].textContent}`);
  return true;
}

$("presetSel").onchange = () => applyPortraitPreset($("presetSel").value, false);


// ---------------------------------------------------------------- de-AI preset
const DEAI_PROMPT =
  "neutral white balance, no text or logos, candid expression not looking at camera, single window light with soft shadows, f1.8 shallow depth of field, subtle film grain, slight lens distortion";
const DEAI_NEG =
  "yellow cast, oversaturated, glow, bloom, text, watermark, plastic skin, over-smoothed";
// 按逗号片段去重后追加：用户词里已有的（如 watermark）不再重复，点两次等于一次。
function appendTokens(value, extra) {
  const seen = new Set(
    value.toLowerCase().split(",").map((s) => s.trim()).filter(Boolean)
  );
  const add = [];
  for (const raw of extra.split(",")) {
    const token = raw.trim();
    if (token && !seen.has(token.toLowerCase())) {
      seen.add(token.toLowerCase());
      add.push(token);
    }
  }
  if (!add.length) return value;
  const base = value.trim().replace(/,\s*$/, "");
  return base ? `${base}, ${add.join(", ")}` : add.join(", ");
}
$("btnDeAi").onclick = () => {
  const box = $("prompt");
  const neg = $("negative");
  const before = `${box.value}\n${neg.value}`;
  box.value = appendTokens(box.value, DEAI_PROMPT);
  neg.value = appendTokens(neg.value, DEAI_NEG);
  const changed = `${box.value}\n${neg.value}` !== before;
  toast(
    changed
      ? "已加：去 AI 感后缀 + 负面词（噪点/轻微畸变；已有的不重复）"
      : "去 AI 感后缀和负面词都已经在了"
  );
};
