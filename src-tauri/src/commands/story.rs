//! 剧本设定的命令层：读、写、预设表。

use crate::story::{self, Story};
use crate::*;

fn cfg_of(app: &tauri::AppHandle) -> PathBuf {
    crate::state::cfg_dir(app)
}

#[tauri::command]
pub fn story_get(app: tauri::AppHandle, project: String) -> Story {
    story::load(&cfg_of(&app), &project)
}

#[tauri::command]
pub fn story_set(app: tauri::AppHandle, project: String, story: Story) -> Result<Story, String> {
    let cfg = cfg_of(&app);
    story::save(&cfg, &project, &story)?;
    Ok(story::load(&cfg, &project))
}

/// 画风预设（id / 标签 / 实际词）——前端下拉直接用这份，别抄第二份。
#[tauri::command]
pub fn story_presets() -> serde_json::Value {
    story::presets()
}

/// 把设定拼到一段描述前面（纯读；`enabled=false` 时逐字返回）。
#[tauri::command]
pub fn story_compose(app: tauri::AppHandle, project: String, text: String) -> String {
    story::load(&cfg_of(&app), &project).compose(&text)
}
