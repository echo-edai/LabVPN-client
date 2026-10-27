//! 日志：内存环形缓冲 + 按天轮转的文件追加。
//!
//! 保留策略（兼顾排障与磁盘占用）：
//! - **内存**：最近 [`MAX_ENTRIES`] 条（日志页每次拉取 500 条）；
//! - **文件**：`app.log` 只写当天；跨天自动归档为 `app-YYYY-MM-DD.log`；
//!   单文件超过 [`MAX_FILE_BYTES`] 时也会强制归档，避免异常刷屏打爆磁盘；
//! - **清理**：归档文件保留 [`RETENTION_DAYS`] 天，启动/轮转时自动删除更早的文件。
//!
//! 这样出问题后既能回溯最近一周（远好于 500 条/1 天），又不会无限增长。

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::{Local, NaiveDate};

use crate::dto::LogEntry;

/// 内存保留条数。
const MAX_ENTRIES: usize = 2000;
/// 归档文件保留天数。
const RETENTION_DAYS: i64 = 7;
/// 单文件大小上限，超过后强制归档。
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// 归档文件名前缀。
const ARCHIVE_PREFIX: &str = "app-";

struct FileState {
    path: PathBuf,
    file: Option<File>,
    day: NaiveDate,
    bytes: u64,
}

impl FileState {
    fn open(&mut self) {
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .ok();
        self.bytes = fs::metadata(&self.path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
    }

    /// 把当前 `app.log` 归档，并打开新文件。
    fn archive(&mut self, today: NaiveDate) {
        if let Some(mut file) = self.file.take() {
            let _ = file.flush();
        }
        if self.path.exists() {
            let archive = archive_path(&self.path, self.day);
            if fs::rename(&self.path, &archive).is_err() {
                // 归档失败时退化为清空当前文件，避免无限增长
                let _ = fs::remove_file(&self.path);
            }
        }
        self.day = today;
        self.open();
        if let Some(parent) = self.path.parent() {
            prune_archives(parent, today);
        }
    }
}

/// 应用日志存储。
pub struct LogStore {
    entries: Mutex<VecDeque<LogEntry>>,
    file: Mutex<FileState>,
    next_id: AtomicU64,
}

impl LogStore {
    /// 创建（日志文件按天轮转，失败时仅内存日志）。
    pub fn new(path: &Path) -> Self {
        let today = Local::now().date_naive();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // 上次退出到今天之间跨天了：先把旧 app.log 归档
        if let Some(parent) = path.parent() {
            archive_stale(path, parent, today);
            prune_archives(parent, today);
        }
        let mut state = FileState {
            path: path.to_path_buf(),
            file: None,
            day: today,
            bytes: 0,
        };
        state.open();
        Self {
            entries: Mutex::new(VecDeque::with_capacity(MAX_ENTRIES)),
            file: Mutex::new(state),
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
        let line = format!("[{ts}] [{level}] [{source}] {message}\n");
        let Ok(mut state) = self.file.lock() else {
            return;
        };
        let today = Local::now().date_naive();
        if state.day != today || state.bytes >= MAX_FILE_BYTES {
            state.archive(today);
        }
        let written = match state.file.as_mut() {
            Some(file) => {
                let ok = write!(file, "{line}").is_ok();
                if ok {
                    let _ = file.flush();
                }
                ok
            }
            None => false,
        };
        if written {
            state.bytes = state.bytes.saturating_add(line.len() as u64);
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

    /// 清空内存日志（文件保留，仍按天轮转/自动清理）。
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

/// 跨天时归档上次的 `app.log`。
fn archive_stale(path: &Path, _parent: &Path, today: NaiveDate) {
    let Some(modified) = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
    else {
        return;
    };
    let last_day = chrono::DateTime::<Local>::from(modified).date_naive();
    if last_day >= today {
        return;
    }
    let archive = archive_path(path, last_day);
    if fs::rename(path, &archive).is_err() {
        let _ = fs::remove_file(path);
    }
}

/// 生成归档文件名（同一天多个文件时追加序号）。
fn archive_path(path: &Path, day: NaiveDate) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let base = format!("{ARCHIVE_PREFIX}{}.log", day.format("%Y-%m-%d"));
    let mut candidate = parent.join(&base);
    let mut index = 1;
    while candidate.exists() {
        candidate = parent.join(format!(
            "{ARCHIVE_PREFIX}{}-{index}.log",
            day.format("%Y-%m-%d")
        ));
        index += 1;
        if index > 100 {
            break;
        }
    }
    candidate
}

/// 删除超过保留期的归档文件（只处理 `app-YYYY-MM-DD*.log`）。
fn prune_archives(parent: &Path, today: NaiveDate) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(day) = parse_archive_date(name) else {
            continue;
        };
        if (today - day).num_days() > RETENTION_DAYS {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// 从归档文件名解析日期：`app-2026-10-27.log` / `app-2026-10-27-3.log`。
fn parse_archive_date(name: &str) -> Option<NaiveDate> {
    let rest = name.strip_prefix(ARCHIVE_PREFIX)?;
    let date = rest.get(..10)?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("labvpn-log-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn parses_archive_dates() {
        assert_eq!(
            parse_archive_date("app-2026-10-27.log"),
            NaiveDate::from_ymd_opt(2026, 10, 27)
        );
        assert_eq!(
            parse_archive_date("app-2026-10-27-3.log"),
            NaiveDate::from_ymd_opt(2026, 10, 27)
        );
        assert_eq!(parse_archive_date("app.log"), None);
        assert_eq!(parse_archive_date("other-2026-10-27.log"), None);
    }

    #[test]
    fn writes_and_reads_recent_entries() {
        let dir = temp_dir("write");
        let path = dir.join("app.log");
        let store = LogStore::new(&path);
        store.info("vpn", "hello");
        store.warn("vpn", "warn-line");
        store.error("vpn", "error-line");
        let text = fs::read_to_string(&path).expect("read log");
        assert!(text.contains("[vpn] hello"));
        assert!(text.contains("[warn] [vpn] warn-line"));
        assert!(text.contains("[error] [vpn] error-line"));
        let recent = store.recent(10);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[2].level, "error");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prunes_expired_archives_on_startup() {
        let dir = temp_dir("prune");
        let expired = dir.join("app-2020-01-01.log");
        let recent = dir.join("app-2099-01-01.log");
        fs::write(&expired, "old").expect("write old");
        fs::write(&recent, "new").expect("write new");
        let _ = LogStore::new(&dir.join("app.log"));
        assert!(!expired.exists(), "过期归档应被删除");
        assert!(recent.exists(), "未过期归档应保留");
        let _ = fs::remove_dir_all(dir);
    }
}
