//! 在线 AI 配置助手（Pi 引擎）：key 存储、引擎检测、环境体检、规划器。
//!
//! 设计口径（见 docs/100）：
//! - L1：不给 Pi 任何工具（`--no-tools` + 关掉 context/skills/extensions/mcp），
//!   它只读我们塞进系统提示的体检报告，输出 JSON 计划；执行由壳逐步确认。
//! - key 用 Windows DPAPI 加密落盘（`agent_key.dat`），不写 settings.json。

use crate::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const KEY_FILE: &str = "agent_key.dat";

// ---------------------------------------------------------------- DPAPI

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB,
    };
    use windows_sys::Win32::Foundation::LocalFree;

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let mut input = CRYPT_INTEGER_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as *mut u8,
            };
            let mut out = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };
            let ok = CryptProtectData(
                &mut input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out,
            );
            if ok == 0 {
                return None;
            }
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as *mut _);
            Some(bytes)
        }
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let mut input = CRYPT_INTEGER_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as *mut u8,
            };
            let mut out = CRYPT_INTEGER_BLOB {
                cbData: 0,
                pbData: std::ptr::null_mut(),
            };
            let ok = CryptUnprotectData(
                &mut input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out,
            );
            if ok == 0 {
                return None;
            }
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            LocalFree(out.pbData as *mut _);
            Some(bytes)
        }
    }
}

pub fn save_key(cfg: &Path, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("key 不能为空".into());
    }
    #[cfg(windows)]
    {
        let enc = dpapi::protect(key.as_bytes()).ok_or("DPAPI 加密失败")?;
        crate::fsutil::atomic_write(&cfg.join(KEY_FILE), &enc)
    }
    #[cfg(not(windows))]
    {
        crate::fsutil::atomic_write(&cfg.join(KEY_FILE), key.as_bytes())
    }
}

pub fn load_key(cfg: &Path) -> Option<String> {
    let bytes = std::fs::read(cfg.join(KEY_FILE)).ok()?;
    #[cfg(windows)]
    {
        let plain = dpapi::unprotect(&bytes)?;
        String::from_utf8(plain).ok().map(|s| s.trim().to_string())
    }
    #[cfg(not(windows))]
    {
        String::from_utf8(bytes).ok().map(|s| s.trim().to_string())
    }
}

pub fn has_key(cfg: &Path) -> bool {
    load_key(cfg).map(|k| !k.is_empty()).unwrap_or(false)
}

pub fn clear_key(cfg: &Path) -> Result<(), String> {
    let p = cfg.join(KEY_FILE);
    if p.exists() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------- 引擎

#[derive(Serialize, Clone)]
pub struct EngineStatus {
    pub found: bool,
    pub node: String,
    pub cli: String,
    pub version: String,
    pub source: String,
}

fn looks_like_pi_dir(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let node = dir.join("node.exe");
    let cli = dir
        .join("node_modules")
        .join("@earendil-works")
        .join("pi-coding-agent")
        .join("dist")
        .join("bundle")
        .join("cli.js");
    if node.is_file() && cli.is_file() {
        return Some((node, cli));
    }
    let node2 = dir.join("node");
    if node2.is_file() && cli.is_file() {
        return Some((node2, cli));
    }
    None
}

/// 找 Pi 引擎：设置里指定的路径 → PATH 上的 pi → 应用数据下的 agent/。
pub fn resolve_engine(cfg: &Path, s: &Settings) -> EngineStatus {
    let mut cands: Vec<(PathBuf, String)> = Vec::new();
    if !s.pi_path.trim().is_empty() {
        cands.push((PathBuf::from(s.pi_path.trim()), "设置".into()));
    }
    if let Ok(out) = Command::new("where").arg("pi").output() {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        for line in text.lines() {
            let p = PathBuf::from(line.trim());
            if let Some(dir) = p.parent() {
                cands.push((dir.to_path_buf(), "PATH".into()));
            }
        }
    }
    cands.push((cfg.join("agent"), "应用数据".into()));
    for (dir, src) in cands {
        if let Some((node, cli)) = looks_like_pi_dir(&dir) {
            return EngineStatus {
                found: true,
                node: node.to_string_lossy().to_string(),
                cli: cli.to_string_lossy().to_string(),
                version: String::new(),
                source: src,
            };
        }
    }
    EngineStatus {
        found: false,
        node: String::new(),
        cli: String::new(),
        version: String::new(),
        source: String::new(),
    }
}

// ---------------------------------------------------------------- 体检

fn tail_text(path: &Path, max_lines: usize, max_bytes: usize) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let start = bytes.len().saturating_sub(max_bytes);
    let text = String::from_utf8_lossy(&bytes[start..]).to_string();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(max_lines)..].join("\n")
}

