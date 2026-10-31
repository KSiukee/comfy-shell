// 锁定区模块的测试（拆出来，别把 vault.rs 顶过 400 行）。
use super::*;

    use super::*;

    fn tmpcfg(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cs_vault_{tag}_{}", std::process::id()))
    }

    #[test]
    fn wrap_unwrap_and_wrong_password() {
        let master = [7u8; KEY_LEN];
        let wrapped = wrap_key("hunter2", &master).unwrap();
        let got = unwrap_key("hunter2", &wrapped).unwrap();
        assert_eq!(got, master);
        assert!(unwrap_key("wrong", &wrapped).is_err());
    }

    #[test]
    fn seal_open_roundtrip_and_tamper() {
        let key = [9u8; KEY_LEN];
        let sealed = seal(&key, b"hello vault").unwrap();
        assert_eq!(open(&key, &sealed).unwrap(), b"hello vault");
        let mut bad = sealed.clone();
        let n = bad.len();
        bad[n - 1] ^= 0xff;
        assert!(open(&key, &bad).is_err());
    }


#[test]
fn make_private_star_and_unprivate_roundtrip() {
    let cfg = tmpcfg("roundtrip");
    let out = tmpcfg("roundtrip_out");
    let _ = std::fs::remove_dir_all(&cfg);
    let _ = std::fs::remove_dir_all(&out);
    let p = crate::project::create(&cfg, "私密测试").unwrap();
    let dir = out.join("projects").join(&p.id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.png"), b"fake-png").unwrap();

    setup(&cfg, "test1234").unwrap();
    let v = make_private(&cfg, &out, &p.id).unwrap();
    assert!(v.private);
    assert!(!dir.exists(), "明文项目目录要消失");
    assert_eq!(list(&cfg).len(), 1);
    set_star(&cfg, &p.id, "a.png", true).unwrap();
    let items = lib_items(&cfg);
    assert_eq!(items.len(), 1);
    assert!(items[0].starred);
    assert!(read_image(&cfg, &p.id, "a.png").is_ok());

    let u = unprivate(&cfg, &out, &p.id).unwrap();
    assert!(!u.private);
    assert!(dir.join("a.png").exists(), "移出后图片要回来");
    lock();
    let _ = std::fs::remove_dir_all(&cfg);
    let _ = std::fs::remove_dir_all(&out);
}
