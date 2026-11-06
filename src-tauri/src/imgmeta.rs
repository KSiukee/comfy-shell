//! PNG 元数据：在生成图里写 `iTXt` 块（keyword=comfyshell），
//! 用于「拖回窗口恢复参数」。私密项目不会调用这里。

use serde_json::Value;
use std::path::Path;

const SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const KEY: &[u8] = b"comfyshell";

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn chunk(typ: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(typ);
    out.extend_from_slice(data);
    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(typ);
    crc_data.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
    out
}

fn itxt_data(text: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(KEY);
    data.push(0);
    data.push(0); // compression flag
    data.push(0); // compression method
    data.push(0); // language tag empty
    data.push(0); // translated keyword empty
    data.extend_from_slice(text.as_bytes());
    data
}

fn chunk_keyword(data: &[u8]) -> Option<&[u8]> {
    let end = data.iter().position(|&b| b == 0)?;
    Some(&data[..end])
}

fn parse_itxt(data: &[u8]) -> Option<String> {
    let key_end = data.iter().position(|&b| b == 0)?;
    let mut p = key_end + 1;
    if p + 2 > data.len() {
        return None;
    }
    let flag = data[p];
    p += 2; // flag + method
    if flag != 0 {
        return None; // 不处理压缩
    }
    let _lang_end = data[p..].iter().position(|&b| b == 0)? + p;
    p = _lang_end + 1;
    let trans_end = data[p..].iter().position(|&b| b == 0)? + p;
    p = trans_end + 1;
    String::from_utf8(data[p..].to_vec()).ok()
}

fn parse_text(data: &[u8]) -> Option<String> {
    let key_end = data.iter().position(|&b| b == 0)?;
    String::from_utf8(data[key_end + 1..].to_vec()).ok()
}

/// 写入/替换 `comfyshell` 元数据块；只处理 PNG。
pub fn write_png_meta(path: &Path, meta: &Value) -> Result<(), String> {
    if path.extension().and_then(|x| x.to_str()).map(|x| x.to_ascii_lowercase()) != Some("png".into()) {
        return Ok(());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() < 12 || bytes[..8] != SIG {
        return Err("不是 PNG".into());
    }
    let text = serde_json::to_string(meta).map_err(|e| e.to_string())?;
    let new_chunk = chunk(b"iTXt", &itxt_data(&text));
    let mut out = Vec::with_capacity(bytes.len() + new_chunk.len());
    out.extend_from_slice(&SIG);

    let mut pos = 8usize;
    let mut inserted = false;
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
        let end = pos + 12 + len;
        if end > bytes.len() {
            return Err("PNG 数据损坏".into());
        }
        let typ = &bytes[pos + 4..pos + 8];
        let data = &bytes[pos + 8..pos + 8 + len];
        if (typ == b"iTXt" || typ == b"tEXt")
            && chunk_keyword(data).map(|k| k == KEY).unwrap_or(false)
        {
            pos = end; // 替换旧块
            continue;
        }
        if typ == b"IEND" && !inserted {
            out.extend_from_slice(&new_chunk);
            inserted = true;
        }
        out.extend_from_slice(&bytes[pos..end]);
        pos = end;
    }
    if !inserted {
        return Err("PNG 缺少 IEND".into());
    }
    crate::fsutil::atomic_write(path, &out)
}

/// 读取 `comfyshell` 元数据（供测试/未来命令行导出用）。
pub fn read_png_meta(path: &Path) -> Option<Value> {    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 12 || bytes[..8] != SIG {
        return None;
    }
    let mut pos = 8usize;
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
        let end = pos + 12 + len;
        if end > bytes.len() {
            return None;
        }
        let typ = &bytes[pos + 4..pos + 8];
        let data = &bytes[pos + 8..pos + 8 + len];
        if (typ == b"iTXt" || typ == b"tEXt")
            && chunk_keyword(data).map(|k| k == KEY).unwrap_or(false)
        {
            let text = if typ == b"iTXt" { parse_itxt(data) } else { parse_text(data) }?;
            return serde_json::from_str(&text).ok();
        }
        pos = end;
    }
    None
}

/// 从 ComfyUI 写进 PNG 的 `prompt` 文本块（完整 API 工作流 JSON）里捞 KSampler 的种子。
///
/// 之前「随机」(-1) 生成时我们自己的元数据只记了 -1，真实种子只在 ComfyUI 这个块里。
/// 换脸那种由壳自己合成的结果图没有这个块，捞不到（只能靠新版本写入的真种子）。
pub fn read_comfy_seed(path: &Path) -> Option<i64> {
    let bytes = std::fs::read(path).ok()?;
    parse_comfy_seed(&bytes)
}

pub fn parse_comfy_seed(bytes: &[u8]) -> Option<i64> {
    if bytes.len() < 12 || bytes[..8] != SIG {
        return None;
    }
    let mut pos = 8usize;
    while pos + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]) as usize;
        let end = pos + 12 + len;
        if end > bytes.len() {
            return None;
        }
        let typ = &bytes[pos + 4..pos + 8];
        let data = &bytes[pos + 8..pos + 8 + len];
        if (typ == b"tEXt" || typ == b"iTXt") && chunk_keyword(data) == Some(b"prompt".as_slice()) {
            let text = if typ == b"iTXt" { parse_itxt(data) } else { parse_text(data) };
            if let Some(seed) = text.as_deref().and_then(seed_from_workflow) {
                return Some(seed);
            }
        }
        pos = end;
    }
    None
}

fn seed_from_workflow(text: &str) -> Option<i64> {
    let v: Value = serde_json::from_str(text).ok()?;
    let obj = v.as_object()?;
    let mut fallback = None;
    for node in obj.values() {
        let ct = node.get("class_type").and_then(|x| x.as_str()).unwrap_or("");
        let Some(inputs) = node.get("inputs").and_then(|x| x.as_object()) else {
            continue;
        };
        let seed = inputs
            .get("seed")
            .and_then(|x| x.as_i64())
            .or_else(|| inputs.get("noise_seed").and_then(|x| x.as_i64()));
        let Some(seed) = seed.filter(|s| *s > 0) else {
            continue;
        };
        if ct.starts_with("KSampler") {
            return Some(seed);
        }
        if fallback.is_none() && (ct.contains("Sampler") || ct.contains("Noise")) {
            fallback = Some(seed);
        }
    }
    fallback
}

/// 读一张输出图的参数块（图库详情用）：**只认 root 目录内的明文图**，
/// 私密图（`vault://`）与越界路径一律 None——私密项目本来就不写明文参数。
pub fn read_meta_under(root: &Path, path: &Path) -> Option<Value> {
    if path.to_string_lossy().starts_with("vault://") {
        return None;
    }
    let canon_root = root.canonicalize().ok()?;
    let canon = path.canonicalize().ok()?;
    if !canon.starts_with(&canon_root) {
        return None;
    }
    read_png_meta(&canon)
}

#[cfg(test)]
#[path = "imgmeta_tests.rs"]
mod tests;
