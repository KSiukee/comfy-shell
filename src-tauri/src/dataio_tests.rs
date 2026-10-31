// 备份 / 导出 / 导入的测试。
use super::*;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cs_dataio_{tag}_{}", std::process::id()))
}

#[test]
fn export_and_import_project_roundtrip() {
    let cfg = tmp("cfg");
    let out = tmp("out");
    let dest = tmp("dest");
    for d in [&cfg, &out, &dest] {
        let _ = std::fs::remove_dir_all(d);
    }
    let p = crate::project::create(&cfg, "导出测试").unwrap();
    let imgs = out.join("projects").join(&p.id);
    std::fs::create_dir_all(&imgs).unwrap();
    std::fs::write(imgs.join("a.png"), b"img").unwrap();

    let dir = export_project(&cfg, &out, &p.id, &dest).unwrap();
    assert!(dir.join("project.json").is_file());
    assert!(dir.join("images").join("a.png").is_file());
    assert!(dir.join("export.json").is_file());

    let imported = import_project(&cfg, &out, &dir).unwrap();
    assert_ne!(imported.id, p.id);
    assert!(imported.name.ends_with("（导入）"));
    assert!(out
        .join("projects")
        .join(&imported.id)
        .join("a.png")
        .is_file());

    for d in [&cfg, &out, &dest] {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[test]
fn backup_copies_config_and_skips_cache() {
    let cfg = tmp("bcfg");
    let dest = tmp("bdest");
    for d in [&cfg, &dest] {
        let _ = std::fs::remove_dir_all(d);
    }
    std::fs::create_dir_all(&cfg).unwrap();
    std::fs::write(cfg.join("settings.json"), "{}").unwrap();
    std::fs::write(cfg.join("library_cache.json"), "big").unwrap();
    std::fs::create_dir_all(cfg.join("vault")).unwrap();
    std::fs::write(cfg.join("vault").join("key.bin"), b"cipher").unwrap();

    let dir = backup(&cfg, &dest).unwrap();
    assert!(dir.join("settings.json").is_file());
    assert!(dir.join("vault").join("key.bin").is_file());
    assert!(!dir.join("library_cache.json").exists(), "缓存不该进备份");

    let _ = std::fs::remove_dir_all(&cfg);
    let _ = std::fs::remove_dir_all(&dest);
}
