// 快捷键（v0.95）：默认单键 + 设置里可改，存 localStorage。
// 作用对象是「当前正在看的那张图」：大图 > 灯箱 > 图库里选中的一张。
// 输入框/编辑器里打字不触发；设置窗打开时不触发（那里在录音键）。

const SC_ACTIONS = [
  { id: "crop", label: "快速裁剪", def: "c" },
  { id: "color", label: "调色", def: "e" },
  { id: "lut", label: "套 LUT", def: "l" },
  { id: "upscale", label: "快速超分（生成中排队）", def: "u" },
  { id: "delete", label: "删除当前图", def: "delete" },
];

const SC_STORE = "cs.shortcuts";
let scMap = null;
let scRecording = null; // { id, btn }

function scLoad() {
  if (scMap) return scMap;
  let saved = {};
  try {
    saved = JSON.parse(localStorage.getItem(SC_STORE) || "{}") || {};
  } catch {}
  scMap = {};
  for (const a of SC_ACTIONS) scMap[a.id] = String(saved[a.id] || a.def).toLowerCase();
  return scMap;
}

function scPersist() {
  try {
    localStorage.setItem(SC_STORE, JSON.stringify(scMap || {}));
  } catch {}
}

function scEventKey(e) {
  const mods = (e.ctrlKey ? "ctrl+" : "") + (e.altKey ? "alt+" : "") + (e.shiftKey ? "shift+" : "");
  let k = String(e.key || "").toLowerCase();
  if (k === " ") k = "space";
  return mods + k;
}

function scKeyLabel(k) {
  if (!k) return "—";
  const parts = String(k).split("+");
  const last = parts.pop();
  const named = {
    delete: "Del",
    escape: "Esc",
    space: "空格",
    arrowleft: "←",
    arrowright: "→",
    arrowup: "↑",
    arrowdown: "↓",
    backspace: "退格",
    enter: "回车",
    tab: "Tab",
  };
  const name = named[last] || (last.length === 1 ? last.toUpperCase() : last.charAt(0).toUpperCase() + last.slice(1));
  return parts
    .map((p) => ({ ctrl: "Ctrl", alt: "Alt", shift: "Shift" })[p] || p)
    .concat(name)
    .join("+");
}

/// 当前操作对象：大图 > 灯箱 > 图库单选。
function scCurrentItem() {
  try {
    if (!$("viewer").hidden && typeof viewer !== "undefined" && viewer.list.length) {
      return viewer.list[viewer.idx];
    }
    if (!$("lightbox").hidden && typeof lightboxItem !== "undefined" && lightboxItem) {
      return lightboxItem;
    }
    if (!$("library").hidden && typeof libSel !== "undefined" && libSel.size === 1) {
      const p = [...libSel][0];
      const it = (typeof libItems !== "undefined" ? libItems : []).find((x) => x.path === p);
      return it || { path: p, filename: String(p).split(/[\\/]/).pop() };
    }
  } catch {}
  return null;
}

async function scDeleteCurrent(it) {
  if (!it) return;
  if (String(it.path).startsWith("vault://")) {
    toast("私密图请在锁定区里删", true);
    return;
  }
  if (!confirm(`删除 ${it.filename || it.path}？\n会移到项目文件夹的 _trash\\ 里，可手动恢复`)) return;
  try {
    await api.call("delete_results", { paths: [it.path] });
    toast("已删除（回收站里可找回）");
    if (!$("viewer").hidden && typeof viewer !== "undefined") {
      const i = viewer.list.findIndex((x) => x.path === it.path);
      if (i >= 0) {
        viewer.list.splice(i, 1);
        if (!viewer.list.length) {
          $("viewer").hidden = true;
        } else {
          viewer.idx = Math.min(i, viewer.list.length - 1);
          await showViewerItem();
        }
      }
    }
    if (typeof state !== "undefined" && state.results) {
      state.results = state.results.filter((r) => r.path !== it.path);
      if (typeof renderResults === "function") renderResults();
    }
    if (typeof loadLibrary === "function" && !$("library").hidden) await loadLibrary();
    if (!$("lightbox").hidden) {
      if (typeof lightboxItem !== "undefined") lightboxItem = null;
      $("lightbox").hidden = true;
    }
  } catch (e) {
    toast(String(e), true);
  }
}

function scRun(id) {
  const it = scCurrentItem();
  if (id === "delete") {
    if (!it) return toast("先点开一张图（或在大图里）", true);
    scDeleteCurrent(it);
    return;
  }
  if (!it) return toast("先点开一张图（或在大图里）", true);
  if (id === "upscale") {
    if (typeof queueUpscale === "function") queueUpscale(it);
    else toast("超分还没准备好", true);
    return;
  }
  const tab = id === "crop" ? "crop" : id === "lut" ? "lut" : "light";
  if (window.photoEditor) {
    window.photoEditor.open({ path: it.path, filename: it.filename, dataUrl: it.dataUrl }, tab);
  }
}

// ---- 设置里的编辑 UI ----

function scBuildSettings() {
  const box = $("scList");
  if (!box) return;
  const map = scLoad();
  box.innerHTML = "";
  for (const a of SC_ACTIONS) {
    const row = document.createElement("div");
    row.className = "sc-row";
    const lab = document.createElement("span");
    lab.className = "sc-label";
    lab.textContent = a.label;
    const btn = document.createElement("button");
    btn.className = "sc-key";
    btn.type = "button";
    btn.textContent = scKeyLabel(map[a.id]);
    btn.onclick = () => {
      if (scRecording) scRecording.btn.classList.remove("rec");
      scRecording = { id: a.id, btn };
      btn.classList.add("rec");
      btn.textContent = "按一个键…";
    };
    const reset = document.createElement("button");
    reset.className = "pe-mini sc-reset";
    reset.type = "button";
    reset.title = "恢复默认";
    reset.textContent = "↺";
    reset.onclick = () => {
      map[a.id] = a.def;
      scPersist();
      btn.textContent = scKeyLabel(a.def);
    };
    row.append(lab, btn, reset);
    box.appendChild(row);
  }
  return map;
}

// 录音键：捕获阶段拦下来，别让下面的快捷键/翻页也跟着响应
window.addEventListener(
  "keydown",
  (e) => {
    if (!scRecording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      scRecording.btn.classList.remove("rec");
      scRecording.btn.textContent = scKeyLabel(scLoad()[scRecording.id]);
      scRecording = null;
      return;
    }
    if (e.key === "Shift" || e.key === "Control" || e.key === "Alt" || e.key === "Meta") return;
    const map = scLoad();
    map[scRecording.id] = scEventKey(e);
    scPersist();
    scRecording.btn.classList.remove("rec");
    scRecording.btn.textContent = scKeyLabel(map[scRecording.id]);
    scRecording = null;
  },
  true
);

// 触发
window.addEventListener(
  "keydown",
  (e) => {
    if (scRecording) return;
    if (e.defaultPrevented) return;
    const t = e.target;
    if (t && t.tagName) {
      const tag = t.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || t.isContentEditable) return;
    }
    if ($("settingsModal") && !$("settingsModal").hidden) return;
    if (typeof peEd !== "undefined" && peEd.open) return;
    const map = scLoad();
    const k = scEventKey(e);
    for (const a of SC_ACTIONS) {
      if (map[a.id] === k) {
        e.preventDefault();
        e.stopPropagation();
        scRun(a.id);
        return;
      }
    }
  },
  true
);

window.shortcuts = { actions: SC_ACTIONS, build: scBuildSettings, current: scCurrentItem };
