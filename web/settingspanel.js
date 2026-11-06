// 设置面板：左侧分类切换、版本/路径信息、工作流预设列表，以及设置页的保存按钮。
// 从 app.js 拆出来（app.js 只许缩不许长）；行为不变。

// settings modal（v0.97：左侧分类 + 右侧内容）
function setSettingsTab(tab) {
  document.querySelectorAll("#setNav .set-tab").forEach((b) => b.classList.toggle("active", b.dataset.tab === tab));
  document.querySelectorAll("#settingsModal .set-pane").forEach((p) => p.classList.toggle("active", p.dataset.pane === tab));
  if (tab === "ai" && window.aiAgent) window.aiAgent.refresh();
  if (tab === "workflows" && typeof wfDirty !== "undefined" && wfDirty) renderWorkflowPresets();

}
document.querySelectorAll("#setNav .set-tab").forEach((b) => {
  b.onclick = () => setSettingsTab(b.dataset.tab);
});

// collectSettings / saveSettings 搬到了 web/settings.js（app.js 只许缩，不许长）

async function refreshSettingsInfo() {
  try {
    const st = await api.call("server_status");
    const el = $("setComfyVersion");
    if (el) {
      el.textContent = st.healthy
        ? `ComfyUI ${st.version || ""} · 运行中${st.managed ? "（壳托管）" : "（外部进程）"}`
        : "未运行";
    }
  } catch {}
  try {
    const v = await api.call("app_version");
    if ($("setAppVersion")) $("setAppVersion").textContent = "v" + v;
  } catch {}
  if (typeof markWorkflowsDirty === "function") markWorkflowsDirty();
}

/// 工作流依赖检查：内置的每个档位需要哪几个模型、本机缺哪个。
let wfDirty = true;

function markWorkflowsDirty() {
  wfDirty = true;
  const pane = document.querySelector('#settingsModal .set-pane[data-pane="workflows"]');
  if (pane && pane.classList.contains("active")) renderWorkflowPresets();
}

async function renderWorkflowPresets() {
  wfDirty = false;
  const box = $("setWorkflows");
  if (!box) return;
  box.innerHTML = '<div class="muted" style="font-size:12px">检查依赖中…</div>';
  try {
    const [profs, models, man] = await Promise.all([
      api.call("profiles"),
      api.call("models_scan"),
      api.call("models_manifest"),
    ]);
    const have = new Set((models || []).map((m) => String(m.name || "").split("/").pop().toLowerCase()));
    const byTarget = new Map((man || []).map((m) => [m.target.toLowerCase(), m]));
    box.innerHTML = "";
    for (const p of profs || []) {
      const req = [p.unet, p.clip, p.vae].filter(Boolean);
      const missing = req.filter((f) => !have.has(String(f).toLowerCase()));
      const item = document.createElement("div");
      item.className = "set-wf-item";
      const head = document.createElement("div");
      head.className = "set-wf-head";
      head.innerHTML = `<i class="set-wf-dot ${missing.length ? "miss" : "ok"}"></i><b></b><span class="muted" style="font-size:11.5px"></span>`;
      head.querySelector("b").textContent = p.label || p.id;
      head.querySelector("span").textContent = `${p.steps} 步 · CFG ${p.cfg}${p.supportsEdit ? " · 支持改图" : ""}`;
      item.appendChild(head);

      if (!missing.length) {
        const ok = document.createElement("div");
        ok.className = "set-wf-sub";
        ok.textContent = "依赖齐全，可直接开跑";
        item.appendChild(ok);
      } else {
        for (const f of missing) {
          const m = byTarget.get(String(f).toLowerCase());
          const row = document.createElement("div");
          row.className = "set-wf-sub";
          const name = document.createElement("span");
          name.textContent = f;
          row.appendChild(name);
          if (m && m.size) {
            const sz = document.createElement("span");
            sz.className = "muted";
            sz.textContent = " · " + dlFmt(m.size);
            row.appendChild(sz);
          }
          if (m && (m.sources || []).length) {
            const btn = document.createElement("button");
            btn.className = "btn small ghost";
            btn.type = "button";
            btn.style.marginLeft = "8px";
            btn.textContent = "下载";
            btn.onclick = () => downloadModel(m, btn);
            row.appendChild(btn);
            if (m.note) {
              const note = document.createElement("div");
              note.className = "muted";
              note.style.fontSize = "11px";
              note.textContent = m.note;
              row.appendChild(note);
            }
          } else {
            const none = document.createElement("span");
            none.className = "muted";
            none.textContent = " · 无公开源（本机定制）";
            row.appendChild(none);
          }
          item.appendChild(row);
        }
      }
      box.appendChild(item);
    }
  } catch (e) {
    box.textContent = "检查失败：" + e;
  }
}

if ($("setOutputPick")) {
  $("setOutputPick").onclick = async () => {
    try {
      const dir = await api.call("pick_folder");
      if (dir) $("setOutputDir").value = dir;
    } catch (e) {
      toast(String(e), true);
    }
  };
}
$("btnSettingsSave").onclick = async () => {
  try {
    await saveSettings(true);
  } catch (e) {
    toast(String(e), true);
  }
};
