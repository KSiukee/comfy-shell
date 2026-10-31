// 队列 / 批量改图 / 扫模型 / 批量超分（从 app.js 拆出；依赖 app.js 的全局）

// ---------------------------------------------------------------- batch queue

const queue = { items: [], running: false, stop: false, done: 0 };

function renderQueue() {
  $("queueCount").textContent = queue.items.length ? `${queue.done}/${queue.items.length}` : "0";
  $("btnQueueRun").hidden = queue.running;
  $("btnQueueStop").hidden = !queue.running;
  $("btnQueueRun").disabled = queue.items.length === 0;
  $("queueState").textContent = queue.running ? "运行中…" : "";
}

function queueAdd() {
  const lines = $("queueText")
    .value.split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  if (!lines.length) {
    toast("先写几行提示词", true);
    return;
  }
  queue.items.push(...lines);
  $("queueText").value = "";
  renderQueue();
  toast(`已加入 ${lines.length} 条`);
}

async function queueRun() {
  if (!(await ensureReady("开始队列"))) return;
  if (queue.running || !queue.items.length) return;
  if (!multiStart()) return;
  queue.running = true;
  queue.stop = false;
  queue.done = 0;
  renderQueue();
  const total = queue.items.length;
  while (queue.items.length && !queue.stop) {
    $("prompt").value = queue.items[0];
    const ok = await generate();
    queue.items.shift();
    if (ok) queue.done++;
    renderQueue();
    if (!ok && queue.stop) break;
  }
  queue.running = false;
  queue.stop = false;
  multiEnd();
  renderQueue();
  toast(`队列结束（完成 ${queue.done}/${total}）`);
}


// ---------------------------------------------------------------- batch edit from library

const batchEdit = { running: false, stop: false, done: 0, total: 0, items: [] };

function openBatchEdit() {
  const picked = libItems.filter((i) => libSel.has(i.path));
  if (!picked.length) return;
  const p = profile();
  const target = p && p.supportsEdit ? p : state.profiles.find((x) => x.id === "qwen21");
  if (!target) {
    toast("没有支持改图的模型", true);
    return;
  }
  batchEdit.items = picked;
  $("batchEditCount").textContent = `${picked.length} 张`;
  $("batchEditModel").textContent = `模型：${target.label}（不支持改图的模型会自动用 Qwen-Image 2.1）`;
  $("batchEditPrompt").value = $("prompt").value.trim();
  $("batchEditModal").hidden = false;
  $("batchEditPrompt").focus();
}

async function runBatchEdit() {
  const instruction = $("batchEditPrompt").value.trim();
  if (!instruction) {
    toast("写下要改什么", true);
    return;
  }
  if (!multiStart()) return;
  const items = batchEdit.items.slice();
  batchEdit.running = true;
  batchEdit.stop = false;
  batchEdit.done = 0;
  batchEdit.total = items.length;
  $("batchEditStart").disabled = true;
  $("batchEditCancel").textContent = "停止";

  // pick a model that can do edits, keep the current size/seed
  const p = profile();
  const target = p && p.supportsEdit ? p.id : "qwen21";
  const w = Number($("width").value);
  const h = Number($("height").value);
  const seed = Number($("seed").value);
  $("model").value = target;
  $("model").dispatchEvent(new Event("change", { bubbles: true }));
  $("width").value = w;
  $("height").value = h;
  $("seed").value = seed;
  setMode("edit");
  $("libClose").click();

  for (const it of items) {
    if (batchEdit.stop) break;
    try {
      const dataUrl = await libUrl(it.path);
      const name = await api.call("save_ref_image", { name: it.filename, dataUrl });
      state.refs = [name];
      thumbCache.set(name, dataUrl);
      renderRefs();
      $("prompt").value = instruction;
      const ok = await generate();
      if (ok) batchEdit.done++;
    } catch (e) {
      toast(String(e), true);
    }
    $("batchEditCount").textContent = `${batchEdit.done}/${batchEdit.total}`;
  }

  batchEdit.running = false;
  multiEnd();
  $("batchEditStart").disabled = false;
  $("batchEditCancel").textContent = "取消";
  $("batchEditModal").hidden = true;
  toast(`批量改图结束（完成 ${batchEdit.done}/${batchEdit.total}）`);
}

// ---------------------------------------------------------------- model sweep

function buildSweepModels() {
  const box = $("sweepModels");
  box.innerHTML = state.profiles
    .map((p) => `<label><input type="checkbox" value="${p.id}" checked> ${p.label}</label>`)
    .join("");
}

function sweepSelection() {
  return [...$("sweepModels").querySelectorAll("input:checked")].map((x) => x.value);
}

const SWEEP_SIZES = [512, 640, 768, 896, 1024];

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


// ---------------------------------------------------------------- batch upscale / external folder / training

async function loadLibUpscaleModels() {
  try {
    const list = await api.call("upscale_models_list");
    const sel = $("libUpModel");
    sel.innerHTML =
      `<option value="__seedvr2__">SeedVR2 3B · 4×</option>` +
      list.map((m) => `<option value="${m}">${m.replace(/\.(pth|safetensors|pt)$/i, "")}</option>`).join("");
  } catch {}
}

async function batchUpscale() {
  const picked = libItems.filter((i) => libSel.has(i.path));
  if (!picked.length) {
    toast("先勾选要超分的图", true);
    return;
  }
  const model = $("libUpModel").value;
  let done = 0;
  for (const it of picked) {
    try {
      toast(`批量超分 ${done + 1}/${picked.length}…`);
      await api.call("upscale_image", { path: it.path, model });
      done++;
    } catch (e) {
      toast(String(e), true);
    }
  }
  await loadLibrary();
  toast(`批量超分完成 ${done}/${picked.length}`);
}

async function externalUpscale(files) {
  const imgs = [...files].filter((f) => /\.(png|jpe?g|webp)$/i.test(f.name));
  if (!imgs.length) {
    toast("这个文件夹里没有图片", true);
    return;
  }
  const model = $("libUpModel").value;
  let done = 0;
  for (const f of imgs) {
    try {
      toast(`外部超分 ${done + 1}/${imgs.length}：${f.name}`);
      const dataUrl = await new Promise((res) => {
        const r = new FileReader();
        r.onload = () => res(r.result);
        r.readAsDataURL(f);
      });
      const name = await api.call("save_ref_image", { name: f.name, dataUrl });
      await api.call("upscale_input", { name, model });
      done++;
    } catch (e) {
      toast(`${f.name}: ${e}`, true);
    }
  }
  await loadLibrary();
  toast(`外部文件夹超分完成 ${done}/${imgs.length}`);
}
