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

fn defaults() -> ShotDefaults {
    ShotDefaults {
        profile: "flux3".into(),
        negative: String::new(),
        width: 1024,
        height: 1365,
        steps: 16,
        cfg: 1.0,
        seed: -1,
        slg: false,
        slg_layers: "22".into(),
        layout: None,
    }
}

#[test]
fn characters_are_deduped_and_capped() {
    let cfg = tmp("cast_ids");
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "挂卡测试").unwrap();
    let shots = add_many(&cfg, &p.id, vec!["格一".into()], &defaults()).unwrap();
    let id = shots[0].id.clone();

    let s = set_characters(
        &cfg,
        &p.id,
        &id,
        vec!["a-1".into(), " ".into(), "a-1".into(), "b-2".into(), "c-3".into(), "d-4".into()],
    )
    .unwrap();
    assert_eq!(s.characters, vec!["a-1", "b-2", "c-3"], "去空 / 去重 / 最多 3 个");

    // 存盘后重读还在（旧 shots.json 没有这两个字段也要能读）
    let again = list(&cfg, &p.id).unwrap();
    assert_eq!(again[0].characters, vec!["a-1", "b-2", "c-3"]);
    assert!(again[0].cast.is_empty());

    assert!(set_characters(&cfg, &p.id, "没有这个镜头", vec![]).is_err());
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn cast_records_plate_and_dedupes() {
    let cfg = tmp("cast_refs");
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "底板记录").unwrap();
    let shots = add_many(&cfg, &p.id, vec!["格一".into()], &defaults()).unwrap();
    let id = shots[0].id.clone();

    let cast = vec![
        CastRef { id: "a-1".into(), name: " 林小满 ".into(), plate: Some("plate_v1.png".into()) },
        CastRef { id: "a-1".into(), name: "重复".into(), plate: None },
        CastRef { id: "b-2".into(), name: "阿满".into(), plate: Some("  ".into()) },
    ];
    let s = set_cast(&cfg, &p.id, &id, cast).unwrap();
    assert_eq!(s.cast.len(), 2, "同 id 只留第一条");
    assert_eq!(s.cast[0].name, "林小满", "名字要去空白");
    assert_eq!(s.cast[0].plate.as_deref(), Some("plate_v1.png"));
    assert_eq!(s.cast[1].plate, None, "空底板等于没底板");

    let again = list(&cfg, &p.id).unwrap();
    assert_eq!(again[0].cast[0].plate.as_deref(), Some("plate_v1.png"));
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn old_shot_file_without_new_fields_still_loads() {
    let cfg = tmp("legacy");
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "老文件").unwrap();
    let dir = cfg.join("projects").join(&p.id);
    std::fs::create_dir_all(&dir).unwrap();
    // v0.89 之前写出来的镜头：没有 characters / cast 两个字段
    std::fs::write(
        dir.join("shots.json"),
        r#"[{"id":"s_1","index":1,"prompt":"旧格","negative":"","profile":"moss","width":1024,"height":1024,"steps":30,"cfg":3.0,"seed":-1,"slg":false,"slgLayers":"22","layout":null,"images":[],"createdAt":1}]"#,
    )
    .unwrap();
    let shots = list(&cfg, &p.id).unwrap();
    assert_eq!(shots.len(), 1, "老文件不能因为缺字段整个读不出来");
    assert!(shots[0].characters.is_empty() && shots[0].cast.is_empty());
    let _ = std::fs::remove_dir_all(&cfg);
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
    set_cast(
        &cfg,
        &p.id,
        &shots[0].id,
        vec![CastRef { id: "a-1".into(), name: "林小满".into(), plate: None }],
    )
    .unwrap();

    let dir = export_episode(&cfg, &out, &p.id, &dest).unwrap();
    assert!(dir.join("images").join(format!("01_{}", "a.png")).is_file());
    assert!(dir.join("images").join(format!("02_{}", "b.png")).is_file());
    assert!(dir.join("story.md").is_file());
    let story = std::fs::read_to_string(dir.join("story.md")).unwrap();
    assert!(story.contains("角色：林小满"), "导出的分镜要带上挂了谁：{story}");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("export.json")).unwrap()).unwrap();
    assert_eq!(manifest["images"], 2);
    assert_eq!(manifest["missing"], 1, "第三格没出图");
    assert_eq!(manifest["castShots"], 1);

    let _ = std::fs::remove_dir_all(&cfg);
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&dest);
}
