//! compiler.rs 的单测（#81）。钉住的是「形状」——语言、长度、原子、清洗——不测模型写得美不美。

use super::*;

#[test]
fn every_profile_has_a_family_and_dialect() {
    let cases = [
        ("qwen21", Family::Qwen, Lang::Zh, "中文长句"),
        ("moss", Family::Moss, Lang::En, "排版散文"),
        ("mossbase", Family::Moss, Lang::En, "排版散文"),
        ("mossimagine", Family::Moss, Lang::En, "排版散文"),
        ("mossedit", Family::Moss, Lang::En, "排版散文"),
        ("kromoss", Family::Moss, Lang::En, "排版散文"),
        ("flux3", Family::Flux, Lang::En, "自然语言"),
        ("flux3base", Family::Flux, Lang::En, "自然语言"),
        ("custom:my-wf", Family::Flux, Lang::En, "自然语言"),
    ];
    for (p, f, lang, label) in cases {
        let d = dialect_of(p);
        assert_eq!(d.family, f, "{p}");
        assert_eq!(d.lang, lang, "{p}");
        assert_eq!(d.label, label, "{p}");
    }
    // 大小写/空白不该改变判定
    assert_eq!(dialect_of("  Qwen21 ").family, Family::Qwen);
}

#[test]
fn dialects_have_sane_length_bands() {
    for p in ["qwen21", "moss", "flux3"] {
        let d = dialect_of(p);
        assert!(d.min_units > 0 && d.max_units > d.min_units, "{p}");
    }
    assert!(dialect_of("qwen21").max_units >= 240);
}

#[test]
fn atoms_keep_quoted_numbers_and_names() {
    let idea = "海报上写“春季特卖 SEN 30% OFF”，3 只猫在窗台，用 iPhone 拍，16:9 横构图，2026 年限定";
    let a = atoms(idea);
    assert!(a.iter().any(|x| x == "春季特卖 SEN 30% OFF"), "{a:?}");
    assert!(a.iter().any(|x| x == "3"), "带量词的数要留：{a:?}");
    assert!(a.iter().any(|x| x == "iPhone"), "{a:?}");
    assert!(a.iter().any(|x| x == "2026"), "{a:?}");
    assert!(!a.iter().any(|x| x.contains(':')), "比例不是原子：{a:?}");
}

#[test]
fn atoms_survive_every_quote_style() {
    let a = atoms("书名叫《雨夜》、招牌是「面馆」、副标『OPEN』、英文 \"Late Night\"");
    let joined = a.join("|");
    for want in ["雨夜", "面馆", "OPEN", "Late Night"] {
        assert!(joined.contains(want), "缺 {want}: {joined}");
    }
}

#[test]
fn atoms_ignore_plain_words_and_tiny_tokens() {
    let a = atoms("a girl standing in the rain, soft light");
    assert!(a.is_empty(), "普通英文句子不该被当成原子：{a:?}");
}

#[test]
fn atoms_drop_fragments_covered_by_a_longer_atom() {
    let a = atoms("标题写“SPRING SALE 30%”，角落标 2026");
    assert!(a.iter().any(|x| x == "SPRING SALE 30%"), "{a:?}");
    assert!(!a.iter().any(|x| x == "SPRING" || x == "SALE" || x == "30"), "碎片别单列：{a:?}");
    assert!(a.iter().any(|x| x == "2026"), "{a:?}");
}

#[test]
fn missing_is_case_and_space_insensitive() {
    let atoms = vec!["Spring SALE".to_string(), "3".to_string()];
    let miss = missing("a poster for the spring   sale with 3 cats", &atoms);
    assert!(miss.is_empty(), "{miss:?}");
    let miss = missing("a poster for spring sale", &atoms);
    assert_eq!(miss, vec!["3".to_string()]);
}

#[test]
fn clean_strips_model_shells() {
    let raw = "以下是编译结果：\n\n```\n# 提示词\n- 雨夜街头，红大衣女孩站在霓虹下，浅景深。\n```\n\n希望你喜欢。";
    let out = clean(raw);
    assert!(out.starts_with("雨夜街头"), "{out}");
    assert!(!out.contains("```") && !out.contains('#'), "{out}");
    assert!(!out.contains("希望你喜欢"), "结尾寒暄也该走：{out}");
}

#[test]
fn clean_unwraps_quotes_but_keeps_single_line() {
    assert_eq!(clean("\"a rainy street at night\""), "a rainy street at night");
    // 只有一句话、又像前言时，不能把唯一内容删光
    assert_eq!(clean("这是雨夜"), "这是雨夜");
    assert_eq!(clean("“雨夜街头，红大衣。”"), "雨夜街头，红大衣。");
}

#[test]
fn strip_ratio_takes_sizes_out_of_the_prompt() {
    let (out, removed) = strip_ratio("a wide 16:9 shot of a 1024x768 poster");
    assert!(!out.contains("16:9") && !out.contains("1024"), "{out}");
    assert_eq!(removed.len(), 2, "{removed:?}");
    // 年份、价格不是比例
    let (out2, r2) = strip_ratio("a 2026 calendar, price 30% off");
    assert!(out2.contains("2026"), "{out2}");
    assert!(r2.is_empty(), "{r2:?}");
}

