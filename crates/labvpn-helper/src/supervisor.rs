//! 代理核心（mihomo）子进程托管。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;

use labvpn_core::ipc::{CoreStatus, MAX_REQUEST_BYTES};

use crate::logging;
use crate::tun_guard::{self, TunRoutePlan};

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
    /// 本次核心会话应有的 TUN 自动路由（供路由自愈线程使用）
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    tun_plan: Option<TunRoutePlan>,
}

impl Supervisor {
    /// 创建托管器（不启动），并清理上次异常退出遗留的核心进程。
    pub fn new(core_path: PathBuf, workdir: PathBuf) -> Self {
        cleanup_stale(&workdir);
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
                tun_plan: None,
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
                    remove_pid_file(&self.workdir);
                }
            }
        }

        fs::create_dir_all(&self.workdir).map_err(|err| format!("创建工作目录失败: {err}"))?;
        let rules_dir = self.workdir.join("rules");
        fs::create_dir_all(&rules_dir).map_err(|err| format!("创建规则目录失败: {err}"))?;
        // 内置规则文件（与 App 版本强一致）
        for (name, content) in labvpn_core::rules::providers::builtin_rule_files() {
            write_file(&rules_dir.join(name), content.as_bytes(), false)
                .map_err(|err| format!("写入内置规则失败: {err}"))?;
        }
        // 内置 geodata（避免 mihomo 首次启动联网下载）
        if let Some(bin_dir) = self.core_path.parent() {
            for name in ["GeoSite.dat", "geoip.metadb"] {
                let source = bin_dir.join(name);
                let target = self.workdir.join(name);
                if source.is_file() && !target.exists() {
                    let _ = fs::copy(&source, &target);
                }
            }
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

        // 核心日志轮转：长期运行会持续追加，超过 20MB 时归档一份，避免无限增长
        const MAX_CORE_LOG_BYTES: u64 = 20 * 1024 * 1024;
        if let Ok(metadata) = fs::metadata(&self.log_path) {
            if metadata.len() > MAX_CORE_LOG_BYTES {
                let _ = fs::rename(&self.log_path, self.workdir.join("core.log.1"));
            }
        }
        let stdout = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .map_err(|err| format!("打开核心日志失败: {err}"))?;
        let stderr = stdout
            .try_clone()
            .map_err(|err| format!("复制日志句柄失败: {err}"))?;

        let mut command = Command::new(&self.core_path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
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
        // 安全清理：核心启动后销毁含明文密码的配置与私钥资产
        let cleanup_config = config_path.clone();
        let cleanup_keys: Vec<PathBuf> = assets
            .keys()
            .filter(|relative| relative.starts_with("keys/"))
            .map(|relative| self.workdir.join(relative))
            .collect();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            if cleanup_config.exists() {
                secure_remove(&cleanup_config);
            }
            for key in &cleanup_keys {
                secure_remove(key);
            }
            logging::info("敏感配置文件已清理（config.yaml / keys），仅保留在核心内存中");
        });

        let pid = child.id();
        let _ = fs::write(self.workdir.join("mihomo.pid"), format!("{pid}\n"));
        inner.child = Some(child);
        inner.started_at = Some(Instant::now());
        inner.last_error = None;
        inner.config_generation = Some(now_secs());
        // 从生成配置解析本会话应有的 TUN 自动路由，交给自愈线程巡检
        inner.tun_plan = tun_guard::plan_from_config(config_yaml);
        if inner.tun_plan.is_some() {
            logging::info("TUN 自动路由守护已启用（缺失时自动补回）");
        }
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
        remove_pid_file(&self.workdir);
        inner.started_at = None;
        inner.tun_plan = None;
        status_locked(&self.log_path, &mut inner)
    }

    /// 当前会话的 TUN 自动路由计划（自愈线程使用）。
    #[cfg(target_os = "macos")]
    pub fn tun_plan(&self) -> Option<TunRoutePlan> {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.tun_plan.clone())
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
            inner.tun_plan = None;
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

/// 覆盖并删除敏感文件。
fn secure_remove(path: &Path) {
    use std::io::Write;
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.is_file() && metadata.len() > 0 {
            let len = metadata.len() as usize;
            if let Ok(mut file) = fs::OpenOptions::new().write(true).open(path) {
                let _ = file.write_all(&vec![0u8; len.min(4096)]);
                let _ = file.sync_all();
            }
        }
    }
    let _ = fs::remove_file(path);
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

/// 清理上次异常退出遗留的核心进程与含密码配置。
fn cleanup_stale(workdir: &Path) {
    let pid_file = workdir.join("mihomo.pid");
    if let Ok(text) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = text.trim().parse::<u32>() {
            if pid_alive(pid) && process_is_core(pid) {
                kill_pid(pid);
                logging::warn(format!("已清理遗留核心进程 pid={pid}"));
            }
        }
    }
    remove_pid_file(workdir);
    let config = workdir.join("config.yaml");
    if config.exists() {
        secure_remove(&config);
    }
}

fn remove_pid_file(workdir: &Path) {
    let _ = fs::remove_file(workdir.join("mihomo.pid"));
}

#[cfg(unix)]
fn process_is_core(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains("mihomo")
        })
        .unwrap_or(false)
}

#[cfg(windows)]
fn process_is_core(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains("mihomo")
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    process_is_core(pid)
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status();
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
