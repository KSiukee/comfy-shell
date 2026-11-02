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

// ---------------------------------------------------------------- 引擎安装（Node + Pi）

pub type EngineProgress = Arc<dyn Fn(&str, u64, u64, f64, &str) + Send + Sync>;

fn find_node_dir(dir: &Path, depth: usize) -> Option<PathBuf> {
    if depth > 2 {
        return None;
    }
    if dir.join("node.exe").is_file() {
        return Some(dir.to_path_buf());
    }
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(hit) = find_node_dir(&p, depth + 1) {
                return Some(hit);
            }
        }
    }
    None
}

/// 下载便携 Node（npmmirror 优先）+ npm 安装 Pi 到 `target`。
pub fn install_engine(cfg: &Path, target: &Path, prog: EngineProgress) -> Result<String, String> {
    std::fs::create_dir_all(target).map_err(|e| format!("建目录失败：{e}"))?;
    let node_exe = target.join("node.exe");
    let npm_cli = target
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js");

    if !node_exe.is_file() || !npm_cli.is_file() {
        let urls = [
            "https://npmmirror.com/mirrors/node/v22.20.0/node-v22.20.0-win-x64.zip",
            "https://nodejs.org/dist/v22.20.0/node-v22.20.0-win-x64.zip",
        ];
        let zip = target.join("_node.zip");
        let mut last_err = String::new();
        let mut ok = false;
        for url in urls {
            prog("下载 Node", 0, 0, 0.0, url);
            let p = prog.clone();
            let on_progress: Arc<dyn Fn(u64, u64, f64) + Send + Sync> = Arc::new(move |got, total, mbps| {
                p("下载 Node", got, total, mbps, "");
            });
            let cfg_dl = netdl::DlCfg {
                url,
                dest: &zip,
                threads: 6,
                proxy: netdl::detect_proxy(),
                cancel: Arc::new(|| false),
                on_progress,
            };
            match netdl::download(cfg_dl) {
                Ok(_) => {
                    ok = true;
                    break;
                }
                Err(e) => last_err = e,
            }
        }
        if !ok {
            return Err(format!("下载 Node 失败：{last_err}"));
        }
        prog("解压 Node", 0, 0, 0.0, "");
        let unpack = target.join("_unpack");
        let _ = std::fs::remove_dir_all(&unpack);
        netdl::extract(&zip, &unpack)?;
        let node_dir = find_node_dir(&unpack, 0).ok_or("Node 压缩包里没找到 node.exe")?;
        for e in std::fs::read_dir(&node_dir).map_err(|e| e.to_string())?.flatten() {
            let name = e.file_name();
            if name == "_node.zip" || name == "_unpack" {
                continue;
            }
            netdl::move_dir(&e.path(), &target.join(&name))?;
        }
        let _ = std::fs::remove_dir_all(&unpack);
        let _ = std::fs::remove_file(&zip);
    }

    if !npm_cli.is_file() {
        return Err("Node 里没有 npm（这个压缩包不完整）".into());
    }

    prog("安装 Pi", 0, 0, 0.0, "npm install @earendil-works/pi-coding-agent");
    let mut cmd = Command::new(&node_exe);
    cmd.arg(&npm_cli)
        .arg("install")
        .arg("--prefix")
        .arg(target)
        .arg("--ignore-scripts")
        .arg("--registry=https://registry.npmmirror.com")
        .arg("--no-audit")
        .arg("--no-fund")
        .arg("--loglevel=error")
        .arg("@earendil-works/pi-coding-agent")
        .current_dir(target)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(p) = netdl::detect_proxy() {
        cmd.env("HTTPS_PROXY", &p)
            .env("HTTP_PROXY", &p)
            .env("https_proxy", &p)
            .env("http_proxy", &p);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| format!("启动 npm 失败：{e}"))?;
    let last_line: Arc<std::sync::Mutex<String>> = Arc::new(std::sync::Mutex::new(String::new()));
    for stream in [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let ll = last_line.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                if !line.trim().is_empty() {
                    if let Ok(mut l) = ll.lock() {
                        *l = line;
                    }
                }
            }
        });
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                if !status.success() {
                    let line = last_line.lock().map(|l| l.clone()).unwrap_or_default();
                    return Err(format!("npm 安装失败（{}）：{}", status.code().unwrap_or(-1), line));
                }
                break;
            }
            None => {
                if std::time::Instant::now() > deadline {
                    let _ = child.kill();
                    return Err("npm 安装超时（10 分钟）".into());
                }
                let line = last_line.lock().map(|l| l.clone()).unwrap_or_default();
                prog("安装 Pi", 0, 0, 0.0, &line);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }

    let cli = target
        .join("node_modules")
        .join("@earendil-works")
        .join("pi-coding-agent")
        .join("dist")
        .join("bundle")
        .join("cli.js");
    if !cli.is_file() {
        return Err("装完了但没找到 pi 的 cli.js（npm 输出异常）".into());
    }
    prog("完成", 0, 0, 0.0, "");
    let _ = cfg;
    Ok(target.to_string_lossy().to_string())
}

