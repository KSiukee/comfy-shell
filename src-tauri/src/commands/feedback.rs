//! 反馈与支持：SMTP 直发作者邮箱（授权码本机加密）、打开外部链接。

use crate::*;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use std::path::Path;
use std::process::Command;

const TO: &str = "ksiukee@qq.com";
const SMTP_FILE: &str = "feedback_smtp.dat";

fn smtp_code(cfg: &Path) -> Option<String> {
    agent::load_secret(&cfg.join(SMTP_FILE))
}

#[tauri::command]
pub fn feedback_smtp_status(app: tauri::AppHandle) -> bool {
    smtp_code(&cfg_dir(&app)).is_some()
}

#[tauri::command]
pub fn feedback_smtp_set(app: tauri::AppHandle, code: String) -> Result<(), String> {
    agent::save_secret(&cfg_dir(&app).join(SMTP_FILE), &code)
}

#[tauri::command]
pub fn feedback_smtp_clear(app: tauri::AppHandle) -> Result<(), String> {
    let p = cfg_dir(&app).join(SMTP_FILE);
    if p.exists() {
        std::fs::remove_file(p).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn feedback_send(
    app: tauri::AppHandle,
    kind: String,
    text: String,
    contact: Option<String>,
) -> Result<(), String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let text = text.trim();
        if text.is_empty() {
            return Err("先写点内容".into());
        }
        if text.chars().count() > 4000 {
            return Err("太长了（上限 4000 字）".into());
        }
        let cfg = cfg_dir(&handle);
        let code = smtp_code(&cfg).ok_or(
            "作者还没配置发信授权码（关于页 → 发信设置，填 QQ 邮箱授权码）".to_string(),
        )?;
        let kind_label = match kind.as_str() {
            "bug" => "Bug",
            "idea" => "建议",
            "opinion" => "意见",
            _ => "反馈",
        };
        let version = env!("CARGO_PKG_VERSION");
        let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        let body = format!(
            "类型：{kind_label}\n版本：{version}\n系统：{os}\n联系方式：{}\n\n{text}",
            contact.unwrap_or_default()
        );
        let email = Message::builder()
            .from(
                format!("显影反馈 <{TO}>")
                    .parse()
                    .map_err(|e| format!("发件地址无效：{e}"))?,
            )
            .to(TO.parse().map_err(|e| format!("收件地址无效：{e}"))?)
            .subject(format!("[显影反馈][{kind_label}] v{version}"))
            .body(body)
            .map_err(|e| format!("组邮件失败：{e}"))?;

        let mailer = SmtpTransport::relay("smtp.qq.com")
            .map_err(|e| format!("连 SMTP 失败：{e}"))?
            .credentials(Credentials::new(TO.to_string(), code))
            .build();
        mailer
            .send(&email)
            .map_err(|e| format!("发送失败：{e}"))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("发送失败：{e}"))?
}

/// 用系统默认程序打开链接（只允许 https / mailto）。
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("mailto:")) {
        return Err("只支持 https / mailto 链接".into());
    }
    #[cfg(windows)]
    {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|e| format!("打开链接失败：{e}"))?;
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("打开链接失败：{e}"))?;
    }
    Ok(())
}
