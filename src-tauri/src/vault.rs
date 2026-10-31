//! 私密项目仓库：口令 → Argon2id → KEK → 包住随机主密钥；文件用 XChaCha20-Poly1305。
//!
//! 一个仓库一把主密钥（会话内只在内存）；每个私密项目一个加密 meta 文件 +
//! 一个图片目录（文件名编号，真实文件名在加密 meta 里）。锁定时把密钥清零。
//!
//! 范围（v0.46）：项目配置 + 图片可以移入/移出；**私密项目暂不支持生成**
//! （ComfyUI 只会写明文，封存流水线下一步做）。

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::project::Project;

const MAGIC: &[u8; 4] = b"VLT1";
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
pub(crate) const KEY_LEN: usize = 32;
const KDF_MEM_KIB: u32 = 64 * 1024;
const KDF_ITERS: u32 = 3;

static SESSION: Mutex<Option<[u8; KEY_LEN]>> = Mutex::new(None);

pub(crate) fn vault_dir(cfg: &Path) -> PathBuf {
    cfg.join("vault")
}
fn key_path(cfg: &Path) -> PathBuf {
    vault_dir(cfg).join("key.bin")
}
pub(crate) fn meta_path(cfg: &Path, id: &str) -> PathBuf {
    vault_dir(cfg).join(format!("{id}.enc"))
}
pub(crate) fn images_dir(cfg: &Path, id: &str) -> PathBuf {
    vault_dir(cfg).join(id)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VaultImage {
    pub name: String,
    pub idx: u32,
    pub width: u32,
    pub height: u32,
    pub size: u64,
    pub mtime: u64,
    #[serde(default)]
    pub starred: bool,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct VaultData {
    pub project: Project,
    pub images: Vec<VaultImage>,
}

pub(crate) fn safe_id(s: &str) -> Result<(), String> {
    if s.is_empty() || s.contains(['\\', '/']) || s == "." || s == ".." {
        return Err("非法的项目 id".into());
    }
    Ok(())
}

fn derive(pass: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], String> {
    let params = Params::new(KDF_MEM_KIB, KDF_ITERS, 1, Some(KEY_LEN))
        .map_err(|e| format!("KDF 参数错误：{e}"))?;
    let a = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; KEY_LEN];
    a.hash_password_into(pass.as_bytes(), salt, &mut out)
        .map_err(|e| format!("口令派生失败：{e}"))?;
    Ok(out)
}

pub(crate) fn seal(key: &[u8; KEY_LEN], plain: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), plain)
        .map_err(|e| format!("加密失败：{e}"))?;
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn open(key: &[u8; KEY_LEN], data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 4 + NONCE_LEN || &data[..4] != MAGIC {
        return Err("不是本程序的加密文件".into());
    }
    let nonce = &data[4..4 + NONCE_LEN];
    let ct = &data[4 + NONCE_LEN..];
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|e| e.to_string())?;
    cipher
        .decrypt(XNonce::from_slice(nonce), ct)
        .map_err(|_| "解不开（口令不对或文件损坏）".to_string())
}

fn wrap_key(pass: &str, master: &[u8; KEY_LEN]) -> Result<Vec<u8>, String> {
    let mut salt = [0u8; SALT_LEN];
    rand::thread_rng().fill_bytes(&mut salt);
    let kek = derive(pass, &salt)?;
    let sealed = seal(&kek, master)?;
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&salt);
    out.extend_from_slice(&sealed);
    Ok(out)
}

fn unwrap_key(pass: &str, data: &[u8]) -> Result<[u8; KEY_LEN], String> {
    if data.len() < 4 + SALT_LEN + 4 + NONCE_LEN || &data[..4] != MAGIC {
        return Err("仓库文件损坏".into());
    }
    let salt = &data[4..4 + SALT_LEN];
    let sealed = &data[4 + SALT_LEN..];
    let kek = derive(pass, salt)?;
    let master = open(&kek, sealed).map_err(|_| "口令不对".to_string())?;
    if master.len() != KEY_LEN {
        return Err("主密钥长度不对".into());
    }
    let mut out = [0u8; KEY_LEN];
    out.copy_from_slice(&master);
    Ok(out)
}

pub fn status(cfg: &Path) -> serde_json::Value {
    let unlocked = SESSION.lock().map(|g| g.is_some()).unwrap_or(false);
    json!({"exists": key_path(cfg).is_file(), "unlocked": unlocked})
}

