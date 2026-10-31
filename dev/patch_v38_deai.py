p = r"D:\deepseek-files\ai-tools\comfy-shell\web\app.js"
lines = open(p, encoding="utf-8").read().split("\n")

start = next(i for i, l in enumerate(lines) if "de-AI preset" in l)
end = next(i for i, l in enumerate(lines) if "// ---------------------------------------------------------------- local assistant" in l)

block = '''// ---------------------------------------------------------------- de-AI preset
const DEAI_PROMPT =
  "neutral white balance, no text or logos, candid expression not looking at camera, single window light with soft shadows, f1.8 shallow depth of field, subtle film grain, slight lens distortion";
const DEAI_NEG =
  "yellow cast, oversaturated, glow, bloom, text, watermark, plastic skin, over-smoothed";
$("btnDeAi").onclick = () => {
  const box = $("prompt");
  if (!box.value.includes("neutral white balance")) {
    box.value = box.value.trim() ? `${box.value.trim()}, ${DEAI_PROMPT}` : DEAI_PROMPT;
  }
  const neg = $("negative");
  if (!neg.value.includes("yellow cast")) {
    neg.value = neg.value.trim() ? `${neg.value.trim()}, ${DEAI_NEG}` : DEAI_NEG;
  }
  toast("已加：去 AI 感后缀 + 负面词（含噪点/轻微畸变）");
};

'''.split("\n")

lines = lines[:start] + block + lines[end:]
open(p, "w", encoding="utf-8", newline="\n").write("\n".join(lines))
print("fixed")
