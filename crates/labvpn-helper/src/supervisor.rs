//! 代理核心（mihomo）子进程托管。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

use labvpn_core::ipc::{CoreStatus, MAX_REQUEST_BYTES};

use crate::logging;

/// 核心进程托管器。
pub struct Supervisor {
    core_path: PathBuf,
    workdir: PathBuf,
    log_path: PathBuf,
    inner: Mutex<Inner>,
}

struct Inner {
    child: Option<Child>,
    started_at: Option<Instant>,
    last_error: Option<String>,
    config_generation: Option<u64>,
}

impl Supervisor {
    /// 创建托管器（不启动）。
    pub fn new(core_path: PathBuf, workdir: PathBuf) -> Self {
        let log_path = workdir.join("core.log");
        Self {
            core_path,
            workdir,
            log_path,
            inner: Mutex::new(Inner {
                child: None,
                started_at: None,
                last_error: None,
                config_generation: None,
            }),
        }
    }

    /// 启动核心；若已在运行返回错误。
    pub fn start(
        &self,
        config_yaml: &str,
        assets: &BTreeMap<String, String>,
    ) -> Result<CoreStatus, String> {
        if config_yaml.len() > MAX_REQUEST_BYTES {
            return Err("配置过大".to_string());
        }
        let mut inner = self.inner.lock().map_err(|_| "内部锁损坏".to_string())?;
        if let Some(child) = inner.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return Err("核心已在运行，请先停止".to_string()),
                _ => {
                    inner.child = None;
                    inner.started_at = None;
                }
            }
        }

        fs::create_dir_all(&self.workdir)
            .map_err(|err| format!("创建工作目录失败: {err}"))?;
        let rules_dir = self.workdir.join("rules");
        fs::create_dir_all(&rules_dir).map_err(|err| format!("创建规则目录失败: {err}"))?;
        // 内置规则文件（与 App 版本强一致）
        for (name, content) in labvpn_core::rules::providers::builtin_rule_files() {
            write_file(&rules_dir.join(name), content.as_bytes(), false)
                .map_err(|err| format!("写入内置规则失败: {err}"))?;
        }
        // 附加资源（订阅缓存 / SSH 私钥等）
        for (rel, content) in assets {
            if content.len() > MAX_REQUEST_BYTES {
                return Err(format!("附加文件过大: {rel}"));
            }
            let path = safe_join(&self.workdir, rel)?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|err| format!("创建目录失败: {err}"))?;
            }
            let private = rel.contains("key") || rel.contains("sub") || rel.contains("profile");
            write_file(&path, content.as_bytes(), private)
                .map_err(|err| format!("写入附加文件失败: {err}"))?;
        }
        let config_path = self.workdir.join("config.yaml");
        write_file(&config_path, config_yaml.as_bytes(), true)
            .map_err(|err| format!("写入核心配置失败: {err}"))?;

        let stdout = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .map_err(|err| format!("打开核心日志失败: {err}"))?;
        let stderr = stdout
            .try_clone()
            .map_err(|err| format!("复制日志句柄失败: {err}"))?;

        let mut command = Command::new(&self.core_path);
        command
            .arg("-d")
            .arg(&self.workdir)
            .arg("-f")
            .arg(&config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        let child = command
            .spawn()
            .map_err(|err| format!("启动核心失败（{}）: {err}", self.core_path.display()))?;
        let pid = child.id();
        inner.child = Some(child);
        inner.started_at = Some(Instant::now());
        inner.last_error = None;
        inner.config_generation = Some(now_secs());
        logging::info(format!(
            "核心已启动 pid={pid} config={}",
            config_path.display()
        ));
        Ok(status_locked(&self.log_path, &mut inner))
    }

    /// 停止核心。
    pub fn stop(&self) -> CoreStatus {
        let Ok(mut inner) = self.inner.lock() else {
            return CoreStatus::default();
        };
        if let Some(mut child) = inner.child.take() {
            let pid = child.id();
            let _ = child.kill();
            let _ = child.wait();
            logging::info(format!("核心已停止 pid={pid}"));
        }
        inner.started_at = None;
        status_locked(&self.log_path, &mut inner)
    }

    /// 查询状态（会顺带回收已退出的子进程）。
    pub fn status(&self) -> CoreStatus {
        match self.inner.lock() {
            Ok(mut inner) => status_locked(&self.log_path, &mut inner),
            Err(_) => CoreStatus {
                last_error: Some("内部锁损坏".to_string()),
                ..CoreStatus::default()
            },
        }
    }

    /// 核心二进制路径。
    pub fn core_path(&self) -> &Path {
        &self.core_path
    }
}

fn status_locked(log_path: &Path, inner: &mut Inner) -> CoreStatus {
    let log_path = Some(log_path.display().to_string());
    let Some(child) = inner.child.as_mut() else {
        return CoreStatus {
            running: false,
            last_error: inner.last_error.clone(),
            log_path,
            ..CoreStatus::default()
        };
    };
    match child.try_wait() {
        Ok(None) => CoreStatus {
            running: true,
            pid: Some(child.id()),
            uptime_secs: inner.started_at.map(|start| start.elapsed().as_secs()),
            last_error: inner.last_error.clone(),
            config_generation: inner.config_generation,
            log_path,
        },
        Ok(Some(status)) => {
            inner.child = None;
            inner.started_at = None;
            inner.last_error = Some(format!("核心已退出（{status}）"));
            CoreStatus {
                running: false,
                last_error: inner.last_error.clone(),
                log_path,
                ..CoreStatus::default()
            }
        }
        Err(err) => {
            inner.last_error = Some(format!("查询核心状态失败: {err}"));
            CoreStatus {
                running: false,
                last_error: inner.last_error.clone(),
                log_path,
                ..CoreStatus::default()
            }
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn safe_join(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(format!("附加文件路径必须为相对路径: {rel}"));
    }
    if rel
        .split(['/', '\\'])
        .any(|segment| segment == ".." || segment.is_empty())
    {
        return Err(format!("附加文件路径非法: {rel}"));
    }
    Ok(base.join(rel_path))
}

fn write_file(path: &Path, content: &[u8], private: bool) -> std::io::Result<()> {
    fs::write(path, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if private { 0o600 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = private;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        let base = Path::new("/tmp/labvpn");
        assert!(safe_join(base, "rules/a.yaml").is_ok());
        assert!(safe_join(base, "../etc/passwd").is_err());
        assert!(safe_join(base, "/etc/passwd").is_err());
        assert!(safe_join(base, "a/../../b").is_err());
    }
}
