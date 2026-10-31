//! User-saved templates for the MOSS layout editor.
//!
//! One JSON file per template under `<config>/layouts/<name>.json`. The file is
//! exactly the array the editor keeps in memory:
//!   [{"type":"text","desc":"headline","text":"SALE","bbox":[x,y,w,h]}, ...]
//! `bbox` is normalized 0..1 relative to the canvas, so a template is
//! resolution independent (the editor converts it to MOSS yxyx at generate time).

use serde_json::Value;
use std::path::{Path, PathBuf};

fn dir(cfg: &Path) -> PathBuf {
    cfg.join("layouts")
}

fn file(cfg: &Path, name: &str) -> PathBuf {
    dir(cfg).join(format!("{}.json", crate::custom::safe_name(name)))
}

pub fn list(cfg: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir(cfg)) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                if let Some(stem) = p.file_stem().and_then(|x| x.to_str()) {
                    out.push(stem.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

/// Validate that `v` is a layout item array the editor can render.
fn check_items(v: &Value) -> Result<(), String> {
    let arr = v.as_array().ok_or("排版模板必须是元素数组")?;
    if arr.is_empty() {
        return Err("模板里还没有元素".into());
    }
    if arr.len() > 64 {
        return Err("模板元素超过 64 个".into());
    }
    for (i, it) in arr.iter().enumerate() {
        let obj = it.as_object().ok_or_else(|| format!("第 {} 个元素不是对象", i + 1))?;
        let kind = obj.get("type").and_then(|x| x.as_str()).unwrap_or("");
        if kind != "text" && kind != "object" {
            return Err(format!("第 {} 个元素的 type 必须是 text 或 object", i + 1));
        }
        if obj.get("desc").and_then(|x| x.as_str()).is_none() {
            return Err(format!("第 {} 个元素缺 desc 字符串", i + 1));
        }
        if kind == "text" && obj.get("text").and_then(|x| x.as_str()).is_none() {
            return Err(format!("第 {} 个文字元素缺 text 字符串", i + 1));
        }
        let bbox = obj
            .get("bbox")
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("第 {} 个元素缺 bbox 数组", i + 1))?;
        if bbox.len() != 4 {
            return Err(format!("第 {} 个元素的 bbox 要 4 个数（x,y,w,h）", i + 1));
        }
        for n in bbox {
            let f = n.as_f64().ok_or_else(|| format!("第 {} 个元素 bbox 不是数字", i + 1))?;
            if !(0.0..=1.0).contains(&f) {
                return Err(format!("第 {} 个元素 bbox 超出 0..1", i + 1));
            }
        }
        if bbox[2].as_f64().unwrap_or(0.0) <= 0.0 || bbox[3].as_f64().unwrap_or(0.0) <= 0.0 {
            return Err(format!("第 {} 个元素宽或高为 0", i + 1));
        }
    }
    Ok(())
}

/// 校验一份 MOSS 排版 JSON，按官方 `references/layout.md` 的规矩来：
/// - 草稿：`width/height`（16 的倍数）+ `items[].label/caption/bbox`（abs、xyxy、像素）
/// - 编译后：`bbox_coord_space=abs_centralized` + `bbox_format=yxyx`（没有画布字段）
/// 只报问题、不改东西；空 Vec 表示通过。
pub fn check_layout(text: &str) -> Result<Vec<String>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("不是合法 JSON：{e}"))?;
    let mut errs: Vec<String> = Vec::new();
    let space = v.get("bbox_coord_space").and_then(|x| x.as_str()).unwrap_or("");
    let fmt = v.get("bbox_format").and_then(|x| x.as_str()).unwrap_or("");
    let compiled = space == "abs_centralized" || fmt == "yxyx";
    let draft = !compiled;

    let (w, h) = if draft {
        let w = v.get("width").and_then(|x| x.as_i64()).unwrap_or(0);
        let h = v.get("height").and_then(|x| x.as_i64()).unwrap_or(0);
        if w <= 0 || h <= 0 {
            errs.push("草稿要写 width / height（正整数）".into());
        } else {
            if w % 16 != 0 {
                errs.push(format!("width {w} 不是 16 的倍数（MOSS 要求）"));
            }
            if h % 16 != 0 {
                errs.push(format!("height {h} 不是 16 的倍数（MOSS 要求）"));
            }
        }
        (w, h)
    } else {
        (0, 0)
    };

    let summary = v
        .get("summary")
        .and_then(|x| x.get("caption"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim();
    if summary.is_empty() {
        errs.push("summary.caption 不能为空".into());
    }

    let items = v.get("items").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    if items.is_empty() {
        errs.push("items 不能为空".into());
    }
    for (i, it) in items.iter().enumerate() {
        let n = i + 1;
        let label = it.get("label").and_then(|x| x.as_str()).unwrap_or("");
        if !["object", "character", "text"].contains(&label) {
            errs.push(format!("第 {n} 项 label 只能是 object / character / text"));
        }
        if it.get("caption").and_then(|x| x.as_str()).unwrap_or("").trim().is_empty() {
            errs.push(format!("第 {n} 项 caption 不能为空"));
        }
        if label == "text" && it.get("text").and_then(|x| x.as_str()).unwrap_or("").trim().is_empty() {
            errs.push(format!("第 {n} 项是 text，必须写要渲染的原字（text）"));
        }
        let Some(bbox) = it.get("bbox").and_then(|x| x.as_array()) else {
            errs.push(format!("第 {n} 项缺 bbox"));
            continue;
        };
        if bbox.len() != 4 {
            errs.push(format!("第 {n} 项 bbox 要 4 个数"));
            continue;
        }
        let Some(nums) = bbox.iter().map(|x| x.as_f64()).collect::<Option<Vec<f64>>>() else {
            errs.push(format!("第 {n} 项 bbox 里有非数字"));
            continue;
        };
        if nums[2] <= nums[0] || nums[3] <= nums[1] {
            errs.push(format!("第 {n} 项 bbox 的右下角要大于左上角"));
        }
        if draft && w > 0 && h > 0 {
            if nums[0] < 0.0 || nums[1] < 0.0 || nums[2] > w as f64 || nums[3] > h as f64 {
                errs.push(format!("第 {n} 项 bbox 超出画布 {w}×{h}"));
            }
        }
    }
    Ok(errs)
}

pub fn load(cfg: &Path, name: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(file(cfg, name))
        .map_err(|e| format!("读取模板失败：{e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("解析模板失败：{e}"))?;
    check_items(&v)?;
    Ok(v)
}

pub fn save(cfg: &Path, name: &str, items_json: &str) -> Result<String, String> {
    let name = crate::custom::safe_name(name);
    if name.is_empty() {
        return Err("模板名称不能为空".into());
    }
    let v: Value = serde_json::from_str(items_json).map_err(|e| format!("不是合法 JSON：{e}"))?;
    check_items(&v)?;
    std::fs::create_dir_all(dir(cfg)).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&file(cfg, &name), &text)?;
    Ok(name)
}

pub fn delete(cfg: &Path, name: &str) -> Result<(), String> {
    let p = file(cfg, name);
    if !p.is_file() {
        return Err("模板不存在".into());
    }
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn temp_cfg(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cs_layouts_{tag}_{}", std::process::id()))
    }

    #[test]
    fn accepts_editor_items() {
        let items = json!([
            {"type": "text", "desc": "headline", "text": "SALE", "bbox": [0.1, 0.1, 0.8, 0.2]},
            {"type": "object", "desc": "cup", "bbox": [0.5, 0.5, 0.2, 0.2]},
        ]);
        assert!(check_items(&items).is_ok());
    }

    #[test]
    fn rejects_bad_items() {
        assert!(check_items(&json!({})).is_err());
        assert!(check_items(&json!([])).is_err());
        assert!(check_items(&json!([{"type": "text", "desc": "x", "bbox": [0, 0, 1, 1]}])).is_err());
        assert!(check_items(&json!([{"type": "object", "desc": "x", "bbox": [0, 0, 2, 1]}])).is_err());
        assert!(check_items(&json!([{"type": "object", "desc": "x", "bbox": [0, 0, 0, 1]}])).is_err());
        assert!(check_items(&json!([{"type": "object", "bbox": [0, 0, 1, 1]}])).is_err());
    }

    #[test]
    fn check_layout_accepts_draft_and_compiled() {
        let draft = json!({
            "width": 1536, "height": 1024,
            "bbox_coord_space": "abs", "bbox_format": "xyxy",
            "summary": {"caption": "campaign"},
            "items": [
                {"label": "object", "caption": "cup", "bbox": [176, 224, 800, 864]},
                {"label": "text", "caption": "title", "text": "把日常，过成诗。", "bbox": [928, 272, 1408, 448]}
            ]
        });
        assert!(check_layout(&draft.to_string()).unwrap().is_empty(), "{:?}", check_layout(&draft.to_string()));

        let compiled = json!({
            "summary": {"caption": "campaign"},
            "items": [{"label": "object", "caption": "cup", "bbox": [-288, -592, 352, 32]}],
            "bbox_coord_space": "abs_centralized", "bbox_format": "yxyx"
        });
        assert!(check_layout(&compiled.to_string()).unwrap().is_empty());
    }

    #[test]
    fn check_layout_reports_real_problems() {
        let cases = [
            (json!({"width": 1000, "height": 1024, "summary": {"caption": "x"},
                    "items": [{"label": "object", "caption": "cup", "bbox": [0, 0, 10, 10]}]}),
             "16 的倍数"),
            (json!({"width": 1024, "height": 1024, "summary": {"caption": ""},
                    "items": [{"label": "object", "caption": "cup", "bbox": [0, 0, 10, 10]}]}),
             "summary.caption"),
            (json!({"width": 1024, "height": 1024, "summary": {"caption": "x"}, "items": []}),
             "items 不能为空"),
            (json!({"width": 1024, "height": 1024, "summary": {"caption": "x"},
                    "items": [{"label": "widget", "caption": "cup", "bbox": [0, 0, 10, 10]}]}),
             "label"),
            (json!({"width": 1024, "height": 1024, "summary": {"caption": "x"},
                    "items": [{"label": "text", "caption": "title", "bbox": [0, 0, 10, 10]}]}),
             "原字"),
            (json!({"width": 1024, "height": 1024, "summary": {"caption": "x"},
                    "items": [{"label": "object", "caption": "cup", "bbox": [900, 0, 1200, 100]}]}),
             "超出画布"),
            (json!({"width": 1024, "height": 1024, "summary": {"caption": "x"},
                    "items": [{"label": "object", "caption": "cup", "bbox": [100, 0, 100, 50]}]}),
             "右下角"),
        ];
        for (doc, want) in cases {
            let errs = check_layout(&doc.to_string()).unwrap();
            assert!(
                errs.iter().any(|e| e.contains(want)),
                "期望有包含「{want}」的错误，实际 {errs:?}"
            );
        }
        assert!(check_layout("{not json").is_err());
    }

    #[test]
    fn save_load_delete_roundtrip() {
        let cfg = temp_cfg("roundtrip");
        let _ = std::fs::remove_dir_all(&cfg);
        let items = json!([
            {"type": "object", "desc": "cup", "bbox": [0.5, 0.5, 0.2, 0.2]},
        ]);
        let name = save(&cfg, "夜间/海报", &items.to_string()).unwrap();
        assert_eq!(name, "夜间_海报");
        assert_eq!(list(&cfg), vec![name.clone()]);
        let loaded = load(&cfg, &name).unwrap();
        assert_eq!(loaded.as_array().unwrap().len(), 1);
        delete(&cfg, &name).unwrap();
        assert!(load(&cfg, &name).is_err());
        let _ = std::fs::remove_dir_all(&cfg);
    }
}

