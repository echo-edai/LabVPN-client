//! 日志：内存环形缓冲 + 文件追加。

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::Local;

use crate::dto::LogEntry;

const MAX_ENTRIES: usize = 2000;

/// 应用日志存储。
pub struct LogStore {
    entries: Mutex<VecDeque<LogEntry>>,
    file: Mutex<Option<std::fs::File>>,
    next_id: AtomicU64,
}

impl LogStore {
    /// 创建（日志文件追加写入，失败时仅内存日志）。
    pub fn new(path: &Path) -> Self {
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_ENTRIES)),
            file: Mutex::new(file),
            next_id: AtomicU64::new(1),
        }
    }

    /// 追加一条日志。
    pub fn push(&self, level: &str, source: &str, message: impl Into<String>) {
        let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let message = message.into();
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut entries) = self.entries.lock() {
            entries.push_back(LogEntry {
                id,
                ts: ts.clone(),
                level: level.to_string(),
                source: source.to_string(),
                message: message.clone(),
            });
            while entries.len() > MAX_ENTRIES {
                entries.pop_front();
            }
        }
        if let Ok(mut file) = self.file.lock() {
            if let Some(file) = file.as_mut() {
                let _ = writeln!(file, "[{ts}] [{level}] [{source}] {message}");
                let _ = file.flush();
            }
        }
    }

    /// INFO 日志。
    pub fn info(&self, source: &str, message: impl Into<String>) {
        self.push("info", source, message);
    }

    /// WARN 日志。
    pub fn warn(&self, source: &str, message: impl Into<String>) {
        self.push("warn", source, message);
    }

    /// ERROR 日志。
    pub fn error(&self, source: &str, message: impl Into<String>) {
        self.push("error", source, message);
    }

    /// 最近日志（按时间正序）。
    pub fn recent(&self, limit: usize) -> Vec<LogEntry> {
        let Ok(entries) = self.entries.lock() else {
            return Vec::new();
        };
        let limit = limit.min(entries.len());
        entries
            .iter()
            .skip(entries.len() - limit)
            .cloned()
            .collect()
    }

    /// 清空内存日志（文件保留）。
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}
