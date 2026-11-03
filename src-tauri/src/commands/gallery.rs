//! 图库 / 历史 / 星标 / 导出 / 翻译。

use crate::*;
use tauri::State;

pub(crate) fn image_as_data_url(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取图片失败: {e}"))?;
    use base64::Engine as _;
    let lower = path.to_string_lossy().to_lowercase();
    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/png"
    };
    Ok(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}


#[tauri::command]
pub fn history_list(app: tauri::AppHandle) -> Vec<history::HistoryItem> {
    history::load(&cfg_dir(&app))
}

#[tauri::command]
pub fn history_clear(app: tauri::AppHandle) {
    history::clear(&cfg_dir(&app));
}



#[tauri::command]
pub async fn library_scan(app: tauri::AppHandle) -> Vec<library::LibItem> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let out = output_dir(&state);
        library::scan(&cfg_dir(&handle), &out, 3000)
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn library_delete(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let out = output_dir(&state);
        library::delete(&out, &paths)
    })
    .await
    .map_err(|e| format!("删除任务失败: {e}"))?
}

#[tauri::command]
pub async fn delete_results(app: tauri::AppHandle, paths: Vec<String>) -> Result<usize, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<usize, String> {
        let state = handle.state::<AppState>();
        let out = output_dir(&state);
        let n = library::delete(&out, &paths)?;

        // drop the deleted shots from history so they do not come back as broken cards
        let dir = cfg_dir(&handle);
        let mut items = history::load(&dir);
        let dead: std::collections::HashSet<&String> = paths.iter().collect();
        for it in items.iter_mut() {
            it.images.retain(|im| !dead.contains(&im.path));
        }
        items.retain(|it| !it.images.is_empty());
        history::save(&dir, &items);
        Ok(n)
    })
    .await
    .map_err(|e| format!("删除任务失败: {e}"))?
}

#[tauri::command]
pub fn stars_list(app: tauri::AppHandle) -> Vec<String> {
    library::stars(&cfg_dir(&app))
}

/// 把已有的收藏补一份项目内 `_favorites` 镜像（幂等；老数据只有 stars.json）。
#[tauri::command]
pub async fn library_sync_favorites(app: tauri::AppHandle) -> usize {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let out = output_dir(&state);
        library::sync_star_mirrors(&cfg_dir(&handle), &out)
    })
    .await
    .unwrap_or(0)
}

#[tauri::command]
pub fn library_star(app: tauri::AppHandle, path: String, starred: bool) {
    if let Some(rest) = path.strip_prefix("vault://") {
        if let Some((id, name)) = rest.split_once('/') {
            let _ = vault::set_star(&cfg_dir(&app), id, name, starred);
        }
        return;
    }
    let state = app.state::<AppState>();
    let out = output_dir(&state);
    library::star(&cfg_dir(&app), &out, &path, starred);
}

/// 图库详情：读一张输出图的 `iTXt` 参数（只认 ComfyUI 目录内的明文图）。
/// 私密图与越界路径返回 null——界面据此显示「私密图不写参数」。
#[tauri::command]
pub fn image_meta(state: State<AppState>, path: String) -> Option<serde_json::Value> {
    let root = { state.settings.lock().ok()?.comfy_root.clone() };
    crate::imgmeta::read_meta_under(std::path::Path::new(&root), std::path::Path::new(&path))
}

pub(crate) fn decode_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let b64 = data_url
        .split_once(',')
        .map(|(_, b)| b)
        .unwrap_or(data_url);
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("base64 解码失败: {e}"))
}

/// 覆盖保存：把调色结果写回原图。
/// 只允许 `ComfyUI/output` 下**已经存在**的图片——前端能传路径，必须当不可信输入处理：
/// 先做一次 canonicalize，再用前缀检查，`..` 和符号链接都过不去。
/// 注意：写回的是纯图片字节，原 PNG 里的提示词参数块会丢（界面会先确认）。
#[tauri::command]
pub fn save_over_image(
    state: State<AppState>,
    path: String,
    data_url: String,
) -> Result<String, String> {
    let root = {
        let s = state.settings.lock().unwrap();
        crate::state::output_root(&s)
    };
    let canon_root = std::fs::canonicalize(&root).map_err(|e| format!("输出目录不可用：{e}"))?;
    let canon = std::fs::canonicalize(&path).map_err(|_| "原图不存在（可能已被移走）".to_string())?;
    if !canon.starts_with(&canon_root) {
        return Err("只能覆盖 output 目录里的图".into());
    }
    let lower = canon.to_string_lossy().to_lowercase();
    if ![".png", ".jpg", ".jpeg", ".webp"].iter().any(|e| lower.ends_with(e)) {
        return Err("不支持的图片格式".into());
    }
    let bytes = decode_data_url(&data_url)?;
    crate::fsutil::atomic_write(&canon, &bytes)?;
    // 返回前端传进来的原样路径：canonicalize 会带上 `\\?\` 前缀，前端拿它跟图库里的
    // 条目比对会对不上（覆盖后缩略图就刷不新）。
    Ok(path)
}

/// Persist an image composed by the frontend (comparison sheet, retouched photo).
/// `subdir` 是 output 下的相对目录（比如 projects/<id>），调色另存时放回原图旁边；
/// 只接受相对路径且不许出现 `..`——前端能传这个参数，必须当不可信输入处理。
#[tauri::command]
pub fn save_export(
    state: State<AppState>,
    name: String,
    data_url: String,
    subdir: Option<String>,
) -> Result<String, String> {
    let out_root = {
        let s = state.settings.lock().unwrap();
        crate::state::output_root(&s)
    };
    let mut dir = out_root;
    if let Some(sub) = subdir.map(|s| s.trim().replace('\\', "/")).filter(|s| !s.is_empty()) {
        let rel = std::path::Path::new(&sub);
        let bad = rel.is_absolute()
            || rel.components().any(|c| {
                !matches!(c, std::path::Component::Normal(_))
            });
        if bad {
            return Err("子目录不合法".into());
        }
        dir = dir.join(rel);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let bytes = decode_data_url(&data_url)?;

    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let mut filename = safe.trim().to_string();
    // 前端（调色/裁切的另存）可能送来 jpg/webp：按扩展名原样落盘，
    // 只有不认识的后缀才补 .png——否则 JPEG 字节会被写成 .png 的假扩展名。
    let lower = filename.to_lowercase();
    let known = [".png", ".jpg", ".jpeg", ".webp"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    if !known {
        filename.push_str(".png");
    }
    if filename == ".png" {
        filename = "export.png".to_string();
    }
    let full = dir.join(&filename);
    std::fs::write(&full, bytes).map_err(|e| e.to_string())?;
    Ok(full.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::decode_data_url;

    #[test]
    fn decodes_data_url_and_rejects_garbage() {
        let bytes = decode_data_url("data:image/png;base64,aGVsbG8=").unwrap();
        assert_eq!(bytes, b"hello");
        // 没有前缀的裸 base64 也认（旧调用方）
        assert_eq!(decode_data_url("aGVsbG8=").unwrap(), b"hello");
        assert!(decode_data_url("data:image/png;base64,!!!!").is_err());
    }
}


#[tauri::command]
pub async fn translate_text(app: tauri::AppHandle, text: String) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let port = handle.state::<AppState>().settings.lock().unwrap().port;
        if api::health(port).is_none() {
            return Err("ComfyUI 没有在运行".into());
        }
        enhance::translate(port, &text)
    })
    .await
    .map_err(|e| format!("翻译任务失败: {e}"))?
}
