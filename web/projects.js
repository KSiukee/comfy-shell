// 项目栏（v0.43.0）：新建 / 改名 / 开工 / 隐藏 / 删除进回收站。
// 所有后端调用走 store.js 的 api.call；本文件不碰生成参数（#16 再接输出目录）。
const projState = { list: [], all: [], hiddenIds: new Set(), current: "", includeHidden: false, modalMode: "create" };
const LIB_SCOPE_KEY = "cs.libScope";
let libScope = localStorage.getItem(LIB_SCOPE_KEY) || "project";

function projCurrent() {
  return projState.list.find((p) => p.id === projState.current) || null;
}

function projFmtTime(ms) {
  if (!ms) return "";
  const d = new Date(ms);
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

async function projRefresh(selectId) {
  let list = [];
  try {
    list = await api.call("project_list", { includeHidden: projState.includeHidden });
  } catch (e) {
    toast(String(e), true);
    return [];
  }
  projState.list = list;
  try {
    projState.all = await api.call("project_list", { includeHidden: true });
  } catch {
    projState.all = list;
  }
  projState.hiddenIds = new Set(projState.all.filter((p) => p.hidden).map((p) => p.id));

  const sel = $("projSel");
  sel.innerHTML = "";
  const none = document.createElement("option");
  none.value = "";
  none.textContent = "临时（不归项目）";
  sel.appendChild(none);
  for (const p of list) {
    const o = document.createElement("option");
    o.value = p.id;
    o.textContent = `${p.hidden ? "（隐藏）" : ""}${p.name}${p.active ? " · 开工" : ""}`;
    sel.appendChild(o);
  }

  const active = list.find((p) => p.active);
  const want = selectId !== undefined ? selectId : active ? active.id : "";
  projState.current = list.some((p) => p.id === want) ? want : "";
  sel.value = projState.current;
  const cur = projCurrent();
  $("projHide").textContent = cur && cur.hidden ? "取消隐藏" : "隐藏";
  const scope = document.getElementById("libScope");
  if (scope) scope.options[0].textContent = `本项目（${active ? active.name : "临时"}）`;
  refreshLibraryIfOpen();
  return list;
}

// ---------------------------------------------------------------- 新建 / 改名
function openProjModal(mode) {
  projState.modalMode = mode;
  const cur = projCurrent();
  $("projModalTitle").textContent = mode === "create" ? "新建项目" : "重命名项目";
  $("projModalHint").textContent =
    mode === "create"
      ? "新建后会自动开工；改名只改显示名，不动图片目录。"
      : "只改显示名，不动图片目录，也不影响已有图片。";
  $("projNameInput").value = mode === "rename" && cur ? cur.name : "";
  $("projModal").hidden = false;
  setTimeout(() => $("projNameInput").focus(), 30);
}

function closeProjModal() {
  $("projModal").hidden = true;
}

$("projNew").onclick = () => openProjModal("create");
$("projRename").onclick = () => {
  if (!projCurrent()) return toast("先选一个项目", true);
  openProjModal("rename");
};
$("projModalCancel").onclick = closeProjModal;

$("projModalOk").onclick = async () => {
  const name = $("projNameInput").value.trim();
  if (!name) return toast("项目名不能为空", true);
  try {
    if (projState.modalMode === "create") {
      const p = await api.call("project_create", { name });
      await api.call("project_set_active", { id: p.id });
      closeProjModal();
      await projRefresh(p.id);
      toast(`已新建并开工：${p.name}`);
    } else {
      const p = await api.call("project_rename", { id: projState.current, name });
      closeProjModal();
      await projRefresh(p.id);
      toast(`已改名：${p.name}`);
    }
  } catch (e) {
    toast(String(e), true);
  }
};

$("projNameInput").addEventListener("keydown", (e) => {
  if (e.key === "Enter") $("projModalOk").click();
  if (e.key === "Escape") closeProjModal();
});

// ---------------------------------------------------------------- 切换 / 隐藏 / 删除
$("projSel").onchange = async () => {
  const id = $("projSel").value;
  try {
    await api.call("project_set_active", { id: id || null });
    await projRefresh(id);
    const cur = projCurrent();
    toast(id ? `已开工：${cur ? cur.name : ""}` : "已回到临时（不归项目）");
  } catch (e) {
    toast(String(e), true);
    await projRefresh(projState.current);
  }
};

$("projHide").onclick = async () => {
  const p = projCurrent();
  if (!p) return toast("先选一个项目", true);
  try {
    if (!p.hidden && p.active) await api.call("project_set_active", { id: null });
    const updated = await api.call("project_set_hidden", { id: p.id, hidden: !p.hidden });
    await projRefresh(updated.hidden ? "" : p.id);
    toast(updated.hidden ? `已隐藏：${p.name}（只是不显示，文件仍是明文）` : `已取消隐藏：${p.name}`);
  } catch (e) {
    toast(String(e), true);
  }
};

$("projDelete").onclick = async () => {
  const p = projCurrent();
  if (!p) return toast("先选一个项目", true);
  if (!confirm(`把「${p.name}」放进回收站？\n项目目录会移过去，可以在回收站恢复。`)) return;
  try {
    await api.call("project_delete", { id: p.id });
    await projRefresh("");
    toast("已放进回收站");
  } catch (e) {
    toast(String(e), true);
  }
};

$("projShowHidden").onchange = () => {
  projState.includeHidden = $("projShowHidden").checked;
  projRefresh(projState.current);
};

// ---------------------------------------------------------------- 回收站
async function openProjTrash() {
  let items = [];
  try {
    items = await api.call("project_trash");
  } catch (e) {
    toast(String(e), true);
    return;
  }
  const box = $("projTrashList");
  box.innerHTML = "";
  if (!items.length) {
    const s = document.createElement("span");
    s.className = "muted";
    s.textContent = "回收站是空的";
    box.appendChild(s);
  }
  for (const it of items) {
    const row = document.createElement("div");
    row.className = "row tight";
    row.style.cssText = "border:1px solid var(--line);border-radius:8px;padding:8px";
    const name = document.createElement("span");
    name.textContent = it.name || it.dir;
    name.style.flex = "1";
    const time = document.createElement("span");
    time.className = "muted mono";
    time.style.fontSize = "11px";
    time.textContent = projFmtTime(it.deletedAt);
    const restore = document.createElement("button");
    restore.className = "btn small";
    restore.textContent = "恢复";
    restore.onclick = async () => {
      try {
        const p = await api.call("project_restore", { dir: it.dir });
        await openProjTrash();
        await projRefresh(p.id);
        toast(`已恢复：${p.name}`);
      } catch (e) {
        toast(String(e), true);
      }
    };
    const purge = document.createElement("button");
    purge.className = "btn small ghost";
    purge.textContent = "永久删除";
    purge.onclick = async () => {
      if (!confirm(`永久删除「${it.name || it.dir}」？\n不可恢复。`)) return;
      try {
        await api.call("project_purge", { dir: it.dir });
        await openProjTrash();
        await projRefresh(projState.current);
        toast("已永久删除");
      } catch (e) {
        toast(String(e), true);
      }
    };
    row.append(name, time, restore, purge);
    box.appendChild(row);
  }
  $("projTrashModal").hidden = false;
}

$("projTrashBtn").onclick = openProjTrash;
$("projTrashClose").onclick = () => {
  $("projTrashModal").hidden = true;
};

// 给以后的功能（#16 图库/生成）留一个入口
window.projectUI = { refresh: projRefresh, current: () => projState.current || null, currentObj: () => projCurrent(), list: () => projState.list };

// ---------------------------------------------------------------- 图库范围
// 图库仍由 app.js 渲染；这里在 api.call 出口按范围过滤，不改 app.js。
const rawApiCall = api.call.bind(api);
api.call = (name, args) => {
  const p = rawApiCall(name, args);
  return name === "library_scan" ? p.then((items) => filterLibItems(items)) : p;
};

function libProjectOf(subfolder) {
  const m = /^projects\/([^/]+)/.exec(subfolder || "");
  return m ? m[1] : "";
}

function filterLibItems(items) {
  const active = projCurrent();
  return (items || []).filter((it) => {
    const pid = libProjectOf(it.subfolder);
    if (pid === "_test") return false;
    if (pid && projState.hiddenIds.has(pid)) return false;
    if (libScope === "private") return false; // #17 未启用
    if (libScope === "all") return true;
    return active ? pid === active.id : !pid;
  });
}

function refreshLibraryIfOpen() {
  const lib = document.getElementById("library");
  if (lib && !lib.hidden && typeof loadLibrary === "function") loadLibrary();
}

const libScopeSel = document.getElementById("libScope");
if (libScopeSel) {
  libScopeSel.value = libScope;
  libScopeSel.addEventListener("change", () => {
    libScope = libScopeSel.value;
    localStorage.setItem(LIB_SCOPE_KEY, libScope);
    refreshLibraryIfOpen();
  });
}

projRefresh();