// ---------------------------------------------------------------- 体检

/// 发给云端前的脱敏：Windows 盘符路径/UNC 只留文件名，URL 不动。
pub fn redact(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let delim = |c: char| {
        c.is_whitespace()
            || matches!(c, '"' | '\'' | ',' | ';' | ')' | ']' | '}' | '|' | '<' | '>' | '`')
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let is_drive = i + 2 < chars.len()
            && (i == 0 || !chars[i - 1].is_alphanumeric())
            && c.is_ascii_alphabetic()
            && chars[i + 1] == ':'
            && (chars[i + 2] == '\\' || chars[i + 2] == '/');
        let is_unc = c == '\\' && i + 1 < chars.len() && chars[i + 1] == '\\';
        if is_drive || is_unc {
            let mut j = i + 2;
            while j < chars.len() && !delim(chars[j]) {
                j += 1;
            }
            let segment: String = chars[i..j].iter().collect();
            let base = segment.rsplit(['\\', '/']).next().unwrap_or("");
            if !base.is_empty() && base.chars().count() <= 80 && !base.contains(':') {
                out.push_str("<path>/");
                out.push_str(base);
            } else {
                out.push_str("<path>");
            }
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Comfy Registry 的节点 id → GitHub 仓库（官方 API，2026 实测可用）。
pub fn registry_resolve(id: &str) -> Result<serde_json::Value, String> {
    let id = id.trim();
    if id.is_empty()
        || id.len() > 100
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("Registry id 不合法（只允许字母数字和 - _ .）".into());
    }
    let mut b = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(25))
        .user_agent("xianying/0.105");
    if let Some(p) = netdl::detect_proxy() {
        b = b.proxy(reqwest::Proxy::all(&p).map_err(|e| e.to_string())?);
    }
    let client = b.build().map_err(|e| e.to_string())?;
    let base = "https://api.comfy.org";
    let fetch = |url: String| -> Option<serde_json::Value> {
        let r = client.get(&url).send().ok()?;
        if !r.status().is_success() {
            return None;
        }
        r.json::<serde_json::Value>().ok()
    };

    let node = match fetch(format!("{base}/nodes/{id}")) {
        Some(n) => Some(n),
        None => {
            // 精确匹配才认；搜索只是拿来给建议，不能把相近的节点装上去
            let s = fetch(format!("{base}/nodes/search?query={id}&limit=5"));
            let exact = s
                .as_ref()
                .and_then(|s| s.get("nodes"))
                .and_then(|x| x.as_array())
                .and_then(|a| {
                    a.iter()
                        .find(|n| n.get("id").and_then(|x| x.as_str()) == Some(id))
                        .cloned()
                });
            if exact.is_none() {
                let names: Vec<String> = s
                    .as_ref()
                    .and_then(|s| s.get("nodes"))
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|n| n.get("id").and_then(|x| x.as_str()).map(str::to_string))
                            .take(3)
                            .collect()
                    })
                    .unwrap_or_default();
                return Err(if names.is_empty() {
                    format!("Registry 里没找到「{id}」")
                } else {
                    format!("Registry 里没找到「{id}」，相近的有：{}", names.join("、"))
                });
            }
            exact
        }
    };
    let node = node.ok_or_else(|| format!("Registry 里没找到「{id}」"))?;
    let repo = node
        .get("repository")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    if !repo.starts_with("https://github.com/") {
        return Err(format!("「{id}」没有可用的 GitHub 仓库（可能只发布在 Registry）"));
    }
    Ok(serde_json::json!({
        "id": node.get("id").cloned().unwrap_or(serde_json::json!(id)),
        "name": node.get("name").cloned().unwrap_or(serde_json::json!("")),
        "repository": repo,
        "version": node.get("latest_version").and_then(|v| v.get("version")).cloned().unwrap_or(serde_json::json!("")),
        "downloads": node.get("downloads").cloned().unwrap_or(serde_json::json!(0)),
    }))
}

