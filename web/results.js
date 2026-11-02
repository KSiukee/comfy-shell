// 首页图区（结果网格 + 大图翻页）。
// 从 app.js 拆出（#6 继续拆「结果」）；app.js 只保留生成与装配。
// v0.95.1：只显示**当前项目**的图——避免切项目时看到别的项目的图。

/** 图片路径 → 所属项目 id（`output/projects/<id>/…`）；根目录的图返回空串（临时）。 */
function resultProjectId(path) {
  const m = /[\\/]projects[\\/]([^\\/]+)[\\/]/.exec(String(path || ""));
  return m ? m[1] : "";
}

function resultScopeWant() {
  const p = window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
  return p ? p.id : "";
}

/** 当前项目下的条目（不改 state.results 本身）。没有项目就什么都不显示。 */
function resultsInScope() {
  const want = resultScopeWant();
  if (!want) return [];
  return state.results.filter((r) => resultProjectId(r.path) === want);
}

function emptyHint() {
  const p = $("empty") && $("empty").querySelector("p");
  if (!p) return;
  p.textContent = !resultScopeWant()
    ? "先在左上角项目栏选一个项目（图区只显示当前项目的图）。"
    : "本项目还没有图。左边写好提示词，点「生成」。";
}

function renderResults() {
  const box = $("results");
  box.querySelectorAll(".card").forEach((c) => c.remove());
  const list = resultsInScope().filter((x) => !x.missing);
  $("empty").hidden = list.length > 0;
  emptyHint();
  for (const r of list) {
    const card = document.createElement("div");
    card.className = "card";
    const img = document.createElement("img");
    img.dataset.path = r.path;
    img.alt = "";
    if (r.dataUrl) img.src = r.dataUrl;
    else observer.observe(img);
    img.onclick = () => openLightbox(r);
    card.oncontextmenu = (e) => {
      e.preventDefault();
      showCtxMenu(e.clientX, e.clientY, resultMenu(r));
    };
    const meta = document.createElement("div");
    meta.className = "meta";
    meta.innerHTML =
      `<span class="label" title="${r.filename}">${r.label}</span>` +
      `<span class="time mono">${r.width}×${r.height} · ${fmtTime(r.seconds)}</span>`;
    const star = document.createElement("button");
    star.className = "btn ghost small star" + (state.stars.has(r.path) ? " on" : "");
    star.title = "收藏";
    star.textContent = state.stars.has(r.path) ? "★" : "☆";
    star.onclick = (e) => {
      e.stopPropagation();
      toggleStar(r, star);
    };
    meta.appendChild(star);
    const reveal = document.createElement("button");
    reveal.className = "btn ghost small";
    reveal.title = "在文件夹中显示";
    reveal.textContent = "📁";
    reveal.onclick = () => api.call("reveal_file", { path: r.path });
    meta.appendChild(reveal);
    const del = document.createElement("button");
    del.className = "btn ghost small";
    del.title = "删除（移到回收站）";
    del.textContent = "🗑";
    del.onclick = (e) => {
      e.stopPropagation();
      deleteResults([r]);
    };
    meta.appendChild(del);
    const more = document.createElement("button");
    more.className = "btn ghost small";
    more.title = "更多";
    more.textContent = "⋯";
    more.onclick = (e) => {
      e.stopPropagation();
      const rect = more.getBoundingClientRect();
      showCtxMenu(rect.left, rect.bottom + 4, resultMenu(r));
    };
    meta.appendChild(more);
    card.appendChild(img);
    card.appendChild(meta);
    box.appendChild(card);
  }
}

// ---------------------------------------------------------------- lightbox

let lightboxItem = null;

async function openLightbox(r) {
  lightboxItem = r;
  $("lightbox").hidden = false;
  await showLightbox(r);
}

async function showLightbox(r) {
  lightboxItem = r;
  const list = resultsInScope();
  const idx = list.findIndex((x) => x.path === r.path);
  $("lbPos").textContent = idx >= 0 ? `${idx + 1} / ${list.length}` : "";
  $("lightboxMeta").textContent = `${r.filename} · ${r.width}×${r.height} · ${(r.prompt || "").slice(0, 40)}`;
  $("lightboxImg").src = r.dataUrl || "";
  if (!r.dataUrl) {
    try {
      r.dataUrl = await api.call("read_output_image", { path: r.path });
      if (lightboxItem === r) $("lightboxImg").src = r.dataUrl;
    } catch {}
  }
}

async function lightboxStep(delta) {
  const list = resultsInScope();
  const idx = list.findIndex((x) => x.path === lightboxItem?.path);
  if (idx < 0 || list.length < 2) return;
  const next = (idx + delta + list.length) % list.length;
  await showLightbox(list[next]);
}

$("lbPrev").onclick = () => lightboxStep(-1);
$("lbNext").onclick = () => lightboxStep(1);

$("lightbox").addEventListener(
  "wheel",
  (e) => {
    if ($("lightbox").hidden) return;
    e.preventDefault();
    if (Math.abs(e.deltaY) < 3) return;
    lightboxStep(e.deltaY > 0 ? 1 : -1);
  },
  { passive: false }
);

// ---------------------------------------------------------------- 图区范围
// v0.95.1：只有「本项目」一档，不再提供「全部」（防止跨项目泄漏）。
function applyResScope() {
  renderResults();
}

applyResScope();