fn basenames(models: &[models::ModelFile]) -> std::collections::HashSet<String> {
    models
        .iter()
        .filter_map(|m| m.name.rsplit('/').next())
        .map(|s| s.to_lowercase())
        .collect()
}

#[derive(Serialize)]
pub struct EnvReport {
    pub summary: Vec<String>,
    pub server: serde_json::Value,
    pub comfy_root: String,
    pub has_python: bool,
    pub has_main: bool,
    pub output_dir: String,
    pub output_ok: bool,
    pub custom_nodes: usize,
    pub models_total: usize,
    pub profiles: Vec<serde_json::Value>,
    pub logs: String,
}

pub fn env_report(_cfg: &Path, s: &Settings) -> EnvReport {
    let out_root = state::output_root(s);
    let comfy = PathBuf::from(&s.comfy_root).join("ComfyUI");
    let has_python = PathBuf::from(&s.comfy_root).join("python_embeded").is_dir();
    let has_main = comfy.join("main.py").is_file();
    let mut custom_nodes = 0usize;
    if let Ok(rd) = std::fs::read_dir(comfy.join("custom_nodes")) {
        custom_nodes = rd.flatten().filter(|e| e.path().is_dir()).count();
    }
    let _ = std::fs::create_dir_all(&out_root);
    let probe = out_root.join("_write_probe");
    let output_ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);

    let health = api::health(s.port);
    let version = api::version(s.port).unwrap_or_default();
    let healthy = health.is_some();

    let models_root = PathBuf::from(&s.comfy_root).join("ComfyUI").join("models");
    let scanned = models::scan(&models_root);
    let have = basenames(&scanned);

    let mut profiles = Vec::new();
    let mut missing_any = 0usize;
    for p in ["qwen21", "flux3", "flux3base", "moss", "mossbase", "mossimagine", "mossedit", "kromoss"] {
        if let Some(pr) = api::profile(p) {
            let req = [pr.unet, pr.clip, pr.vae];
            let missing: Vec<&str> = req
                .iter()
                .filter(|f| !have.contains(&f.to_lowercase()))
                .copied()
                .collect();
            if !missing.is_empty() {
                missing_any += 1;
            }
            profiles.push(serde_json::json!({
                "id": p, "label": pr.label, "missing": missing,
            }));
        }
    }

    let mut summary = Vec::new();
    summary.push(if healthy {
        format!("ComfyUI {} 运行中（端口 {}）", if version.is_empty() { "?" } else { &version }, s.port)
    } else {
        format!("ComfyUI 未运行（端口 {}）", s.port)
    });
    if !has_main {
        summary.push("ComfyUI 目录不完整（缺 main.py）".into());
    }
    if !has_python {
        summary.push("缺 python_embeded（不像便携版）".into());
    }
    if !output_ok {
        summary.push(format!("出图目录不可写：{}", out_root.display()));
    }
    if missing_any > 0 {
        summary.push(format!("{} 个档位缺模型文件", missing_any));
    }
    summary.push(format!("模型 {} 个 · 自定义节点 {} 个", scanned.len(), custom_nodes));

    let logs = tail_text(
        &PathBuf::from(&s.comfy_root).join("comfyshell-server.log"),
        40,
        8000,
    );

    EnvReport {
        summary,
        server: serde_json::json!({
            "healthy": healthy,
            "version": version,
            "port": s.port,
            "managed": false,
        }),
        comfy_root: s.comfy_root.clone(),
        has_python,
        has_main,
        output_dir: out_root.to_string_lossy().to_string(),
        output_ok,
        custom_nodes,
        models_total: scanned.len(),
        profiles,
        logs,
    }
}

// ---------------------------------------------------------------- 规划器

const PLAN_SYSTEM: &str = r#"你是「显影」（本地 ComfyUI 生图/改图壳）的配置助手。你的唯一任务是读体检报告，输出一份可执行的 JSON 计划。

规则：
- 只输出一个 JSON 对象，不要 markdown、不要解释、不要代码块。
- 结构：{"summary":"一句话结论","steps":[{"title":"给人看的动作","action":"动作名","args":{...},"why":"为什么"}]}
- 可用动作（只能用这些）：
  - set_settings  args: {"comfyRoot"?:string, "outputDir"?:string, "port"?:number}
  - start_comfy   args: {}
  - stop_comfy    args: {}
  - restart_comfy args: {}
  - install_comfy args: {"mode":"source"} 或 {"mode":"portable"}（portable 需要空目录，慎用）
  - open_folder   args: {"path":string}   （只用于提示用户去看某个文件夹）
