//! vision.rs 的单测：钉请求格式与「只有推理没有正文」这一坑。

use super::*;

#[test]
fn payload_is_openai_chat_with_data_url() {
    let v = payload("deepseek-flash", "image/png", "QUJD", "读这张图", 1800);
    assert_eq!(v["model"], "deepseek-flash");
    assert_eq!(v["max_tokens"], 1800);
    assert_eq!(v["stream"], false);
    let content = &v["messages"][0]["content"];
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[0]["text"], "读这张图");
    assert_eq!(content[1]["type"], "image_url");
    assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,QUJD");
}

#[test]
fn extract_text_reads_the_first_choice() {
    let v = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": "{\"subject\":[\"a\"]}"},
                     "finish_reason": "stop"}]
    });
    assert_eq!(extract_text(&v).unwrap(), "{\"subject\":[\"a\"]}");
}

#[test]
fn extract_text_explains_reasoning_only_replies() {
    let v = serde_json::json!({
        "choices": [{"message": {"content": "", "reasoning_content": "先想一下……"},
                     "finish_reason": "length"}]
    });
    let err = extract_text(&v).unwrap_err();
    assert!(err.contains("推理"), "{err}");
    assert!(err.contains("max_tokens"), "{err}");
}

#[test]
fn extract_text_surfaces_api_errors_and_empty_bodies() {
    let v = serde_json::json!({"error": {"message": "invalid api key"}});
    assert!(extract_text(&v).unwrap_err().contains("invalid api key"));
    let v = serde_json::json!({"choices": [{"message": {"content": "   "}}]});
    assert!(extract_text(&v).unwrap_err().contains("空的"));
    let v = serde_json::json!({"choices": []});
    assert!(extract_text(&v).unwrap_err().contains("没有 choices"));
}

#[test]
fn base_and_model_fall_back_to_defaults() {
    let mut s = Settings::default();
    assert_eq!(base(&s), DEFAULT_BASE);
    assert_eq!(model(&s), DEFAULT_MODEL);
    s.vision_base = "https://example.com/api/".into();
    s.vision_model = "glm-4v".into();
    assert_eq!(base(&s), "https://example.com/api");
    assert_eq!(model(&s), "glm-4v");
}

#[test]
fn load_input_image_rejects_missing_and_odd_names() {
    let tmp = std::env::temp_dir().join("cs-vision-test");
    let input = tmp.join("ComfyUI").join("input");
    std::fs::create_dir_all(&input).unwrap();
    let root = tmp.to_string_lossy().to_string();

    assert!(load_input_image(&root, "").is_err());
    assert!(load_input_image(&root, "nope.png").is_err());

    std::fs::write(input.join("ok.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    let (mime, b64) = load_input_image(&root, "ok.png").unwrap();
    assert_eq!(mime, "image/png");
    assert!(!b64.is_empty());

    // 路径分隔符会被压成下划线，读不到越界文件
    assert!(load_input_image(&root, "../secret.png").is_err());
    std::fs::write(input.join("pic.gif"), b"GIF89a").unwrap();
    assert!(load_input_image(&root, "pic.gif").is_err());
}

/// 真机探针（默认不跑）：拿本机配置里的 key 打一次 DeepSeek 视觉。
/// 跑法：先把一张合成图放进 ComfyUI/input/vb_probe.png，再
/// `cargo test cloud_smoke_live -- --ignored --nocapture`
#[test]
#[ignore]
fn cloud_smoke_live() {
    let cfg = std::path::PathBuf::from(std::env::var("APPDATA").expect("APPDATA"))
        .join("com.dsh.comfyshell");
    let text = std::fs::read_to_string(cfg.join("settings.json")).expect("settings.json");
    let s: Settings = serde_json::from_str(&text).expect("解析设置");
    let (mime, b64) = load_input_image(&s.comfy_root, "vb_probe.png").expect("读探针图");
    let prompt = r#"Look at the image. Return ONLY one JSON object:
{"subject":["…","…"],"camera":["…","…"],"light":["…","…"],"color":["…","…"],"style":["…","…"]}
Each value is an array of 2 short English fragments. No markdown."#;
    let out = ask(&cfg, &s, &mime, &b64, prompt).expect("云端调用");
    println!("--- cloud reply ---\n{out}");
    assert!(out.contains("subject"), "回复里没有 subject：{out}");
}
