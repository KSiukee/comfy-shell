proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- api.rs: layout passthrough ----------------
p = proj + r"\src-tauri\src\api.rs"
s = open(p, encoding="utf-8").read()
s = s.replace('''    #[serde(default)]
    pub slg_layers: String,''', '''    #[serde(default)]
    pub slg_layers: String,
    #[serde(default)]
    pub layout: Option<String>,''', 1)

# positive text uses layout when present
anchor = "pub fn build_graph(req: &GenReq, turbo: bool) -> Result<Value, String> {"
if "let pos_text" not in s:
    s = s.replace(anchor, anchor + '''
    let pos_text = req.layout.clone().unwrap_or_else(|| req.prompt.clone());''', 1)

s = s.replace('"text": req.prompt', '"text": pos_text.clone()')
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("api ok", "let pos_text" in s)

# ---------------- main.rs eval literal ----------------
p = proj + r"\src-tauri\src\main.rs"
m = open(p, encoding="utf-8").read()
m = m.replace('''                slg_layers: "22".to_string(),''', '''                slg_layers: "22".to_string(),
                layout: None,''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(m)
print("main ok")

# ---------------- enhance.rs: extras in two-pass ----------------
p = proj + r"\src-tauri\src\enhance.rs"
e = open(p, encoding="utf-8").read()
e = e.replace(
    '''{{"pose":"","expression":"","clothing":"","accessories":"","background":"","style":"","colors":"","lighting":"","mood":"","prompt":""}}''',
    '''{{"pose":"","expression":"","clothing":"","accessories":"","background":"","style":"","colors":"","lighting":"","mood":"","extras":[{{"label":"","value":""}}],"prompt":""}}''',
    1,
)
e = e.replace(
    '''Rules: values are short English phrases; "prompt" is one comma-separated English prompt under 90 words combining all fields; empty string when unknown.''',
    '''Rules: values are short English phrases; "prompt" is one comma-separated English prompt under 90 words combining all fields; empty string when unknown.
"extras" holds elements that fit no fixed field, such as weather, vehicles, animals, food, time of day, signage. Use a short English label and value, e.g. {"label":"weather","value":"light rain"}; return [] if nothing extra.''',
    1,
)
open(p, "w", encoding="utf-8", newline="\n").write(e)
print("enhance ok")
