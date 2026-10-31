// 左栏 / 结果区之间的分隔条：拖动改左栏宽度，双击复位；宽度记在 localStorage。
// 不动 app.js（棘轮），也不新增栏位——分隔条就在两栏中间。
(() => {
  const KEY = "cs.panelWidth";
  const MIN = 300;
  const MAX = 640;
  const DEF = 372;
  const layout = document.querySelector(".layout");
  const panel = document.querySelector(".panel");
  const splitter = document.getElementById("panelSplitter");
  if (!layout || !panel || !splitter) return;

  const clamp = (w) => Math.max(MIN, Math.min(MAX, Math.round(w)));
  const apply = (w, save) => {
    const v = clamp(w);
    layout.style.setProperty("--panel-w", v + "px");
    if (save) localStorage.setItem(KEY, String(v));
    return v;
  };

  const saved = Number(localStorage.getItem(KEY) || 0);
  if (saved >= MIN) apply(saved, false);

  let drag = null;
  const onMove = (e) => {
    if (!drag) return;
    apply(drag.startW + (e.clientX - drag.startX), false);
  };
  const onUp = () => {
    if (!drag) return;
    drag = null;
    document.body.classList.remove("col-resizing");
    document.removeEventListener("mousemove", onMove);
    splitter.classList.remove("dragging");
    apply(panel.getBoundingClientRect().width, true);
  };

  splitter.addEventListener("mousedown", (e) => {
    e.preventDefault();
    drag = { startX: e.clientX, startW: panel.getBoundingClientRect().width };
    splitter.classList.add("dragging");
    document.body.classList.add("col-resizing");
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp, { once: true });
  });
  splitter.addEventListener("dblclick", () => apply(DEF, true));

  // 窗口变窄时，别让左栏把结果区挤没
  window.addEventListener("resize", () => {
    const max = Math.min(MAX, Math.max(MIN, window.innerWidth - 420));
    if (panel.getBoundingClientRect().width > max) apply(max, true);
  });
})();
