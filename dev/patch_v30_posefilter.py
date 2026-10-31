proj = r"D:\deepseek-files\ai-tools\comfy-shell"

p = proj + r"\src-tauri\src\main.rs"
s = open(p, encoding="utf-8").read()
old_start = s.index("#[tauri::command]\nfn poses_catalog")
old_end = s.index("#[tauri::command]\nfn read_input_image", old_start)
new_fn = '''#[tauri::command]
fn poses_catalog(state: State<AppState>) -> Result<String, String> {
    let root = state.settings.lock().unwrap().comfy_root.clone();
    let dir = std::path::PathBuf::from(root).join("ComfyUI").join("input").join("shisei");
    let text = std::fs::read_to_string(dir.join("catalog.json"))
        .map_err(|e| format!("找不到姿势库（{}）：{e}", dir.display()))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("姿势库 catalog 解析失败: {e}"))?;
    let available: std::collections::HashSet<String> = std::fs::read_dir(dir.join("line"))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    if let Some(list) = value.as_array_mut() {
        for pose in list.iter_mut() {
            if let Some(views) = pose.get_mut("views").and_then(|v| v.as_object_mut()) {
                views.retain(|_, v| v.as_str().map(|n| available.contains(n)).unwrap_or(false));
            }
        }
        // drop poses with no usable image at all
        list.retain(|p| {
            p.get("views")
                .and_then(|v| v.as_object())
                .map(|o| !o.is_empty())
                .unwrap_or(false)
        });
    }
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

'''
s = s[:old_start] + new_fn + s[old_end:]
open(p, "w", encoding="utf-8", newline="\n").write(s)
print("rust ok")

p = proj + r"\web\app.js"
j = open(p, encoding="utf-8").read().replace("\r\n", "\n")
j = j.replace(
    'const file = (p.views || {})[angle] || (p.views || {}).front;',
    'const file = (p.views || {})[angle] || (p.views || {}).front || Object.values(p.views || {})[0];',
    1,
)
j = j.replace(
    '$("poseCount").textContent = `${endCount} / ${list.length}`;',
    '$("poseCount").textContent = `${endCount} / ${list.length}`;',
    1,
)
open(p, "w", encoding="utf-8", newline="\n").write(j)
print("js ok")
