proj = r"D:\deepseek-files\ai-tools\comfy-shell"

# ---------------- enhance.rs: mini local agent ----------------
p = proj + r"\src-tauri\src\enhance.rs"
s = open(p, encoding="utf-8").read()

agent = '''
const ASSIST_SYS: &str = r#"You are a small assistant inside an image generation app. You can call exactly two tools:
- translate: translate the user's Chinese into an English image prompt fragment.
- expand: rewrite an English fragment into a richer image prompt (lighting, lens, style, details).
Reply with ONE JSON object per turn and nothing else:
{"tool":"translate","text":"中文"}
{"tool":"expand","prompt":"english fragment"}
{"tool":"final","prompt":"final english prompt","note":"one short Chinese sentence"}
Rules: at most 3 tool calls total. When the prompt is ready, answer with "final". Never invent image content the user did not ask for."#;

pub struct AssistStep {
    pub tool: String,
    pub output: String,
}

/// A tiny tool loop: translate -> expand -> final, driven by the local text model.
pub fn assist(port: u16, request: &str) -> Result<(String, String, Vec<AssistStep>), String> {
    let mut steps: Vec<AssistStep> = Vec::new();
    let mut convo = format!("用户需求：{}", request.trim());
    let mut final_prompt = String::new();
    let mut note = String::new();

    for _ in 0..4 {
        let wf = json!({
            "1": {"class_type": "CLIPLoader",
                  "inputs": {"clip_name": "qwen35_9b_heretic_int8_convrot.safetensors",
                             "type": "flux3", "device": "default"}},
            "2": {"class_type": "TextGenerate",
                  "inputs": {"clip": ["1", 0], "prompt": convo, "max_length": 320,
                             "sampling_mode": "off", "system_prompt": ASSIST_SYS}},
            "3": {"class_type": "PreviewAny", "inputs": {"source": ["2", 0]}},
        });
        let raw = extract_text(api::run_raw(port, wf, 300)?)?;
        let action: serde_json::Value = match serde_json::from_str(extract_json(&raw).as_str()) {
            Ok(v) => v,
            Err(_) => {
                final_prompt = raw.trim().to_string();
                break;
            }
        };
        let tool = action.get("tool").and_then(|v| v.as_str()).unwrap_or("final");
        match tool {
            "translate" => {
                let text = action.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let out = translate(port, text).unwrap_or_else(|_| text.to_string());
                steps.push(AssistStep { tool: "translate".into(), output: out.clone() });
                convo.push_str(&format!("\\n工具 translate 结果：{}", out.trim()));
            }
            "expand" => {
                let text = action.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                let out = enhance_free(port, text, None).unwrap_or_else(|_| text.to_string());
                steps.push(AssistStep { tool: "expand".into(), output: out.clone() });
                convo.push_str(&format!("\\n工具 expand 结果：{}", out.trim()));
            }
            _ => {
                final_prompt = action.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                note = action.get("note").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                break;
            }
        }
    }
    if final_prompt.is_empty() {
        return Err("模型没有给出最终提示词".into());
    }
    Ok((final_prompt, note, steps))
}

'''
anchor = "const DESC_PROMPT: &str"
if "pub fn assist(" not in s:
    s = s.replace(anchor, agent + anchor, 1)
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("enhance ok", "pub fn assist(" in s)
