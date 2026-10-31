import re
proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- enhance.rs: two-pass analyze ----------------
p = proj + r"\src-tauri\src\enhance.rs"
s = open(p, encoding="utf-8").read()

two_pass = '''
const DESC_PROMPT: &str = r#"Describe this image in detail for an AI image generator.
Cover: camera framing and angle, subject's pose, facial expression, clothing and fabric, accessories, background and setting, style, dominant colors, lighting direction and quality, overall mood.
Write 120-180 words of plain English prose. No lists, no JSON, no markdown."#;

const SPLIT_SYS: &str = "You convert an image description into a strict JSON object. Output only JSON, never markdown.";

/// Two-stage analysis: vision model describes, text model sorts into fields.
pub fn analyze_two_pass(port: u16, image: &str) -> Result<String, String> {
    let wf1 = json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": image}},
        "2": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": "qwen3vl_8b_int8_convrot.safetensors",
                         "type": "qwen_image", "device": "default"}},
        "3": {"class_type": "TextGenerate",
              "inputs": {"clip": ["2", 0], "prompt": DESC_PROMPT, "image": ["1", 0],
                         "max_length": 420, "sampling_mode": "off"}},
        "4": {"class_type": "PreviewAny", "inputs": {"source": ["3", 0]}},
    });
    let desc = extract_text(api::run_raw(port, wf1, 600)?)?;
    let desc = desc.trim().to_string();
    if desc.is_empty() {
        return Err("视觉模型没给出描述".into());
    }

    let ask = format!(
        r#"Schema:
{{"pose":"","expression":"","clothing":"","accessories":"","background":"","style":"","colors":"","lighting":"","mood":"","prompt":""}}
Rules: values are short English phrases; "prompt" is one comma-separated English prompt under 90 words combining all fields; empty string when unknown.
Image description:
{}"#,
        desc
    );
    let wf2 = json!({
        "1": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": "qwen35_9b_heretic_int8_convrot.safetensors",
                         "type": "flux3", "device": "default"}},
        "2": {"class_type": "TextGenerate",
              "inputs": {"clip": ["1", 0], "prompt": ask, "max_length": 700,
                         "sampling_mode": "off", "system_prompt": SPLIT_SYS}},
        "3": {"class_type": "PreviewAny", "inputs": {"source": ["2", 0]}},
    });
    let raw = extract_text(api::run_raw(port, wf2, 300)?)?;
    let mut json_text = extract_json(&raw);
    // attach the prose so the UI can show it in the raw panel
    if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&json_text) {
        if let Some(obj) = v.as_object_mut() {
            obj.insert("_description".into(), serde_json::Value::String(desc));
        }
        if let Ok(s) = serde_json::to_string(&v) {
            json_text = s;
        }
    }
    Ok(json_text)
}

'''
anchor = "/// Detailed, category-sorted description of one input image."
if "fn analyze_two_pass" not in s:
    s = s.replace(anchor, two_pass + anchor, 1)
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("enhance ok")

# ---------------- main.rs: two_pass flag ----------------
p = proj + r"\src-tauri\src\main.rs"
m = open(p, encoding="utf-8").read()
m = m.replace('''async fn analyze_image(app: tauri::AppHandle, image: String) -> Result<String, String> {''',
'''async fn analyze_image(app: tauri::AppHandle, image: String, two_pass: Option<bool>) -> Result<String, String> {''', 1)
m = m.replace('''        enhance::analyze(port, &image)
    })''', '''        if two_pass.unwrap_or(true) {
            enhance::analyze_two_pass(port, &image)
        } else {
            enhance::analyze(port, &image)
        }
    })''', 1)
open(p, "w", encoding="utf-8", newline="\n").write(m)
print("main ok")
