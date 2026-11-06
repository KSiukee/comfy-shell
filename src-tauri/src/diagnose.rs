//! 崩图诊断：给一张坏图 + 当次参数，回「原因猜测 + 可执行的改法」。
//!
//! 这是独立能力（跟看图拆提示词一样吃视觉模型），所以单独成模块：
//! 云端 / 本地的闸门、参数收紧、JSON 解析都在这里，命令层只做转发。

use crate::*;
use crate::commands::ai::cloud_allowed;

/// 把诊断模型的回答收成界面能用的形状：原因最多 3 条，修复只留「能改表单」的四个字段。
/// 模型偶尔会带代码块或多余的话，所以先用 extract_json 抠出对象；越界数值在这里夹紧。
fn parse_diagnose(raw: &str) -> Result<serde_json::Value, String> {
    let v = crate::agent::extract_json(raw).ok_or("诊断结果不是 JSON（模型没按要求回答）")?;
    let obj = v.as_object().ok_or("诊断结果不是对象")?;

    let mut causes: Vec<serde_json::Value> = Vec::new();
    if let Some(arr) = obj.get("causes").and_then(|c| c.as_array()) {
        for c in arr.iter().take(3) {
            let title = c.get("title").and_then(|x| x.as_str()).unwrap_or("").trim();
            if title.is_empty() {
                continue;
            }
            let why = c.get("why").and_then(|x| x.as_str()).unwrap_or("").trim();
            causes.push(serde_json::json!({
                "title": title.chars().take(60).collect::<String>(),
                "why": why.chars().take(160).collect::<String>(),
            }));
        }
    }

    let fixes = obj.get("fixes").cloned().unwrap_or(serde_json::json!({}));
    let cfg = fixes
        .get("cfg")
        .and_then(|x| x.as_f64())
        .filter(|x| *x > 0.0)
        .map(|x| ((x.clamp(1.0, 20.0)) * 10.0).round() / 10.0);
    let steps = fixes
        .get("steps")
        .and_then(|x| x.as_f64())
        .filter(|x| *x > 0.0)
        .map(|x| (x.clamp(4.0, 80.0)).round() as i64);
    let neg = fixes
        .get("negativeAdd")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .chars()
        .take(300)
        .collect::<String>();
    let hint = fixes
        .get("promptHint")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .chars()
        .take(200)
        .collect::<String>();

    let note = obj.get("note").and_then(|x| x.as_str()).unwrap_or("").trim();
    Ok(serde_json::json!({
        "causes": causes,
        "fixes": {
            "cfg": cfg,
            "steps": steps,
            "negativeAdd": neg,
            "promptHint": hint,
        },
        "note": note.chars().take(120).collect::<String>(),
    }))
}

