fn main() {
    // 反馈邮箱授权码：本地私有文件（已 gitignore），不随源码发布；缺失时生成空值。
    let code = std::fs::read_to_string("src/feedback_secret.txt")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let generated = format!("pub const SMTP_CODE: &str = {code:?};\n");
    std::fs::write(
        std::path::Path::new(&out_dir).join("feedback_secret.rs"),
        generated,
    )
    .expect("write feedback_secret.rs");
    println!("cargo:rerun-if-changed=src/feedback_secret.txt");

    tauri_build::build()
}