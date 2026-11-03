//! Process-wide state and on-disk settings.
//!
//! Everything a command layer needs to find its working set lives here: the
//! config directory, the settings file, and the shared AppState guards.
//! New domain modules should take `&AppState` / `cfg_dir(&app)` instead of
//! reaching into `main.rs`.

use serde::{Deserialize, Serialize};
use tauri::Manager;
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) fn true_b() -> bool {
    true
}

pub(crate) fn default_schema_version() -> u32 {
    1
}

pub(crate) fn default_kohya_python() -> String {
    String::new()
}

pub(crate) fn default_kohya_dir() -> String {
    String::new()
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Settings {
    /// bumped by hand when a field needs migrating; readers use serde defaults
    #[serde(default = "default_schema_version")]
    pub(crate) schema_version: u32,
    pub(crate) comfy_root: String,
    pub(crate) port: u16,
    pub(crate) auto_start: bool,
    pub(crate) stop_on_exit: bool,
    /// skip the official PE and rewrite prompts with the local uncensored model
    #[serde(default)]
    pub(crate) free_enhance: bool,
    /// simple UI (plain-language controls) vs pro UI
    #[serde(default = "true_b")]
    pub(crate) simple_ui: bool,
    #[serde(default = "default_kohya_python")]
    pub(crate) kohya_python: String,
    #[serde(default = "default_kohya_dir")]
    pub(crate) kohya_dir: String,
    /// 出图目录；留空 = `<comfy_root>\ComfyUI\output`。
    /// 由壳托管 ComfyUI 时通过 `--output-directory` 传下去；外部进程不生效。
    #[serde(default)]
    pub(crate) output_dir: String,
    /// AI 助手：provider / model（key 单独 DPAPI 存）
    #[serde(default)]
    pub(crate) ai_provider: String,
    #[serde(default)]
    pub(crate) ai_model: String,
    /// 自定义 Pi 引擎目录（空 = 自动找 PATH / 应用数据）
    #[serde(default)]
    pub(crate) pi_path: String,
    /// 超分后替换原图（#86，默认关）：原图进回收站，新图继承收藏与提示词参数
    #[serde(default)]
    pub(crate) replace_original: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            comfy_root: String::new(),
            port: 8188,
            auto_start: true,
            stop_on_exit: false,
            free_enhance: false,
            simple_ui: true,
            kohya_python: default_kohya_python(),
            kohya_dir: default_kohya_dir(),
            output_dir: String::new(),
            ai_provider: String::new(),
            ai_model: String::new(),
            pi_path: String::new(),
            replace_original: false,
        }
    }
}

/// 出图根目录：自定义优先，否则 `<comfy_root>\ComfyUI\output`。
/// 所有读写图库的地方都必须走这里，别各自拼路径（v0.96 起支持自定义）。
pub(crate) fn output_root(s: &Settings) -> PathBuf {
    let custom = s.output_dir.trim();
    if custom.is_empty() {
        PathBuf::from(&s.comfy_root)
            .join("ComfyUI")
            .join("output")
    } else {
        PathBuf::from(custom)
    }
}

pub(crate) struct AppState {
    pub(crate) child: Mutex<Option<std::process::Child>>,
    pub(crate) settings: Mutex<Settings>,
    pub(crate) cancel: Mutex<bool>,
}

/// `<config>` — created on first use. All persistence goes under here.
pub(crate) fn cfg_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub(crate) fn load_settings(app: &tauri::AppHandle) -> Settings {
    let p = cfg_dir(app).join("settings.json");
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub(crate) fn write_settings(app: &tauri::AppHandle, s: &Settings) -> Result<(), String> {
    let p = cfg_dir(app).join("settings.json");
    let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    crate::fsutil::atomic_write_str(&p, &text)
}
