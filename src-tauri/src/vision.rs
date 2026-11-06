//! 云端视觉（「云解析」）：OpenAI 兼容的 `chat/completions` + `image_url`。
//!
//! 实测（2026-11-05）：DeepSeek 的 `deepseek-flash` **能看图**——同一套 `image_url` dataURL
//! 格式，返回正文之前会先产出一大段 `reasoning_content`（推理 token 也算在 max_tokens 里），
//! 所以预算要给足（这里 1800），并单独处理「只有推理没有正文」这种情况。
//!
//! 这一层只做三件事：把 ComfyUI/input 里的图读成 base64、发出去、把正文取回来。
//! 谁许发（私密项目不给发）不在这里判断——那是 `commands::ai::analyze_blocks` 的闸。

use crate::state::Settings;
use serde_json::json;
use std::path::Path;

pub const DEFAULT_BASE: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-flash";
/// 单张图上限：12 MB（base64 之后约 16 MB，正常出图远小于这个数）。
const MAX_BYTES: u64 = 12 * 1024 * 1024;
/// 视觉端点超时：deepseek-flash 要推理，冷的时候 10~30 秒。
const TIMEOUT_SECS: u64 = 120;
/// 输出预算：推理 token 与正文共用，给少了会只剩推理。
pub const MAX_TOKENS: u32 = 1800;

pub fn base(s: &Settings) -> String {
    let b = s.vision_base.trim();
    if b.is_empty() {
        DEFAULT_BASE.to_string()
    } else {
        b.trim_end_matches('/').to_string()
    }
}

pub fn model(s: &Settings) -> String {
    let m = s.vision_model.trim();
    if m.is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        m.to_string()
    }
}

/// ComfyUI/input 里的图 → (mime, base64)。只认 png/jpg/webp，路径不许越界。
pub fn load_input_image(root: &str, name: &str) -> Result<(String, String), String> {
    let safe: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if safe.trim_matches('_').is_empty() {
        return Err("图片名是空的".into());
    }
    let path = Path::new(root).join("ComfyUI").join("input").join(&safe);
    let meta = std::fs::metadata(&path).map_err(|e| format!("读不到图片 {safe}：{e}"))?;
    if !meta.is_file() {
        return Err("不是文件".into());
    }
    if meta.len() > MAX_BYTES {
        return Err(format!("图片太大（{:.1} MB > 12 MB），云端解析请先缩小", meta.len() as f64 / 1048576.0));
    }
    let lower = safe.to_lowercase();
    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".png") {
        "image/png"
    } else {
        return Err("只支持 png / jpg / webp".into());
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("读不到图片 {safe}：{e}"))?;
    use base64::Engine as _;
    Ok((mime.to_string(), base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// 拼一份 OpenAI 兼容的请求体（纯函数，单测钉格式）。
pub fn payload(model: &str, mime: &str, b64: &str, prompt: &str, max_tokens: u32) -> serde_json::Value {
    json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{b64}")}}
            ]
        }],
        "max_tokens": max_tokens,
        "temperature": 0,
        "stream": false
    })
}

/// 从返回里取正文（纯函数）。只有推理没有正文时给一条能看懂的错。
pub fn extract_text(v: &serde_json::Value) -> Result<String, String> {
    if let Some(err) = v.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
        return Err(format!("云端返回错误：{err}"));
    }
    let choice = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .ok_or("返回里没有 choices")?;
    let msg = choice.get("message").cloned().unwrap_or_default();
    let text = msg.get("content").and_then(|c| c.as_str()).unwrap_or("").trim();
    if !text.is_empty() {
        return Ok(text.to_string());
    }
    let reasoning = msg.get("reasoning_content").and_then(|c| c.as_str()).unwrap_or("");
    let finish = choice.get("finish_reason").and_then(|c| c.as_str()).unwrap_or("");
    if !reasoning.is_empty() || finish == "length" {
        return Err(format!(
            "云端只产出了推理、没有正文（finish={finish}）——把 max_tokens 调大再试"
        ));
    }
    Err("云端返回是空的".into())
}

/// 发出去（阻塞；调用方在 spawn_blocking 里）。key 走 AI 助手那份（DPAPI）。
pub fn ask(cfg: &Path, s: &Settings, mime: &str, b64: &str, prompt: &str) -> Result<String, String> {
    let key = crate::agent::load_key(cfg).ok_or("还没填 API key（设置 → AI 助手）")?;
    let url = format!("{}/v1/chat/completions", base(s));
    let body = payload(&model(s), mime, b64, prompt, MAX_TOKENS);
    let resp = crate::api::client(TIMEOUT_SECS)
        .post(&url)
        .bearer_auth(key)
        .json(&body)
        .send()
        .map_err(|e| format!("连不上视觉端点 {url}：{e}"))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        let tail: String = text.chars().take(300).collect();
        return Err(format!("云端视觉 HTTP {}：{}", status.as_u16(), tail));
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("云端返回不是 JSON：{e}"))?;
    extract_text(&v)
}

#[cfg(test)]
#[path = "vision_tests.rs"]
mod tests;
