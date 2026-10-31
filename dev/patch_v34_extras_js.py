proj = r"D:\deepseek-files\ai-tools\comfy-shell"
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")

# show layout button only for the MOSS family
j = j.replace('''  $("btnMossPreset").hidden = !mossy;''', '''  $("btnMossPreset").hidden = !mossy;
  $("btnLayout").hidden = !mossy;''', 1)

# extras state + renderer
extras = '''let anaExtras = [];

function renderExtras(list) {
  anaExtras = (list || []).map((x) => ({ label: x.label || "", value: x.value || "" }));
  paintExtras();
}

function paintExtras() {
  const box = $("anaExtras");
  box.innerHTML = "";
  anaExtras.forEach((ex, i) => {
    const row = document.createElement("div");
    row.className = "xrow";
    const label = document.createElement("input");
    label.value = ex.label;
    label.placeholder = "分类（如 weather）";
    label.oninput = () => (ex.label = label.value);
    const value = document.createElement("input");
    value.value = ex.value;
    value.placeholder = "内容（如 light rain）";
    value.oninput = () => (ex.value = value.value);
    const del = document.createElement("button");
    del.className = "btn small ghost";
    del.textContent = "×";
    del.onclick = () => {
      anaExtras.splice(i, 1);
      paintExtras();
    };
    row.append(label, value, del);
    box.appendChild(row);
  });
}

$("anaAddExtra").onclick = () => {
  anaExtras.push({ label: "", value: "" });
  paintExtras();
};

'''
j = j.replace("async function runAnalyze() {", extras + "async function runAnalyze() {", 1)

# fill extras after parsing
j = j.replace('''    $("anaPrompt").value = data.prompt || "";''', '''    renderExtras(data.extras || []);
    $("anaPrompt").value = data.prompt || "";''', 1)

# compose extras into the prompt
j = j.replace('''  const parts = ANA_FIELDS.map(([k]) => ($("ana_" + k)?.value || "").trim()).filter(Boolean);''',
'''  const parts = ANA_FIELDS.map(([k]) => ($("ana_" + k)?.value || "").trim()).filter(Boolean);
  for (const ex of anaExtras) {
    const v = (ex.value || "").trim();
    if (v) parts.push(v);
  }''', 1)

open(p, "w", encoding="utf-8", newline="\n").write(j)
print("extras ok")
