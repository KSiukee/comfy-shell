use serde_json::json;

/// 超分后替换原图（#86）：把旧图的**提示词参数**搬到新图的元数据里。
///
/// 只搬「这次出图是什么」那几项；模式、尺寸、创建时间、历史 id 必须用新图自己的，
/// 否则图库详情会把一张 4× 图认成原来的 1×。
pub fn merge_upscale_meta(
    old: Option<&serde_json::Value>,
    base: serde_json::Value,
    old_name: &str,
) -> serde_json::Value {
    const COPY: [&str; 12] = [
        "prompt", "negative", "seed", "steps", "cfg", "lora", "loraStrength", "lora2",
        "lora2Strength", "slg", "slgLayers", "layout",
    ];
    let mut out = base;
    if let Some(serde_json::Value::Object(old)) = old {
        if let serde_json::Value::Object(cur) = &mut out {
            for k in COPY {
                if let Some(v) = old.get(k) {
                    if !v.is_null() {
                        cur.insert(k.to_string(), v.clone());
                    }
                }
            }
            if !old_name.is_empty() {
                cur.insert("upscaledFrom".to_string(), json!(old_name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::merge_upscale_meta;
    use serde_json::json;

    #[test]
    fn merges_prompt_params_from_the_old_image() {
        let old = json!({
            "prompt": "雨夜街头，撑伞", "negative": "lowres", "seed": 42, "steps": 16,
            "cfg": 1.0, "lora": "x.safetensors", "layout": {"items": []},
            "mode": "t2i", "width": 1024, "height": 1024,
        });
        let base = json!({"mode": "upscale", "width": 4096, "height": 4096, "createdAt": 1});
        let m = merge_upscale_meta(Some(&old), base, "a.png");
        assert_eq!(m["prompt"], "雨夜街头，撑伞");
        assert_eq!(m["seed"], 42);
        assert_eq!(m["lora"], "x.safetensors");
        assert_eq!(m["upscaledFrom"], "a.png");
        // 新图自己的字段不许被旧图盖掉
        assert_eq!(m["mode"], "upscale");
        assert_eq!(m["width"], 4096);
        assert_eq!(m["createdAt"], 1);
    }

    #[test]
    fn without_old_meta_the_new_meta_is_untouched() {
        let base = json!({"mode": "upscale", "width": 2048, "height": 2048});
        let m = merge_upscale_meta(None, base.clone(), "a.png");
        assert_eq!(m, base);
        assert!(m.get("upscaledFrom").is_none());
    }

    #[test]
    fn null_values_are_not_copied() {
        let old = json!({"prompt": "x", "lora": null});
        let base = json!({"mode": "upscale"});
        let m = merge_upscale_meta(Some(&old), base, "a.png");
        assert_eq!(m["prompt"], "x");
        assert!(m.get("lora").is_none());
    }
}
