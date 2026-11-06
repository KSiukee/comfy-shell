// 图库详情抽屉：右侧看 prompt / 参数 / 项目 / 版本，
// 动作＝填入表单、作参考图、复制提示词、恢复参数（同一入口）。
// 数据优先级：PNG 参数块（v0.64 起生成图）→ 生成历史 → 只有文件信息。
// 依赖 app.js（$/api/toast/state）、library.js（libItems/libUrl/libHist）、meta.js（applyImageMeta）。

let libDetailPath = null;

function ld(tag, cls, text) {
  const el = document.createElement(tag);
  if (cls) el.className = cls;
  if (text !== undefined && text !== null) el.textContent = text;
  return el;
}

function ldProjectId(it) {
  const m = /^projects\/([^/]+)/.exec((it && it.subfolder) || "");
  return m ? m[1] : "";
}

function ldProjectName(id) {
  if (!id) return "";
  const list = (window.projectUI && window.projectUI.list && window.projectUI.list()) || [];
  const p = list.find((x) => x.id === id);
  return p ? p.name || p.id : id;
}

function ldSize(n) {
  if (!n) return "";
  const mb = n / (1024 * 1024);
  return mb >= 1 ? `${mb.toFixed(1)} MB` : `${Math.max(1, Math.round(n / 1024))} KB`;
}

function ldWhen(sec) {
  if (!sec) return "";
  return new Date(sec * 1000).toLocaleString();
}

function ldProfileName(id) {
  if (!id) return "";
  const p = (state.profiles || []).find((x) => x.id === id);
  return p ? p.label : id;
}

async function ldReadMeta(it) {
  if (String(it.path).startsWith("vault://")) return null;
  try {
    return await api.call("image_meta", { path: it.path });
  } catch {
    return null;
  }
}

async function ldFindVersion(it, projectId) {
  if (!projectId) return null;
  try {
    const list = await api.call("versions_list", { project: projectId });
    const idx = list.findIndex((v) => (v.images || []).includes(it.path));
    return idx >= 0 ? { v: list[idx], index: idx + 1, total: list.length } : null;
  } catch {
    return null;
  }
}

function ldParamRows(meta, hist) {
  const src = meta || hist || {};
  const rows = [];
  if (src.width || src.height) rows.push(["尺寸", `${src.width || 0}×${src.height || 0}`]);
  if (src.steps) rows.push(["步数", String(src.steps)]);
  if (src.cfg) rows.push(["CFG", String(src.cfg)]);
  if (src.seed !== undefined && src.seed !== null) {
    // 老图的随机种子是 -1，后端从 ComfyUI 写在 PNG 里的工作流块读回来了
    rows.push(["种子", src.seedRecovered ? `${src.seed}（从图里读回）` : String(src.seed)]);
  }
  if (src.profile) rows.push(["模型", ldProfileName(src.profile)]);
  if (src.mode) rows.push(["模式", String(src.mode)]);
  if (meta) {
    if (meta.lora) {
      const s = meta.loraStrength != null ? ` ×${Number(meta.loraStrength).toFixed(2)}` : "";
      rows.push(["LoRA", `${meta.lora}${s}`]);
    }
    if (meta.lora2) {
      const s = meta.lora2Strength != null ? ` ×${Number(meta.lora2Strength).toFixed(2)}` : "";
      rows.push(["LoRA2", `${meta.lora2}${s}`]);
    }
    if (meta.slg) rows.push(["SLG", meta.slgLayers || "开"]);
    if (meta.layout) rows.push(["排版", `${String(meta.layout).length} 字符`]);
    if (meta.versionId) rows.push(["版本 id", String(meta.versionId)]);
  }
  return rows;
}

function ldPromptOf(it) {
  const meta = it._meta;
  const hist = (typeof libHist !== "undefined" && libHist.get(it.path)) || null;
  if (meta && meta.prompt) return meta.prompt;
  if (hist && hist.prompt) return hist.prompt;
  return (typeof libPrompts !== "undefined" && libPrompts.get(it.path)) || "";
}

function ldCopyPrompt(it) {
  const text = ldPromptOf(it);
  if (!text) {
    toast("这张图没有提示词记录", true);
    return;
  }
  navigator.clipboard.writeText(text).then(
    () => toast("提示词已复制"),
    () => toast("复制失败", true)
  );
}

