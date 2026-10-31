//! Silent lifecycle management of the bundled portable ComfyUI.

use std::fs::File;
use std::process::{Child, Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn spawn(root: &str, port: u16) -> Result<Child, String> {
    let root = root.trim_end_matches('\\');
    let py = format!("{root}\\python_embeded\\python.exe");
    if !std::path::Path::new(&py).exists() {
        return Err(format!("找不到内置 python：{py}"));
    }
    let main = format!("{root}\\ComfyUI\\main.py");
    if !std::path::Path::new(&main).exists() {
        return Err(format!("找不到 ComfyUI/main.py：{main}"));
    }

    let log_path = format!("{root}\\comfyshell-server.log");
    let out = File::create(&log_path).map_err(|e| format!("创建日志失败：{e}"))?;
    let err = out.try_clone().map_err(|e| format!("复制日志句柄失败：{e}"))?;

    let mut cmd = Command::new(&py);
    cmd.arg("-s")
        .arg("ComfyUI\\main.py")
        .arg("--windows-standalone-build")
        .arg("--port")
        .arg(port.to_string())
        .arg("--disable-auto-launch")
        .arg("--disable-pinned-memory")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err));

    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    cmd.spawn().map_err(|e| format!("启动 ComfyUI 失败：{e}"))
}

/// Kill the child and everything it spawned.
pub fn kill(child: &mut Child) {
    let pid = child.id();
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn log_path(root: &str) -> String {
    format!("{}\\comfyshell-server.log", root.trim_end_matches('\\'))
}
