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

/// Persist a PNG produced by the frontend (e.g. a composed comparison image).
#[tauri::command]
pub fn save_export(state: State<AppState>, name: String, data_url: String) -> Result<String, String> {
    let root = PathBuf::from(state.settings.lock().unwrap().comfy_root.clone());
    let dir = root.join("ComfyUI").join("output");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let b64 = data_url
        .split_once(',')
        .map(|(_, b)| b.to_string())
        .unwrap_or(data_url);
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| format!("base64 解码失败: {e}"))?;

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
    if !filename.to_lowercase().ends_with(".png") {
        filename.push_str(".png");
    }
    if filename == ".png" {
        filename = "export.png".to_string();
    }
    let full = dir.join(&filename);
    std::fs::write(&full, bytes).map_err(|e| e.to_string())?;
    Ok(full.to_string_lossy().to_string())
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
