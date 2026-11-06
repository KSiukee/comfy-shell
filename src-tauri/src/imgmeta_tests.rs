use super::*;
use serde_json::json;

fn tiny_png() -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(&SIG);
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png.extend_from_slice(&chunk(b"IHDR", &ihdr));
    png.extend_from_slice(&chunk(b"IEND", &[]));
    png
}

#[test]
fn crc_matches_known_iend() {
    assert_eq!(crc32(b"IEND"), 0xAE42_6082);
}

#[test]
fn embed_and_read_roundtrip() {
    let dir = std::env::temp_dir().join(format!("cs_imgmeta_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("a.png");
    std::fs::write(&p, tiny_png()).unwrap();

    let meta = json!({"app":"comfy-shell","prompt":"雨夜街头","seed":42,"project":"p1"});
    write_png_meta(&p, &meta).unwrap();
    let got = read_png_meta(&p).unwrap();
    assert_eq!(got["prompt"], "雨夜街头");
    assert_eq!(got["seed"], 42);

    // 再写一次：旧块被替换，不重复
    write_png_meta(&p, &json!({"app":"comfy-shell","prompt":"第二版"})).unwrap();
    assert_eq!(read_png_meta(&p).unwrap()["prompt"], "第二版");
    let bytes = std::fs::read(&p).unwrap();
    let mut count = 0;
    let mut pos = 8;
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
        if &bytes[pos + 4..pos + 8] == b"iTXt" {
            count += 1;
        }
        pos += 12 + len;
    }
    assert_eq!(count, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn read_meta_under_only_inside_root() {
    let base = std::env::temp_dir().join(format!("cs_imgmeta_scope_{}", std::process::id()));
    let root = base.join("ComfyUI");
    let inside = root.join("output").join("a.png");
    let outside = base.join("outside.png");
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(inside.parent().unwrap()).unwrap();
    std::fs::write(&inside, tiny_png()).unwrap();
    std::fs::write(&outside, tiny_png()).unwrap();
    let meta = json!({"app":"comfy-shell","prompt":"根内图"});
    write_png_meta(&inside, &meta).unwrap();
    write_png_meta(&outside, &json!({"app":"comfy-shell","prompt":"根外图"})).unwrap();

    assert_eq!(read_meta_under(&root, &inside).unwrap()["prompt"], "根内图");
    assert!(read_meta_under(&root, &outside).is_none(), "根外路径必须拒绝");
    assert!(
        read_meta_under(&root, std::path::Path::new("vault://p1/a.png")).is_none(),
        "私密图不读明文参数"
    );
    assert!(read_meta_under(&root, &root.join("missing.png")).is_none());
    let _ = std::fs::remove_dir_all(&base);
}

// ---------------------------------------------------------------- 种子互查

/// 造一张「ComfyUI 会写出来的样子」的 PNG：带一个 fake 的 prompt 文本块。
fn png_with_prompt(json_text: &str) -> Vec<u8> {
    let mut png = tiny_png();
    let mut data = Vec::new();
    data.extend_from_slice(b"prompt");
    data.push(0);
    data.extend_from_slice(json_text.as_bytes());
    let mut out = Vec::new();
    out.extend_from_slice(&png[..png.len() - 12]); // 去掉 IEND
    out.extend_from_slice(&chunk(b"tEXt", &data));
    out.extend_from_slice(&chunk(b"IEND", &[]));
    png = out;
    png
}

#[test]
fn reads_the_seed_out_of_comfyuis_prompt_chunk() {
    let wf = json!({
        "3": {"class_type": "CLIPTextEncode", "inputs": {"text": "x"}},
        "8": {"class_type": "KSampler", "inputs": {"seed": 1918999279702401148i64, "steps": 16}},
    });
    let png = png_with_prompt(&wf.to_string());
    assert_eq!(parse_comfy_seed(&png), Some(1918999279702401148));
}

#[test]
fn reads_noise_seed_from_advanced_samplers() {
    let wf = json!({
        "8": {"class_type": "KSamplerAdvanced", "inputs": {"noise_seed": 777, "steps": 8}},
    });
    assert_eq!(parse_comfy_seed(&png_with_prompt(&wf.to_string())), Some(777));
}

#[test]
fn ignores_random_sentinel_and_missing_chunks() {
    let wf = json!({"8": {"class_type": "KSampler", "inputs": {"seed": -1}}});
    assert_eq!(parse_comfy_seed(&png_with_prompt(&wf.to_string())), None);
    // 没有 prompt 块（比如壳自己合成的换脸图）
    assert_eq!(parse_comfy_seed(&tiny_png()), None);
    // 不是 PNG
    assert_eq!(parse_comfy_seed(b"not a png at all"), None);
}
