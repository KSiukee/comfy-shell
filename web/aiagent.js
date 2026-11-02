// AI 配置助手（L1）：体检 → Pi 出计划 → 用户逐条确认 → 壳执行。
// 口径：AI 只能碰 ComfyUI 与设置；项目/图库不给摸；不许改显影程序文件（工具里也没有）。

let aiPlanData = null;

function aiSetText(id, text) {
  const el = $(id);
  if (el) el.textContent = text;
}

async function refreshAiSettings() {
  try {
    const s = state.settings || (await api.call("settings_get"));
    if ($("aiProvider")) $("aiProvider").value = s.aiProvider || "deepseek";
    if ($("aiModel")) $("aiModel").value = s.aiModel || "deepseek-chat";
  } catch {}
  const st = await api.call("ai_engine_status").catch(() => null);
  if (st && st.found) {
    aiSetText("aiEngineState", `已找到（${st.source}）`);
    if ($("aiEngineState")) $("aiEngineState").title = st.node + "\n" + st.cli;
  } else {
    aiSetText("aiEngineState", "未找到：需要 Node + Pi，可在终端 npm i -g @earendil-works/pi-coding-agent");
  }
  const has = await api.call("ai_key_status").catch(() => false);
  aiSetText("aiKeyState", has ? "已保存（DPAPI 加密）" : "未保存");
  if ($("aiKey")) $("aiKey").value = "";
}

async function saveAiKey() {
  const key = ($("aiKey") ? $("aiKey").value : "").trim();
  if (!key) return toast("先填 key", true);
  try {
    await api.call("ai_key_set", { key });
    toast("key 已保存（加密存在本机）");
    await refreshAiSettings();
  } catch (e) {
    toast(String(e), true);
  }
}

async function clearAiKey() {
  if (!confirm("清除已保存的 API key？")) return;
  try {
    await api.call("ai_key_clear");
    toast("已清除");
    await refreshAiSettings();
  } catch (e) {
    toast(String(e), true);
  }
}

function aiFmtEnv(r) {
  if (!r) return "";
  const lines = [];
  lines.push("== 体检 ==");
  for (const s of r.summary || []) lines.push("· " + s);
  lines.push(`端口 ${r.server ? r.server.port : "?"} · 模型 ${r.models_total} 个 · 自定义节点 ${r.custom_nodes} 个`);
  const miss = (r.profiles || []).filter((p) => (p.missing || []).length);
  if (miss.length) {
    lines.push("缺模型的档位：");
    for (const p of miss) lines.push(`  - ${p.label}: ${p.missing.join("、")}`);
  }
  if (r.logs) {
    lines.push("== 日志尾 ==");
    lines.push(r.logs.split("\n").slice(-12).join("\n"));
  }
  return lines.join("\n");
}

async function runAiEnvCheck() {
  try {
    const r = await api.call("ai_env_check");
    const out = $("aiEnvOut");
    if (out) {
      out.hidden = false;
      out.textContent = aiFmtEnv(r);
    }
    aiSetText("aiEnvHint", (r.summary || [])[0] || "体检完成");
    return r;
  } catch (e) {
    toast(String(e), true);
    return null;
  }
}

function openAiAssistant() {
  const m = $("aiModal");
  if (!m) return;
  m.hidden = false;
  if ($("aiPlanOut")) $("aiPlanOut").innerHTML = "";
  aiPlanData = null;
  runAiEnvCheck();
}

function aiStepLabel(step) {
  return step.title || step.action || "步骤";
}

function aiRenderPlan(data) {
  const box = $("aiPlanOut");
  if (!box) return;
  box.innerHTML = "";
  const plan = (data && data.plan) || {};
  const head = document.createElement("div");
  head.className = "ai-plan-summary";
  head.textContent = plan.summary || "（模型没有给总结）";
  box.appendChild(head);

  const steps = Array.isArray(plan.steps) ? plan.steps : [];
  if (!steps.length) {
    const ok = document.createElement("div");
    ok.className = "hint";
    ok.textContent = "没有需要执行的动作。";
    box.appendChild(ok);
    return;
  }
  steps.forEach((step, i) => {
    const card = document.createElement("div");
    card.className = "ai-step";
    const title = document.createElement("div");
    title.className = "ai-step-title";
    title.textContent = `${i + 1}. ${aiStepLabel(step)}`;
    const why = document.createElement("div");
    why.className = "ai-step-why";
    why.textContent = step.why || "";
    const args = document.createElement("div");
    args.className = "ai-step-args mono";
    args.textContent = step.args && Object.keys(step.args).length ? JSON.stringify(step.args) : "";
    card.append(title, why, args);

    if (step.action === "advice") {
      const note = document.createElement("div");
      note.className = "ai-advice";
      note.textContent = (step.args && step.args.text) || "需要你手动处理";
      card.appendChild(note);
    } else {
      const btn = document.createElement("button");
      btn.className = "btn small";
      btn.textContent = "执行这一步";
      btn.onclick = () => aiExecuteStep(step, btn);
      card.appendChild(btn);
    }
    box.appendChild(card);
  });
}

