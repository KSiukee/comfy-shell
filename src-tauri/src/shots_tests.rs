use super::*;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cs_shots_{tag}_{}", std::process::id()))
}

#[test]
fn add_list_image_delete() {
    let cfg = tmp("crud");
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "镜头测试").unwrap();
    let d = ShotDefaults {
        profile: "moss_sft".into(),
        negative: "bad".into(),
        width: 512,
        height: 512,
        steps: 8,
        cfg: 3.0,
        seed: 42,
        slg: true,
        slg_layers: "22".into(),
        layout: None,
    };
    let shots = add_many(&cfg, &p.id, vec!["格一".into(), "  ".into(), "格二".into()], &d).unwrap();
    assert_eq!(shots.len(), 2, "空白行要跳过");
    assert_eq!(shots[0].index, 1);
    assert_eq!(shots[1].index, 2);
    assert_eq!(shots[0].prompt, "格一");

    let s = add_image(&cfg, &p.id, &shots[0].id, "vault://x/y.png").unwrap();
    assert_eq!(s.images.len(), 1);
    assert_eq!(set_layout(&cfg, &p.id, &shots[0].id, Some("{}".into())).unwrap().layout.as_deref(), Some("{}"));

    delete(&cfg, &p.id, &shots[0].id).unwrap();
    assert_eq!(list(&cfg, &p.id).unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn unknown_project_is_rejected() {
    let cfg = tmp("missing");
    let _ = std::fs::remove_dir_all(&cfg);
    assert!(list(&cfg, "nope").is_err());
}
