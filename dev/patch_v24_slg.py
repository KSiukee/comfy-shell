import re
proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- api.rs ----------------
p = proj + r"\src-tauri\src\api.rs"
s = open(p, encoding="utf-8").read()
s = s.replace('''    #[serde(default)]
    pub ref_images: Vec<String>,''', '''    #[serde(default)]
    pub ref_images: Vec<String>,
    #[serde(default)]
    pub slg: bool,''', 1)

anchor = '''    let (lora_name, _) = lora_node(&model_src, req, &p, turbo);'''
insert = '''    // MOSS official guidance: skip layer 22 on the negative branch (SLG)
    if req.slg && matches!(req.profile.as_str(), "moss" | "mossbase") {
        g.insert(
            "22".into(),
            json!({"class_type": "SkipLayerGuidanceDiT", "inputs": {
                "model": [model_src.clone(), 0], "double_layers": "", "single_layers": "22",
                "scale": 3.0, "start_percent": 0.01, "end_percent": 1.0, "rescaling_scale": 0.0}}),
        );
        model_src = "22".into();
    }
'''
if anchor in s:
    s = s.replace(anchor, insert + anchor, 1)
    print("slg graph ok")
else:
    print("slg anchor MISSING")
open(p, "w", encoding="utf-8", newline="\n").write(s)

# eval GenReq literal
p = proj + r"\src-tauri\src\main.rs"
m = open(p, encoding="utf-8").read()
m = m.replace('''                ref_images: Vec::new(),
            };''', '''                ref_images: Vec::new(),
                slg: false,
            };''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(m)
print("eval slg ok")

# ---------------- index.html ----------------
p = proj + r"\web\index.html"
h = open(p, encoding="utf-8").read()
h = h.replace('''      <div class="field pro-only">
        <label class="check"><input type="checkbox" id="turbo" /> 加速（turbo LoRA，8 步）</label>
      </div>''', '''      <div class="field pro-only">
        <label class="check"><input type="checkbox" id="turbo" /> 加速（turbo LoRA，8 步）</label>
      </div>

      <div class="field pro-only" id="slgField" hidden>
        <label class="check"><input type="checkbox" id="mossSlg" checked /> SLG（MOSS 官方：skip 层 22，约慢 25%，细节更实）</label>
      </div>''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(h)
print("html slg ok")

# ---------------- app.js ----------------
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")
j = j.replace('''  turboBox.parentElement.hidden = !p.turboLora;''',
'''  turboBox.parentElement.hidden = !p.turboLora;
  const mossy = (p.id || "").startsWith("moss");
  $("slgField").hidden = !mossy;
  if (!mossy) $("mossSlg").checked = false;''', 1)
j = j.replace('''    lora2Strength: Number($("lora2Strength").value),''',
'''    lora2Strength: Number($("lora2Strength").value),
    slg: !!$("mossSlg").checked,''', 1)
j = j.replace('''    if (r.lora2 && [...$("lora2").options].some((o) => o.value === r.lora2)) $("lora2").value = r.lora2;''',
'''    if (r.lora2 && [...$("lora2").options].some((o) => o.value === r.lora2)) $("lora2").value = r.lora2;
    if (r.slg !== undefined) $("mossSlg").checked = !!r.slg;''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js slg ok")
