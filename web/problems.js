// 问题面板：收集失败/错误，避免只闪一下就消失。
const problems = [];
let problemsSeq = 0;

function problemsAdd(source, message, detail = "") {
  const msg = String(message || "").trim();
  if (!msg) return;
  const last = problems[problems.length - 1];
  if (last && last.message === msg && Date.now() - last.at < 1500) return;
  problems.push({ id: ++problemsSeq, source, message: msg, detail, at: Date.now() });
  if (problems.length > 100) problems.shift();
  renderProblems();
}

function renderProblems() {
  const btn = $("problemsBtn");
  const list = $("problemsList");
  const cnt = $("problemsCount");
  if (!btn || !cnt) return;
  btn.hidden = problems.length === 0;
  cnt.textContent = String(problems.length);
  if (!list) return;
  list.innerHTML = "";
  for (const p of problems.slice().reverse()) {
    const row = document.createElement("div");
    row.className = "problem-row";
    const head = document.createElement("div");
    head.className = "muted mono";
    head.style.fontSize = "11px";
    head.textContent = `${p.source} · ${new Date(p.at).toLocaleTimeString()}`;
    const msg = document.createElement("div");
    msg.textContent = p.message;
    row.append(head, msg);
    if (p.detail) {
      const d = document.createElement("div");
      d.className = "muted";
      d.style.cssText = "font-size:11px;white-space:pre-wrap";
      d.textContent = p.detail;
      row.append(d);
    }
    list.appendChild(row);
  }
}

$("problemsBtn").onclick = () => {
  $("problemsModal").hidden = false;
  renderProblems();
};
$("problemsClose").onclick = () => ($("problemsModal").hidden = true);
$("problemsClear").onclick = () => {
  problems.length = 0;
  renderProblems();
};

// 所有 toast(..., true) 都进问题面板；不改变原提示。
const _problemsToast = window.toast;
window.toast = function (msg, isErr) {
  if (isErr) problemsAdd("提示", String(msg));
  return _problemsToast.apply(this, arguments);
};
window.addEventListener("error", (e) => problemsAdd("脚本", e.message));
window.addEventListener("unhandledrejection", (e) => problemsAdd("异步", String(e.reason)));
