// 提示词编译（#81）：一句话 + 当前模型 → 按该模型方言写成完整提示词。
// 只填提示词，不改参数（步数/CFG/尺寸都不动）；走在线还是本地由后端决定，这里只如实显示。

let compileLast = null;

function compileModel() {
  return typeof profile === "function" ? profile() : null;
}

async function compileDescribe() {
  const box = $("compileFor");
  const p = compileModel();
  if (!p) {
    box.textContent = "先选一个模型";
    return;
  }
  // 先立刻显示模型名（方言要问后端，慢半拍也不能让这行空着）
  box.textContent = `${p.label} · …`;
  try {
    const d = await api.call("prompt_dialect", { profile: p.id });
    box.textContent = `${p.label} · ${d.dialect}（${d.lang === "zh" ? "中文" : "英文"}）`;
  } catch {
    box.textContent = p.label;
  }
}

async function openCompile() {
  const m = $("compileModal");
  if (!m) return;
  const cur = (($("prompt") && $("prompt").value) || "").trim();
  const idea = $("compileIdea");
  if (cur && !idea.value.trim()) idea.value = cur;
  $("compileOutWrap").hidden = true;
  $("compileState").textContent = "";
  compileLast = null;
  m.hidden = false;
  await compileDescribe();
  idea.focus();
  idea.setSelectionRange(idea.value.length, idea.value.length);
}

function renderCompile(r, ms) {
  $("compileOutWrap").hidden = false;
  $("compileOut").value = r.prompt || "";
  const notes = $("compileNotes");
  notes.textContent = "";
  const rows = [r.source === "online" ? "🌐 在线模型" : "💻 本机模型"];
  if (r.private) rows.push("🔒 私密项目：只用本机模型");
  for (const n of r.notes || []) rows.push(n);
  for (const text of rows) {
    const d = document.createElement("div");
    d.className = "compile-note";
    d.textContent = text;
    notes.appendChild(d);
  }
  const chars = [...(r.prompt || "")].length;
  const head = `${r.dialect} · ${chars} 字`;
  $("compileSize").textContent = head;
  $("compileState").textContent = `完成 · ${(ms / 1000).toFixed(1)} 秒`;
  let sent = "这次没有联网：本机模型在跑。";
  if (r.source === "online") {
    sent = r.usedContext ? "这次发出去的是：这句话 + 模型名 + 这部戏的设定（不含图片）。" : "这次发出去的是：这句话 + 模型名（不含图片）。";
  }
  $("compileSend").textContent = sent;
}

async function runCompile() {
  const idea = ($("compileIdea").value || "").trim();
  if (!idea) return toast("先写一句话想画什么", true);
  const p = compileModel();
  if (!p) return toast("先选一个模型", true);
  const btn = $("compileRun");
  btn.disabled = true;
  $("compileState").textContent = "编译中…";
  const t0 = Date.now();
  try {
    const r = await api.call("prompt_compile", {
      idea,
      profile: p.id,
      project: (window.projectUI && projectUI.current && projectUI.current()) || null,
      keepWording: $("compileKeep").checked,
      width: Number($("width").value) || null,
      height: Number($("height").value) || null,
    });
    compileLast = r;
    renderCompile(r, Date.now() - t0);
  } catch (e) {
    $("compileState").textContent = "失败";
    toast(String(e), true);
  } finally {
    btn.disabled = false;
  }
}

function compileResult() {
  return ($("compileOut").value || "").trim();
}

function compileFill() {
  if (!compileLast) return;
  const text = compileResult();
  if (!text) return toast("结果是空的", true);
  $("prompt").value = text;
  $("compileModal").hidden = true;
  $("prompt").dispatchEvent(new Event("input", { bubbles: true }));
  toast("已填入提示词（参数没动）");
}

function compileAppend() {
  if (!compileLast) return;
  const text = compileResult();
  if (!text) return toast("结果是空的", true);
  const cur = ($("prompt").value || "").trim();
  $("prompt").value = cur ? `${cur}\n${text}` : text;
  $("compileModal").hidden = true;
  $("prompt").dispatchEvent(new Event("input", { bubbles: true }));
  toast("已接到提示词末尾");
}

async function compileCopy() {
  const text = compileResult();
  if (!text) return toast("结果是空的", true);
  try {
    await navigator.clipboard.writeText(text);
    toast("已复制");
  } catch {
    $("compileOut").select();
    toast("复制失败，已全选，Ctrl+C 一下", true);
  }
}

const compileModal = $("compileModal");
if (compileModal) {
  $("compileRun").onclick = runCompile;
  $("compileFill").onclick = compileFill;
  $("compileAppend").onclick = compileAppend;
  $("compileCopy").onclick = compileCopy;
  $("compileClose").onclick = () => (compileModal.hidden = true);
  compileModal.addEventListener("click", (e) => {
    if (e.target === compileModal) compileModal.hidden = true;
  });
  $("compileModal").addEventListener("keydown", (e) => {
    if (e.key === "Escape") compileModal.hidden = true;
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) runCompile();
  });
}

window.compileOpen = openCompile;
