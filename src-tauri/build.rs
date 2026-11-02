fn main() {
    // 反馈邮箱授权码：优先读构建环境变量（CI 用 Secret 注入），否则读本机私有文件
    // （已 gitignore）；都没有就生成空值，运行时会退回复制 + 邮件客户端。
    let code = std::env::var("FEEDBACK_SMTP_CODE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::fs::read_to_string("src/feedback_secret.txt")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default();
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let generated = format!("pub const SMTP_CODE: &str = {code:?};\n");
    std::fs::write(
        std::path::Path::new(&out_dir).join("feedback_secret.rs"),
        generated,
    )
    .expect("write feedback_secret.rs");
    println!("cargo:rerun-if-changed=src/feedback_secret.txt");
    println!("cargo:rerun-if-env-changed=FEEDBACK_SMTP_CODE");

    tauri_build::build()
}