async function aiPlan() {
  const req = ($("aiRequest") ? $("aiRequest").value : "").trim();
  if (!req) return toast("先说一句要它做什么", true);
  const btn = $("aiPlanBtn");
  btn.disabled = true;
  aiSetText("aiPlanState", "体检 + 让 Pi 出方案…");
  try {
    const data = await api.call("ai_plan", { request: req });
    aiPlanData = data;
    aiRenderPlan(data);
    aiSetText("aiPlanState", `完成 · ${Math.round((data.ms || 0) / 1000)} 秒`);
  } catch (e) {
    aiSetText("aiPlanState", "失败");
    toast(String(e), true);
  } finally {
    btn.disabled = false;
  }
}

async function aiExecuteStep(step, btn) {
  const action = step.action;
  const args = step.args || {};
  const desc = `${aiStepLabel(step)}\n${step.why || ""}\n${JSON.stringify(args)}`;
  if (!confirm(`执行这一步？\n\n${desc}`)) return;
  btn.disabled = true;
  btn.textContent = "执行中…";
  try {
    if (action === "set_settings") {
      const cur = state.settings || (await api.call("settings_get"));
      const next = Object.assign({}, cur, {});
      if (args.comfyRoot) next.comfyRoot = String(args.comfyRoot);
      if (args.outputDir !== undefined) next.outputDir = String(args.outputDir);
      if (args.port) next.port = Number(args.port);
      await api.call("settings_set", { settings: next });
      state.settings = await api.call("settings_get");
      await refreshSettingsInfo();
      toast("设置已更新");
    } else if (action === "start_comfy") {
      await api.call("server_start");
      toast("ComfyUI 已启动");
    } else if (action === "stop_comfy") {
      await api.call("server_stop");
      toast("ComfyUI 已停止");
    } else if (action === "restart_comfy") {
      await api.call("server_stop");
      await api.call("server_start");
      toast("ComfyUI 已重启");
    } else if (action === "install_comfy") {
      const mode = args.mode === "portable" ? "portable" : "source";
      const url =
        mode === "portable"
          ? "https://github.com/Comfy-Org/ComfyUI/releases/latest/download/ComfyUI_windows_portable_nvidia.7z"
          : "https://github.com/comfyanonymous/ComfyUI/archive/refs/heads/master.zip";
      if (!confirm(`开始${mode === "portable" ? "完整安装（约 2GB）" : "源码更新"}？`)) return;
      const p = await api.call("comfy_install", { mode, url, threads: 6 });
      toast("完成：" + p);
      state.settings = await api.call("settings_get");
      await refreshSettingsInfo();
    } else if (action === "open_folder") {
      await api.call("open_folder", { path: String(args.path || "") });
    } else if (action === "install_node") {
      const url = String(args.url || "");
      if (!/^https:\/\/github\.com\/[^/]+\/[^/]+/.test(url)) {
        throw new Error("只支持 https://github.com/owner/repo 的仓库");
      }
      if (!confirm(`安装自定义节点？\n\n${url}\n\n会执行：\ngit clone --depth 1 到 ComfyUI/custom_nodes\n然后 pip install -r requirements.txt（如果有）\n\n第三方节点会执行任意 Python，确认仓库可信再继续。`)) return;
      btn.textContent = "安装中…";
      const p = await api.call("node_install", { url });
      toast("节点已装：" + p + "（记得重启 ComfyUI）");
    } else if (action === "generate_test") {
      const prof = args.profile || "flux3";
      const p = (state.profiles || []).find((x) => x.id === prof);
      const w = Number(args.width) || 1024;
      const h = Number(args.height) || 1024;
      if (!confirm(`跑一张验收图？\n\n尺寸 ${w}×${h}\n模型 ${p ? p.label : prof}\n约 1 分钟、会占用显卡。`)) return;
      btn.textContent = "生成中…";
      const req = {
        profile: prof,
        mode: "t2i",
        prompt: args.prompt || "a photo of a cat sitting by the window, soft natural light, 50mm",
        negative: "",
        width: w,
        height: h,
        steps: p ? p.steps : 16,
        cfg: p ? p.cfg : 1,
        seed: -1,
        project: (window.projectUI && window.projectUI.current()) || null,
      };
      if (typeof setBusy === "function") setBusy(true);
      try {
        const res = await api.call("generate", { req, turbo: false });
        const items = res.images.map((im) => ({
          path: im.path,
          filename: im.filename,
          width: im.width,
          height: im.height,
          label: res.label,
          seconds: res.seconds,
          ts: Date.now(),
          prompt: req.prompt,
          mode: req.mode,
          dataUrl: im.dataUrl,
        }));
        if (typeof mergeResults === "function") mergeResults(items);
        toast(`验收图完成：${Math.round(res.seconds)} 秒`);
      } finally {
        if (typeof setBusy === "function") setBusy(false);
      }
    } else {
      toast("这个动作还不支持：" + action, true);
      return;
    }
    btn.textContent = "已执行";
  } catch (e) {
    btn.disabled = false;
    btn.textContent = "重试";
    toast(String(e), true);
  }
}

