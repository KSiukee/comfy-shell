import re
proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- backend: poses_catalog ----------------
p = proj + r"\src-tauri\src\main.rs"
s = open(p, encoding="utf-8").read()
cmd = '''
#[tauri::command]
fn poses_catalog(state: State<AppState>) -> Result<String, String> {
    let root = state.settings.lock().unwrap().comfy_root.clone();
    let path = std::path::PathBuf::from(root)
        .join("ComfyUI")
        .join("input")
        .join("shisei")
        .join("catalog.json");
    std::fs::read_to_string(&path).map_err(|e| format!("找不到姿势库（{}）：{e}", path.display()))
}
'''
anchor = "#[tauri::command]\nfn read_input_image"
if anchor in s and "fn poses_catalog" not in s:
    s = s.replace(anchor, cmd + "\n" + anchor, 1)
s = s.replace("            read_input_image,", "            read_input_image,\n            poses_catalog,", 1)
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("backend ok", "fn poses_catalog" in s)

# ---------------- HTML ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''        <label>参考图 <span class="muted" id="refHint"></span></label>''',
'''        <label class="row tight">
          <span>参考图 <span class="muted" id="refHint"></span></span>
          <span class="spacer"></span>
          <button class="btn small ghost" id="btnPoses" title="白模姿势库（Shisei，CC0）">🕺 姿势库</button>
        </label>''', 1)

