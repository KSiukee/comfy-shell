// 项目模块的测试（拆出来，别把 project.rs 顶过 400 行）。
use super::*;

    use super::*;

    fn tmpcfg(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cs_project_{tag}_{}", std::process::id()))
    }

    #[test]
    fn create_rename_hide_and_sort() {
        let cfg = tmpcfg("crud");
        let _ = std::fs::remove_dir_all(&cfg);
        let a = create(&cfg, "  海报 A  ").unwrap();
        assert_eq!(a.name, "海报 A");
        let b = create(&cfg, "海报 B").unwrap();
        let renamed = rename(&cfg, &a.id, "海报 A2").unwrap();
        assert_eq!(renamed.name, "海报 A2");
        set_active(&cfg, Some(&b.id)).unwrap();
        let items = list(&cfg, false);
        assert_eq!(items[0].id, b.id, "开工的排第一");
        set_hidden(&cfg, &b.id, true).unwrap();
        assert!(list(&cfg, false).iter().all(|p| p.id != b.id));
        assert_eq!(list(&cfg, true).len(), 2);
        let _ = std::fs::remove_dir_all(&cfg);
    }

    #[test]
    fn delete_restore_purge_roundtrip() {
        let cfg = tmpcfg("trash");
        let out = tmpcfg("trash_out");
        let _ = std::fs::remove_dir_all(&cfg);
        let _ = std::fs::remove_dir_all(&out);
        let a = create(&cfg, "要删的").unwrap();
        let img = out.join("projects").join(&a.id).join("x.png");
        std::fs::create_dir_all(img.parent().unwrap()).unwrap();
        std::fs::write(&img, b"fake").unwrap();
        set_active(&cfg, Some(&a.id)).unwrap();
        delete(&cfg, &a.id, &out).unwrap();
        assert!(list(&cfg, true).is_empty());
        assert!(!img.exists(), "删除后图片也要离开 output");
        let t = trash(&cfg);
        assert_eq!(t.len(), 1);
        let restored = restore(&cfg, &t[0].dir, &out).unwrap();
        assert_eq!(restored.id, a.id);
        assert!(!restored.active, "恢复不应自动开工");
        assert!(img.exists(), "恢复后图片要回到 output/projects/<id>");
        assert_eq!(list(&cfg, true).len(), 1);
        delete(&cfg, &a.id, &out).unwrap();
        let t = trash(&cfg);
        purge(&cfg, &t[0].dir).unwrap();
        assert!(trash(&cfg).is_empty());
        let _ = std::fs::remove_dir_all(&cfg);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn rejects_bad_names_and_ids() {
        let cfg = tmpcfg("bad");
        let _ = std::fs::remove_dir_all(&cfg);
        assert!(create(&cfg, "   ").is_err());
        assert!(create(&cfg, &"x".repeat(61)).is_err());
        assert!(create(&cfg, "a\nb").is_err());
        assert!(load(&cfg, "../x").is_err());
        let _ = std::fs::remove_dir_all(&cfg);
    }

    // ---- 云端看图开关 ----

    #[test]
    fn cloud_vision_defaults_follow_the_red_line() {
        let mut p = Project::default();
        assert!(p.cloud_vision_on(false), "普通项目默认允许");
        assert!(!p.cloud_vision_on(true), "私密项目默认关");
        // 隐藏项目也按「锁定」算：界面上看不见它，就不该默认把图发上云
        let mut hidden = Project::default();
        hidden.hidden = true;
        let locked = hidden.private || hidden.hidden;
        assert!(locked, "隐藏就是要按锁定处理");
        assert!(!hidden.cloud_vision_on(locked), "隐藏项目默认关");
        // 显式打开过才放行（界面里要二次确认）
        hidden.cloud_vision = Some(true);
        assert!(hidden.cloud_vision_on(locked), "显式选择优先");
        p.cloud_vision = Some(true);
        assert!(p.cloud_vision_on(true), "显式选择优先");
        p.cloud_vision = Some(false);
        assert!(!p.cloud_vision_on(false), "显式关掉也算");
    }

    #[test]
    fn cloud_vision_is_persisted_and_old_records_still_load() {
        let cfg = tmpcfg("cloudvision");
        let _ = std::fs::remove_dir_all(&cfg);
        let a = create(&cfg, "海报").unwrap();
        assert_eq!(load(&cfg, &a.id).unwrap().cloud_vision, None, "新项目按规矩");

        let off = set_cloud_vision(&cfg, &a.id, Some(false)).unwrap();
        assert_eq!(off.cloud_vision, Some(false));
        assert!(!load(&cfg, &a.id).unwrap().cloud_vision_on(false));

        // 老记录没有这个字段也要能读（serde default）
        let file = project_file(&cfg, &a.id);
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        v.as_object_mut().unwrap().remove("cloudVision");
        std::fs::write(&file, serde_json::to_string(&v).unwrap()).unwrap();
        let old = load(&cfg, &a.id).unwrap();
        assert_eq!(old.cloud_vision, None);
        assert!(old.cloud_vision_on(false));

        let _ = std::fs::remove_dir_all(&cfg);
    }
