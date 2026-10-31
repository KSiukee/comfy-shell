import re
proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- enhance.rs: structured analysis ----------------
p = proj + r"\src-tauri\src\enhance.rs"
s = open(p, encoding="utf-8").read()

analyze = '''
const ANALYZE_PROMPT: &str = r#"You are an image analyst for AI image generation.
Look at the image and return ONLY one JSON object, no markdown, no extra text:
{"pose":"...","expression":"...","clothing":"...","accessories":"...","background":"...","style":"...","colors":"...","lighting":"...","mood":"...","prompt":"..."}
Rules:
- every value is a short English phrase (3-20 words), concrete and visual, no full sentences
- pose: body position, camera angle, framing
- expression: face, gaze, emotion shown
- clothing: garments, fabric, fit
- accessories: jewelry, bags, glasses, props (empty string if none)
- background: setting, environment, depth
- style: photo/illustration/3d, genre, artist-ish look
- colors: dominant palette and color mood
- lighting: light source, direction, hardness, time of day
- mood: atmosphere and feeling
- prompt: one detailed English prompt combining all fields, comma separated, under 90 words
- if something is not visible, use ""
"#;

/// Detailed, category-sorted description of one input image.
pub fn analyze(port: u16, image: &str) -> Result<String, String> {
    let wf = json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": image}},
        "2": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": "qwen3vl_8b_int8_convrot.safetensors",
                         "type": "qwen_image", "device": "default"}},
        "3": {"class_type": "TextGenerate",
              "inputs": {"clip": ["2", 0], "prompt": ANALYZE_PROMPT, "image": ["1", 0],
                         "max_length": 700, "sampling_mode": "off"}},
        "4": {"class_type": "PreviewAny", "inputs": {"source": ["3", 0]}},
    });
    let outputs = api::run_raw(port, wf, 600)?;
    let text = extract_text(outputs)?;
    Ok(extract_json(&text))
}

fn extract_json(text: &str) -> String {
    let t = text.trim().trim_matches('`').trim();
    if let (Some(a), Some(b)) = (t.find('{'), t.rfind('}')) {
        if b > a {
            return t[a..=b].to_string();
        }
    }
    t.to_string()
}

'''
anchor = "/// Captions a batch of input images"
if "pub fn analyze(" not in s:
    s = s.replace(anchor, analyze + anchor, 1)
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("enhance ok", "pub fn analyze(" in s)

# ---------------- main.rs: command ----------------
p = proj + r"\src-tauri\src\main.rs"
m = open(p, encoding="utf-8").read()
cmd = '''
#[tauri::command]
async fn analyze_image(app: tauri::AppHandle, image: String) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let port = state.settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".to_string());
        }
        enhance::analyze(port, &image)
    })
    .await
    .map_err(|e| format!("解析任务失败: {e}"))?
}
'''
anchor = "#[tauri::command]\nasync fn train_kohya_files"
if "async fn analyze_image" not in m:
    m = m.replace(anchor, cmd + "\n" + anchor, 1)
m = m.replace("            train_lora_files,", "            analyze_image,\n            train_lora_files,", 1)
open(p, "w", encoding="utf-8", newline="\n").write(m)
print("main ok", "async fn analyze_image" in m)
