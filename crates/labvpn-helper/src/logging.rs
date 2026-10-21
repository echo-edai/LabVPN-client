//! 简单文件日志（helper 以 root 运行，不能依赖用户态日志框架）。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// 初始化日志文件（追加写入）。
pub fn init(path: PathBuf) {
    let _ = LOG_PATH.set(path);
}

/// INFO 日志。
pub fn info(message: impl AsRef<str>) {
    write("INFO", message.as_ref());
}

/// WARN 日志。
pub fn warn(message: impl AsRef<str>) {
    write("WARN", message.as_ref());
}

/// ERROR 日志。
pub fn error(message: impl AsRef<str>) {
    write("ERROR", message.as_ref());
}

fn write(level: &str, message: &str) {
    let line = format!(
        "[{}] [{level}] {message}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    eprint!("{line}");
    if let Some(path) = LOG_PATH.get() {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(line.as_bytes());
        }
    }
}
