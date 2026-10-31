//! Prompt enhancement (the official "prompt upsampler" / "PE" models).
//!
//! Each supported image model has a companion text model that rewrites a short
//! request into the long scene caption the DiT expects. They are driven through
//! `TextGenerate` with the official system prompt shipped in `prompts/`.

use serde_json::{json, Value};
use crate::api;

const QWEN_T2I: &str = include_str!("../prompts/qwen_t2i.txt");
const QWEN_EDIT: &str = include_str!("../prompts/qwen_edit.txt");
const FLUX3_T2I: &str = include_str!("../prompts/flux3_t2i.txt");
const MOSS_T2I: &str = include_str!("../prompts/moss_t2i.txt");
const TRANSLATE_SYS: &str = include_str!("../prompts/translate.txt");
const FREE_SYS: &str = include_str!("../prompts/enhance_free.txt");
const STORY_SPLIT_SYS: &str = include_str!("../prompts/story_split.txt");

struct Enhancer {
    clip: &'static str,
    clip_type: &'static str,
    system: &'static str,
    needs_image: bool,
}

fn pick(profile: &str, mode: &str) -> Option<Enhancer> {
    Some(match (profile, mode) {
        ("qwen21", "edit") => Enhancer {
            clip: "qwen3.5_9b_qwen_image_2.1_pe_i2i.int8_convrot.safetensors",
            clip_type: "qwen_image",
            system: QWEN_EDIT,
            needs_image: true,
        },
        ("qwen21", _) => Enhancer {
            clip: "qwen3.5_9b_qwen_image_2.1_pe_t2i.int8_convrot.safetensors",
            clip_type: "qwen_image",
            system: QWEN_T2I,
            needs_image: false,
        },
        ("flux3", _) | ("flux3base", _) => Enhancer {
            clip: "qwen35_9b_heretic_int8_convrot.safetensors",
            clip_type: "flux3",
            system: FLUX3_T2I,
            needs_image: false,
        },
        ("moss", _) => Enhancer {
            clip: "moss_te_heretic_full_int8_convrot.safetensors",
            clip_type: "moss_image",
            system: MOSS_T2I,
            needs_image: false,
        },
        _ => return None,
    })
}

fn build(e: &Enhancer, prompt: &str, ref_image: Option<&str>) -> Value {
    let mut wf = json!({
        "1": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": e.clip, "type": e.clip_type, "device": "default"}},
        "2": {"class_type": "TextGenerate",
              "inputs": {"clip": ["1", 0], "prompt": prompt, "max_length": 1200,
                         "sampling_mode": "off", "system_prompt": e.system}},
        "3": {"class_type": "PreviewAny", "inputs": {"source": ["2", 0]}},
    });
    if let (true, Some(img)) = (e.needs_image, ref_image) {
        wf["10"] = json!({"class_type": "LoadImage", "inputs": {"image": img}});
        wf["2"]["inputs"]["image"] = json!(["10", 0]);
    }
    wf
}

/// Runs the enhancer and returns the rewritten prompt.
pub fn enhance(
    port: u16,
    profile: &str,
    mode: &str,
    prompt: &str,
    ref_images: &[String],
) -> Result<String, String> {
    let e = pick(profile, mode).ok_or("这个模型没有配置提示词扩写器")?;
    let ref_image = ref_images.first().map(|s| s.as_str());
    if e.needs_image && ref_image.is_none() {
        return Err("改图扩写需要先放一张参考图".into());
    }
    let wf = build(&e, prompt, ref_image);
    let outputs = api::run_raw(port, wf, 300)?;
    let text = extract_text(outputs)?;
    if looks_refused(&text) {
        // official PE refused: retry with the local uncensored rewriter
        if let Ok(free) = enhance_free(port, prompt, ref_image) {
            if !looks_refused(&free) {
                return Ok(free);
            }
        }
    }
    Ok(text)
}

const CAPTION_PROMPT: &str = "Describe this image in one line as comma-separated English tags for a training caption: subject, action, clothing, setting, lighting, color palette, style. No preamble, at most 30 words.";

