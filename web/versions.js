// 项目内版本谱系：一次生成一条，默认 1 星，只和父版本 diff。
let verCache = [];
let verSort = "time";
let verLimit = 100;

function verProject() {
  return window.projectUI && window.projectUI.currentObj ? window.projectUI.currentObj() : null;
}

function spanDiff(a, b) {
  if (a === b) return { same: true, pre: "", old: "", neu: "", post: "" };
  let i = 0;
  while (i < a.length && i < b.length && a[i] === b[i]) i++;
  let j = 0;
  while (j < a.length - i && j < b.length - i && a[a.length - 1 - j] === b[b.length - 1 - j]) j++;
  return {
    same: false,
    pre: a.slice(0, i),
    old: a.slice(i, a.length - j),
    neu: b.slice(i, b.length - j),
    post: a.slice(a.length - j),
  };
}

function diffInto(box, a, b) {
  box.textContent = "";
  const d = spanDiff(a || "", b || "");
  if (d.same) {
    box.className = "muted";
    box.style.fontSize = "12px";
    box.textContent = "（无变化）";
    return;
  }
  box.className = "";
  box.style.cssText = "font-size:12px;line-height:1.6;white-space:pre-wrap;word-break:break-word";
  const pre = document.createElement("span");
  pre.textContent = d.pre;
  const old = document.createElement("span");
  old.style.cssText = "color:#e06c75;text-decoration:line-through";
  old.textContent = d.old;
  const neu = document.createElement("span");
  neu.style.cssText = "color:#98c379";
  neu.textContent = d.neu;
  const post = document.createElement("span");
  post.textContent = d.post;
  box.append(pre, old, neu, post);
}

function verStars(v, project, rerender) {
  const box = document.createElement("span");
  box.style.cssText = "letter-spacing:1px;cursor:pointer;color:#d9a441";
  box.title = "点击改 1–5 星";
  for (let i = 1; i <= 5; i++) {
    const s = document.createElement("span");
    s.textContent = i <= v.stars ? "★" : "☆";
    s.onclick = async (e) => {
      e.stopPropagation();
      try {
        const updated = await api.call("version_star", { project: project.id, id: v.id, stars: i });
        Object.assign(v, updated);
        rerender();
      } catch (err) {
        toast(String(err), true);
      }
    };
    box.appendChild(s);
  }
  return box;
}

function verRow(v, project, rerender) {
  const row = document.createElement("div");
  row.style.cssText = "border:1px solid var(--line);border-radius:8px;padding:8px;display:flex;flex-direction:column;gap:6px";

  const head = document.createElement("div");
  head.className = "row tight";
  head.style.alignItems = "center";
  const isLayout = v.kind === "layout";
  const kind = document.createElement("span");
  kind.textContent = isLayout ? "排版" : "出图";
  kind.style.cssText = "font-size:11px;padding:1px 5px;border-radius:4px;background:" + (isLayout ? "#3a2f1e" : "#1e2a3a") + ";color:" + (isLayout ? "#d9a441" : "#7aa2d9");
  const time = document.createElement("span");
  time.className = "muted mono";
  time.style.fontSize = "11px";
  time.textContent = new Date(v.createdAt).toLocaleString();
  const parentIdx = v.parent ? verCache.findIndex((x) => x.id === v.parent) : -1;
  const parent = document.createElement("span");
  parent.className = "muted mono";
  parent.style.fontSize = "11px";
  parent.textContent = parentIdx >= 0 ? `父 #${parentIdx + 1}` : "根版本";
  const idx = verCache.indexOf(v) + 1;
  const branch = document.createElement("button");
  branch.className = "btn small ghost";
  branch.textContent = "从这版继续";
  branch.title = "下一次生成挂在这条版本下面（分叉）";
  branch.onclick = () => {
    fillVersion(v);
    state.versionBranch = v.id;
    $("versionsModal").hidden = true;
    toast(`下次生成从 #${idx} 分叉`);
  };
  branch.hidden = isLayout;
  const fill = document.createElement("button");
  fill.className = "btn small";
  fill.textContent = isLayout ? "载入排版" : "填入";
  fill.onclick = () => {
    if (isLayout) {
      $("versionsModal").hidden = true;
      if (typeof layoutLoadJson === "function") {
        try {
          const n = layoutLoadJson(v.layout || "{}");
          toast(`已载入排版（${n} 个框），下次生成使用`);
        } catch (e) {
          toast(String(e), true);
        }
      } else {
        state.pendingLayout = v.layout || null;
        toast("已载入排版，下次生成使用");
      }
      return;
    }
    fillVersion(v);
    $("versionsModal").hidden = true;
    toast("已填入这版参数");
  };
  const diffBtn = document.createElement("button");
  diffBtn.className = "btn small ghost";
  diffBtn.textContent = "对比父版";
  diffBtn.disabled = isLayout || parentIdx < 0;
  const del = document.createElement("button");
  del.className = "btn small ghost";
  del.textContent = "删";
  del.onclick = async () => {
    if (!confirm("删除这条版本记录？（不影响图片）")) return;
    try {
      await api.call("version_delete", { project: project.id, id: v.id });
      await versionsRefresh();
    } catch (e) {
      toast(String(e), true);
    }
  };
  head.append(verStars(v, project, rerender), kind, time, parent, branch, fill, diffBtn, del);

  const prompt = document.createElement("div");
  prompt.textContent = v.prompt;
  prompt.style.cssText = "font-size:13px;white-space:pre-wrap;word-break:break-word";

  const diffBox = document.createElement("div");
  diffBox.hidden = true;
  diffBtn.onclick = () => {
    diffBox.hidden = !diffBox.hidden;
    if (!diffBox.hidden && parentIdx >= 0) {
      diffBox.textContent = "";
      const pv = verCache[parentIdx];
      const label = document.createElement("div");
      label.className = "muted";
      label.style.fontSize = "11px";
      label.textContent = "提示词";
      const pd = document.createElement("div");
      diffInto(pd, pv.prompt, v.prompt);
      const nlabel = document.createElement("div");
      nlabel.className = "muted";
      nlabel.style.fontSize = "11px";
      nlabel.textContent = "负面";
      const nd = document.createElement("div");
      diffInto(nd, pv.negative, v.negative);
      const params = document.createElement("div");
      params.className = "muted mono";
      params.style.cssText = "font-size:11px;margin-top:4px";
      params.textContent = `${v.width}×${v.height} · ${v.steps}步 · cfg ${v.cfg} · seed ${v.seed} · ${v.profile} · ${v.mode}`;
      diffBox.append(label, pd, nlabel, nd, params);
    }
  };

  if (isLayout) {
    const info = document.createElement("div");
    info.className = "muted mono";
    info.style.fontSize = "11px";
    info.textContent = `MOSS 排版 · ${(v.layout || "").length} 字符`;
    row.append(head, prompt, info);
    return row;
  }
  row.append(head, prompt, diffBox);
  return row;
}

