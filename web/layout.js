// 左栏分隔条 + 结果图大小滑条。
// 都记忆在本机；大小变化即时重排（不加过渡），缩到 260 以下自动密排。
(() => {
  // ---------- 左栏分隔条 ----------
  const PKEY = "cs.panelWidth";
  const PMIN = 300;
  const PMAX = 640;
  const PDEF = 372;
  const layout = document.querySelector(".layout");
  const panel = document.querySelector(".panel");
  const splitter = document.getElementById("panelSplitter");

  if (layout && panel && splitter) {
    const pclamp = (w) => Math.max(PMIN, Math.min(PMAX, Math.round(w)));
    const papply = (w, save) => {
      const v = pclamp(w);
      layout.style.setProperty("--panel-w", v + "px");
      if (save) localStorage.setItem(PKEY, String(v));
    };
    const psaved = Number(localStorage.getItem(PKEY) || 0);
    if (psaved >= PMIN) papply(psaved, false);

    let drag = null;
    const onMove = (e) => {
      if (!drag) return;
      papply(drag.startW + (e.clientX - drag.startX), false);
    };
    const onUp = () => {
      if (!drag) return;
      drag = null;
      document.body.classList.remove("col-resizing");
      document.removeEventListener("mousemove", onMove);
      splitter.classList.remove("dragging");
      papply(panel.getBoundingClientRect().width, true);
    };
    splitter.addEventListener("mousedown", (e) => {
      e.preventDefault();
      drag = { startX: e.clientX, startW: panel.getBoundingClientRect().width };
      splitter.classList.add("dragging");
      document.body.classList.add("col-resizing");
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp, { once: true });
    });
    splitter.addEventListener("dblclick", () => papply(PDEF, true));
    window.addEventListener("resize", () => {
      const max = Math.min(PMAX, Math.max(PMIN, window.innerWidth - 420));
      if (panel.getBoundingClientRect().width > max) papply(max, true);
    });
  }

  // ---------- 结果图大小 ----------
  const SKEY = "cs.resultSize";
  const SMIN = 100;
  const SMAX = 560;
  const SDEF = 440;
  const DENSE = 260;
  const results = document.getElementById("results");
  const input = document.getElementById("resSize");
  if (!results || !input) return;

  const sapply = (v, save) => {
    const n = Math.max(SMIN, Math.min(SMAX, Math.round(Number(v) / 10) * 10));
    results.style.setProperty("--res-thumb", n + "px");
    results.classList.toggle("dense", n < DENSE);
    input.value = String(n);
    if (save) localStorage.setItem(SKEY, String(n));
  };

  const ssaved = Number(localStorage.getItem(SKEY) || 0);
  sapply(ssaved >= SMIN ? ssaved : SDEF, false);
  input.addEventListener("input", () => sapply(input.value, true));
  input.addEventListener("dblclick", () => sapply(SDEF, true));
})();
