//! 项目 / 锁定区 / 镜头 / 版本 / 提示词库 / 备份导入导出 的命令包装。

use crate::*;
use tauri::State;

pub fn merged_projects(cfg: &std::path::Path, include_hidden: bool) -> Vec<project::Project> {
    let mut list = project::list(cfg, include_hidden);
    if vault::is_unlocked() {
        list.extend(vault::list(cfg));
    }
    list.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.last_opened_at.cmp(&a.last_opened_at))
            .then(a.name.cmp(&b.name))
    });
    list
}

#[tauri::command]
pub fn project_list(app: tauri::AppHandle, include_hidden: Option<bool>) -> Vec<project::Project> {
    merged_projects(&cfg_dir(&app), include_hidden.unwrap_or(false))
}

#[tauri::command]
pub fn project_create(app: tauri::AppHandle, name: String) -> Result<project::Project, String> {
    project::create(&cfg_dir(&app), &name)
}

#[tauri::command]
pub fn project_rename(app: tauri::AppHandle, id: String, name: String) -> Result<project::Project, String> {
    project::rename(&cfg_dir(&app), &id, &name)
}

#[tauri::command]
pub fn project_set_active(
    app: tauri::AppHandle,
    id: Option<String>,
) -> Result<Vec<project::Project>, String> {
    let cfg = cfg_dir(&app);
    let target_private = id
        .as_deref()
        .map(|x| vault::is_private(&cfg, x))
        .unwrap_or(false);
    if target_private {
        vault::set_active(&cfg, id.as_deref())?;
        project::set_active(&cfg, None)?;
    } else {
        project::set_active(&cfg, id.as_deref())?;
        if vault::is_unlocked() {
            let _ = vault::set_active(&cfg, None);
        }
    }
    Ok(merged_projects(&cfg, false))
}

#[tauri::command]
pub fn project_set_hidden(app: tauri::AppHandle, id: String, hidden: bool) -> Result<project::Project, String> {
    project::set_hidden(&cfg_dir(&app), &id, hidden)
}

/// 项目的云端看图开关。
///
/// **私密项目打开必须带 `confirm: true`**——界面会先弹一次写明后果的确认；
/// 没带就直接拒。红线由后端守，不指望每个调用方都记得。
#[tauri::command]
pub fn project_set_cloud_vision(
    app: tauri::AppHandle,
    id: String,
    on: bool,
    confirm: Option<bool>,
) -> Result<project::Project, String> {
    let cfg = cfg_dir(&app);
    let private = vault::is_private(&cfg, &id);
    let hidden = project::load(&cfg, &id).map(|p| p.hidden).unwrap_or(false);
    if on && (private || hidden) && !confirm.unwrap_or(false) {
        return Err(if private {
            "私密项目打开云端看图要再确认一次：这张图会离开本机".into()
        } else {
            "隐藏项目打开云端看图要再确认一次：这张图会离开本机".into()
        });
    }
    project::set_cloud_vision(&cfg, &id, Some(on))
}

#[tauri::command]
pub fn project_delete(app: tauri::AppHandle, state: State<AppState>, id: String) -> Result<(), String> {
    project::delete(&cfg_dir(&app), &id, &output_dir(&state))
}

#[tauri::command]
pub fn project_trash(app: tauri::AppHandle) -> Vec<project::TrashItem> {
    project::trash(&cfg_dir(&app))
}

#[tauri::command]
pub fn project_restore(app: tauri::AppHandle, state: State<AppState>, dir: String) -> Result<project::Project, String> {
    project::restore(&cfg_dir(&app), &dir, &output_dir(&state))
}

#[tauri::command]
pub fn project_purge(app: tauri::AppHandle, dir: String) -> Result<(), String> {
    project::purge(&cfg_dir(&app), &dir)
}

// ---------------------------------------------------------------------------
// 私密仓库（口令 → Argon2id → 主密钥；项目配置 + 图片加密）
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn vault_status(app: tauri::AppHandle) -> serde_json::Value {
    vault::status(&cfg_dir(&app))
}

#[tauri::command]
pub fn vault_setup(app: tauri::AppHandle, pass: String) -> Result<(), String> {
    vault::setup(&cfg_dir(&app), &pass)
}

#[tauri::command]
pub fn vault_unlock(app: tauri::AppHandle, pass: String) -> Result<(), String> {
    vault::unlock(&cfg_dir(&app), &pass)
}

#[tauri::command]
pub fn vault_lock() {
    vault::lock();
}

#[tauri::command]
pub fn project_make_private(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
) -> Result<project::Project, String> {
    vault::make_private(&cfg_dir(&app), &output_dir(&state), &id)
}

#[tauri::command]
pub fn project_private_list(app: tauri::AppHandle) -> Vec<project::Project> {
    vault::list(&cfg_dir(&app))
}

#[tauri::command]
pub fn project_unprivate(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
) -> Result<project::Project, String> {
    vault::unprivate(&cfg_dir(&app), &output_dir(&state), &id)
}

#[tauri::command]
pub fn project_private_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if !vault::is_unlocked() {
        return Err("锁定区没解锁".into());
    }
    vault::delete(&cfg_dir(&app), &id)
}

