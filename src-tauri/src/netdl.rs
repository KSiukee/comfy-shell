//! 下载引擎：镜像测速 + 多线程 Range 分块 + 进度/取消。
//!
//! 只依赖已有的 `reqwest`（blocking）与系统自带的 `tar`（bsdtar/libarchive 能解 zip/7z）。
//! 代理默认跟系统（环境变量 → Windows 注册表），这样国内直连 GitHub/HF 超时的时候能自动走 Clash 之类。

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Probe {
    pub url: String,
    pub ok: bool,
    pub ms: u64,
    pub mbps: f64,
    pub error: String,
}

/// 系统代理：环境变量优先，其次 Windows Internet Settings 的 ProxyServer。
pub fn detect_proxy() -> Option<String> {
    for k in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        if let Ok(v) = std::env::var(k) {
            let v = v.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    #[cfg(windows)]
    {
        let out = std::process::Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
                "/v",
                "ProxyServer",
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        for line in text.lines() {
            if let Some(pos) = line.find("REG_SZ") {
                let v = line[pos + 6..].trim().to_string();
                if !v.is_empty() {
                    return Some(if v.starts_with("http") {
                        v
                    } else {
                        format!("http://{v}")
                    });
                }
            }
        }
    }
    None
}

fn client(proxy: Option<&str>) -> Result<reqwest::blocking::Client, String> {
    let mut b = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(20))
        .user_agent("xianying/0.98 (ComfyUI shell)");
    if let Some(p) = proxy.map(str::trim).filter(|p| !p.is_empty()) {
        b = b
            .proxy(reqwest::Proxy::all(p).map_err(|e| format!("代理不可用：{e}"))?);
    }
    b.build().map_err(|e| e.to_string())
}

/// 测速：拉 256KB 看吞吐 + 首包耗时。失败也要有结果（镜像不可用是常态）。
pub fn probe(url: &str, proxy: Option<&str>) -> Probe {
    let t0 = Instant::now();
    let res = (|| -> Result<f64, String> {
        let c = client(proxy)?;
        let mut r = c
            .get(url)
            .header("Range", "bytes=0-262143")
            .send()
            .map_err(|e| e.to_string())?;
        if !r.status().is_success() {
            return Err(format!("HTTP {}", r.status()));
        }
        let mut buf = Vec::with_capacity(262_144);
        r.copy_to(&mut buf).map_err(|e| e.to_string())?;
        if buf.is_empty() {
            return Err("空响应".into());
        }
        let secs = t0.elapsed().as_secs_f64().max(1e-3);
        Ok(buf.len() as f64 / 1_048_576.0 / secs)
    })();
    match res {
        Ok(mbps) => Probe {
            url: url.to_string(),
            ok: true,
            ms: t0.elapsed().as_millis() as u64,
            mbps,
            error: String::new(),
        },
        Err(e) => Probe {
            url: url.to_string(),
            ok: false,
            ms: t0.elapsed().as_millis() as u64,
            mbps: 0.0,
            error: e,
        },
    }
}

/// 拿总大小 + 是否支持 Range。
fn probe_size(c: &reqwest::blocking::Client, url: &str) -> Result<(u64, bool), String> {
    let r = c
        .get(url)
        .header("Range", "bytes=0-0")
        .send()
        .map_err(|e| e.to_string())?;
    let status = r.status();
    let total = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        r.headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit('/').next())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
    } else {
        r.content_length().unwrap_or(0)
    };
    Ok((total, status == reqwest::StatusCode::PARTIAL_CONTENT))
}

pub struct DlCfg<'a> {
    pub url: &'a str,
    pub dest: &'a Path,
    pub threads: usize,
    pub proxy: Option<String>,
    pub cancel: Arc<dyn Fn() -> bool + Send + Sync>,
    /// (已下载字节, 总字节, MB/s)
    pub on_progress: Arc<dyn Fn(u64, u64, f64) + Send + Sync>,
}

