proj = r"D:\deepseek-files\ai-tools\comfy-shell"
p = proj + r"\src-tauri\src\enhance.rs"
s = open(p, encoding="utf-8").read()

s = s.replace('''const ASSIST_SYS: &str = r#"You are a small assistant inside an image generation app. You can call exactly two tools:
- translate: translate the user's Chinese into an English image prompt fragment.
- expand: rewrite an English fragment into a richer image prompt (lighting, lens, style, details).
Reply with ONE JSON object per turn and nothing else:
{"tool":"translate","text":"中文"}
{"tool":"expand","prompt":"english fragment"}
{"tool":"final","prompt":"final english prompt","note":"one short Chinese sentence"}
Rules: at most 3 tool calls total. When the prompt is ready, answer with "final". Never invent image content the user did not ask for."#;''',
'''const ASSIST_SYS: &str = r#"You are a small assistant inside an image generation app. Tools:
- translate: translate Chinese into an English image prompt fragment.
- expand: rewrite an English fragment into a richer prompt (lighting, lens, style, details).
- pose: search the local mannequin pose library; argument "query" in English or Chinese, e.g. "sitting on the floor".
- analyze: describe the image the user attached (no argument).
Reply with ONE JSON object per turn and nothing else:
{"tool":"translate","text":"中文"}
{"tool":"expand","prompt":"english fragment"}
{"tool":"pose","query":"sitting"}
{"tool":"analyze"}
{"tool":"final","prompt":"final english prompt","note":"one short Chinese sentence"}
Rules: at most 4 tool calls total. When the prompt is ready, answer "final". For pose requests call pose first, then weave its description into the prompt. Never invent content the user did not ask for."#;''', 1)

s = s.replace("pub fn assist(port: u16, request: &str) -> Result<(String, String, Vec<AssistStep>), String> {",
              "pub fn assist(port: u16, request: &str, image: Option<&str>) -> Result<(String, String, Vec<AssistStep>), String> {", 1)

s = s.replace('''            "expand" => {''', '''            "pose" => {
                let q = action.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let out = pose_search(port, q);
                steps.push(AssistStep { tool: "pose".into(), output: out.clone() });
                convo.push_str(&format!("\\n工具 pose 结果：{}", out));
            }
            "analyze" => {
                let out = match image {
                    Some(img) => analyze(port, img).unwrap_or_else(|e| format!("分析失败：{e}")),
                    None => "用户没有附参考图".to_string(),
                };
                steps.push(AssistStep { tool: "analyze".into(), output: out.clone() });
                convo.push_str(&format!("\\n工具 analyze 结果：{}", out));
            }
            "expand" => {''', 1)

helper = '''
fn pose_search(port: u16, query: &str) -> String {
    let _ = port;
    let root = std::env::var("COMFY_ROOT").unwrap_or_default();
    let path = std::path::PathBuf::from(root)
        .join("ComfyUI")
        .join("input")
        .join("shisei")
        .join("catalog.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        return "姿势库不可用".to_string();
    };
    let Ok(list) = serde_json::from_str::<Vec<serde_json::Value>>(&text) else {
        return "姿势库解析失败".to_string();
    };
    let q = query.to_lowercase();
    let mut hits: Vec<(String, String)> = Vec::new();
    for p in &list {
        let hay = format!(
            "{} {} {} {} {}",
            p.get("id").and_then(|v| v.as_str()).unwrap_or(""),
            p.get("name_cn").and_then(|v| v.as_str()).unwrap_or(""),
            p.get("name_en").and_then(|v| v.as_str()).unwrap_or(""),
            p.get("class_cn").and_then(|v| v.as_str()).unwrap_or(""),
            p.get("desc_en").and_then(|v| v.as_str()).unwrap_or("")
        )
        .to_lowercase();
        if !q.is_empty() && hay.contains(&q) {
            hits.push((
                p.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                p.get("desc_en").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            ));
        }
        if hits.len() >= 3 {
            break;
        }
    }
    if hits.is_empty() {
        return "没有匹配的姿势".to_string();
    }
    hits.iter()
        .map(|(id, d)| format!("{id}: {d}"))
        .collect::<Vec<_>>()
        .join("\\n")
}

'''
s = s.replace("/// A tiny tool loop:", helper + "/// A tiny tool loop:", 1)
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("enhance ok")

# main.rs: image arg + pass COMFY_ROOT env
p = proj + r"\src-tauri\src\main.rs"
m = open(p, encoding="utf-8").read()
m = m.replace('''async fn assist(app: tauri::AppHandle, request: String) -> Result<serde_json::Value, String> {''',
'''async fn assist(app: tauri::AppHandle, request: String, image: Option<String>) -> Result<serde_json::Value, String> {''', 1)
m = m.replace('''        let state = handle.state::<AppState>();
        let port = state.settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".to_string());
        }
        let (prompt, note, steps) = enhance::assist(port, &request)?;''',
'''        let state = handle.state::<AppState>();
        let (port, root) = {
            let st = state.settings.lock().unwrap();
            (st.port, st.comfy_root.clone())
        };
        std::env::set_var("COMFY_ROOT", &root);
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".to_string());
        }
        let (prompt, note, steps) = enhance::assist(port, &request, image.as_deref())?;''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(m)
print("main ok")

# app.js: pass image + hint
p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")
j = j.replace('const res = await invoke("assist", { request });',
              'const res = await invoke("assist", { request, image: state.refs[0] || null });', 1)
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok")
