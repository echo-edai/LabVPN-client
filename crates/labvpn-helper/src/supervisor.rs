//! 代理核心（mihomo）子进程托管。

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
#[cfg(unix)]
use std::time::Duration;
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
    #[cfg(target_os = "macos")]
    route_guard_ready: bool,
    sensitive_files: Vec<PathBuf>,
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
                #[cfg(target_os = "macos")]
                route_guard_ready: false,
                sensitive_files: Vec::new(),
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

        let result = (|| {
            cleanup_sensitive(&mut inner);
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
                if rel.starts_with("keys/") {
                    inner.sensitive_files.push(path.clone());
                }
                write_file(&path, content.as_bytes(), private)
                    .map_err(|err| format!("写入附加文件失败: {err}"))?;
            }
            let generation = next_generation();
            let config_path = self.workdir.join(format!("config-{generation}.yaml"));
            inner.sensitive_files.push(config_path.clone());
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
            let pid = child.id();
            let _ = fs::write(self.workdir.join("mihomo.pid"), format!("{pid}\n"));
            inner.child = Some(child);
            inner.started_at = Some(Instant::now());
            inner.last_error = None;
            inner.config_generation = Some(generation);
            #[cfg(target_os = "macos")]
            {
                inner.route_guard_ready = false;
            }
            inner.tun_plan = tun_guard::plan_from_config(config_yaml);
            logging::info(format!(
                "核心已启动 pid={pid} config={}",
                config_path.display()
            ));
            Ok(status_locked(&self.log_path, &mut inner))
        })();
        if result.is_err() {
            cleanup_sensitive(&mut inner);
        }
        result
    }

    /// 停止核心。
    pub fn stop(&self) -> Result<CoreStatus, String> {
        let mut inner = self.inner.lock().map_err(|_| "内部锁损坏".to_string())?;
        #[cfg(target_os = "macos")]
        {
            inner.route_guard_ready = false;
        }
        if let Some(child) = inner.child.as_mut() {
            let pid = child.id();
            terminate(child)
                .map_err(|err| format!("核心停止未确认 / Core stop unconfirmed: {err}"))?;
            inner.child = None;
            logging::info(format!("核心已停止 pid={pid}"));
        }
        remove_pid_file(&self.workdir);
        cleanup_sensitive(&mut inner);
        inner.started_at = None;
        inner.config_generation = None;
        inner.tun_plan = None;
        Ok(status_locked(&self.log_path, &mut inner))
    }

    /// 仅清理已确认就绪的当前会话，拒绝延迟到达的旧确认。
    pub fn confirm_ready(&self, pid: u32, generation: u64) -> Result<CoreStatus, String> {
        let mut inner = self.inner.lock().map_err(|_| "内部锁损坏".to_string())?;
        let status = status_locked(&self.log_path, &mut inner);
        if !status.running
            || status.pid != Some(pid)
            || status.config_generation != Some(generation)
        {
            return Err("核心会话已变化，拒绝清理敏感配置".to_string());
        }
        cleanup_sensitive(&mut inner);
        #[cfg(target_os = "macos")]
        {
            inner.route_guard_ready = true;
        }
        logging::info("核心就绪已确认，本会话敏感配置已清理");
        Ok(status)
    }

    /// 只在已确认就绪的会话中巡检；单次写路由与启停互斥。
    #[cfg(target_os = "macos")]
    pub fn with_ready_tun<R>(
        &self,
        expected_generation: Option<u64>,
        f: impl FnOnce(u64, &TunRoutePlan) -> R,
    ) -> Option<R> {
        let mut inner = self.inner.lock().ok()?;
        let generation = inner.config_generation?;
        if !inner.route_guard_ready
            || expected_generation.is_some_and(|expected| expected != generation)
            || inner.started_at?.elapsed() < Duration::from_secs(5)
            || inner.child.as_mut()?.try_wait().ok()?.is_some()
        {
            return None;
        }
        Some(f(generation, inner.tun_plan.as_ref()?))
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

fn terminate(child: &mut Child) -> io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        // 安全：只向仍由本 helper 持有、尚未回收的子进程发信号。
        if unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) } == 0 {
            // 先让 mihomo 释放 TUN 路由；卡住时仍有退出时限。
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                if child.try_wait()?.is_some() {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
    if let Err(err) = child.kill() {
        if child.try_wait()?.is_none() {
            return Err(err);
        }
    }
    child.wait().map(|_| ())
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
            cleanup_sensitive(inner);
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

fn next_generation() -> u64 {
    // 同一 helper 内单调递增，快速重连不会复用路径。
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    GENERATION.fetch_add(1, Ordering::Relaxed) + 1
}

fn cleanup_sensitive(inner: &mut Inner) {
    for path in inner.sensitive_files.drain(..) {
        secure_remove(&path);
    }
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
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(if private { 0o600 } else { 0o644 })
            .custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if private { 0o600 } else { 0o644 };
        file.set_permissions(fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = private;
    }
    file.write_all(content)?;
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
    if let Ok(entries) = fs::read_dir(workdir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("config-") && name.ends_with(".yaml") {
                secure_remove(&entry.path());
            }
        }
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

    #[cfg(unix)]
    #[test]
    fn private_write_rejects_symlink_without_truncating_target() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = std::env::temp_dir().join(format!("labvpn-private-{}", next_generation()));
        fs::create_dir(&dir).expect("创建测试目录");
        let target = dir.join("target");
        fs::write(&target, "original").expect("写入目标");
        let link = dir.join("link");
        symlink(&target, &link).expect("创建链接");
        assert!(write_file(&link, b"secret", true).is_err());
        assert_eq!(fs::read_to_string(&target).expect("读取目标"), "original");
        write_file(&target, b"secret", true).expect("写入私有文件");
        assert_eq!(
            fs::metadata(&target)
                .expect("文件信息")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::remove_dir_all(dir).expect("清理测试目录");
    }

    #[test]
    fn rejects_path_traversal() {
        let base = Path::new("/tmp/labvpn");
        assert!(safe_join(base, "rules/a.yaml").is_ok());
        assert!(safe_join(base, "../etc/passwd").is_err());
        assert!(safe_join(base, "/etc/passwd").is_err());
        assert!(safe_join(base, "a/../../b").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn stale_ready_confirmation_cannot_delete_new_session() {
        let dir = std::env::temp_dir().join(format!(
            "labvpn-ready-{}-{}",
            std::process::id(),
            next_generation()
        ));
        fs::create_dir_all(&dir).unwrap();
        let supervisor = Supervisor::new(PathBuf::from("/bin/sleep"), dir.clone());
        let old_file = dir.join("config-old.yaml");
        let new_file = dir.join("config-new.yaml");
        fs::write(&old_file, "old").unwrap();
        fs::write(&new_file, "new").unwrap();
        let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        {
            let mut inner = supervisor.inner.lock().unwrap();
            inner.child = Some(child);
            inner.config_generation = Some(2);
            inner.sensitive_files = vec![new_file.clone()];
        }
        let stale = supervisor.confirm_ready(pid, 1);
        let retained = new_file.exists();
        let confirmed = supervisor.confirm_ready(pid, 2);
        let removed = !new_file.exists();
        let old_retained = old_file.exists();
        supervisor.stop().unwrap();
        fs::remove_dir_all(dir).unwrap();
        assert!(stale.is_err());
        assert!(retained, "旧确认不得删除新配置");
        assert!(confirmed.is_ok());
        assert!(removed, "就绪确认必须清理当前敏感配置");
        assert!(old_retained, "清理必须限定在本会话文件列表");
    }

    #[test]
    fn rapid_sessions_have_distinct_generations() {
        let first = next_generation();
        let second = next_generation();
        assert!(second > first);
    }

    #[cfg(unix)]
    #[test]
    fn terminate_lets_child_handle_sigterm() {
        use std::io::{BufRead, BufReader};

        let mut child = Command::new("/bin/sh")
            .args([
                "-c",
                "trap 'exit 42' TERM; printf 'ready\\n'; while :; do /bin/sleep 0.05; done",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "ready");
        terminate(&mut child).unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(42));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn tun_repair_requires_ready_current_generation() {
        let dir = std::env::temp_dir().join(format!(
            "labvpn-tun-{}-{}",
            std::process::id(),
            next_generation()
        ));
        let supervisor = Supervisor::new(PathBuf::from("/bin/sleep"), dir);
        let start = |generation| {
            let mut inner = supervisor.inner.lock().unwrap();
            let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
            let pid = child.id();
            inner.child = Some(child);
            inner.started_at = Some(Instant::now() - Duration::from_secs(6));
            inner.config_generation = Some(generation);
            inner.tun_plan =
                tun_guard::plan_from_config("tun:\n  enable: true\n  auto-route: true\n");
            pid
        };
        let previous_pid = start(1);
        let before_ready = supervisor.with_ready_tun(None, |_, _| ());
        supervisor.confirm_ready(previous_pid, 1).unwrap();
        let after_ready = supervisor.with_ready_tun(None, |generation, _| generation);
        supervisor.stop().unwrap();
        let current_pid = start(2);
        supervisor.confirm_ready(current_pid, 2).unwrap();
        let stale_write = supervisor.with_ready_tun(Some(1), |_, _| ());
        let current_write = supervisor.with_ready_tun(Some(2), |_, _| ());
        supervisor.stop().unwrap();

        assert_eq!(before_ready, None);
        assert_eq!(after_ready, Some(1));
        assert_eq!(stale_write, None);
        assert_eq!(current_write, Some(()));
    }

    #[test]
    fn failed_start_removes_session_secrets() {
        let dir = std::env::temp_dir().join(format!(
            "labvpn-failed-{}-{}",
            std::process::id(),
            next_generation()
        ));
        let supervisor = Supervisor::new(dir.join("missing-core"), dir.clone());
        let assets = BTreeMap::from([("keys/test.pem".into(), "private-test-key".into())]);
        let result = supervisor.start("test-config", &assets);
        let retained_key = dir.join("keys/test.pem").exists();
        let retained_config = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("config-"));
        fs::remove_dir_all(dir).unwrap();
        assert!(result.is_err());
        assert!(!retained_key && !retained_config);
    }
}