/// 多线程分块下载到 `dest`（先写 .partN，再拼成 `.part`，最后改名）。
pub fn download(cfg: DlCfg) -> Result<PathBuf, String> {
    let client = client(cfg.proxy.as_deref())?;
    let (total, ranges) = probe_size(&client, cfg.url)?;
    let threads = cfg.threads.clamp(1, 8);
    let dest = cfg.dest.to_path_buf();
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let got = Arc::new(AtomicU64::new(0));
    let last = Arc::new(std::sync::Mutex::new((Instant::now(), 0u64)));
    let cancel = cfg.cancel.clone();
    let prog = cfg.on_progress.clone();

    let report = {
        let got = got.clone();
        let last = last.clone();
        let prog = prog.clone();
        move || {
            let n = got.load(Ordering::Relaxed);
            let mut l = last.lock().unwrap();
            let dt = l.0.elapsed().as_secs_f64();
            if dt >= 0.35 {
                let mbps = (n.saturating_sub(l.1)) as f64 / 1_048_576.0 / dt.max(1e-3);
                prog(n, total, mbps);
                *l = (Instant::now(), n);
            }
        }
    };

    if ranges && total > 8 * 1_048_576 && threads > 1 {
        let chunk = total / threads as u64 + 1;
        let mut parts = Vec::new();
        let mut handles = Vec::new();
        for t in 0..threads as u64 {
            let start = t * chunk;
            if start >= total {
                break;
            }
            let end = ((t + 1) * chunk - 1).min(total - 1);
            let part = dest.with_extension(format!("part{t}"));
            parts.push(part.clone());
            let c = client.clone();
            let url = cfg.url.to_string();
            let cancel = cancel.clone();
            let got = got.clone();
            let report = report.clone();
            handles.push(std::thread::spawn(move || -> Result<(), String> {
                let mut f = std::fs::File::create(&part).map_err(|e| e.to_string())?;
                let mut pos = start;
                while pos <= end {
                    if cancel() {
                        return Err("已取消".into());
                    }
                    let seg_end = (pos + 4 * 1_048_576 - 1).min(end);
                    let resp = c
                        .get(&url)
                        .header("Range", format!("bytes={pos}-{seg_end}"))
                        .send()
                        .map_err(|e| e.to_string())?;
                    if !resp.status().is_success() {
                        return Err(format!("HTTP {}", resp.status()));
                    }
                    let mut rd = resp;
                    let mut buf = vec![0u8; 64 * 1024];
                    loop {
                        if cancel() {
                            return Err("已取消".into());
                        }
                        let n = rd.read(&mut buf).map_err(|e| e.to_string())?;
                        if n == 0 {
                            break;
                        }
                        f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
                        got.fetch_add(n as u64, Ordering::Relaxed);
                        report();
                    }
                    pos = seg_end + 1;
                }
                Ok(())
            }));
        }
        for h in handles {
            h.join().map_err(|_| "下载线程崩溃".to_string())??;
        }
        // 拼接
        let tmp = dest.with_extension("part");
        {
            let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            for p in &parts {
                let mut f = std::fs::File::open(p).map_err(|e| e.to_string())?;
                std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
            }
        }
        std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
        for p in parts {
            let _ = std::fs::remove_file(p);
        }
    } else {
        // 单线程（不支持 Range 的源）
        let mut resp = client.get(cfg.url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let tmp = dest.with_extension("part");
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 128 * 1024];
        loop {
            if cancel() {
                let _ = std::fs::remove_file(&tmp);
                return Err("已取消".into());
            }
            let n = resp.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            got.fetch_add(n as u64, Ordering::Relaxed);
            report();
        }
        std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    }
    prog(got.load(Ordering::Relaxed), total, 0.0);
    Ok(dest)
}

/// 用系统 tar（bsdtar）解压 zip/7z 到 dir。
pub fn extract(archive: &Path, dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(dir)
        .status()
        .map_err(|e| format!("调不起 tar：{e}"))?;
    if !status.success() {
        return Err(format!("解压失败（tar 退出码 {:?}）", status.code()));
    }
    Ok(())
}

/// 递归找一个包含 `main.py` 的目录（压缩包顶层目录名各版本可能不同）。
pub fn find_comfy_root(dir: &Path) -> Option<PathBuf> {
    fn walk(d: &Path, depth: usize) -> Option<PathBuf> {
        if depth > 3 {
            return None;
        }
        if d.join("main.py").is_file() && d.join("comfy").is_dir() {
            return Some(d.to_path_buf());
        }
        for e in std::fs::read_dir(d).ok()?.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(hit) = walk(&p, depth + 1) {
                    return Some(hit);
                }
            }
        }
        None
    }
    walk(dir, 0)
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
        let from = e.path();
        let to = dst.join(e.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn move_dir(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_dir(from, to)?;
    std::fs::remove_dir_all(from).map_err(|e| e.to_string())
}

pub fn now_stamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    d.to_string()
}

/// 源码模式：把新 ComfyUI 换进去，保留旧的 models/input/output/custom_nodes/user。
pub fn swap_source(new_root: &Path, target_root: &Path) -> Result<PathBuf, String> {
    let comfy = target_root.join("ComfyUI");
    let backup = target_root.join(format!("ComfyUI.bak-{}", now_stamp()));
    if comfy.exists() {
        // 先把用户数据挪进新版，再整体替换
        for sub in ["models", "input", "output", "custom_nodes", "user"] {
            let old = comfy.join(sub);
            if old.exists() {
                let dst = new_root.join(sub);
                if dst.exists() {
                    let _ = std::fs::remove_dir_all(&dst);
                }
                move_dir(&old, &dst)?;
            }
        }
        move_dir(&comfy, &backup)?;
    }
    if let Err(e) = move_dir(new_root, &comfy) {
        // 回滚
        if backup.exists() && !comfy.exists() {
            let _ = move_dir(&backup, &comfy);
        }
        return Err(format!("切换失败：{e}"));
    }
    Ok(comfy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_proxy_from_env() {
        // 只验证解析逻辑：环境变量存在时返回原样
        std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:7897");
        let p = detect_proxy();
        assert!(p.is_some());
        assert!(p.unwrap().contains("7897"));
        std::env::remove_var("HTTPS_PROXY");
    }

    #[test]
    fn finds_comfy_root_by_main_py() {
        let d = std::env::temp_dir().join(format!("cs_dl_root_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let sub = d.join("ComfyUI-master").join("comfy");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(d.join("ComfyUI-master").join("main.py"), b"print(1)").unwrap();
        let hit = find_comfy_root(&d).unwrap();
        assert!(hit.ends_with("ComfyUI-master"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
