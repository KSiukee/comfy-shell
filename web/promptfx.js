// 提示词效果层（#26）：风格包 / 通配符 / [[数组]] / 内联 LoRA。
// 不改 app.js：包一层 generate()，在真正提交前把提示词展开成一条或多条任务。

const STYLE_KEY = "cs.styleName";
const MAX_ARRAY_JOBS = 8;
let stylePacks = [];
let wildcardMap = {};
let styleName = localStorage.getItem(STYLE_KEY) || "";

// ---------------------------------------------------------------- 读取

async function loadStylePacks() {
  try {
    stylePacks = (await api.call("style_list")) || [];
  } catch {
    stylePacks = [];
  }
  if (styleName && !stylePacks.some((s) => s.name === styleName)) styleName = "";
  if (typeof stylePickRefresh === "function") stylePickRefresh();
  updateFxHint();
}

/** 当前选中的风格（没有就 null）；选择器在 stylepick.js 里画。 */
function currentStyle() {
  if (!styleName) return null;
  return stylePacks.find((s) => s.name === styleName) || null;
}

function setStyleName(name) {
  styleName = name || "";
  localStorage.setItem(STYLE_KEY, styleName);
  if (typeof stylePickRefresh === "function") stylePickRefresh();
  updateFxHint();
}

async function loadWildcards() {
  try {
    wildcardMap = (await api.call("wildcard_map")) || {};
  } catch {
    wildcardMap = {};
  }
}

// ---------------------------------------------------------------- 纯逻辑

/** `[[a,b,c]]` → 多份提示词（笛卡尔积，最多 cap 份）。 */
function expandArrays(text, cap = MAX_ARRAY_JOBS) {
  let out = [text];
  for (let guard = 0; guard < 6; guard++) {
    const m = /\[\[([^\[\]]+)\]\]/.exec(out[0] || "");
    if (!m) break;
    const opts = m[1]
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    if (opts.length < 2) {
      out = out.map((t) => t.replace(m[0], opts[0] || ""));
      continue;
    }
    const next = [];
    for (const t of out) {
      for (const o of opts) {
        if (next.length >= cap) break;
        next.push(t.replace(m[0], o));
      }
      if (next.length >= cap) break;
    }
    out = next;
  }
  return out;
}

const LORA_RE = /<lora:([^:<>]+):([0-9]*\.?[0-9]+)>/gi;

/** 抠出内联 LoRA，并从提示词里删掉。 */
function takeLoras(text) {
  const loras = [];
  const cleaned = text
    .replace(LORA_RE, (_m, name, w) => {
      loras.push({ name: String(name).trim(), weight: Number(w) });
      return " ";
    })
    .replace(/\s{2,}/g, " ")
    .replace(/\s+,/g, ",")
    .trim();
  return { cleaned, loras };
}

function hash32(str) {
  let h = 2166136261 >>> 0;
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h >>> 0;
}

/** `__名字__` → 该通配符文件里的一条；同一种子同一处永远选同一条。 */
function expandWildcards(text, seed) {
  let used = 0;
  const out = text.replace(/__([A-Za-z0-9_\u4e00-\u9fa5-]+)__/g, (m, name) => {
    const list = wildcardMap[name];
    if (!list || !list.length) return m;
    used += 1;
    const idx = list.length === 1 ? 0 : hash32(`${seed}|${name}|${used}`) % list.length;
    return list[idx];
  });
  return { text: out, used };
}

function applyStyle(prompt, negative) {
  const pack = currentStyle();
  if (!pack) return { prompt, negative };
  const merged = pack.positive.includes("{prompt}")
    ? pack.positive.replace(/\{prompt\}/g, prompt)
    : `${pack.positive}, ${prompt}`;
  const neg = [negative.trim(), (pack.negative || "").trim()].filter(Boolean).join(", ");
  return { prompt: merged.trim(), negative: neg };
}

/** 把当前表单展开成任务列表（不含 IO，hint 也用它）。 */
function planJobs() {
  const rawPrompt = $("prompt").value.trim();
  if (!rawPrompt) return [];
  const seedField = Number($("seed").value);
  const baseSeed = seedField >= 0 ? seedField : Math.floor(Math.random() * 2147483647);
  const variants = expandArrays(rawPrompt);
  const negVariants = expandArrays($("negative").value.trim());
  return variants.map((text, i) => {
    const negRaw = negVariants[Math.min(i, negVariants.length - 1)] || "";
    const styled = applyStyle(text, negRaw);
    const lora = takeLoras(styled.prompt);
    const wSeed = baseSeed + i * 7919;
    const p = expandWildcards(lora.cleaned, wSeed);
    const n = expandWildcards(styled.negative, wSeed + 1);
    return {
      prompt: p.text,
      negative: n.text,
      loras: lora.loras,
      wildcards: p.used + n.used,
      seedUsed: wSeed,
    };
  });
}

// ---------------------------------------------------------------- 提交

function pickLora(job) {
  const out = {};
  const loras = job.loras || [];
  if (!loras.length) return out;
  const names = new Set([...($("lora").options || [])].map((o) => o.value));
  const ok = loras.filter((l) => names.has(l.name)).slice(0, 2);
  if (ok.length < loras.length) {
    toast(`有内联 LoRA 不在模型列表里，已跳过：${loras.filter((l) => !names.has(l.name)).map((l) => l.name).join(", ")}`, true);
  }
  if (ok[0]) {
    out.lora = ok[0].name;
    out.loraStrength = ok[0].weight;
  }
  if (ok[1]) {
    out.lora2 = ok[1].name;
    out.lora2Strength = ok[1].weight;
  }
  return out;
}