function renderVersions() {
  const box = $("versionsList");
  const p = verProject();
  box.textContent = "";
  if (!p) {
    box.textContent = "先在项目栏选一个项目";
    $("verCount").textContent = "";
    return;
  }
  if (p.private) {
    box.textContent = "私密项目不记录版本（避免明文）";
    $("verCount").textContent = "";
    return;
  }
  const sorted = verCache.slice().sort((a, b) =>
    verSort === "stars" ? b.stars - a.stars || b.createdAt - a.createdAt : b.createdAt - a.createdAt
  );
  const shown = sorted.slice(0, verLimit);
  $("verCount").textContent = `显示 ${shown.length} / ${sorted.length}`;
  if (!sorted.length) {
    box.textContent = "还没有版本。生成一次就会记一条。";
    return;
  }
  for (const v of shown) box.appendChild(verRow(v, p, renderVersions));
  if (sorted.length > shown.length) {
    const more = document.createElement("button");
    more.className = "btn small";
    more.textContent = `再显示 ${Math.min(100, sorted.length - shown.length)} 条`;
    more.onclick = () => {
      verLimit += 100;
      renderVersions();
    };
    box.appendChild(more);
  }
}

async function versionsRefresh() {
  const p = verProject();
  if (!p || p.private) return renderVersions();
  try {
    verCache = await api.call("versions_list", { project: p.id });
  } catch (e) {
    verCache = [];
    toast(String(e), true);
  }
  renderVersions();
}

function fillVersion(v) {
  state.versionBranch = null;
  $("prompt").value = v.prompt || "";
  $("negative").value = v.negative || "";
  $("width").value = v.width;
  $("height").value = v.height;
  $("steps").value = v.steps;
  $("cfg").value = v.cfg;
  $("seed").value = v.seed;
  if (v.profile) {
    const sel = $("model");
    if ([...sel.options].some((o) => o.value === v.profile)) sel.value = v.profile;
  }
  state.pendingLayout = v.layout || null;
}

$("versionsBtn").onclick = async () => {
  $("versionsModal").hidden = false;
  verLimit = 100;
  await versionsRefresh();
};
$("versionsClose").onclick = () => ($("versionsModal").hidden = true);
$("verSortTime").onclick = () => {
  verSort = "time";
  $("verSortTime").classList.remove("ghost");
  $("verSortStars").classList.add("ghost");
  renderVersions();
};
$("verSortStars").onclick = () => {
  verSort = "stars";
  $("verSortStars").classList.remove("ghost");
  $("verSortTime").classList.add("ghost");
  renderVersions();
};
