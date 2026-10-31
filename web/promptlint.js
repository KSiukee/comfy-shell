// 提示词体检：生成前扫一遍常见毛病，只提示、不拦。
// 规则来自 MOSS 官方 skill 的写法（质量词无益、可见文字要原样、4K token 上限）。

function estimatePromptTokens(text) {
  let cjk = 0;
  let latin = 0;
  for (const ch of text) {
    if (/[\u3400-\u9fff\uf900-\ufaff]/.test(ch)) cjk += 1;
    else if (/[A-Za-z0-9]/.test(ch)) latin += 1;
  }
  // 中文字 ≈ 1 token/字；英文 ≈ 1 token/4 字符（按词边界略放宽）
  return Math.round(cjk + latin / 4);
}

function lintPrompt() {
  const prompt = ($("prompt").value || "").trim();
  const issues = [];
  if (!prompt) return issues;

  const arrays = prompt.match(/\[\[[^\[\]]+\]\]/g) || [];
  if (arrays.length) {
    issues.push({ level: "info", text: `还有 ${arrays.length} 处 [[…]]：生成时会按候选各出一张（不是错，只是提醒）` });
  }

  const unknown = [];
  for (const m of prompt.matchAll(/__([A-Za-z0-9_\u4e00-\u9fa5-]+)__/g)) {
    if (!wildcardMap[m[1]]) unknown.push(m[1]);
  }
  if (unknown.length) {
    issues.push({ level: "warn", text: `通配符没有对应的 txt：${[...new Set(unknown)].join(", ")}（会原样发给模型）` });
  }

  const loraNames = new Set([...($("lora").options || [])].map((o) => o.value));
  const missingLora = [];
  for (const m of prompt.matchAll(/<lora:([^:<>]+):([0-9]*\.?[0-9]+)>/gi)) {
    if (!loraNames.has(m[1].trim())) missingLora.push(m[1].trim());
  }
  if (missingLora.length) {
    issues.push({ level: "warn", text: `内联 LoRA 不在模型列表里：${[...new Set(missingLora)].join(", ")}（会被跳过）` });
  }

  const spam = prompt.match(/\b(masterpiece|best quality|ultra detailed|highly detailed|8k|4k|high resolution)\b/gi) || [];
  if (spam.length >= 2) {
    const kinds = [...new Set(spam.map((s) => s.toLowerCase()))];
    issues.push({ level: "info", text: `有 ${spam.length} 个质量词（${kinds.join(", ")}）：MOSS 不靠这些，删掉更听话` });
  }

  const tokens = estimatePromptTokens(prompt);
  if (tokens > 4000) {
    issues.push({ level: "warn", text: `提示词约 ${tokens} tokens，超过 MOSS 的 4K 上限，会被截断` });
  } else if (tokens > 3000) {
    issues.push({ level: "info", text: `提示词约 ${tokens} tokens，接近 4K 上限` });
  }

  const neg = ($("negative").value || "").trim();
  if (!neg && prompt.length > 0) {
    issues.push({ level: "info", text: "反向提示词是空的：要压噪点/文字/水印的话可以补一句（或用「去AI感」）" });
  }
  return issues;
}

function renderLint() {
  const list = $("lintList");
  list.textContent = "";
  const issues = lintPrompt();
  if (!issues.length) {
    const p = document.createElement("div");
    p.className = "lint-ok";
    p.textContent = "没发现问题，直接生成吧。";
    list.appendChild(p);
    return issues;
  }
  for (const it of issues) {
    const row = document.createElement("div");
    row.className = `lint-row ${it.level === "warn" ? "warn" : ""}`;
    const tag = document.createElement("b");
    tag.textContent = it.level === "warn" ? "注意" : "提示";
    const txt = document.createElement("span");
    txt.textContent = it.text;
    row.append(tag, txt);
    list.appendChild(row);
  }
  return issues;
}

function openLint() {
  renderLint();
  $("lintModal").hidden = false;
}

$("lintClose").onclick = () => ($("lintModal").hidden = true);
$("lintModal").addEventListener("click", (e) => {
  if (e.target === $("lintModal")) $("lintModal").hidden = true;
});
$("lintRefresh").onclick = () => {
  const issues = renderLint();
  toast(issues.length ? `体检：${issues.length} 条` : "体检通过");
};
["prompt", "negative"].forEach((id) => {
  const el = $(id);
  if (el) el.addEventListener("input", () => { if (!$("lintModal").hidden) renderLint(); });
});
