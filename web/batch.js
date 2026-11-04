// 队列 / 批量改图 / 扫模型 / 批量超分（从 app.js 拆出；依赖 app.js 的全局）

// ---------------------------------------------------------------- batch queue

const queue = {
  items: [],
  running: false,
  paused: false,
  stop: false,
  done: 0,
  cancelCurrent: false,
  seq: 0,
};

const qSleep = (ms) => new Promise((r) => setTimeout(r, ms));

function queueEta() {
  const secs = (state.results || [])
    .slice(0, 20)
    .map((r) => Number(r.seconds))
    .filter((n) => Number.isFinite(n) && n > 0);
  const avg = secs.length ? secs.reduce((a, b) => a + b, 0) / secs.length : 15;
  const waiting = queue.items.filter((i) => i.status === "waiting").length;
  return Math.max(1, Math.round(waiting * avg));
}

function queueMove(item, dir) {
  if (item.status !== "waiting") return;
  const idx = queue.items.indexOf(item);
  if (idx < 0) return;
  for (let j = idx + dir; j >= 0 && j < queue.items.length; j += dir) {
    if (queue.items[j].status === "waiting") {
      [queue.items[idx], queue.items[j]] = [queue.items[j], queue.items[idx]];
      renderQueue();
      return;
    }
  }
}

function renderQueue() {
  const box = $("queueList");
  if (box) {
    box.innerHTML = "";
    queue.items.forEach((it, i) => {
      const row = document.createElement("div");
      row.className = `queue-item ${it.status}`;
      const st = document.createElement("span");
      st.className = "qs";
      st.textContent =
        it.status === "running" ? "▶" : it.status === "done" ? "✓" : it.status === "error" ? "!" : "·";
      const txt = document.createElement("span");
      txt.className = "qt";
      txt.textContent = it.text;
      txt.title = it.text;
      const time = document.createElement("span");
      time.className = "muted mono";
      time.style.fontSize = "10px";
      time.textContent = it.seconds ? `${Math.round(it.seconds)}s` : "";
      row.append(st, txt, time);
      if (it.status === "waiting") {
        const up = document.createElement("button");
        up.className = "btn ghost small";
        up.textContent = "↑";
        up.disabled = i === 0;
        up.onclick = () => queueMove(it, -1);
        const down = document.createElement("button");
        down.className = "btn ghost small";
        down.textContent = "↓";
        down.onclick = () => queueMove(it, 1);
        const del = document.createElement("button");
        del.className = "btn ghost small";
        del.textContent = "×";
        del.title = "从队列移除";
        del.onclick = () => {
          queue.items = queue.items.filter((x) => x !== it);
          renderQueue();
        };
        row.append(up, down, del);
      } else if (it.status === "running") {
        const cancel = document.createElement("button");
        cancel.className = "btn ghost small";
        cancel.textContent = "取消";
        cancel.onclick = () => {
          queue.cancelCurrent = true;
          api.call("job_cancel");
        };
        row.append(cancel);
      }
      box.appendChild(row);
    });
  }
  const waiting = queue.items.filter((i) => i.status === "waiting").length;
  $("queueCount").textContent = queue.items.length
    ? `${queue.done}/${queue.items.length}`
    : "0";
  $("btnQueueRun").hidden = queue.running;
  $("btnQueueStop").hidden = !queue.running;
  const pause = $("btnQueuePause");
  if (pause) {
    pause.hidden = !queue.running;
    pause.textContent = queue.paused ? "继续" : "暂停";
  }
  $("btnQueueRun").disabled = queue.items.length === 0 || !waiting;
  $("queueState").textContent = queue.running
    ? queue.paused
      ? `已暂停 · 剩 ${waiting} 条 · ~${queueEta()}s`
      : `运行中… 剩 ${waiting} 条 · ~${queueEta()}s`
    : waiting
      ? `待跑 ${waiting} 条`
      : "";
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
  for (const text of lines) {
    queue.items.push({ id: ++queue.seq, text, status: "waiting", seconds: 0 });
  }
  $("queueText").value = "";
  renderQueue();
  toast(`已加入 ${lines.length} 条`);
}

async function queueRun() {
  if (!(await ensureReady("开始队列"))) return;
  if (queue.running || !queue.items.length) return;
  const waiting = queue.items.filter((i) => i.status === "waiting");
  if (!waiting.length) {
    toast("队列里没有待跑的条目", true);
    return;
  }
  if (!multiStart()) return;
  queue.running = true;
  queue.paused = false;
  queue.stop = false;
  queue.cancelCurrent = false;
  queue.done = 0;
  for (const it of queue.items) {
    if (it.status === "error") it.status = "waiting";
    it.seconds = 0;
  }
  renderQueue();
  const total = queue.items.filter((i) => i.status !== "done").length;

  while (!queue.stop) {
    if (queue.paused) {
      await qSleep(250);
      continue;
    }
    const item = queue.items.find((i) => i.status === "waiting");
    if (!item) break;
    item.status = "running";
    item.startedAt = Date.now();
    queue.cancelCurrent = false;
    renderQueue();
    $("prompt").value = item.text;
    let ok = false;
    try {
      ok = await generate();
    } catch (e) {
      problemsAdd("queue", String(e), item.text);
    }
    item.seconds = (Date.now() - (item.startedAt || Date.now())) / 1000;
    item.status = ok ? "done" : "error";
    if (ok) queue.done += 1;
    else problemsAdd("queue", `队列第 ${queue.done + 1} 条失败`, item.text);
    renderQueue();
  }
  queue.running = false;
  queue.paused = false;
  queue.stop = false;
  queue.cancelCurrent = false;
  multiEnd();
  renderQueue();
  toast(`队列结束（完成 ${queue.done}/${total}）`);
}

$("btnQueuePause").onclick = () => {
  if (!queue.running) return;
  queue.paused = !queue.paused;
  renderQueue();
};

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
  const seed = $("seed").value; // #88 原样保留（可能是超过 2^53 的老种子）
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
  if (typeof refreshLibExtra === "function") await refreshLibExtra();
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