async function downloadEngine() {
  if (
    !confirm(
      "下载便携 Node（约 35MB）并用 npm 安装 Pi（约 1–3 分钟），装到应用数据目录？\n装完会自动检测到引擎。"
    )
  )
    return;
  const btn = $("aiEngineDownload");
  const bar = $("aiBar");
  const fill = $("aiFill");
  const st = $("aiDlState");
  if (btn) {
    btn.disabled = true;
    btn.textContent = "安装中…";
  }
  if (bar) bar.hidden = false;
  if (fill) {
    fill.classList.add("pulse");
    fill.style.width = "35%";
  }
  if (st) {
    st.hidden = false;
    st.textContent = "准备下载 Node…";
  }
  try {
    const p = await api.call("ai_engine_install", {});
    toast("引擎已装好");
    if (st) st.textContent = "完成：" + p;
    await refreshAiSettings();
  } catch (e) {
    if (st) st.textContent = "失败：" + e;
    toast(String(e), true);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.textContent = "下载引擎…";
    }
    if (fill) fill.classList.remove("pulse");
    if (bar) bar.hidden = true;
  }
}

listen("node-progress", (ev) => {
  const p = ev.payload || {};
  aiSetText("aiPlanState", `${p.phase || "安装"} · ${(p.line || "").slice(0, 110)}`);
});

listen("ai-progress", (ev) => {
  const p = ev.payload || {};
  const st = $("aiDlState");
  const fill = $("aiFill");
  const parts = [p.phase || "处理中"];
  if (p.got) parts.push(`${(p.got / 1048576).toFixed(1)}MB / ${p.total ? (p.total / 1048576).toFixed(0) + "MB" : "?"}`);
  if (p.mbps) parts.push(`${p.mbps.toFixed(1)} MB/s`);
  if (p.line) parts.push(p.line.slice(0, 120));
  if (st) {
    st.hidden = false;
    st.textContent = parts.join(" · ");
  }
  if (fill && p.total) {
    fill.classList.remove("pulse");
    fill.style.width = Math.min(100, (p.got / p.total) * 100).toFixed(1) + "%";
  }
});

(() => {
  const bind = (id, fn) => {
    const el = $(id);
    if (el) el.onclick = fn;
  };
  bind("aiKeySave", saveAiKey);
  bind("aiKeyClear", clearAiKey);
  bind("aiEnvRun", runAiEnvCheck);
  bind("aiOpen", openAiAssistant);
  bind("aiPlanBtn", aiPlan);
  bind("aiClose", () => ($("aiModal").hidden = true));
  bind("aiEngineRecheck", refreshAiSettings);
  bind("aiEngineDownload", downloadEngine);
})();

window.aiAgent = { refresh: refreshAiSettings, open: openAiAssistant, env: runAiEnvCheck };