/// Vision caption for one image inside ComfyUI/input (used by folder training).
pub fn caption(port: u16, image: &str) -> Result<String, String> {
    let wf = json!({
        "1": {"class_type": "LoadImage", "inputs": {"image": image}},
        "2": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": "qwen3vl_8b_int8_convrot.safetensors",
                         "type": "qwen_image", "device": "default"}},
        "3": {"class_type": "TextGenerate",
              "inputs": {"clip": ["2", 0], "prompt": CAPTION_PROMPT, "image": ["1", 0],
                         "max_length": 160, "sampling_mode": "off"}},
        "4": {"class_type": "PreviewAny", "inputs": {"source": ["3", 0]}},
    });
    let outputs = api::run_raw(port, wf, 300)?;
    let text = extract_text(outputs)?;
    Ok(text.trim().trim_matches('"').to_string())
}


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



const ASSIST_SYS: &str = r#"You are a small assistant inside an image generation app. Tools:
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
Rules: at most 4 tool calls total. When the prompt is ready, answer "final". For pose requests call pose first, then weave its description into the prompt. Never invent content the user did not ask for."#;

pub struct AssistStep {
    pub tool: String,
    pub output: String,
}


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
        .join("\n")
}

/// A tiny tool loop: translate -> expand -> final, driven by the local text model.
pub fn assist(port: u16, request: &str, image: Option<&str>) -> Result<(String, String, Vec<AssistStep>), String> {
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
                convo.push_str(&format!("\n工具 translate 结果：{}", out.trim()));
            }
            "pose" => {
                let q = action.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let out = pose_search(port, q);
                steps.push(AssistStep { tool: "pose".into(), output: out.clone() });
                convo.push_str(&format!("\n工具 pose 结果：{}", out));
            }
            "analyze" => {
                let out = match image {
                    Some(img) => analyze(port, img).unwrap_or_else(|e| format!("分析失败：{e}")),
                    None => "用户没有附参考图".to_string(),
                };
                steps.push(AssistStep { tool: "analyze".into(), output: out.clone() });
                convo.push_str(&format!("\n工具 analyze 结果：{}", out));
            }
            "expand" => {
                let text = action.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                let out = enhance_free(port, text, None).unwrap_or_else(|_| text.to_string());
                steps.push(AssistStep { tool: "expand".into(), output: out.clone() });
                convo.push_str(&format!("\n工具 expand 结果：{}", out.trim()));
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
{{"pose":"","expression":"","clothing":"","accessories":"","background":"","style":"","colors":"","lighting":"","mood":"","extras":[{{"label":"","value":""}}],"prompt":""}}
Rules: values are short English phrases; "prompt" is one comma-separated English prompt under 90 words combining all fields; empty string when unknown.
"extras" holds elements that fit no fixed field, such as weather, vehicles, animals, food, time of day, signage. Use a short English label and value, e.g. {{"label":"weather","value":"light rain"}}; return [] if nothing extra.
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

/// Captions a batch of input images in one prompt so the vision model loads once.
pub fn caption_batch(port: u16, images: &[String]) -> Vec<Option<String>> {
    let mut wf = json!({
        "1": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": "qwen3vl_8b_int8_convrot.safetensors",
                         "type": "qwen_image", "device": "default"}},
    });
    for (i, img) in images.iter().enumerate() {
        wf[format!("1{i}")] = json!({"class_type": "LoadImage", "inputs": {"image": img}});
        wf[format!("2{i}")] = json!({"class_type": "TextGenerate", "inputs": {
            "clip": ["1", 0], "prompt": CAPTION_PROMPT, "image": [format!("1{i}"), 0],
            "max_length": 160, "sampling_mode": "off"}});
        wf[format!("3{i}")] = json!({"class_type": "PreviewAny", "inputs": {"source": [format!("2{i}"), 0]}});
    }
    let mut out: Vec<Option<String>> = vec![None; images.len()];
    let Ok(outputs) = api::run_raw(port, wf, 1800) else {
        return out;
    };
    if let Some(map) = outputs.as_object() {
        for (key, node) in map {
            if let Some(idx) = key.strip_prefix("3").and_then(|x| x.parse::<usize>().ok()) {
                if idx < out.len() {
                    let text = node
                        .get("text")
                        .and_then(|t| t.as_array())
                        .and_then(|a| a.iter().find_map(|x| x.as_str()))
                        .map(|t| t.trim().trim_matches('"').to_string());
                    out[idx] = text;
                }
            }
        }
    }
    out
}

