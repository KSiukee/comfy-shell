import re
proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- api.rs: configurable SLG layers ----------------
p = proj + r"\src-tauri\src\api.rs"
s = open(p, encoding="utf-8").read()
s = s.replace('''    #[serde(default)]
    pub slg: bool,''', '''    #[serde(default)]
    pub slg: bool,
    #[serde(default = "slg_default_layers")]
    pub slg_layers: String,''', 1)
s = s.replace('''pub struct GenReq {''', '''fn slg_default_layers() -> String {
    "22".to_string()
}

pub struct GenReq {''', 1)
s = s.replace('''"model": [model_src.clone(), 0], "double_layers": "", "single_layers": "22",''',
              '''"model": [model_src.clone(), 0], "double_layers": "", "single_layers": req.slg_layers.clone(),''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("api slg layers ok")

# eval literal
p = proj + r"\src-tauri\src\main.rs"
m = open(p, encoding="utf-8").read()
m = m.replace('''                slg: false,
            };''', '''                slg: false,
                slg_layers: "22".to_string(),
            };''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(m)
print("main slg layers ok")

# ---------------- index.html ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''      <div class="field pro-only" id="slgField" hidden>
        <label class="check"><input type="checkbox" id="mossSlg" checked /> SLG（MOSS 官方：skip 层 22，约慢 25%，细节更实）</label>
      </div>''', '''      <div class="field pro-only" id="slgField" hidden>
        <label class="check"><input type="checkbox" id="mossSlg" checked /> SLG（MOSS 官方：约慢 25%，细节更实）</label>
        <select id="slgTier">
          <option value="22">标准（skip 22）</option>
          <option value="22,24,26">强（skip 22/24/26）</option>
        </select>
      </div>''', 1)
h = h.replace('''        <button class="btn icon" id="btnNewModel"''', '''        <button class="btn small ghost" id="btnMossPreset" hidden title="MOSS 官方采样：50 步 / CFG 3 / 1024 / SLG 22">官方预设</button>
        <button class="btn icon" id="btnNewModel"''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html ok")

# ---------------- app.js ----------------
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")
j = j.replace('''  $("slgField").hidden = !mossy;''', '''  $("slgField").hidden = !mossy;
  $("btnMossPreset").hidden = !mossy;''', 1)
j = j.replace('''    slg: !!$("mossSlg").checked,''', '''    slg: !!$("mossSlg").checked,
    slgLayers: $("slgTier").value,''', 1)
j = j.replace('''$("mossSlg").onchange = () => (state.slgTouched = true);''',
'''$("mossSlg").onchange = () => (state.slgTouched = true);
$("btnMossPreset").onclick = () => {
  $("steps").value = 50;
  $("cfg").value = 3;
  $("width").value = 1024;
  $("height").value = 1024;
  $("mossSlg").checked = true;
  $("slgTier").value = "22";
  state.slgTouched = true;
  toast("已套用 MOSS 官方采样：50 步 / CFG 3 / 1024² / SLG 22");
};''', 1)

# generic MOSS LoRA family rule (beauty / myjc / community)
j = j.replace('''  { test: /MOSS-Image-NSFW/i,''',
'''  { test: /moss/i, models: ["moss", "mossbase"], rank: 3, strength: 0.8, note: "MOSS 生态 LoRA" },
  { test: /MOSS-Image-NSFW/i,''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok")