async function ldDelete(it) {
  if (!confirm(`删除 ${it.filename}？\n会移到项目文件夹的 _trash\\ 里，可手动恢复`)) return;
  try {
    await api.call("library_delete", { paths: [it.path] });
    closeLibDetail();
    await loadLibrary();
    toast("已移到回收站");
  } catch (e) {
    toast(String(e), true);
  }
}

function ldFillHistory(hist) {
  if (typeof fillVersion === "function") fillVersion(hist);
  else {
    $("prompt").value = hist.prompt || "";
    $("negative").value = hist.negative || "";
    if (hist.width) $("width").value = hist.width;
    if (hist.height) $("height").value = hist.height;
    if (hist.steps) $("steps").value = hist.steps;
    if (hist.cfg) $("cfg").value = hist.cfg;
    if (hist.seed !== undefined) $("seed").value = hist.seed;
  }
  if (typeof setMode === "function") setMode(hist.mode === "edit" ? "edit" : "t2i");
  toast("已填入这张图的参数");
}

function ldBlock(body, label) {
  const box = ld("div", "ld-block");
  if (label) box.appendChild(ld("div", "ld-label", label));
  body.appendChild(box);
  return box;
}

function ldRenderActions(box, it, meta, hist) {
  const row = ld("div", "ld-actions");
  const hasParams = !!meta || !!hist;
  const fill = ld("button", "btn small", meta ? "填入表单" : "填入参数");
  fill.disabled = !hasParams;
  fill.title = "把这张图的 prompt 与参数填回生成页（含项目/排版）";
  fill.onclick = async () => {
    if (meta && typeof applyImageMeta === "function") await applyImageMeta(meta);
    else if (hist) ldFillHistory(hist);
  };
  const edit = ld("button", "btn small ghost", "调色 / 裁切");
  edit.disabled = String(it.path).startsWith("vault://");
  edit.title = "裁切、套 LUT、调明暗（原图保留，另存新图）";
  edit.onclick = () => {
    if (window.photoEditor) window.photoEditor.open({ path: it.path, filename: it.filename });
  };
  const ref = ld("button", "btn small ghost", "作参考图");
  ref.onclick = () => {
    if (typeof useAsReference === "function") useAsReference({ path: it.path, filename: it.filename });
  };
  const copy = ld("button", "btn small ghost", "复制提示词");
  copy.onclick = () => ldCopyPrompt(it);
  const folder = ld("button", "btn small ghost", "文件夹");
  folder.disabled = String(it.path).startsWith("vault://");
  folder.onclick = () => api.call("reveal_file", { path: it.path });
  const del = ld("button", "btn danger small", "删除");
  del.onclick = () => ldDelete(it);
  row.append(fill, edit, ref, copy, folder, del);
  box.appendChild(row);
}

