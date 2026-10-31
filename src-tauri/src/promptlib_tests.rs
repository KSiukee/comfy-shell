use super::*;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cs_promptlib_{tag}_{}", std::process::id()))
}

#[test]
fn upsert_star_use_delete() {
    let cfg = tmp("crud");
    let _ = std::fs::remove_dir_all(&cfg);
    let p = crate::project::create(&cfg, "提示词测试").unwrap();
    let a = upsert(&cfg, "雨夜街头，女孩回头", "bad", vec!["夜景".into()], Some(&p.id)).unwrap();
    assert_eq!(a.project_name, "提示词测试");
    assert_eq!(a.uses, 0);

    // 同文本再存：不新增，标签合并
    let again = upsert(&cfg, "雨夜街头，女孩回头", "", vec!["常用".into()], Some(&p.id)).unwrap();
    assert_eq!(again.id, a.id);
    assert_eq!(again.tags.len(), 2);
    assert_eq!(list(&cfg).unwrap().len(), 1);

    assert!(star(&cfg, &a.id, true).unwrap().favorite);
    assert_eq!(bump_use(&cfg, &a.id).unwrap().uses, 1);
    delete(&cfg, &a.id).unwrap();
    assert!(list(&cfg).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&cfg);
}

#[test]
fn clean_removes_cliches() {
    let r = clean("masterpiece, 雨夜街头, 8k, cinematic lighting, 女孩回头");
    assert_eq!(r.text, "雨夜街头, 女孩回头");
    assert_eq!(r.removed, 3);
    let r2 = clean("大师作品，超高清，雨中车站");
    assert_eq!(r2.text, "雨中车站");
    assert_eq!(r2.removed, 2);
}
