//! LUT（`.cube`）目录：扫描 / 读取 / 导入。
//!
//! 图库调色用它：前端把 LUT 列表展示出来，选中后读文本、在浏览器里做三线性插值。
//! 目录固定在配置目录 `luts/`，读取按「单层文件名 + 必须 .cube」收口——
//! 前端拿到的是命令，不是文件系统句柄，不能借这里读任意路径。

use crate::*;
use serde::Serialize;

/// 一份可用的 LUT（只给前端看名字与大小，路径不出后端）。
#[derive(Serialize)]
pub struct LutEntry {
    pub name: String,
    pub bytes: u64,
}

const MAX_LUT_BYTES: u64 = 32 * 1024 * 1024;

fn luts_dir(app: &tauri::AppHandle) -> PathBuf {
    let d = cfg_dir(app).join("luts");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// 只收单层文件名；带目录分隔符的一律拒绝（防 ../ 越界）。
fn safe_cube_name(name: &str) -> Result<String, String> {
    let base = name.trim();
    if base.contains('/') || base.contains('\\') {
        return Err("LUT 名字不能带路径".into());
    }
    if base.is_empty() || base == "." || base == ".." {
        return Err("LUT 名字不合法".into());
    }
    if !base.to_lowercase().ends_with(".cube") {
        return Err("只支持 .cube 文件".into());
    }
    Ok(base.to_string())
}

/// LUT 目录（供界面「打开文件夹」用；顺带确保它存在）。
#[tauri::command]
pub fn luts_dir_path(app: tauri::AppHandle) -> String {
    luts_dir(&app).to_string_lossy().to_string()
}

#[tauri::command]
pub fn luts_list(app: tauri::AppHandle) -> Vec<LutEntry> {
    let dir = luts_dir(&app);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<LutEntry> = rd
        .flatten()
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("cube"))
                .unwrap_or(false)
        })
        .map(|e| LutEntry {
            name: e.file_name().to_string_lossy().to_string(),
            bytes: e.metadata().map(|m| m.len()).unwrap_or(0),
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

#[tauri::command]
pub fn luts_read(app: tauri::AppHandle, name: String) -> Result<String, String> {
    let name = safe_cube_name(&name)?;
    let path = luts_dir(&app).join(&name);
    let meta = std::fs::metadata(&path).map_err(|_| "找不到这个 LUT（可能已被移走）".to_string())?;
    if meta.len() > MAX_LUT_BYTES {
        return Err("LUT 文件太大（上限 32 MB）".into());
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取 LUT 失败：{e}"))
}

/// 前端读 `.cube` 文本后经这里落盘（浏览器拿不到真实路径，见 luts 说明）。
#[tauri::command]
pub fn luts_save(app: tauri::AppHandle, name: String, text: String) -> Result<String, String> {
    let name = safe_cube_name(&name)?;
    if text.len() as u64 > MAX_LUT_BYTES {
        return Err("LUT 文件太大（上限 32 MB）".into());
    }
    if !text.contains("LUT_3D_SIZE") && !text.contains("LUT_1D_SIZE") {
        return Err("这不像 .cube 文件（缺 LUT_3D_SIZE）".into());
    }
    let path = luts_dir(&app).join(&name);
    crate::fsutil::atomic_write_str(&path, &text)?;
    Ok(path.to_string_lossy().to_string())
}

/// 删除一份 LUT（只删目录内单层文件）。
#[tauri::command]
pub fn luts_delete(app: tauri::AppHandle, name: String) -> Result<(), String> {
    let name = safe_cube_name(&name)?;
    std::fs::remove_file(luts_dir(&app).join(&name)).map_err(|e| format!("删除 LUT 失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::safe_cube_name;

    #[test]
    fn rejects_traversal_and_non_cube() {
        assert!(safe_cube_name("../evil.cube").is_err());
        assert!(safe_cube_name("a/b.cube").is_err());
        assert!(safe_cube_name("..\\evil.cube").is_err());
        assert!(safe_cube_name("photo.png").is_err());
        assert!(safe_cube_name("").is_err());
        assert!(safe_cube_name("..").is_err());
        assert_eq!(safe_cube_name(" FILM01.cube ").unwrap(), "FILM01.cube");
        assert!(safe_cube_name("FILM.CUBE").is_ok());
    }
}