/// 只读查询（L2）：AI 计划里的 `inspect` 步用，查完把结果补给它重新出方案。
pub fn inspect(s: &Settings, target: &str, lines: usize) -> Result<serde_json::Value, String> {
    let root = PathBuf::from(&s.comfy_root);
    match target {
        "logs" => {
            let n = lines.clamp(10, 400);
            let text = tail_text(&root.join("comfyshell-server.log"), n, 60_000);
            Ok(serde_json::json!({
                "target": "logs",
                "lines": text.lines().count(),
                "tail": text,
            }))
        }
        "models" => {
            let scanned = models::scan(&root.join("ComfyUI").join("models"));
            let items: Vec<serde_json::Value> = scanned
                .iter()
                .take(400)
                .map(|m| serde_json::json!({"name": m.name, "kind": m.kind, "size": m.size}))
                .collect();
            Ok(serde_json::json!({"target": "models", "count": scanned.len(), "items": items}))
        }
        "nodes" => {
            let dir = root.join("ComfyUI").join("custom_nodes");
            let mut items = Vec::new();
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    if e.path().is_dir() {
                        let name = e.file_name().to_string_lossy().to_string();
                        if !name.starts_with("__") {
                            items.push(name);
                        }
                    }
                }
            }
            Ok(serde_json::json!({"target": "nodes", "count": items.len(), "items": items}))
        }
        "status" => {
            let health = api::health(s.port);
            let vram = health
                .as_ref()
                .and_then(|h| h.get("devices"))
                .and_then(|d| d.as_array())
                .and_then(|a| a.first())
                .map(|d| {
                    serde_json::json!({
                        "vram_total": d.get("vram_total"),
                        "vram_free": d.get("vram_free"),
                    })
                });
            Ok(serde_json::json!({
                "target": "status",
                "healthy": health.is_some(),
                "version": api::version(s.port).unwrap_or_default(),
                "port": s.port,
                "vram": vram,
            }))
        }
        _ => Err("target 只能是 logs / models / nodes / status".into()),
    }
}