/// 诊断的图可能来自输出目录（绝对路径），也可能已经在 ComfyUI/input 里。
/// 本地视觉节点只认 input 里的文件名，所以绝对路径要先拷一份进去（不动原图）。
fn local_input_name(root: &str, image: &str) -> Result<String, String> {
    let input_dir = std::path::Path::new(root)
        .join("ComfyUI")
        .join("input");
    let src = std::path::Path::new(image);
    if !src.is_file() {
        return Ok(image.to_string()); // 当作 input 里的名字
    }
    if let (Ok(f), Ok(d)) = (src.canonicalize(), input_dir.canonicalize()) {
        if f.parent() == Some(d.as_path()) {
            return Ok(src.file_name().unwrap_or_default().to_string_lossy().to_string());
        }
    }
    std::fs::create_dir_all(&input_dir).map_err(|e| format!("建 input 目录失败：{e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!("diag_{stamp}.png");
    std::fs::copy(src, input_dir.join(&name)).map_err(|e| format!("拷进 input 失败：{e}"))?;
    Ok(name)
}

/// 崩图诊断：给一张坏图 + 当次参数，回「原因猜测 + 可执行的改法」（人点确认后才改表单）。
///
/// 云端和看图拆提示词走同一套闸：私密项目默认只能本地；云端失败回落本地并说明原因。
#[tauri::command]
pub async fn diagnose_image(
    app: tauri::AppHandle,
    image: String,
    project: Option<String>,
    cloud: Option<bool>,
    params: Option<String>,
) -> Result<serde_json::Value, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let s = state.settings.lock().unwrap().clone();
        let cfg = cfg_dir(&handle);
        let private = project
            .as_deref()
            .map(|p| vault::is_private(&cfg, p))
            .unwrap_or(false);
        let want_cloud = cloud.unwrap_or(false);
        let mut notes: Vec<String> = Vec::new();
        let t0 = std::time::Instant::now();
        let params_text = params.unwrap_or_else(|| "{}".into());

        let project_rec = project
            .as_deref()
            .and_then(|p| crate::project::load(&cfg, p).ok());
        // 私密 **或** 隐藏都算锁定：没显式开过就只许本地
        let hidden = project_rec.as_ref().map(|p| p.hidden).unwrap_or(false);
        let locked = private || hidden;
        let allows = match project.as_deref() {
            Some(_) => project_rec
                .as_ref()
                .map(|p| p.cloud_vision_on(locked))
                .unwrap_or(false),
            None => !locked,
        };

        // ① 云端
        if cloud_allowed(private, allows, want_cloud, agent::has_key(&cfg))? {
            let prompt = format!(
                "{}\n\n## 这次生成用的参数（JSON）\n```json\n{}\n```",
                enhance::DIAG_PROMPT, params_text
            );
            let src = std::path::Path::new(&image);
            let load = if src.is_file() {
                vision::load_image_file(src)
            } else {
                vision::load_input_image(&s.comfy_root, &image)
            };
            let tried = load
                .and_then(|(mime, b64)| vision::ask(&cfg, &s, &mime, &b64, &prompt))
                .and_then(|raw| parse_diagnose(&raw).map(|d| (raw, d)));
            match tried {
                Ok((raw, diag)) => {
                    let mut out = diag;
                    out["raw"] = serde_json::json!(raw);
                    out["source"] = serde_json::json!("cloud");
                    out["model"] = serde_json::json!(vision::model(&s));
                    out["notes"] = serde_json::json!(notes);
                    out["ms"] = serde_json::json!(t0.elapsed().as_millis() as u64);
                    return Ok(out);
                }
                Err(e) => notes.push(format!("云端没成（{e}），改用本地")),
            }
        } else if want_cloud && !agent::has_key(&cfg) {
            notes.push("没有 API key，改走本地".into());
        }

        // ② 本地兜底
        if api::health(s.port).is_none() {
            let why = if notes.is_empty() { String::new() } else { format!("；{}", notes.join("；")) };
            return Err(format!("ComfyUI 没有在运行，云端也没成{why}"));
        }
        let input_name = local_input_name(&s.comfy_root, &image)?;
        let raw = enhance::diagnose_local(s.port, &input_name, &params_text).map_err(|e| {
            if notes.is_empty() {
                e
            } else {
                format!("{e}（{}）", notes.join("；"))
            }
        })?;
        let mut out = parse_diagnose(&raw)?;
        out["raw"] = serde_json::json!(raw);
        out["source"] = serde_json::json!("local");
        out["model"] = serde_json::json!("Qwen3-VL（本机）");
        out["notes"] = serde_json::json!(notes);
        out["ms"] = serde_json::json!(t0.elapsed().as_millis() as u64);
        Ok(out)
    })
    .await
    .map_err(|e| format!("诊断任务失败: {e}"))?
}


#[cfg(test)]
mod diagnose_tests {
    use super::parse_diagnose;

    #[test]
    fn parses_a_clean_answer() {
        let raw = r#"{"causes":[{"title":"手指融化","why":"步数太低"}],"fixes":{"cfg":6,"steps":28,"negativeAdd":"blurry, extra fingers","promptHint":"natural hands"},"note":"先加步数"}"#;
        let d = parse_diagnose(raw).unwrap();
        assert_eq!(d["causes"].as_array().unwrap().len(), 1);
        assert_eq!(d["fixes"]["cfg"].as_f64().unwrap(), 6.0);
        assert_eq!(d["fixes"]["steps"].as_i64().unwrap(), 28);
        assert_eq!(d["fixes"]["negativeAdd"], "blurry, extra fingers");
    }

    #[test]
    fn digs_json_out_of_code_fence_and_clamps_ranges() {
        let raw = "```json\n{\"causes\":[{\"title\":\"过曝\",\"why\":\"cfg 太高\"}],\"fixes\":{\"cfg\":99,\"steps\":2},\"note\":\"降一点\"}\n```";
        let d = parse_diagnose(raw).unwrap();
        assert_eq!(d["fixes"]["cfg"].as_f64().unwrap(), 20.0); // 夹到上限
        assert_eq!(d["fixes"]["steps"].as_i64().unwrap(), 4); // 夹到下限
    }

    #[test]
    fn empty_fixes_stay_null_and_bad_text_is_an_error() {
        let d = parse_diagnose(r#"{"causes":[],"fixes":{},"note":"看着正常"}"#).unwrap();
        assert!(d["fixes"]["cfg"].is_null());
        assert!(d["fixes"]["steps"].is_null());
        assert!(parse_diagnose("抱歉，我无法判断").is_err());
    }
}