async function renderLibDetail(it) {
  const body = $("libDetailBody");
  if (!body) return;
  body.textContent = "";
  it._meta = await ldReadMeta(it);
  const meta = it._meta;
  const hist = (typeof libHist !== "undefined" && libHist.get(it.path)) || null;
  const projectId = ldProjectId(it) || (meta && meta.project) || "";
  const ver = await ldFindVersion(it, projectId);
  if (libDetailPath !== it.path) return; // 期间又点了别的图

  const thumb = ld("img", "ld-thumb");
  thumb.alt = it.filename;
  try {
    thumb.src = await libUrl(it.path);
  } catch {
    thumb.alt = "读取失败";
  }
  body.appendChild(thumb);

  body.appendChild(ld("div", "ld-title", it.filename));
  const facts = [ldSize(it.size), it.width && it.height ? `${it.width}×${it.height}` : "", ldWhen(it.mtime)]
    .filter(Boolean)
    .join(" · ");
  body.appendChild(ld("div", "ld-note", facts));

  const badges = ld("div", "ld-actions");
  badges.appendChild(ld("span", "ld-badge", projectId ? `项目 · ${ldProjectName(projectId)}` : "未归属项目"));
  if (ver) badges.appendChild(ld("span", "ld-badge", `版本 #${ver.index} ${"★".repeat(ver.v.stars || 0)}`));
  if (it.starred) badges.appendChild(ld("span", "ld-badge", "已收藏"));
  body.appendChild(badges);

  if (!meta && !hist) {
    body.appendChild(
      ld("div", "ld-note", "没有参数记录：v0.64 之前的图、外部导入的图、或私密图（私密不写明文参数）。")
    );
  } else if (!meta && hist) {
    body.appendChild(ld("div", "ld-note", "这张图没有 PNG 参数块，下面读的是生成历史（v0.64 之前的图）。"));
  }

  const prompt = (meta && meta.prompt) || (hist && hist.prompt) || "";
  if (prompt) {
    const box = ldBlock(body, "提示词");
    box.appendChild(ld("div", "ld-prompt", prompt));
  }
  const negative = (meta && meta.negative) || (hist && hist.negative) || "";
  if (negative) {
    const box = ldBlock(body, "负面提示词");
    box.appendChild(ld("div", "ld-prompt", negative));
  }

  const rows = ldParamRows(meta, hist);
  if (rows.length) {
    const box = ldBlock(body, "参数");
    const grid = ld("div", "ld-grid");
    for (const [k, v] of rows) grid.append(ld("span", null, k), ld("span", null, v));
    box.appendChild(grid);
  }

  if (ver) {
    const box = ldBlock(body, "版本");
    box.appendChild(
      ld(
        "div",
        "ld-note",
        `#${ver.index} / 共 ${ver.total} 条 · ${"★".repeat(ver.v.stars || 0)} · ${new Date(ver.v.createdAt).toLocaleString()}`
      )
    );
    const open = ld("button", "btn small ghost", "打开版本窗");
    open.onclick = () => $("versionsBtn").click();
    box.appendChild(open);
  }

  const actions = ldBlock(body, "动作");
  ldRenderActions(actions, it, meta, hist);
}

async function openLibDetail(it) {
  if (!it) return;
  libDetailPath = it.path;
  $("libDetail").hidden = false;
  await renderLibDetail(it);
}

function closeLibDetail() {
  libDetailPath = null;
  $("libDetail").hidden = true;
  const body = $("libDetailBody");
  if (body) body.textContent = "";
}

async function refreshLibDetail() {
  if (!libDetailPath) return;
  const it = (typeof libItems !== "undefined" ? libItems : []).find((x) => x.path === libDetailPath);
  if (!it) {
    closeLibDetail();
    return;
  }
  await renderLibDetail(it);
}

function libItemMenu(it) {
  return [
    { label: "详情（参数 / 项目 / 版本）", run: () => openLibDetail(it) },
    {
      label: "调色 / 裁切…",
      run: () => window.photoEditor && window.photoEditor.open({ path: it.path, filename: it.filename }),
    },
    { label: "用作参考图", run: () => useAsReference({ path: it.path, filename: it.filename }) },
    { label: "复制提示词", run: () => ldCopyPrompt(it) },
    { label: "在文件夹中显示", run: () => api.call("reveal_file", { path: it.path }) },
    { label: "删除（移到回收站）", danger: true, run: () => ldDelete(it) },
  ];
}

/// 图库卡片上加「ⓘ 详情」入口与右键菜单（library.js 只调这一行，免得图库文件变上帝文件）。
function attachLibItemDetail(card, it) {
  const edit = document.createElement("button");
  edit.className = "btn ghost small li-info";
  edit.title = "调色 / 裁切（裁切、LUT、明暗）";
  edit.textContent = "调";
  edit.disabled = String(it.path).startsWith("vault://");
  edit.onclick = (e) => {
    e.stopPropagation();
    if (window.photoEditor) window.photoEditor.open({ path: it.path, filename: it.filename });
  };
  const info = document.createElement("button");
  info.className = "btn ghost small li-info";
  info.title = "详情：参数 / 项目 / 版本";
  info.textContent = "ⓘ";
  info.onclick = (e) => {
    e.stopPropagation();
    openLibDetail(it);
  };
  const meta = card.querySelector(".li-meta");
  if (meta) meta.append(edit, info);
  card.oncontextmenu = (e) => {
    e.preventDefault();
    showCtxMenu(e.clientX, e.clientY, libItemMenu(it));
  };
}

$("libDetailClose").onclick = closeLibDetail;