/// 去掉终端颜色码（`\x1b[32m` 之类）。
fn strip_ansi(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '[' {
            let mut j = i + 2;
            while j < chars.len() && !chars[j].is_ascii_alphabetic() {
                j += 1;
            }
            i = (j + 1).min(chars.len());
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// ComfyUI 的 tqdm 进度行（`12%|███| 2/16 [00:00<00:02, 3.02it/s]`），界面上是乱码来源。
fn is_progress_line(line: &str) -> bool {
    let has_rate = line.contains("it/s]") || line.contains("s/it]");
    let has_bar = line.contains("%|") || (line.matches('|').count() >= 2 && line.contains('%'));
    has_rate || has_bar
}

fn tail_text(path: &Path, max_lines: usize, max_bytes: usize) -> String {
    let Ok(bytes) = std::fs::read(path) else {
        return String::new();
    };
    let start = bytes.len().saturating_sub(max_bytes);
    let text = String::from_utf8_lossy(&bytes[start..]).to_string();
    // 进度用 \r 刷新，先拆成独立行，再滤掉 ANSI 与进度条
    let text = strip_ansi(&text).replace('\r', "\n");
    let mut lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter(|l| !is_progress_line(l))
        .collect();
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }
    lines.join("\n")
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
  - install_node  args: {"url":"https://github.com/<owner>/<repo>"} 或 {"registry":"<Comfy Registry 的 id，如 comfyui-impact-pack>"}
    （装自定义节点：clone 到 custom_nodes 并装 requirements；url 只支持 GitHub https，registry 由壳解析成 GitHub 地址）
  - generate_test args: {"profile"?:string,"prompt"?:string,"width"?:number,"height"?:number}（真跑一张验收图，约 1 分钟、占 GPU）
  - inspect       args: {"target":"logs"|"models"|"nodes"|"status","lines"?:number}（只读；查完壳会把结果补给你、让你重新出方案）
- 体检报告只是快照；需要看最新日志/模型/节点/显存时，先给一步 inspect，别猜。
- 装完自定义节点记得加一步 restart_comfy 让它生效；generate_test 每轮最多一次。
- 最多 6 步；不要给删除项目、读写图片库、执行任意命令之类的动作（你也没有这些工具）。
- **绝对不许尝试修改、替换、重编译显影自己的程序或源码**；你只能通过上面这些动作改 ComfyUI 的配置与环境。
- 说话要短：summary 不超过 80 个汉字；每步的 why 不超过 30 个汉字；advice 的 text 不超过 120 个汉字。
- 只输出一个 JSON 对象，从 `{` 开始到 `}` 结束，前后不要有任何文字或代码块。
- 如果体检报告已经一切正常，就返回空 steps，并在 summary 里说明。
- 如果问题超出你的动作范围（驱动、CUDA、磁盘、显影自身 bug），在 summary 里说清楚，并加一步：
  {"title":"需要你手动处理","action":"advice","why":"原因","args":{"text":"具体建议，比如：去 GitHub 更新显影到新版本 / 提交 issue（附上日志）"}}
"#;

/// 转义字符串内部的裸控制字符（模型偶尔会在 JSON 字符串里直接换行）。
fn escape_control_in_strings(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let mut in_str = false;
    let mut esc = false;
    for ch in s.chars() {
        if in_str {
            if esc {
                out.push(ch);
                esc = false;
                continue;
            }
            match ch {
                '\\' => {
                    out.push(ch);
                    esc = true;
                }
                '"' => {
                    out.push(ch);
                    in_str = false;
                }
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                _ => out.push(ch),
            }
        } else {
            if ch == '"' {
                in_str = true;
            }
            out.push(ch);
        }
    }
    out
}

fn extract_json(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let body = &text[start..];
    // 从最后一个 } 往前试几个候选结尾，兼容「后面还有话」或「字符串里带裸换行」
    let ends: Vec<usize> = body.match_indices('}').map(|(i, _)| i).collect();
    for &e in ends.iter().rev().take(10) {
        let slice = &body[..=e];
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(slice) {
            return Some(v);
        }
        let fixed = escape_control_in_strings(slice);
        if fixed != slice {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&fixed) {
                return Some(v);
            }
        }
    }
    None
}

pub struct PlanResult {
    pub plan: serde_json::Value,
    pub raw: String,
    pub ms: u64,
}

pub fn run_plan(
    cfg: &Path,
    s: &Settings,
    report: &EnvReport,
    request: &str,
    extra: Option<&str>,
) -> Result<PlanResult, String> {
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
    let report_json = redact(&serde_json::to_string_pretty(report).unwrap_or_default());
    let extra_block = match extra.map(str::trim).filter(|x| !x.is_empty()) {
        Some(e) => format!("\n\n## 补充信息（刚刚查到的，以此为准）\n{}", redact(e)),
        None => String::new(),
    };
    let sys = format!(
        "{}\n\n## 体检报告\n```json\n{}\n```\n\n## 用户请求\n{}{}",
        PLAN_SYSTEM, report_json, request, extra_block
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
                    if let Some(err) = msg.and_then(|m| m.get("errorMessage")).and_then(|x| x.as_str()) {
                        if !err.trim().is_empty() {
                            last_err = err.to_string();
                        }
                    }
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
            format!(
                "Pi 调用失败：{}",
                last_err.chars().take(400).collect::<String>()
            )
        });
    }

    let plan = extract_json(&assistant).ok_or_else(|| {
        let tail: String = assistant
            .chars()
            .rev()
            .take(160)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!(
            "模型没按 JSON 返回（{} 字）。尾部：{}",
            assistant.chars().count(),
            tail
        )
    })?;
    Ok(PlanResult {
        plan,
        raw: assistant,
        ms: t0.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod redact_tests {
    use super::redact;

    #[test]
    fn redacts_drive_paths_but_keeps_filename() {
        let s = redact("open D:\\ai\\out\\x.png now");
        assert!(s.contains("<path>/x.png"), "{s}");
        assert!(!s.contains("D:\\"), "{s}");
    }

    #[test]
    fn keeps_urls_and_chinese() {
        let u = "https://hf-mirror.com/Comfy-Org/x/resolve/main/a.safetensors";
        assert_eq!(redact(u), u);
        assert_eq!(redact("中文 正常"), "中文 正常");
    }

    #[test]
    fn redacts_unc_shares() {
        let s = redact("share \\\\nas\\share\\file.txt end");
        assert!(s.contains("<path>/file.txt"), "{s}");
        assert!(!s.contains("nas\\share"), "{s}");
    }
}

#[cfg(test)]
mod log_clean_tests {
    use super::{is_progress_line, strip_ansi, tail_text};

    #[test]
    fn strips_ansi_colors() {
        assert_eq!(strip_ansi("\u{1b}[32m[INFO]\u{1b}[0m ok"), "[INFO] ok");
    }

    #[test]
    fn drops_tqdm_progress_lines() {
        assert!(is_progress_line(" 12%|███| 2/16 [00:00<00:02, 3.02it/s]"));
        assert!(is_progress_line("16/16 [00:05<00:00,  3.01it/s]"));
        assert!(!is_progress_line("[INFO] Prompt executed in 9.8 seconds"));
    }

    #[test]
    fn tail_filters_and_limits() {
        let p = std::env::temp_dir().join(format!("cs_tail_{}", std::process::id()));
        std::fs::write(
            &p,
            "a\n\u{1b}[32m[INFO]\u{1b}[0m keep\n10%|##| 1/10 [00:01, 2it/s]\r20%|###| 2/10 [00:02, 2it/s]\nlast\n",
        )
        .unwrap();
        let out = tail_text(&p, 10, 10_000);
        assert!(out.contains("[INFO] keep"), "{out}");
        assert!(out.contains("last"), "{out}");
        assert!(!out.contains("it/s"), "{out}");
        assert!(!out.contains('\u{1b}'), "{out}");
        let _ = std::fs::remove_file(&p);
    }
}