#[test]
fn units_counts_chinese_chars_and_english_words() {
    assert_eq!(units("雨夜街头"), 4);
    assert_eq!(units("a rainy street"), 3);
    assert_eq!(units("雨夜 street"), 3);
    assert_eq!(units(""), 0);
}

#[test]
fn cjk_ratio_separates_languages() {
    assert!(cjk_ratio("雨夜街头，红大衣女孩") > 0.9);
    assert!(cjk_ratio("a rainy street at night") < 0.1);
}

#[test]
fn fit_truncates_chinese_at_a_sentence_end() {
    let d = dialect_of("qwen21");
    let body = "雨夜街头，红大衣女孩站在霓虹灯下。".repeat(30); // 约 510 字
    let (out, note) = fit(&body, &d);
    assert!(note.is_some(), "超长要说明");
    assert!(units(&out) <= d.max_units, "{} 字", units(&out));
    assert!(out.ends_with('。'), "应停在句末：…{}", &out[out.len().saturating_sub(8)..]);
}

#[test]
fn fit_never_cuts_an_english_word_in_half() {
    let d = dialect_of("flux3");
    let body = "A photograph of a rainy street with neon signs and a woman in a red coat. ".repeat(40);
    let (out, note) = fit(&body, &d);
    assert!(note.is_some());
    assert!(units(&out) <= d.max_units, "{}", units(&out));
    let last = out.trim_end_matches('.').split_whitespace().last().unwrap_or("");
    assert!(body.contains(last), "别切出半个词：{last}");
}

#[test]
fn fit_flags_short_output_without_changing_it() {
    let d = dialect_of("moss");
    let (out, note) = fit("a red chair", &d);
    assert_eq!(out, "a red chair");
    assert!(note.unwrap().contains("偏短"));
}

#[test]
fn review_calls_out_language_atoms_cliches_and_ratio() {
    let d = dialect_of("qwen21");
    let notes = review(
        "A cinematic photo of a girl, masterpiece, 8k, 16:9, spring sale",
        &d,
        &["春季特卖".to_string()],
        None,
        &["16:9".to_string()],
    );
    let all = notes.join("|");
    assert!(all.contains("中文"), "{all}");
    assert!(all.contains("春季特卖"), "{all}");
    assert!(all.contains("masterpiece") || all.contains("8k"), "{all}");
    assert!(all.contains("16:9"), "{all}");
}

#[test]
fn review_is_quiet_on_a_good_compile() {
    let d = dialect_of("moss");
    let notes = review(
        "A poster, upper left, with \"SPRING SALE\" in bold red sans-serif, and a woman in a red coat at frame-right.",
        &d,
        &[],
        None,
        &[],
    );
    assert!(notes.is_empty(), "{notes:?}");
}

#[test]
fn user_message_carries_context_canvas_and_atoms() {
    let atoms = vec!["春季特卖".to_string()];
    let msg = user_message(
        "画一张春季特卖海报",
        Some("画风：90 年代胶片"),
        &atoms,
        true,
        Some("2:3 竖构图（832×1216）".to_string()),
    );
    assert!(msg.contains("画一张春季特卖海报"));
    assert!(msg.contains("90 年代胶片"));
    assert!(msg.contains("春季特卖"));
    assert!(msg.contains("2:3"), "画布要带上");
    assert!(msg.contains("别把比例"), "要提醒别写进正文");

    let free = user_message("画一张春季特卖海报", None, &atoms, false, None);
    assert!(!free.contains("必须原样出现"), "关掉保留时不列原子：{free}");
    assert!(free.contains("可以按你判断重写"));
}

#[test]
fn repair_message_lists_only_the_gaps() {
    let m = repair_message("a poster", &["春季特卖".to_string()]);
    assert!(m.contains("a poster"));
    assert!(m.contains("春季特卖"));
    assert!(m.contains("只输出修补后的完整提示词"));
}

#[test]
fn refuses_look_like_refusals_and_empty_replies() {
    assert!(looks_refused("抱歉，我不能帮你写这个。"));
    assert!(looks_refused("I'm sorry, but I can't help with that request."));
    assert!(looks_refused("ok"));
    assert!(!looks_refused(
        "A rainy street at night, a woman in a red coat walks past neon signs, wet asphalt reflecting pink light."
    ));
}

#[test]
fn system_prompts_differ_per_family_and_stay_on_topic() {
    let q = system_prompt(Family::Qwen);
    let m = system_prompt(Family::Moss);
    let f = system_prompt(Family::Flux);
    assert_ne!(q, m);
    assert_ne!(m, f);
    assert!(q.contains("中文"));
    assert!(m.to_lowercase().contains("layout"));
    assert!(f.to_lowercase().contains("natural language"));
    for s in [q, m, f] {
        assert!(s.contains("not") || s.contains("不写"), "格式约束要写进去");
    }
}
