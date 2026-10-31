use super::*;

fn tmp(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("cs_promptfx_{}_{}", tag, std::process::id()))
}

#[test]
fn reads_styles_and_skips_bad_files() {
    let d = tmp("styles");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("a.json"),
        r#"{"name":"电影感","positive":"cinematic, {prompt}","negative":"text"}"#,
    )
    .unwrap();
    std::fs::write(d.join("b.json"), r#"{"positive":"no name {prompt}"}"#).unwrap();
    std::fs::write(d.join("bad.json"), "{not json").unwrap();
    std::fs::write(d.join("empty.json"), r#"{"positive":"   "}"#).unwrap();
    std::fs::write(d.join("note.txt"), "not a style").unwrap();

    let list = read_styles(&d);
    assert_eq!(list.len(), 2, "坏 JSON 与空 positive 都要跳过");
    let b = list.iter().find(|s| s.name == "b").expect("缺 name 时用文件名");
    assert_eq!(b.positive, "no name {prompt}");
    assert_eq!(b.negative, "");
    let film = list.iter().find(|s| s.name == "电影感").unwrap();
    assert_eq!(film.negative, "text");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn reads_wildcards_with_comments_and_blank_lines() {
    let d = tmp("wild");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("lighting.txt"), "# 注释\n soft window light \n\nneon rim light\n").unwrap();
    std::fs::write(d.join("empty.txt"), "# only comments\n\n").unwrap();
    std::fs::write(d.join("readme.md"), "ignored").unwrap();

    let map = read_wildcards(&d);
    assert_eq!(map.get("lighting").unwrap(), &vec!["soft window light".to_string(), "neon rim light".to_string()]);
    assert!(map.get("empty").is_none(), "全是注释的应跳过");
    assert!(map.get("readme").is_none(), "非 txt 不读");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn seed_file_never_overwrites() {
    let d = tmp("seed");
    let _ = std::fs::remove_dir_all(&d);
    seed_file(&d, "x.txt", "one").unwrap();
    seed_file(&d, "x.txt", "two").unwrap();
    assert_eq!(std::fs::read_to_string(d.join("x.txt")).unwrap(), "one");
    let _ = std::fs::remove_dir_all(&d);
}