modal = '''
  <!-- pose library -->
  <div class="modal" id="poseModal" hidden>
    <div class="modal-card" style="width: 1000px">
      <h3>白模姿势库 <span class="muted" style="font-size:12px">Shisei · CC0 · 280 组 × 5 机位</span></h3>
      <div class="row tight">
        <input id="poseSearch" placeholder="搜编号/名称，如 坐姿、P001、sitting" style="flex: 2 1 0" />
        <select id="poseClass" style="flex: 1 1 0"><option value="">全部类别</option></select>
        <select id="poseAngle"><option value="front">正面</option><option value="diag">斜侧</option><option value="side">侧面</option><option value="above">俯视</option><option value="below">仰视</option></select>
        <span class="muted mono" id="poseCount"></span>
        <span class="spacer"></span>
        <button class="btn ghost small" id="poseClose">关闭</button>
      </div>
      <div class="hint">点卡片 = 加入参考图；点卡片右下「词」= 复制该姿势的英文描述</div>
      <div class="pose-grid" id="poseGrid"></div>
    </div>
  </div>
'''
h = h.replace("  <!-- crop picker -->", modal + "\n  <!-- crop picker -->", 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html ok")

# ---------------- CSS ----------------
p = proj + r"\web\style.css"
c = open(p, encoding="utf-8").read()
c += '''
.pose-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)); gap: 8px; max-height: 62vh; overflow: auto; padding-right: 4px; }
.pose-card { position: relative; border: 1px solid #2a2f3a; border-radius: 8px; overflow: hidden; cursor: pointer; background: #14171e; }
.pose-card:hover { border-color: #5d7bf0; }
.pose-card img { width: 100%; aspect-ratio: 3 / 4; object-fit: cover; display: block; background: #0f1116; }
.pose-card .pc-name { font-size: 11px; padding: 4px 6px; color: #aab2c0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.pose-card .pc-word { position: absolute; right: 4px; bottom: 22px; font-size: 10px; padding: 1px 5px; border-radius: 4px; background: #0009; color: #cdd6e4; }
.pose-card .pc-word:hover { background: #5d7bf0; }
'''
open(p, "w", encoding="utf-8", newline="\n").write(c)
print("css ok")

# ---------------- JS ----------------
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")

code = '''// ---------------------------------------------------------------- pose library
let poseCatalog = null;
let poseShown = 0;
const poseCache = new Map();

async function openPoses() {
  if (!poseCatalog) {
    try {
      poseCatalog = JSON.parse(await invoke("poses_catalog"));
    } catch (e) {
      toast(String(e), true);
      return;
    }
    const classes = [...new Set(poseCatalog.map((p) => p.class_cn).filter(Boolean))].sort();
    $("poseClass").innerHTML =
      `<option value="">全部类别</option>` + classes.map((c) => `<option value="${c}">${c}</option>`).join("");
  }
  poseShown = 0;
  $("poseGrid").innerHTML = "";
  $("poseModal").hidden = false;
  renderPoses();
}

function poseFiltered() {
  const q = $("poseSearch").value.trim().toLowerCase();
  const cls = $("poseClass").value;
  return poseCatalog.filter((p) => {
    if (cls && p.class_cn !== cls) return false;
    if (!q) return true;
    const hay = `${p.id} ${p.name_cn || ""} ${p.name_en || ""} ${p.class_cn || ""} ${p.desc_en || ""}`.toLowerCase();
    return hay.includes(q);
  });
}

function renderPoses() {
  const list = poseFiltered();
  const grid = $("poseGrid");
  $("poseCount").textContent = `${Math.min(poseShown, list.length)} / ${list.length}`;
  if (!list.length) {
    grid.innerHTML = '<div class="lib-empty">没有匹配的姿势</div>';
    return;
  }
  const angle = $("poseAngle").value;
  const end = Math.min(list.length, poseShown || 80);
  poseShown = end;
  grid.innerHTML = "";
  for (const p of list.slice(0, end)) {
    const file = (p.views || {})[angle] || (p.views || {}).front;
    if (!file) continue;
    const d = document.createElement("div");
    d.className = "pose-card";
    d.innerHTML = `<img data-file="${file}" alt="${p.id}"><div class="pc-name">${p.id} · ${p.name_cn || p.name_en || ""}</div><span class="pc-word" title="复制英文描述">词</span>`;
    const img = d.querySelector("img");
    d.querySelector(".pc-word").onclick = (e) => {
      e.stopPropagation();
      navigator.clipboard.writeText(p.desc_en || p.name_en || "").then(() => toast("已复制姿势描述"));
    };
    d.onclick = async () => {
      try {
        let url = poseCache.get(file);
        if (!url) {
          url = await invoke("read_input_image", { name: `shisei/line/${file}` });
          poseCache.set(file, url);
        }
        await addRefFromDataUrl(url, `pose_${p.id}_${angle}`);
        $("poseModal").hidden = true;
      } catch (e) {
        toast(String(e), true);
      }
    };
    grid.appendChild(d);
  }
  lazyLoadPoses();
}

let poseObserver = null;
function lazyLoadPoses() {
  if (!poseObserver) {
    poseObserver = new IntersectionObserver((entries) => {
      for (const en of entries) {
        if (!en.isIntersecting) continue;
        const img = en.target;
        const file = img.dataset.file;
        if (!file || img.src) continue;
        invoke("read_input_image", { name: `shisei/line/${file}` })
          .then((url) => {
            img.src = url;
            poseCache.set(file, url);
          })
          .catch(() => {});
        poseObserver.unobserve(img);
      }
    }, { root: $("poseGrid"), rootMargin: "300px" });
  }
  document.querySelectorAll("#poseGrid img[data-file]").forEach((img) => {
    if (!img.src) poseObserver.observe(img);
  });
}

$("btnPoses").onclick = openPoses;
$("poseClose").onclick = () => ($("poseModal").hidden = true);
$("poseSearch").oninput = () => { poseShown = 0; renderPoses(); };
$("poseClass").onchange = () => { poseShown = 0; renderPoses(); };
$("poseAngle").onchange = () => { poseShown = 0; renderPoses(); };
$("poseGrid").addEventListener("scroll", () => {
  const g = $("poseGrid");
  if (g.scrollTop + g.clientHeight < g.scrollHeight - 200) return;
  if (poseShown < poseFiltered().length) {
    poseShown += 80;
    renderPoses();
  }
});

'''
j = j.replace("// ---------------------------------------------------------------- region marker", code + "// ---------------------------------------------------------------- region marker", 1)

# fourth preset: Base + UltraReal
j = j.replace('''  polished: {
    model: "moss",''', '''  raw: {
    model: "mossbase",
    lora: "ultra_real_moss_v2.safetensors",
    strength: 0.7,
    lora2: "",
    lora2Strength: 0.8,
    steps: 50,
    cfg: 3,
    slg: true,
  },
  polished: {
    model: "moss",''', 1)
j = j.replace('''            <option value="polished">精致人像（SFT + Beauty 0.7）</option>''', '''            <option value="polished">精致人像（SFT + Beauty 0.7）</option>''')
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok")
