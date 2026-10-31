use super::*;

fn tmp() -> PathBuf {
    std::env::temp_dir().join(format!("cs_pose_pack_{}", std::process::id()))
}

#[test]
fn discovers_legacy_and_packs_and_rewrites_paths() {
    let input = tmp();
    let _ = std::fs::remove_dir_all(&input);
    let legacy = input.join("shisei");
    std::fs::create_dir_all(legacy.join("line")).unwrap();
    std::fs::write(legacy.join("line").join("a.webp"), b"x").unwrap();
    std::fs::write(
        legacy.join("catalog.json"),
        r#"[{"id":"P1","name_cn":"坐","views":{"front":"a.webp","side":"missing.webp"}}]"#,
    )
    .unwrap();

    let demo = input.join("pose_packs").join("demo");
    std::fs::create_dir_all(demo.join("line")).unwrap();
    std::fs::write(demo.join("line").join("b.webp"), b"y").unwrap();
    std::fs::write(demo.join("pack.json"), r#"{"name":"演示包"}"#).unwrap();
    std::fs::write(
        demo.join("catalog.json"),
        r#"[{"id":"D1","name_cn":"站","views":{"front":"b.webp"}}]"#,
    )
    .unwrap();

    let dirs = pack_dirs(&input);
    assert_eq!(dirs.len(), 2);
    let legacy_list = load_catalog(&input, "shisei", &legacy, true).unwrap();
    assert_eq!(legacy_list.len(), 1);
    assert_eq!(legacy_list[0]["views"]["front"], "shisei/line/a.webp");
    assert!(legacy_list[0]["views"].get("side").is_none(), "缺失文件要过滤");

    let demo_list = load_catalog(&input, "demo", &demo, false).unwrap();
    assert_eq!(demo_list[0]["views"]["front"], "pose_packs/demo/line/b.webp");

    let legacy_meta = pack_meta(&legacy, "shisei", true);
    assert!(legacy_meta.publishable, "旧 shisei 默认 CC0 可发布");
    assert!(legacy_meta.license.contains("CC0"));
    let demo_meta = pack_meta(&demo, "demo", false);
    assert!(!demo_meta.publishable, "未知 pack 保守不可发布");
    assert_eq!(demo_meta.license, "未标注");
    assert_eq!(search(&input, "站").len(), 1);
    let _ = std::fs::remove_dir_all(&input);
}
