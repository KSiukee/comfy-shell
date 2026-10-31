proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- main.rs command ----------------
p = proj + r"\src-tauri\src\main.rs"
m = open(p, encoding="utf-8").read()
cmd = '''
#[tauri::command]
async fn assist(app: tauri::AppHandle, request: String) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let port = state.settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".to_string());
        }
        let (prompt, note, steps) = enhance::assist(port, &request)?;
        Ok(serde_json::json!({
            "prompt": prompt,
            "note": note,
            "steps": steps.iter().map(|s| serde_json::json!({"tool": s.tool, "output": s.output})).collect::<Vec<_>>(),
        }))
    })
    .await
    .map_err(|e| format!("助手任务失败: {e}"))?
}
'''
anchor = "#[tauri::command]\nasync fn analyze_image"
if "async fn assist(" not in m:
    m = m.replace(anchor, cmd + "\n" + anchor, 1)
m = m.replace("            analyze_image,", "            assist,\n            analyze_image,", 1)
open(p, "w", encoding="utf-8", newline="\n").write(m)
print("main ok")

# ---------------- HTML ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''          <button class="btn small ghost" id="btnAnalyze"''',
'''          <button class="btn small ghost" id="btnAssist" title="本地小助手：中文需求 → 翻译 → 扩写 → 提示词">🤖 助手</button>
          <button class="btn small ghost" id="btnAnalyze"''', 1)

modal = '''
  <!-- local assistant -->
  <div class="modal" id="assistModal" hidden>
    <div class="modal-card" style="width: 720px">
      <h3>本地小助手 <span class="muted" style="font-size:12px">Qwen3.5-9B · 最多 3 步工具调用</span></h3>
      <textarea id="assistInput" rows="3" placeholder="用中文说需求，例如：雨夜街头穿红大衣的银发女孩，电影感，要能看到湿地面反光"></textarea>
      <div class="row tight">
        <button class="btn small primary" id="assistRun">运行</button>
        <span class="muted mono" id="assistState"></span>
        <span class="spacer"></span>
        <button class="btn ghost small" id="assistClose">关闭</button>
      </div>
      <div id="assistSteps" style="display:flex;flex-direction:column;gap:6px;max-height:280px;overflow:auto"></div>
      <textarea id="assistOut" rows="3" placeholder="最终提示词"></textarea>
      <div class="row tight">
        <button class="btn small primary" id="assistFill">填入提示词</button>
      </div>
    </div>
  </div>
'''
h = h.replace("  <!-- MOSS layout editor -->", modal + "\n  <!-- MOSS layout editor -->", 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html ok")

# ---------------- CSS ----------------
p = proj + r"\web\style.css"
c = open(p, encoding="utf-8").read()
c += '''
#assistSteps .astep { font-size: 12px; border: 1px solid #2a2f3a; border-radius: 6px; padding: 6px 8px; background: #12151c; }
#assistSteps .astep b { color: #8fb0ff; margin-right: 6px; }
#assistInput, #assistOut { background: #0f1116 !important; color: #e6e9ef !important; border: 1px solid #2a2f3a !important; border-radius: 6px; }
'''
open(p, "w", encoding="utf-8", newline="\n").write(c)
print("css ok")

# ---------------- JS ----------------
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")
code = '''// ---------------------------------------------------------------- local assistant
$("btnAssist").onclick = () => {
  $("assistModal").hidden = false;
};
$("assistClose").onclick = () => ($("assistModal").hidden = true);
$("assistFill").onclick = () => {
  const text = $("assistOut").value.trim();
  if (!text) return toast("还没有结果", true);
  $("prompt").value = text;
  $("assistModal").hidden = true;
  toast("已填入提示词");
};
$("assistRun").onclick = async () => {
  const request = $("assistInput").value.trim();
  if (!request) return toast("先写需求", true);
  const btn = $("assistRun");
  btn.disabled = true;
  $("assistState").textContent = "思考中…";
  $("assistSteps").innerHTML = "";
  try {
    const res = await invoke("assist", { request });
    const box = $("assistSteps");
    for (const s of res.steps || []) {
      const d = document.createElement("div");
      d.className = "astep";
      d.innerHTML = `<b>${s.tool}</b>${s.output}`;
      box.appendChild(d);
    }
    if (res.note) {
      const d = document.createElement("div");
      d.className = "astep";
      d.innerHTML = `<b>note</b>${res.note}`;
      box.appendChild(d);
    }
    $("assistOut").value = res.prompt || "";
    $("assistState").textContent = `完成 · ${(res.steps || []).length} 步`;
  } catch (e) {
    $("assistState").textContent = "";
    toast(String(e), true);
  } finally {
    btn.disabled = false;
  }
};

'''
j = j.replace("// ---------------------------------------------------------------- MOSS layout editor", code + "// ---------------------------------------------------------------- MOSS layout editor", 1)
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok")
