//! vblocks.rs 的单测（#82）：模型给的花样都收得住，收不住的明确报错。

use super::*;

#[test]
fn parses_five_blocks_with_fences_and_chatter() {
    let raw = r#"Sure! Here is the JSON:
```json
{"subject":["silver-haired woman in a red coat","close-up of her face"],
 "camera":["waist-up, eye level","35mm, shallow depth of field"],
 "light":["neon rim light from frame-right","warm practical lamps"],
 "color":["teal shadows","warm skin tones"],
 "style":["cinematic photograph","1990s film still"]}
```
Hope that helps."#;
    let blocks = parse(raw).expect("应当能解析");
    assert_eq!(blocks.len(), 5);
    assert_eq!(blocks[0].key, "subject");
    assert_eq!(blocks[4].key, "style");
    assert_eq!(blocks[1].items[1], "35mm, shallow depth of field");
    assert_eq!(blocks[3].items[0], "teal shadows");
}

#[test]
fn keeps_fixed_order_even_if_the_model_reorders() {
    let raw = r#"{"style":["oil painting","impasto"],"color":["ochre","umber"],
                  "light":["candlelight"],"camera":["close-up"],"subject":["an old sailor"]}"#;
    let blocks = parse(raw).unwrap();
    let keys: Vec<&str> = blocks.iter().map(|b| b.key).collect();
    assert_eq!(keys, BLOCK_KEYS.to_vec());
    assert_eq!(blocks[0].items, vec!["an old sailor"]);
}

#[test]
fn accepts_a_single_string_and_a_wrapped_object() {
    let raw = r#"{"subject":"a woman in a red coat","camera":{"items":["medium shot","eye level"]},
                  "light":"soft window light","color":["teal","amber"],"style":["photograph","film still"]}"#;
    let blocks = parse(raw).unwrap();
    assert_eq!(blocks[0].items, vec!["a woman in a red coat"]);
    assert_eq!(blocks[1].items, vec!["medium shot", "eye level"]);
    assert_eq!(blocks[2].items, vec!["soft window light"]);
}

#[test]
fn pipes_split_one_string_into_candidates() {
    let raw = r#"{"subject":["a"],"camera":["b"],"light":["c"],
                  "color":["teal shadows | amber highlights"],"style":["d"]}"#;
    let blocks = parse(raw).unwrap();
    let color = &blocks[3].items;
    assert_eq!(color, &vec!["teal shadows".to_string(), "amber highlights".to_string()]);
}

#[test]
fn dedupes_and_caps_at_three() {
    let raw = r#"{"subject":["a woman","A woman","a man","a child","a dog"],
                  "camera":["x"],"light":["y"],"color":["z"],"style":["w"]}"#;
    let blocks = parse(raw).unwrap();
    assert_eq!(blocks[0].items, vec!["a woman", "a man", "a child"]);
}

#[test]
fn truncates_sentences_to_twelve_words() {
    let long = "a very long sentence about a woman standing in a rainy street at night with neon signs everywhere behind her";
    let raw = format!(
        r#"{{"subject":["{long}"],"camera":["x"],"light":["y"],"color":["z"],"style":["w"]}}"#
    );
    let blocks = parse(&raw).unwrap();
    let item = &blocks[0].items[0];
    assert_eq!(item.split_whitespace().count(), MAX_WORDS, "{item}");
}

#[test]
fn tidy_strips_numbering_quotes_and_end_punctuation() {
    assert_eq!(tidy("1. soft window light."), "soft window light");
    assert_eq!(tidy("- “teal shadows”"), "teal shadows");
    assert_eq!(tidy("2) 35mm, shallow"), "35mm, shallow");
    assert_eq!(tidy("  neon   rim light  ; "), "neon rim light");
    assert_eq!(tidy(""), "");
}

#[test]
fn needs_at_least_three_filled_blocks() {
    let raw = r#"{"subject":["a woman"],"camera":["x"],"light":[],"color":[],"style":[]}"#;
    let err = parse(raw).unwrap_err();
    assert!(err.contains("只认出 2 块"), "{err}");
}

#[test]
fn rejects_garbage_with_a_clear_error() {
    assert!(parse("the model refused to answer").unwrap_err().contains("没找到 JSON"));
    assert!(parse("{\"subject\": [oops}").is_err());
    assert!(parse("[1,2,3]").is_err());
}

#[test]
fn tolerates_raw_newlines_inside_strings() {
    // 模型偶尔在 JSON 字符串里直接换行；先原文失败，再转义重试
    let raw = "{\"subject\":[\"a woman in\na red coat\"],\"camera\":[\"x\"],\"light\":[\"y\"],\"color\":[\"z\"],\"style\":[\"w\"]}";
    let blocks = parse(raw).unwrap();
    assert_eq!(blocks[0].items[0], "a woman in a red coat");
}

#[test]
fn keys_are_case_insensitive() {
    let raw = r#"{"Subject":["a"],"CAMERA":["b"],"Light":["c"],"Color":["d"],"Style":["e"]}"#;
    let blocks = parse(raw).unwrap();
    assert!(blocks.iter().all(|b| !b.items.is_empty()));
}
