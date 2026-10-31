// 跨项目提示词库：搜索、收藏、常用、去 AI 套话。
let promptsCache = [];
let promptSort = "recent";
let promptQuery = "";

function promptsProject() {
  return window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
}

function promptMatches(e) {
  const q = promptQuery.trim().toLowerCase();
  if (!q) return true;
  const hay = `${e.text} ${e.negative || ""} ${e.tags.join(" ")} ${e.projectName || ""}`.toLowerCase();
  return hay.includes(q);
}

function renderPrompts() {
  const box = $("promptsList");
  box.textContent = "";
  const list = promptsCache.filter(promptMatches).sort((a, b) => {
    if (promptSort === "uses") return b.uses - a.uses || b.updatedAt - a.updatedAt;
    if (promptSort === "fav") return Number(b.favorite) - Number(a.favorite) || b.uses - a.uses;
    return b.updatedAt - a.updatedAt;
  });
  if (!list.length) {
    box.textContent = promptQuery ? "没有匹配的提示词" : "库还是空的。写一条提示词后点「存当前提示词」。";
    return;
  }
  for (const e of list) box.appendChild(promptRow(e));
}

function promptRow(e) {
  const row = document.createElement("div");
  row.style.cssText = "border:1px solid var(--line);border-radius:8px;padding:8px;display:flex;flex-direction:column;gap:6px";
  const head = document.createElement("div");
  head.className = "row tight";
  head.style.alignItems = "center";
  const meta = document.createElement("span");
  meta.className = "muted mono";
  meta.style.fontSize = "11px";
  meta.textContent = `${e.projectName || "无项目"} · 用 ${e.uses} 次${e.tags.length ? " · " + e.tags.join(" / ") : ""}`;
  const fill = document.createElement("button");
  fill.className = "btn small";
  fill.textContent = "填入";
  fill.onclick = async () => {
    $("prompt").value = e.text;
    if (e.negative) $("negative").value = e.negative;
    $("promptsModal").hidden = true;
    try {
      await api.call("promptlib_use", { id: e.id });
    } catch {}
    toast("已填入提示词");
  };
  const star = document.createElement("button");
  star.className = "btn small ghost";
  star.textContent = e.favorite ? "★ 已收藏" : "☆ 收藏";
  star.onclick = async () => {
    try {
      const updated = await api.call("promptlib_star", { id: e.id, favorite: !e.favorite });
      Object.assign(e, updated);
      renderPrompts();
    } catch (err) {
      toast(String(err), true);
    }
  };
  const del = document.createElement("button");
  del.className = "btn small ghost";
  del.textContent = "删";
  del.onclick = async () => {
    if (!confirm("从提示词库删除这条？")) return;
    try {
      await api.call("promptlib_delete", { id: e.id });
      await promptsRefresh();
    } catch (err) {
      toast(String(err), true);
    }
  };
  head.append(meta, fill, star, del);

  const text = document.createElement("div");
  text.textContent = e.text;
  text.style.cssText = "font-size:13px;white-space:pre-wrap;word-break:break-word";
  row.append(head, text);
  if (e.negative) {
    const neg = document.createElement("div");
    neg.className = "muted";
    neg.style.cssText = "font-size:11px;white-space:pre-wrap";
    neg.textContent = `负面：${e.negative}`;
    row.appendChild(neg);
  }
  return row;
}

async function promptsRefresh() {
  try {
    promptsCache = await api.call("promptlib_list");
  } catch (e) {
    promptsCache = [];
    toast(String(e), true);
  }
  renderPrompts();
}

$("promptsBtn").onclick = async () => {
  $("promptsModal").hidden = false;
  await promptsRefresh();
};
$("promptsClose").onclick = () => ($("promptsModal").hidden = true);

$("promptSearch").oninput = () => {
  promptQuery = $("promptSearch").value;
  renderPrompts();
};
$("promptSort").onchange = () => {
  promptSort = $("promptSort").value;
  renderPrompts();
};

$("promptSaveCurrent").onclick = async () => {
  const p = promptsProject();
  const text = $("prompt").value.trim();
  if (!text) return toast("先写一条提示词", true);
  if (p && p.private) return toast("私密项目不存进提示词库", true);
  try {
    await api.call("promptlib_save", {
      text,
      negative: $("negative").value,
      tags: [],
      project: p ? p.id : null,
    });
    await promptsRefresh();
    toast("已存入提示词库");
  } catch (e) {
    toast(String(e), true);
  }
};

$("promptCleanCurrent").onclick = async () => {
  try {
    const r = await api.call("promptlib_clean", { text: $("prompt").value });
    $("prompt").value = r.text;
    toast(r.removed ? `去掉 ${r.removed} 个 AI 套话` : "没有发现套话");
  } catch (e) {
    toast(String(e), true);
  }
};