- 最多 6 步；不要给删除项目、读写图片库、执行任意命令之类的动作（你也没有这些工具）。
- **绝对不许尝试修改、替换、重编译显影自己的程序或源码**；你只能通过上面这些动作改 ComfyUI 的配置与环境。
- 如果体检报告已经一切正常，就返回空 steps，并在 summary 里说明。
- 如果问题超出你的动作范围（驱动、CUDA、磁盘、显影自身 bug），在 summary 里说清楚，并加一步：
  {"title":"需要你手动处理","action":"advice","why":"原因","args":{"text":"具体建议，比如：去 GitHub 更新显影到新版本 / 提交 issue（附上日志）"}}
"#;

fn extract_json(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

pub struct PlanResult {
    pub plan: serde_json::Value,
    pub raw: String,
    pub ms: u64,
}

pub fn run_plan(cfg: &Path, s: &Settings, report: &EnvReport, request: &str) -> Result<PlanResult, String> {
    let key = load_key(cfg).ok_or("还没填 API key（设置 → AI 助手）")?;
    let eng = resolve_engine(cfg, s);
    if !eng.found {
        return Err("没找到 Pi 引擎（设置 → AI 助手 → 下载引擎）".into());
    }
    let provider = if s.ai_provider.trim().is_empty() {
        "deepseek"
    } else {
        s.ai_provider.trim()
    };
    let model = if s.ai_model.trim().is_empty() {
        "deepseek-chat"
    } else {
        s.ai_model.trim()
    };

    let tmp = cfg.join("agent_tmp");
    let _ = std::fs::create_dir_all(&tmp);
    let prompt_path = tmp.join("system.md");
    let report_json = serde_json::to_string_pretty(report).unwrap_or_default();
    let sys = format!(
        "{}\n\n## 体检报告\n```json\n{}\n```\n\n## 用户请求\n{}",
        PLAN_SYSTEM, report_json, request
    );
    crate::fsutil::atomic_write_str(&prompt_path, &sys)?;

    let mut cmd = Command::new(&eng.node);
    cmd.arg(&eng.cli)
        .args(["-p", "--mode", "json"])
        .args(["--no-tools", "--no-context-files", "--no-skills"])
        .args(["--no-extensions", "--no-mcp", "--no-themes"])
        .args(["--no-prompt-templates", "--offline"])
        .arg("--provider")
        .arg(provider)
        .arg("--model")
        .arg(model)
        .arg("--api-key")
        .arg(key)
        .arg("--system-prompt")
        .arg(&prompt_path)
        .arg("按系统提示输出 JSON 计划。")
        .current_dir(&tmp)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }

    let t0 = std::time::Instant::now();
    let mut child = cmd.spawn().map_err(|e| format!("启动 Pi 失败：{e}"))?;
    let stdout = child.stdout.take().ok_or("拿不到 Pi 输出")?;
    let collected: Arc<std::sync::Mutex<Vec<String>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    let lines_ref = collected.clone();
    let done_ref = done.clone();
    std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(mut v) = lines_ref.lock() {
                v.push(line);
            }
        }
        done_ref.store(true, Ordering::SeqCst);
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(150);
    loop {
        if done.load(Ordering::SeqCst) || child.try_wait().ok().flatten().is_some() {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return Err("Pi 超时（150 秒）".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }
    let _ = child.wait();

    let lines = collected.lock().map(|v| v.clone()).unwrap_or_default();
    let mut assistant = String::new();
    let mut last_err = String::new();
    for line in &lines {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match v.get("type").and_then(|x| x.as_str()).unwrap_or("") {
            "message_end" => {
                let msg = v.get("message");
                if msg.and_then(|m| m.get("role")).and_then(|r| r.as_str()) == Some("assistant") {
                    if let Some(c) = msg.and_then(|m| m.get("content")) {
                        let text = match c {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Array(arr) => arr
                                .iter()
                                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join(""),
                            _ => String::new(),
                        };
                        if !text.trim().is_empty() {
                            assistant = text;
                        }
                    }
                }
            }
            "auto_retry_end" => {
                if let Some(e) = v.get("finalError").and_then(|x| x.as_str()) {
                    last_err = e.to_string();
                }
            }
            _ => {}
        }
    }

    if assistant.trim().is_empty() {
        return Err(if last_err.is_empty() {
            "Pi 没有返回内容（检查 key/provider/model）".into()
        } else {
            format!("Pi 调用失败：{last_err}")
        });
    }

    let plan = extract_json(&assistant).ok_or_else(|| {
        format!(
            "模型没按 JSON 返回，原文前 300 字：{}",
            assistant.chars().take(300).collect::<String>()
        )
    })?;
    Ok(PlanResult {
        plan,
        raw: assistant,
        ms: t0.elapsed().as_millis() as u64,
    })
}
