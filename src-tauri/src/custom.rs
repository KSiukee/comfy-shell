//! Custom ComfyUI API-format workflows imported by the user.
//!
//! Stored as plain JSON files under `<config>/workflows/`. Placeholders that may
//! appear anywhere in the JSON are substituted at run time:
//!   {{prompt}} {{negative}} {{seed}} {{width}} {{height}} {{steps}} {{cfg}}

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn dir(cfg: &Path) -> PathBuf {
    cfg.join("workflows")
}

pub(crate) fn safe_name(name: &str) -> String {
    // Keep CJK and spaces; only strip what Windows forbids in file names.
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(60)
        .collect();
    cleaned.trim().trim_end_matches('.').to_string()
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

pub fn save(cfg: &Path, name: &str, json_text: &str) -> Result<String, String> {
    let name = safe_name(name);
    if name.is_empty() {
        return Err("工作流名称不能为空".into());
    }
    let v: Value = serde_json::from_str(json_text).map_err(|e| format!("不是合法 JSON：{e}"))?;
    let obj = v.as_object().ok_or("工作流必须是节点映射（API 格式）")?;
    let looks_like_prompt = obj.values().all(|n| {
        n.get("class_type").and_then(|c| c.as_str()).is_some() && n.get("inputs").is_some()
    });
    if obj.is_empty() || !looks_like_prompt {
        return Err("这不像是 ComfyUI 的 API 格式工作流（每个节点应有 class_type 与 inputs）".into());
    }
    std::fs::create_dir_all(dir(cfg)).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&dir(cfg).join(format!("{name}.json")), json_text)?;
    Ok(name)
}

pub fn load(cfg: &Path, name: &str) -> Result<Value, String> {
    let p = dir(cfg).join(format!("{}.json", safe_name(name)));
    let text = std::fs::read_to_string(&p).map_err(|e| format!("读取工作流失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析工作流失败：{e}"))
}

pub fn delete(cfg: &Path, name: &str) -> Result<(), String> {
    let p = dir(cfg).join(format!("{}.json", safe_name(name)));
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

/// Replace `{{token}}` placeholders in the serialized graph.
pub fn substitute(
    graph: &Value,
    prompt: &str,
    negative: &str,
    seed: i64,
    width: u32,
    height: u32,
    steps: u32,
    cfg: f64,
) -> Value {
    let raw = serde_json::to_string(graph).unwrap_or_default();
    let escaped = |s: &str| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into());
    let prompt_json = escaped(prompt);
    let negative_json = escaped(negative);
    // values are JSON-escaped strings without the surrounding quotes where numeric
    let text = raw
        .replace("{{prompt}}", prompt_json.trim_matches('"'))
        .replace("{{negative}}", negative_json.trim_matches('"'))
        .replace("{{seed}}", &seed.to_string())
        .replace("{{width}}", &width.to_string())
        .replace("{{height}}", &height.to_string())
        .replace("{{steps}}", &steps.to_string())
        .replace("{{cfg}}", &cfg.to_string());
    serde_json::from_str(&text).unwrap_or_else(|_| graph.clone())
}