pub fn setup(cfg: &Path, pass: &str) -> Result<(), String> {
    if pass.chars().count() < 4 {
        return Err("口令至少 4 个字符".into());
    }
    if key_path(cfg).is_file() {
        return Err("锁定区已存在，直接解锁".into());
    }
    std::fs::create_dir_all(vault_dir(cfg)).map_err(|e| e.to_string())?;
    let mut master = [0u8; KEY_LEN];
    rand::thread_rng().fill_bytes(&mut master);
    let data = wrap_key(pass, &master)?;
    crate::fsutil::atomic_write(&key_path(cfg), &data)?;
    *SESSION.lock().unwrap() = Some(master);
    Ok(())
}

pub fn unlock(cfg: &Path, pass: &str) -> Result<(), String> {
    let data = std::fs::read(key_path(cfg)).map_err(|_| "还没设置锁定区".to_string())?;
    let master = unwrap_key(pass, &data)?;
    *SESSION.lock().unwrap() = Some(master);
    Ok(())
}

pub fn lock() {
    if let Ok(mut g) = SESSION.lock() {
        if let Some(mut k) = g.take() {
            for b in k.iter_mut() {
                *b = 0;
            }
        }
    }
}

pub(crate) fn key_clone() -> Option<[u8; KEY_LEN]> {
    SESSION.lock().ok().and_then(|g| *g)
}

/// 项目是否在锁定区里（不需要解锁也能判断，只查文件名）。
pub fn is_private(cfg: &Path, id: &str) -> bool {
    meta_path(cfg, id).is_file()
}

pub(crate) fn session_key() -> Result<[u8; KEY_LEN], String> {
    SESSION
        .lock()
        .ok()
        .and_then(|g| *g)
        .ok_or_else(|| "锁定区没解锁".to_string())
}

pub fn is_unlocked() -> bool {
    SESSION.lock().map(|g| g.is_some()).unwrap_or(false)
}

pub(crate) fn write_meta(cfg: &Path, data: &VaultData, key: &[u8; KEY_LEN]) -> Result<(), String> {
    let text = serde_json::to_string(data).map_err(|e| e.to_string())?;
    let sealed = seal(key, text.as_bytes())?;
    crate::fsutil::atomic_write(&meta_path(cfg, &data.project.id), &sealed)
}

pub(crate) fn read_meta(cfg: &Path, id: &str, key: &[u8; KEY_LEN]) -> Result<VaultData, String> {
    let raw = std::fs::read(meta_path(cfg, id)).map_err(|e| format!("读取私密项目失败：{e}"))?;
    let plain = open(key, &raw)?;
    serde_json::from_slice(&plain).map_err(|e| format!("解析私密项目失败：{e}"))
}

/// 当前解锁状态下的全部私密项目（不解图片）。
pub fn list(cfg: &Path) -> Vec<Project> {
    let Ok(key) = session_key() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(vault_dir(cfg)) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("enc") {
            continue;
        }
        let Some(id) = p.file_stem().and_then(|x| x.to_str()) else {
            continue;
        };
        if let Ok(d) = read_meta(cfg, id, &key) {
            out.push(d.project);
        }
    }
    out.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at).then(a.name.cmp(&b.name)));
    out
}

fn project_image_name(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|x| x.to_str())
        .map(|x| x.to_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp")
}

