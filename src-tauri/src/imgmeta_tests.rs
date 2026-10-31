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