// ---------------------------------------------------------------------------
// 备份 / 导出 / 导入
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn pick_folder() -> Option<String> {
    tauri::async_runtime::spawn_blocking(|| {
        rfd::FileDialog::new()
            .pick_folder()
            .map(|p| p.to_string_lossy().to_string())
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub fn backup_now(app: tauri::AppHandle, dest: String) -> Result<String, String> {
    let dir = dataio::backup(&cfg_dir(&app), std::path::Path::new(&dest))?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn project_export(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
    dest: String,
) -> Result<String, String> {
    let dir = dataio::export_project(
        &cfg_dir(&app),
        &output_dir(&state),
        &id,
        std::path::Path::new(&dest),
    )?;
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn workspace_get(
    app: tauri::AppHandle,
    project: String,
) -> Result<Option<serde_json::Value>, String> {
    crate::workspace::load(&cfg_dir(&app), &project)
}

#[tauri::command]
pub fn workspace_save(
    app: tauri::AppHandle,
    project: String,
    data: serde_json::Value,
) -> Result<(), String> {
    crate::workspace::save(&cfg_dir(&app), &project, &data)
}

#[tauri::command]
pub fn project_import(
    app: tauri::AppHandle,
    state: State<AppState>,
    folder: String,
) -> Result<project::Project, String> {
    dataio::import_project(
        &cfg_dir(&app),
        &output_dir(&state),
        std::path::Path::new(&folder),
    )
}

// ---------------------------------------------------------------------------
// 镜头（一话 = 一组镜头）
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn shots_list(app: tauri::AppHandle, project: String) -> Result<Vec<shots::Shot>, String> {
    shots::list(&cfg_dir(&app), &project)
}

#[tauri::command]
pub fn shots_add(
    app: tauri::AppHandle,
    project: String,
    prompts: Vec<String>,
    defaults: shots::ShotDefaults,
) -> Result<Vec<shots::Shot>, String> {
    shots::add_many(&cfg_dir(&app), &project, prompts, &defaults)
}

#[tauri::command]
pub fn shot_add_image(
    app: tauri::AppHandle,
    project: String,
    id: String,
    path: String,
) -> Result<shots::Shot, String> {
    shots::add_image(&cfg_dir(&app), &project, &id, &path)
}

#[tauri::command]
pub fn shot_set_layout(
    app: tauri::AppHandle,
    project: String,
    id: String,
    layout: Option<String>,
) -> Result<shots::Shot, String> {
    shots::set_layout(&cfg_dir(&app), &project, &id, layout)
}

#[tauri::command]
pub fn shot_delete(app: tauri::AppHandle, project: String, id: String) -> Result<(), String> {
    shots::delete(&cfg_dir(&app), &project, &id)
}

/// 给一镜挂角色卡：只存 id，不校验存在性——角色被删了要在体检里报出来。
#[tauri::command]
pub fn shot_set_characters(
    app: tauri::AppHandle,
    project: String,
    id: String,
    characters: Vec<String>,
) -> Result<shots::Shot, String> {
    shots::set_characters(&cfg_dir(&app), &project, &id, characters)
}

/// 记下这一镜出图实际用到的角色与底板（整话一致性体检的证据）。
#[tauri::command]
pub fn shot_set_cast(
    app: tauri::AppHandle,
    project: String,
    id: String,
    cast: Vec<shots::CastRef>,
) -> Result<shots::Shot, String> {
    shots::set_cast(&cfg_dir(&app), &project, &id, cast)
}

#[tauri::command]
pub fn shots_export(
    app: tauri::AppHandle,
    state: State<AppState>,
    project: String,
    dest: String,
) -> Result<String, String> {
    let dir = shots::export_episode(
        &cfg_dir(&app),
        &output_dir(&state),
        &project,
        std::path::Path::new(&dest),
    )?;
    Ok(dir.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// 版本谱系（项目内）
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn layout_version_save(
    app: tauri::AppHandle,
    project: String,
    name: String,
    items: String,
) -> Result<versions::Version, String> {
    versions::save_layout(&cfg_dir(&app), &project, &name, &items)
}

#[tauri::command]
pub fn versions_list(app: tauri::AppHandle, project: String) -> Result<Vec<versions::Version>, String> {
    versions::list(&cfg_dir(&app), &project)
}

#[tauri::command]
pub fn version_star(
    app: tauri::AppHandle,
    project: String,
    id: String,
    stars: u8,
) -> Result<versions::Version, String> {
    versions::star(&cfg_dir(&app), &project, &id, stars)
}

#[tauri::command]
pub fn version_delete(app: tauri::AppHandle, project: String, id: String) -> Result<(), String> {
    versions::delete(&cfg_dir(&app), &project, &id)
}

// ---------------------------------------------------------------------------
// 跨项目提示词库
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn promptlib_list(app: tauri::AppHandle) -> Result<Vec<promptlib::PromptEntry>, String> {
    promptlib::list(&cfg_dir(&app))
}

#[tauri::command]
pub fn promptlib_save(
    app: tauri::AppHandle,
    text: String,
    negative: String,
    tags: Vec<String>,
    project: Option<String>,
) -> Result<promptlib::PromptEntry, String> {
    promptlib::upsert(
        &cfg_dir(&app),
        &text,
        &negative,
        tags,
        project.as_deref(),
    )
}

#[tauri::command]
pub fn promptlib_star(
    app: tauri::AppHandle,
    id: String,
    favorite: bool,
) -> Result<promptlib::PromptEntry, String> {
    promptlib::star(&cfg_dir(&app), &id, favorite)
}

#[tauri::command]
pub fn promptlib_use(app: tauri::AppHandle, id: String) -> Result<promptlib::PromptEntry, String> {
    promptlib::bump_use(&cfg_dir(&app), &id)
}

#[tauri::command]
pub fn promptlib_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    promptlib::delete(&cfg_dir(&app), &id)
}

#[tauri::command]
pub fn promptlib_clean(text: String) -> promptlib::CleanResult {
    promptlib::clean(&text)
}