/// 把一个普通项目整个搬进锁定区（配置 + 图片），并删掉明文。
pub fn make_private(cfg: &Path, output: &Path, id: &str) -> Result<Project, String> {
    safe_id(id)?;
    let key = session_key()?;
    if meta_path(cfg, id).exists() {
        return Err("这个项目已经在锁定区里了".into());
    }
    let mut project = crate::project::load(cfg, id)?;
    project.private = true;

    let src_imgs = output.join("projects").join(id);
    let dst_imgs = images_dir(cfg, id);
    let mut images = Vec::new();
    if src_imgs.is_dir() {
        std::fs::create_dir_all(&dst_imgs).map_err(|e| e.to_string())?;
        let mut idx = 0u32;
        let mut rd: Vec<_> = std::fs::read_dir(&src_imgs)
            .map_err(|e| e.to_string())?
            .flatten()
            .collect();
        rd.sort_by_key(|e| e.file_name());
        for e in rd {
            let p = e.path();
            if !p.is_file() || !project_image_name(&p) {
                continue;
            }
            let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
            let size = bytes.len() as u64;
            let (width, height) = crate::library::image_size(&p);
            let mtime = std::fs::metadata(&p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let sealed = seal(&key, &bytes)?;
            crate::fsutil::atomic_write(&dst_imgs.join(format!("{idx}.enc")), &sealed)?;
            images.push(VaultImage {
                name: p.file_name().and_then(|x| x.to_str()).unwrap_or("image.png").to_string(),
                idx,
                width,
                height,
                size,
                mtime,
                starred: false,
            });
            idx += 1;
        }
    }

    let data = VaultData { project: project.clone(), images };
    write_meta(cfg, &data, &key)?;
    // meta 写成功后才删明文（失败时至少原始数据还在）
    for im in &data.images {
        let _ = std::fs::remove_file(src_imgs.join(&im.name));
    }
    let _ = std::fs::remove_dir_all(&src_imgs);
    let _ = std::fs::remove_dir_all(cfg.join("projects").join(id));
    Ok(project)
}

/// 移出锁定区：解密配置与图片，写回普通项目目录。
pub fn unprivate(cfg: &Path, output: &Path, id: &str) -> Result<Project, String> {
    safe_id(id)?;
    let key = session_key()?;
    let data = read_meta(cfg, id, &key)?;
    let mut project = data.project;
    project.private = false;
    project.updated_at = now_ms();

    let dst_imgs = output.join("projects").join(id);
    if !data.images.is_empty() {
        std::fs::create_dir_all(&dst_imgs).map_err(|e| e.to_string())?;
    }
    for im in &data.images {
        let raw = std::fs::read(images_dir(cfg, id).join(format!("{}.enc", im.idx)))
            .map_err(|e| format!("读取加密图片失败：{e}"))?;
        let plain = open(&key, &raw)?;
        crate::fsutil::atomic_write(&dst_imgs.join(&im.name), &plain)?;
    }

    let dir = cfg.join("projects").join(id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write(&dir.join("project.json"), text.as_bytes())?;
    // 明文都写成功后再清仓库，失败时至少密文还完整
    let _ = std::fs::remove_file(meta_path(cfg, id));
    let _ = std::fs::remove_dir_all(images_dir(cfg, id));
    Ok(project)
}

/// 私密图片的收藏：写在加密 meta 里，不落 stars.json。
pub fn set_star(cfg: &Path, id: &str, name: &str, on: bool) -> Result<(), String> {
    safe_id(id)?;
    let key = session_key()?;
    let mut data = read_meta(cfg, id, &key)?;
    let im = data
        .images
        .iter_mut()
        .find(|x| x.name == name)
        .ok_or("私密项目里没有这张图")?;
    im.starred = on;
    write_meta(cfg, &data, &key)
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `vault://<id>/<name>` → 解密后的字节（read_output_image 用）。
pub fn read_image(cfg: &Path, id: &str, name: &str) -> Result<Vec<u8>, String> {
    let key = session_key()?;
    let data = read_meta(cfg, id, &key)?;
    let im = data
        .images
        .iter()
        .find(|x| x.name == name)
        .ok_or("私密项目里没有这张图")?;
    let raw = std::fs::read(images_dir(cfg, id).join(format!("{}.enc", im.idx)))
        .map_err(|e| format!("读取加密图片失败：{e}"))?;
    open(&key, &raw)
}

/// 图库条目（未解锁时为空）。
pub fn lib_items(cfg: &Path) -> Vec<crate::library::LibItem> {
    let Ok(key) = session_key() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(vault_dir(cfg)) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("enc") {
            continue;
        }
        let Some(id) = p.file_stem().and_then(|x| x.to_str()) else {
            continue;
        };
        let Ok(data) = read_meta(cfg, id, &key) else { continue };
        for im in data.images {
            out.push(crate::library::LibItem {
                path: format!("vault://{id}/{}", im.name),
                filename: im.name.clone(),
                subfolder: format!("vault/{id}"),
                size: im.size,
                mtime: im.mtime,
                width: im.width,
                height: im.height,
                starred: im.starred,
            });
        }
    }
    out
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;

pub use crate::vault_ops::{delete, seal_paths, set_active};
