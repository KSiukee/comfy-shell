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
fn reads_upstream_array_style_files() {
    let d = tmp("array");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    // sdxl_prompt_styler / Fooocus 的形状：一整个数组，键叫 prompt / negative_prompt
    std::fs::write(
        d.join("sdxl_styles_twri.json"),
        r#"[
            {"name":"sai-cinematic","prompt":"cinematic film still {prompt} . film grain","negative_prompt":"anime, text"},
            {"name":"sai-anime","prompt":"anime artwork {prompt} . vibrant","negative_prompt":"photo"},
            {"name":"","prompt":"   ","negative_prompt":"skip me"}
        ]"#,
    )
    .unwrap();
    let list = read_styles(&d);
    assert_eq!(list.len(), 2, "空 positive 的那条要跳过");
    let film = list.iter().find(|s| s.name == "sai-cinematic").unwrap();
    assert_eq!(film.positive, "cinematic film still {prompt} . film grain");
    assert_eq!(film.negative, "anime, text");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn import_normalizes_keys_and_never_overwrites() {
    let d = tmp("import");
    let _ = std::fs::remove_dir_all(&d);
    let src = r#"[
        {"name":"sai-anime","prompt":"anime {prompt}","negative_prompt":"photo"},
        {"name":"sai-film","prompt":"film {prompt}","negative_prompt":""}
    ]"#;
    let (n, p1) = import_styles_into(&d, "twri pack", src).unwrap();
    assert_eq!(n, 2);
    assert!(p1.exists());
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&p1).unwrap()).unwrap();
    assert_eq!(saved[0]["positive"], "anime {prompt}");
    assert_eq!(saved[0]["negative"], "photo");
    assert_eq!(saved[0]["zh"], "动画", "导入时把中文名一起写进去");

    let (_, p2) = import_styles_into(&d, "twri pack", src).unwrap();
    assert_ne!(p1, p2, "同名导入要另起一号，别覆盖");
    assert!(import_styles_into(&d, "bad", "{not json").is_err());
    assert!(import_styles_into(&d, "empty", "[]").is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn upstream_names_get_chinese_labels() {
    let d = tmp("zh");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("up.json"),
        r#"[
            {"name":"photo-silhouette","prompt":"silhouette {prompt}"},
            {"name":"ads-fashion editorial","prompt":"fashion {prompt}"},
            {"name":"my-own-style","prompt":"own {prompt}"},
            {"name":"photo-hdr","zh":"高动态","prompt":"hdr {prompt}"}
        ]"#,
    )
    .unwrap();
    let list = read_styles(&d);
    let by = |n: &str| list.iter().find(|s| s.name == n).unwrap();
    assert_eq!(by("photo-silhouette").zh, "剪影");
    assert_eq!(by("ads-fashion editorial").zh, "时尚杂志");
    assert_eq!(by("my-own-style").zh, "", "表里没有就留空，界面回退英文名");
    assert_eq!(by("photo-hdr").zh, "高动态", "文件里写了的优先");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn delete_style_keeps_the_rest_and_drops_empty_files() {
    let d = tmp("del");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("pack.json"),
        r#"[{"name":"a","prompt":"p {prompt}"},{"name":"b","prompt":"q {prompt}"}]"#,
    )
    .unwrap();
    std::fs::write(d.join("solo.json"), r#"{"name":"solo","prompt":"s {prompt}"}"#).unwrap();

    assert!(delete_style_in(&d, "a").unwrap());
    let left = read_styles(&d);
    assert!(!left.iter().any(|s| s.name == "a"));
    assert!(left.iter().any(|s| s.name == "b"), "同文件里的其余条目要留着");
    assert_eq!(left.iter().filter(|s| s.name == "b").count(), 1, "不能删重");

    assert!(delete_style_in(&d, "solo").unwrap());
    assert!(!d.join("solo.json").exists(), "删空的文件要一起删掉");
    assert!(!delete_style_in(&d, "nope").unwrap(), "找不到就如实返回 false");
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
