use super::*;
use serde_json::json;

#[test]
fn save_and_load_roundtrip() {
    let cfg = std::env::temp_dir().join(format!("cs_ws_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "工作区测试").unwrap();
    let data = json!({"schema":1,"form":{"prompt":"草稿","seed":7},"panel":420});
    save(&cfg, &p.id, &data).unwrap();
    let got = load(&cfg, &p.id).unwrap().unwrap();
    assert_eq!(got["form"]["prompt"], "草稿");
    assert_eq!(got["panel"], 420);

    let big = json!({"x": "a".repeat(520 * 1024)});
    assert!(save(&cfg, &p.id, &big).is_err());
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn unknown_project_is_rejected() {
    let cfg = std::env::temp_dir().join(format!("cs_ws_bad_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cfg);
    assert!(load(&cfg, "nope").is_err());
}
