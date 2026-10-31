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

#[test]
fn export_episode_copies_latest_images() {
    let cfg = tmp("episode_cfg");
    let out = tmp("episode_out");
    let dest = tmp("episode_dest");
    for d in [&cfg, &out, &dest] {
        let _ = std::fs::remove_dir_all(d);
    }
    let p = crate::project::create(&cfg, "整话测试").unwrap();
    let img_dir = out.join("projects").join(&p.id);
    std::fs::create_dir_all(&img_dir).unwrap();
    std::fs::write(img_dir.join("a.png"), b"a").unwrap();
    std::fs::write(img_dir.join("b.png"), b"b").unwrap();

    let d = ShotDefaults {
        profile: "moss".into(),
        negative: String::new(),
        width: 512,
        height: 512,
        steps: 8,
        cfg: 3.0,
        seed: 1,
        slg: false,
        slg_layers: "22".into(),
        layout: None,
    };
    let shots = add_many(&cfg, &p.id, vec!["格一".into(), "格二".into(), "格三".into()], &d).unwrap();
    add_image(&cfg, &p.id, &shots[0].id, &img_dir.join("a.png").to_string_lossy()).unwrap();
    add_image(&cfg, &p.id, &shots[1].id, &img_dir.join("b.png").to_string_lossy()).unwrap();

    let dir = export_episode(&cfg, &out, &p.id, &dest).unwrap();
    assert!(dir.join("images").join(format!("01_{}", "a.png")).is_file());
    assert!(dir.join("images").join(format!("02_{}", "b.png")).is_file());
    assert!(dir.join("story.md").is_file());
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("export.json")).unwrap()).unwrap();
    assert_eq!(manifest["images"], 2);
    assert_eq!(manifest["missing"], 1, "第三格没出图");

    let _ = std::fs::remove_dir_all(&cfg);
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&dest);
}
