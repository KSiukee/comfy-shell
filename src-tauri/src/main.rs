#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod custom;
mod dataio;
mod enhance;
mod fsutil;
mod history;
mod hints;
mod imgmeta;
mod layouts;
mod library;
mod models;
mod commands;
mod pose;
mod project;
mod promptlib;
mod recipes;
mod server;
mod shots;
mod single_instance;
mod state;
mod vault;
mod versions;
mod vault_ops;
mod workspace;

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;
use state::{cfg_dir, load_settings, write_settings, true_b, AppState, Settings};
use commands::system::ensure_server;
use tauri::{Emitter, Manager, State};

// ---------------------------------------------------------------------------
// Status helpers
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Projects（一个文件夹一条记录，删除进回收站）
// ---------------------------------------------------------------------------

fn output_dir(state: &AppState) -> PathBuf {
    PathBuf::from(state.settings.lock().unwrap().comfy_root.clone())
        .join("ComfyUI")
        .join("output")
}

fn main() {
    // release 不认调试端口参数：否则任何人都能连上 CDP 把私密项目读出来。
    #[cfg(not(debug_assertions))]
    {
        std::env::remove_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS");
        std::env::remove_var("WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER");
    }

    // 同一份数据只许一个进程写：第二个实例拉一下前台信号就退出。
    let _instance = match single_instance::acquire_single() {
        Ok(Some(guard)) => guard,
        Ok(None) => {
            single_instance::signal_focus();
            return;
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let focus_event = single_instance::create_focus_event();

    tauri::Builder::default()
        .setup(move |app| {
            let handle = app.handle().clone();
            single_instance::spawn_focus_listener(focus_event, handle.clone());
            let cfg = cfg_dir(&handle);
            #[cfg(debug_assertions)]
            {
                let _ = project::ensure_test(&cfg);
            }
            fsutil::cleanup_parts(&cfg);
            fsutil::cleanup_parts(&cfg.join("workflows"));
            fsutil::cleanup_parts(&cfg.join("layouts"));
            let settings = load_settings(&handle);
            app.manage(AppState {
                child: Mutex::new(None),
                settings: Mutex::new(settings.clone()),
                cancel: Mutex::new(false),
            });
            // 私密封存没走完的残留只能是明文：启动直接清掉（宁可丢，不泄漏）。
            let staging = std::path::PathBuf::from(&settings.comfy_root)
                .join("ComfyUI")
                .join("output")
                .join("_private_staging");
            let _ = std::fs::remove_dir_all(staging);

            // Silent auto-start in the background: the window shows up immediately.
            if settings.auto_start {
                std::thread::spawn(move || {
                    let _ = ensure_server(handle);
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let app = window.app_handle().clone();
                let state = app.state::<AppState>();
                let stop = state.settings.lock().unwrap().stop_on_exit;
                if stop {
                    let mut guard = state.child.lock().unwrap();
                    if let Some(mut child) = guard.take() {
                        server::kill(&mut child);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::settings_get,
            commands::system::settings_set,
            commands::system::server_status,
            commands::system::server_start,
            commands::system::server_stop,
            commands::system::models_list,
            commands::system::profiles,
            commands::system::save_ref_image,
            commands::system::ref_preview,
            commands::system::read_input_image,
            pose::pose_pack_list,
            pose::poses_catalog,
            commands::system::read_output_image,
            commands::generation::generate,
            commands::generation::enhance_prompt,
            commands::gallery::history_list,
            commands::gallery::history_clear,
            commands::system::custom_list,
            commands::system::custom_save,
            commands::system::custom_delete,
            commands::system::custom_params,
            commands::system::recipe_list,
            commands::system::recipe_save,
            commands::system::recipe_delete,
            commands::system::models_scan,
            commands::system::model_toggle,
            commands::system::model_preview,
            commands::system::phrase_mine,
            commands::system::layout_tpl_list,
            commands::system::layout_tpl_load,
            commands::system::layout_tpl_save,
            commands::system::layout_tpl_delete,
            commands::system::layout_check,
            commands::projects::project_list,
            commands::projects::project_create,
            commands::projects::project_rename,
            commands::projects::project_set_active,
            commands::projects::project_set_hidden,
            commands::projects::project_delete,
            commands::projects::project_trash,
            commands::projects::project_restore,
            commands::projects::project_purge,
            commands::projects::vault_status,
            commands::projects::vault_setup,
            commands::projects::vault_unlock,
            commands::projects::vault_lock,
            commands::projects::project_make_private,
            commands::projects::project_private_list,
            commands::projects::project_unprivate,
            commands::projects::project_private_delete,
            commands::projects::pick_folder,
            commands::projects::backup_now,
            commands::projects::project_export,
            commands::projects::project_import,
            commands::projects::workspace_get,
            commands::projects::workspace_save,
            commands::projects::shots_list,
            commands::projects::shots_add,
            commands::projects::shot_add_image,
            commands::projects::shot_set_layout,
            commands::projects::shot_delete,
            commands::projects::shots_export,
            commands::projects::layout_version_save,
            commands::projects::versions_list,
            commands::projects::version_star,
            commands::projects::version_delete,
            commands::projects::promptlib_list,
            commands::projects::promptlib_save,
            commands::projects::promptlib_star,
            commands::projects::promptlib_use,
            commands::projects::promptlib_delete,
            commands::projects::promptlib_clean,
            commands::gallery::library_scan,
            commands::gallery::image_meta,
            commands::gallery::library_delete,
            commands::gallery::delete_results,
            commands::gallery::library_star,
            commands::gallery::stars_list,
            commands::gallery::save_export,
            commands::gallery::translate_text,
            commands::upscale::upscale_image,
            commands::upscale::upscale_models_list,
            commands::upscale::inpaint_image,
            commands::upscale::upscale_input,
            commands::training_files::train_lora,
            commands::ai::assist,
            commands::ai::layout_draft,
            commands::ai::analyze_image,
            commands::ai::layout_detect,
            commands::training_files::train_lora_files,
            commands::training_kohya::train_kohya_files,
            commands::ai::story_split,
            commands::system::job_cancel,
            commands::system::open_folder,
            commands::system::reveal_file,
            commands::promptfx::style_list,
            commands::promptfx::style_import,
            commands::promptfx::style_delete,
            commands::promptfx::wildcard_map,
            commands::promptfx::style_pack_open,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}