/// Some of the official rewriters answer with a refusal instead of a prompt.
/// Those are short and start with an apology, so they are easy to spot.
fn looks_refused(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    if t.len() < 24 {
        return true;
    }
    const NEEDLES: [&str; 10] = [
        "i can't", "i cannot", "i can not", "i'm sorry", "i am sorry",
        "cannot assist", "can't assist", "unable to help", "抱歉", "无法",
    ];
    NEEDLES.iter().any(|n| t.contains(n))
}

/// Uncensored rewriter: local Qwen3-VL heretic, driven by `prompts/enhance_free.txt`.
pub fn enhance_free(port: u16, prompt: &str, ref_image: Option<&str>) -> Result<String, String> {
    let mut wf = json!({
        "1": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": "qwen3vl_8b_int8_convrot_heretic.safetensors",
                         "type": "qwen_image", "device": "default"}},
        "2": {"class_type": "TextGenerate",
              "inputs": {"clip": ["1", 0], "prompt": prompt, "max_length": 420,
                         "sampling_mode": "off", "system_prompt": FREE_SYS}},
        "3": {"class_type": "PreviewAny", "inputs": {"source": ["2", 0]}},
    });
    if let Some(img) = ref_image {
        wf["10"] = json!({"class_type": "LoadImage", "inputs": {"image": img}});
        wf["2"]["inputs"]["image"] = json!(["10", 0]);
    }
    let outputs = api::run_raw(port, wf, 420)?;
    extract_text(outputs)
}

/// Split a story into panel descriptions with the local LLM.
/// Falls back to sentence chunking when the model is unavailable.
pub fn story_split(port: u16, text: &str, max_panels: u32) -> Vec<String> {
    let max = max_panels.clamp(1, 40);
    let prompt = format!(
        "最多拆 {} 格。剧情如下：\n{}",
        max,
        text.trim()
    );
    let wf = json!({
        "1": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": "qwen35_9b_heretic_int8_convrot.safetensors",
                         "type": "flux3", "device": "default"}},
        "2": {"class_type": "TextGenerate",
              "inputs": {"clip": ["1", 0], "prompt": prompt, "max_length": 1600,
                         "sampling_mode": "off", "system_prompt": STORY_SPLIT_SYS}},
        "3": {"class_type": "PreviewAny", "inputs": {"source": ["2", 0]}},
    });
    let mut panels: Vec<String> = Vec::new();
    if let Ok(outputs) = api::run_raw(port, wf, 420) {
        if let Ok(raw) = extract_text(outputs) {
            for line in raw.lines() {
                let line = line
                    .trim()
                    .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == '、' || c == '-' || c == '*')
                    .trim();
                if line.chars().count() >= 8 && !line.starts_with('（') {
                    panels.push(line.to_string());
                }
            }
        }
    }
    if panels.is_empty() {
        panels = split_fallback(text, max as usize);
    }
    panels.truncate(max as usize);
    panels
}

fn split_fallback(text: &str, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for ch in text.chars() {
        buf.push(ch);
        if "。！？!?".contains(ch) && buf.chars().count() >= 40 {
            out.push(buf.trim().to_string());
            buf.clear();
            if out.len() >= max {
                break;
            }
        }
    }
    if out.len() < max && !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

/// Chinese (or any language) -> English, used by the library search.
pub fn translate(port: u16, text: &str) -> Result<String, String> {
    let wf = json!({
        "1": {"class_type": "CLIPLoader",
              "inputs": {"clip_name": "qwen35_9b_heretic_int8_convrot.safetensors",
                         "type": "flux3", "device": "default"}},
        "2": {"class_type": "TextGenerate",
              "inputs": {"clip": ["1", 0], "prompt": text, "max_length": 200,
                         "sampling_mode": "off", "system_prompt": TRANSLATE_SYS}},
        "3": {"class_type": "PreviewAny", "inputs": {"source": ["2", 0]}},
    });
    let outputs = api::run_raw(port, wf, 180)?;
    extract_text(outputs)
}

fn extract_text(outputs: serde_json::Value) -> Result<String, String> {
    let mut found: Option<String> = None;
    if let Some(map) = outputs.as_object() {
        for out in map.values() {
            if let Some(t) = out.get("text") {
                let s = t
                    .as_array()
                    .and_then(|a| a.iter().find_map(|x| x.as_str()))
                    .or_else(|| t.as_str());
                if let Some(s) = s {
                    found = Some(s.to_string());
                    break;
                }
            }
        }
    }
    let text = found.ok_or("模型没有返回文本（可能是模型与 ComfyUI 版本不匹配）")?;
    Ok(text.trim().to_string())
}