async function runFxJob(job) {
  const f = {
    prompt: $("prompt"),
    negative: $("negative"),
    lora: $("lora"),
    loraStrength: $("loraStrength"),
    lora2: $("lora2"),
    lora2Strength: $("lora2Strength"),
  };
  const saved = {
    prompt: f.prompt.value,
    negative: f.negative.value,
    lora: f.lora.value,
    loraStrength: f.loraStrength.value,
    lora2: f.lora2.value,
    lora2Strength: f.lora2Strength.value,
    loraText: $("loraStrengthText") ? $("loraStrengthText").textContent : "",
    lora2Text: $("lora2StrengthText") ? $("lora2StrengthText").textContent : "",
  };
  try {
    f.prompt.value = job.prompt;
    f.negative.value = job.negative;
    const pick = pickLora(job);
    if (pick.lora) {
      f.lora.value = pick.lora;
      f.loraStrength.value = String(pick.loraStrength);
      if ($("loraStrengthText")) $("loraStrengthText").textContent = pick.loraStrength.toFixed(2);
    }
    if (pick.lora2) {
      f.lora2.value = pick.lora2;
      f.lora2Strength.value = String(pick.lora2Strength);
      if ($("lora2StrengthText")) $("lora2StrengthText").textContent = pick.lora2Strength.toFixed(2);
    }
    return await baseGenerate();
  } finally {
    f.prompt.value = saved.prompt;
    f.negative.value = saved.negative;
    f.lora.value = saved.lora;
    f.loraStrength.value = saved.loraStrength;
    f.lora2.value = saved.lora2;
    f.lora2Strength.value = saved.lora2Strength;
    if ($("loraStrengthText")) $("loraStrengthText").textContent = saved.loraText;
    if ($("lora2StrengthText")) $("lora2StrengthText").textContent = saved.lora2Text;
    // baseGenerate 会把展开后的提示词写进「上次请求」；这里换回模板，重启后还能改
    try {
      const last = JSON.parse(localStorage.getItem("cs.lastReq") || "null");
      if (last) {
        last.prompt = saved.prompt;
        last.negative = saved.negative;
        localStorage.setItem("cs.lastReq", JSON.stringify(last));
      }
    } catch {}
  }
}

const baseGenerate = generate;
generate = async function fxGenerate() {
  let jobs = [];
  try {
    jobs = planJobs();
  } catch {
    jobs = [];
  }
  const style = currentStyle();
  const fxOn = jobs.some((j) => (j.loras && j.loras.length) || j.wildcards > 0) || !!style;
  if (!jobs.length || (jobs.length === 1 && !fxOn)) return baseGenerate();
  if (typeof lintPrompt === "function") {
    const warn = lintPrompt().filter((i) => i.level === "warn");
    if (warn.length) toast(`体检发现 ${warn.length} 条要注意的（🧰 → 提示词体检）`, true);
  }
  if (jobs.length > 1) toast(`数组展开：${jobs.length} 组，依次生成`);
  let ok = 0;
  for (const job of jobs) {
    if (await runFxJob(job)) ok += 1;
    else break;
  }
  return ok > 0;
};

// ---------------------------------------------------------------- 界面

function updateFxHint() {
  const el = $("fxHint");
  if (!el) return;
  let jobs = [];
  try {
    jobs = planJobs();
  } catch {}
  const style = currentStyle();
  const loras = jobs.reduce((n, j) => n + (j.loras ? j.loras.length : 0), 0);
  const wc = jobs.reduce((n, j) => n + j.wildcards, 0);
  const bits = [];
  if (jobs.length > 1) bits.push(`${jobs.length} 组`);
  if (style) bits.push(`风格 · ${style.zh || style.name}`);
  if (loras) bits.push(`LoRA ${loras}`);
  if (wc) bits.push(`通配符 ${wc} 处`);
  el.textContent = bits.length
    ? bits.join(" · ")
    : "支持 [[甲,乙]] 多组 · __通配符__ · <lora:名字:0.8>";
  el.title = bits.length ? jobs.map((j, i) => `#${i + 1} ${j.prompt}`).join("\n\n") : "";
}

$("styleDirBtn").addEventListener("click", async () => {
  try {
    const dir = await api.call("style_pack_open");
    await api.call("open_folder", { path: dir });
    await loadStylePacks();
    await loadWildcards();
    toast("样式目录已打开：往里放 *.json 与 wildcards/*.txt");
  } catch (e) {
    toast(String(e), true);
  }
});
$("styleImportBtn").addEventListener("click", () => $("styleFile").click());
$("styleFile").addEventListener("change", async () => {
  const f = $("styleFile").files && $("styleFile").files[0];
  if (!f) return;
  try {
    const text = await f.text();
    const n = await api.call("style_import", {
      name: f.name.replace(/\.json$/i, ""),
      jsonText: text,
    });
    await loadStylePacks();
    toast(`已导入 ${n} 个风格，点「风格」挑一个`);
  } catch (e) {
    toast(String(e), true);
  } finally {
    $("styleFile").value = "";
  }
});
["prompt", "negative", "seed"].forEach((id) => {
  const el = $(id);
  if (el) el.addEventListener("input", updateFxHint);
});

loadStylePacks();
loadWildcards();
updateFxHint();
