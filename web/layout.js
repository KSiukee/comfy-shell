// 结果图大小：工具栏里的「大小」滑条改 --res-thumb；记忆在本机；双击复位。
// 不新增栏位，也不碰 app.js。
(() => {
  const KEY = "cs.resultSize";
  const MIN = 240;
  const MAX = 560;
  const DEF = 440;
  const results = document.getElementById("results");
  const input = document.getElementById("resSize");
  if (!results || !input) return;

  const apply = (v, save) => {
    const n = Math.max(MIN, Math.min(MAX, Math.round(Number(v) / 10) * 10));
    results.style.setProperty("--res-thumb", n + "px");
    input.value = String(n);
    if (save) localStorage.setItem(KEY, String(n));
  };

  const saved = Number(localStorage.getItem(KEY) || 0);
  apply(saved >= MIN ? saved : DEF, false);
  input.addEventListener("input", () => apply(input.value, true));
  input.addEventListener("dblclick", () => apply(DEF, true));
})();
