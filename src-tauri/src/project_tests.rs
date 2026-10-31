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
