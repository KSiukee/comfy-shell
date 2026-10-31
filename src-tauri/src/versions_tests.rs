use super::*;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cs_versions_{tag}_{}", std::process::id()))
}

fn seed(text: &str) -> VersionSeed {
    VersionSeed {
        prompt: text.into(),
        negative: "bad".into(),
        profile: "moss".into(),
        mode: "t2i".into(),
        width: 512,
        height: 512,
        steps: 8,
        cfg: 3.0,
        seed: 7,
        layout: None,
    }
}

#[test]
fn append_chain_and_stars() {
    let cfg = tmp("chain");
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "版本测试").unwrap();
    let a = append(&cfg, &p.id, &seed("第一版"), &["a.png".into()], "h1", None).unwrap();
    assert_eq!(a.stars, 1, "默认 1 星");
    assert!(a.parent.is_none());
    let b = append(&cfg, &p.id, &seed("第二版"), &["b.png".into()], "h2", None).unwrap();
    assert_eq!(b.parent.as_deref(), Some(a.id.as_str()));
    assert_eq!(list(&cfg, &p.id).unwrap().len(), 2);

    let starred = star(&cfg, &p.id, &a.id, 5).unwrap();
    assert_eq!(starred.stars, 5);
    assert_eq!(star(&cfg, &p.id, &a.id, 9).unwrap().stars, 5, "上限 5");
    assert_eq!(star(&cfg, &p.id, &a.id, 0).unwrap().stars, 1, "下限 1");
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn delete_keeps_the_rest() {
    let cfg = tmp("delete");
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "版本删除").unwrap();
    let a = append(&cfg, &p.id, &seed("a"), &[], "h", None).unwrap();
    let b = append(&cfg, &p.id, &seed("b"), &[], "h", None).unwrap();
    delete(&cfg, &p.id, &a.id).unwrap();
    let rest = list(&cfg, &p.id).unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].id, b.id);
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn branch_from_old_version() {
    let cfg = tmp("branch");
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "分叉").unwrap();
    let a = append(&cfg, &p.id, &seed("a"), &[], "h", None).unwrap();
    let b = append(&cfg, &p.id, &seed("b"), &[], "h", None).unwrap();
    let c = append(&cfg, &p.id, &seed("c"), &[], "h", Some(&a.id)).unwrap();
    assert_eq!(c.parent.as_deref(), Some(a.id.as_str()));
    assert_ne!(c.parent.as_deref(), Some(b.id.as_str()));
    let d = append(&cfg, &p.id, &seed("d"), &[], "h", Some("nope")).unwrap();
    assert_eq!(d.parent.as_deref(), Some(c.id.as_str()), "无效父版本退回最新");
    let _ = std::fs::remove_dir_all(&cfg);
}