/// 一个可调的节点输入（从工作流里推断出来的，不是用户手写的）。
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Param {
    /// `节点id|输入名`，覆盖时按这个键找
    pub key: String,
    pub node: String,
    pub input: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub default: Value,
    pub options: Vec<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

const SAMPLERS: &[&str] = &[
    "euler", "euler_ancestral", "dpmpp_2m", "dpmpp_2m_sde", "dpmpp_3m_sde", "ddim", "uni_pc",
    "lcm", "res_multistep",
];
const SCHEDULERS: &[&str] = &["normal", "karras", "exponential", "sgm_uniform", "simple", "ddim_uniform", "beta"];

/// 这些输入名值得做成控件；别的字符串输入（提示词之类）不碰。
const KNOWN_INPUTS: &[&str] = &[
    "steps", "cfg", "denoise", "seed", "noise_seed", "width", "height", "batch_size", "strength",
    "strength_model", "strength_clip", "weight", "start_at_step", "end_at_step", "shift",
    "sampler_name", "scheduler", "upscale_method", "grow_mask_by", "expand",
];

fn param_range(name: &str) -> (Option<f64>, Option<f64>) {
    match name {
        "steps" => (Some(1.0), Some(150.0)),
        "cfg" => (Some(0.0), Some(30.0)),
        "denoise" => (Some(0.0), Some(1.0)),
        "width" | "height" => (Some(64.0), Some(4096.0)),
        "batch_size" => (Some(1.0), Some(16.0)),
        "strength" | "strength_model" | "strength_clip" | "weight" => (Some(0.0), Some(2.0)),
        "start_at_step" | "end_at_step" => (Some(0.0), Some(150.0)),
        _ => (None, None),
    }
}

/// 从图里推出可调参数：只取没接线的标量输入，名字在已知集合里（枚举另给候选）。
pub fn infer_params(graph: &Value) -> Vec<Param> {
    let Some(obj) = graph.as_object() else {
        return Vec::new();
    };
    let mut out: Vec<Param> = Vec::new();
    for (node_id, node) in obj {
        let Some(inputs) = node.get("inputs").and_then(|i| i.as_object()) else {
            continue;
        };
        let title = node
            .get("_meta")
            .and_then(|m| m.get("title"))
            .and_then(|t| t.as_str())
            .or_else(|| node.get("class_type").and_then(|c| c.as_str()))
            .unwrap_or("节点");
        for (name, val) in inputs {
            if val.is_array() {
                continue; // 连线
            }
            let ln = name.to_ascii_lowercase();
            let known = KNOWN_INPUTS.contains(&ln.as_str());
            let kind = if matches!(ln.as_str(), "sampler_name" | "scheduler") {
                "enum"
            } else if val.is_boolean() {
                "bool"
            } else if val.is_i64() || val.is_u64() {
                "int"
            } else if val.is_f64() {
                "float"
            } else if val.is_string() {
                "string"
            } else {
                continue;
            };
            if !known && kind != "enum" {
                continue;
            }
            let options = match ln.as_str() {
                "sampler_name" => SAMPLERS.iter().map(|s| s.to_string()).collect(),
                "scheduler" => SCHEDULERS.iter().map(|s| s.to_string()).collect(),
                _ => Vec::new(),
            };
            let (min, max) = param_range(&ln);
            out.push(Param {
                key: format!("{node_id}|{name}"),
                node: node_id.clone(),
                input: name.clone(),
                label: format!("{title} · {name}"),
                kind: kind.to_string(),
                default: val.clone(),
                options,
                min,
                max,
            });
        }
    }
    out.sort_by(|a, b| {
        let na = a.node.parse::<i64>().unwrap_or(i64::MAX);
        let nb = b.node.parse::<i64>().unwrap_or(i64::MAX);
        na.cmp(&nb).then(a.input.cmp(&b.input))
    });
    out.truncate(16);
    out
}

/// 按 `节点|输入` 覆盖；接线的输入不覆盖（改了会把图改坏）。
pub fn apply_overrides(graph: &mut Value, overrides: &BTreeMap<String, Value>) {
    for (key, val) in overrides {
        let Some((node, input)) = key.split_once('|') else {
            continue;
        };
        let Some(inputs) = graph
            .get_mut(node)
            .and_then(|n| n.get_mut("inputs"))
            .and_then(|i| i.as_object_mut())
        else {
            continue;
        };
        if inputs.get(input).map(|v| v.is_array()).unwrap_or(false) {
            continue;
        }
        inputs.insert(input.to_string(), val.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_overrides, infer_params, safe_name};
    use std::collections::BTreeMap;


    #[test]
    fn keeps_cjk_and_replaces_forbidden() {
        assert_eq!(safe_name("海报:双十一?"), "海报_双十一_");
        assert_eq!(safe_name("a/b\\c"), "a_b_c");
    }

    #[test]
    fn infers_scalar_inputs_and_skips_wires() {
        let g = serde_json::json!({
            "3": {"class_type": "KSampler", "_meta": {"title": "采样"},
                  "inputs": {"seed": 42, "steps": 30, "cfg": 4.5, "denoise": 1.0,
                             "sampler_name": "euler", "scheduler": "normal",
                             "model": ["4", 0], "positive": ["6", 0], "negative": ["7", 0]}},
            "10": {"class_type": "LoraLoader", "inputs": {"lora_name": "x.safetensors",
                   "strength_model": 0.8, "model": ["4", 0]}}
        });
        let ps = infer_params(&g);
        let names: Vec<&str> = ps.iter().map(|p| p.input.as_str()).collect();
        assert!(names.contains(&"steps") && names.contains(&"cfg") && names.contains(&"seed"));
        assert!(names.contains(&"strength_model"));
        assert!(!names.contains(&"model"), "接线的输入不能做成控件");
        assert!(!names.contains(&"lora_name"), "任意字符串不碰");
        let steps = ps.iter().find(|p| p.input == "steps").unwrap();
        assert_eq!(steps.kind, "int");
        assert_eq!(steps.label, "采样 · steps");
        assert_eq!(steps.key, "3|steps");
        assert_eq!((steps.min, steps.max), (Some(1.0), Some(150.0)));
        let sampler = ps.iter().find(|p| p.input == "sampler_name").unwrap();
        assert_eq!(sampler.kind, "enum");
        assert!(sampler.options.contains(&"euler".to_string()));
        let cfg = ps.iter().find(|p| p.input == "cfg").unwrap();
        assert_eq!(cfg.kind, "float");
    }

    #[test]
    fn overrides_scalars_but_never_wires() {
        let mut g = serde_json::json!({
            "3": {"class_type": "KSampler", "inputs": {"steps": 30, "positive": ["6", 0]}}
        });
        let mut ov = BTreeMap::new();
        ov.insert("3|steps".to_string(), serde_json::json!(55));
        ov.insert("3|positive".to_string(), serde_json::json!("不该生效"));
        ov.insert("9|steps".to_string(), serde_json::json!(1));
        apply_overrides(&mut g, &ov);
        assert_eq!(g["3"]["inputs"]["steps"], serde_json::json!(55));
        assert_eq!(g["3"]["inputs"]["positive"], serde_json::json!(["6", 0]));
    }

    #[test]
    fn trims_and_caps_length() {
        assert_eq!(safe_name("  a.b.  "), "a.b");
        assert_eq!(safe_name(&"x".repeat(80)).len(), 60);
        assert_eq!(safe_name("   "), "");
    }
}

