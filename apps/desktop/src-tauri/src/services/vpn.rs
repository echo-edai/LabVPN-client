//! 校内 VPN（fdu-connect）进程管理。
//!
//! 设计：
//! - 管理模式下由 LabVPN 启动/停止 fdu-connect（SOCKS5 出口）；
//! - 复用模式下只探测用户已有实例（如 launchd/systemd 托管的 1080 端口）；
//! - 密码只写入权限 0600 的临时 config.toml，不进入命令行参数（避免 ps 泄漏）；
//! - 通过日志识别登录成功/失败，失败时回传最近日志便于排障。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use labvpn_core::config::CampusVpnConfig;
use tauri::AppHandle;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::dto::{ExistingVpn, VpnStatus, VpnTestResult};
use crate::error::{CmdResult, CommandError};
use crate::events;
use crate::paths::AppPaths;
use crate::services::logs::LogStore;
use crate::services::secrets::SecretStore;

/// 创建 fdu-connect 子进程命令；Windows 下隐藏控制台窗口（避免黑框/抢焦点）。
fn fdu_command(binary: &Path) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(binary);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// 打补丁版 fdu-connect 的 SOCKS5 建连宽限（见 vendor/fdu-connect/PATCH.md）。
/// 隧道握手在用户态栈里完成，偶发丢包/短暂卡顿会明显超过 mihomo 的 5s 拨号超时；
/// 补丁版先应答 SOCKS5 客户端（2s），再在该预算内用「新连接重试」继续握手。
const SOCKS_CONNECT_TIMEOUT_SECS: u16 = 20;
const SOCKS_EARLY_REPLY_MS: u16 = 2000;

/// 就绪判定关键字
const READY_MARKERS: &[&str] = &[
    "SOCKS5 server listening on",
    "EasyConnect client started",
    "KeepAlive: OK",
];
/// 失败判定关键字
const ERROR_MARKERS: &[&str] = &[
    "登录失败",
    "Login failed",
    "password",
    "认证失败",
    "用户名或密码错误",
    "SMS code verification failed",
    "Initial FDU-Connect failed",
    "cannot login",
    "Permission denied",
];

/// fdu-connect 的 stdout 里混有错误行（如 `[SOCKS5] ... [E]: server: context deadline exceeded`），
/// 统一按 info 记录会让「运行日志 → 校内 VPN」难以定位；这里把错误关键字提升为 warn。
fn vpn_log_level(line: &str) -> &'static str {
    const WARN_MARKERS: &[&str] = &[
        "[e]:",
        " error",
        "error:",
        "failed",
        "timeout",
        "deadline exceeded",
        "reset by peer",
        "broken pipe",
        "connection refused",
        "no route to host",
    ];
    let lower = line.to_ascii_lowercase();
    if WARN_MARKERS.iter().any(|marker| lower.contains(marker)) {
        "warn"
    } else {
        "info"
    }
}

struct Inner {
    state: String,
    child: Option<Child>,
    pid: Option<u32>,
    log_task: Option<tokio::task::JoinHandle<()>>,
    log_lines: Arc<Mutex<Vec<String>>>,
    binary_path: Option<PathBuf>,
    existing: bool,
    last_error: Option<String>,
    message: Option<String>,
    config: Option<CampusVpnConfig>,
}

/// 校内 VPN 管理器。
pub struct VpnManager {
    app: AppHandle,
    logs: Arc<LogStore>,
    inner: Mutex<Inner>,
}

impl VpnManager {
    /// 创建管理器。
    pub fn new(app: AppHandle, logs: Arc<LogStore>) -> Self {
        Self {
            app,
            logs,
            inner: Mutex::new(Inner {
                state: "stopped".to_string(),
                child: None,
                pid: None,
                log_task: None,
                log_lines: Arc::new(Mutex::new(Vec::new())),
                binary_path: None,
                existing: false,
                last_error: None,
                message: None,
                config: None,
            }),
        }
    }

    /// 当前状态。
    pub async fn status(&self, config: &CampusVpnConfig) -> VpnStatus {
        let inner = self.inner.lock().await;
        VpnStatus {
            enabled: config.enabled,
            state: if !config.enabled {
                "disabled".to_string()
            } else {
                inner.state.clone()
            },
            managed: config.managed,
            socks_port: config.socks_port,
            binary_path: inner
                .binary_path
                .as_ref()
                .map(|path| path.display().to_string()),
            pid: inner.pid,
            message: inner.message.clone(),
            last_error: inner.last_error.clone(),
            existing_instance: inner.existing,
        }
    }

    /// 启动（或复用）校内 VPN。
    pub async fn start(
        &self,
        config: &CampusVpnConfig,
        secrets: &SecretStore,
        paths: &AppPaths,
    ) -> CmdResult<VpnStatus> {
        if !config.enabled {
            return Ok(self.status(config).await);
        }
        let mut inner = self.inner.lock().await;
        inner.config = Some(config.clone());

        // 已经在运行
        if (inner.child.is_some() || inner.existing) && self.port_open(config.socks_port).await {
            inner.state = if inner.existing { "reusing" } else { "running" }.to_string();
            return Ok(self.snapshot(config, &inner));
        }

        // 复用模式：只探测端口
        if !config.managed {
            inner.binary_path = resolve_binary(&self.app, config, paths);
            if self.port_open(config.socks_port).await {
                inner.state = "reusing".to_string();
                inner.existing = true;
                inner.message = Some(format!(
                    "已复用运行中的 fdu-connect（127.0.0.1:{}）",
                    config.socks_port
                ));
                self.emit(config, &inner);
                return Ok(self.snapshot(config, &inner));
            }
            let message = format!(
                "复用模式未检测到监听 127.0.0.1:{} 的 fdu-connect，请先启动已有服务或切换为托管模式",
                config.socks_port
            );
            inner.state = "error".to_string();
            inner.last_error = Some(message.clone());
            self.emit(config, &inner);
            return Err(CommandError::new("vpn", message));
        }

        // 托管模式
        let binary = resolve_binary(&self.app, config, paths).ok_or_else(|| {
            CommandError::new(
                "vpn",
                "找不到 fdu-connect 可执行文件：请在配置页指定路径，或参考 docs/BUILD.md 编译安装",
            )
        })?;
        inner.binary_path = Some(binary.clone());

        // 端口被占用 → 视为已有实例，直接复用（避免重复登录）
        if self.port_open(config.socks_port).await {
            inner.state = "reusing".to_string();
            inner.existing = true;
            inner.message = Some(format!(
                "端口 {} 已有 fdu-connect 实例，直接复用",
                config.socks_port
            ));
            self.logs
                .info("vpn", "检测到已有 fdu-connect 实例，复用现有 SOCKS 出口");
            self.emit(config, &inner);
            return Ok(self.snapshot(config, &inner));
        }

        let password = secrets
            .get(config.password_ref.as_deref().unwrap_or(""))
            .or_else(|| secrets.get(labvpn_core::config::secret_keys::CAMPUS_PASSWORD))
            .ok_or_else(|| CommandError::new("vpn", "未找到复旦上网密码，请在配置页重新填写"))?;
        let totp = config
            .totp_secret_ref
            .as_deref()
            .and_then(|key| secrets.get(key));

        let server_address = resolve_server_address(config).await;
        let config_text = build_config_toml(
            config,
            &password,
            totp.as_deref(),
            &server_address,
            config.socks_port,
        );
        std::fs::create_dir_all(&paths.vpn_dir)?;
        std::fs::write(&paths.vpn_config_file, config_text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &paths.vpn_config_file,
                std::fs::Permissions::from_mode(0o600),
            );
        }

        inner.state = "starting".to_string();
        inner.last_error = None;
        inner.message = Some("正在登录校园 VPN…".to_string());
        self.emit(config, &inner);
        self.logs
            .info("vpn", format!("启动 fdu-connect：{}", binary.display()));

        let mut command = fdu_command(&binary);
        command.arg("-config").arg(&paths.vpn_config_file);
        for extra in &config.extra_args {
            if !extra.trim().is_empty() {
                command.arg(extra);
            }
        }
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(false);
        let mut child = command
            .spawn()
            .map_err(|err| CommandError::new("vpn", format!("启动 fdu-connect 失败: {err}")))?;
        let pid = child.id();
        inner.pid = pid;
        inner.state = "starting".to_string();

        // 日志采集
        let log_lines = Arc::clone(&inner.log_lines);
        log_lines.lock().await.clear();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let logs = Arc::clone(&self.logs);
        let log_lines_stdout = Arc::clone(&log_lines);
        inner.log_task = Some(tokio::spawn(async move {
            if let Some(stdout) = stdout {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if vpn_log_level(&line) == "warn" {
                        logs.warn("vpn", &line);
                    } else {
                        logs.info("vpn", &line);
                    }
                    let mut buffer = log_lines_stdout.lock().await;
                    buffer.push(line);
                    if buffer.len() > 200 {
                        buffer.remove(0);
                    }
                }
            }
        }));
        let logs_err = Arc::clone(&self.logs);
        let log_lines_stderr = Arc::clone(&log_lines);
        tokio::spawn(async move {
            if let Some(stderr) = stderr {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    logs_err.warn("vpn", &line);
                    let mut buffer = log_lines_stderr.lock().await;
                    buffer.push(line);
                    if buffer.len() > 200 {
                        buffer.remove(0);
                    }
                }
            }
        });
        inner.child = Some(child);

        // 等待就绪/失败（最多 45s）
        let deadline = Instant::now() + Duration::from_secs(45);
        let mut ready = false;
        let mut failure: Option<String> = None;
        while Instant::now() < deadline {
            if let Some(child) = inner.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        failure = Some(format!("fdu-connect 进程退出（{status}）"));
                        break;
                    }
                    Ok(None) => {}
                    Err(err) => {
                        failure = Some(format!("查询 fdu-connect 状态失败: {err}"));
                        break;
                    }
                }
            }
            {
                let buffer = inner.log_lines.lock().await;
                if buffer
                    .iter()
                    .any(|line| READY_MARKERS.iter().any(|marker| line.contains(marker)))
                {
                    ready = true;
                    break;
                }
                if let Some(line) = buffer
                    .iter()
                    .find(|line| ERROR_MARKERS.iter().any(|marker| line.contains(marker)))
                {
                    failure = Some(format!("登录失败：{line}"));
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }

        if !ready {
            secure_remove(&paths.vpn_config_file);
            let detail = failure.unwrap_or_else(|| "等待 fdu-connect 就绪超时（45s）".to_string());
            let recent = inner.log_lines.lock().await.clone();
            let _ = self.kill_child(&mut inner).await;
            inner.state = "error".to_string();
            inner.last_error = Some(detail.clone());
            self.emit(config, &inner);
            return Err(CommandError::new("vpn", detail).with_detail(
                recent
                    .into_iter()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .join(" | "),
            ));
        }

        // 安全清理：fdu-connect 已完成配置读取，销毁含明文密码的临时配置文件
        secure_remove(&paths.vpn_config_file);
        self.logs
            .info("vpn", "临时配置文件已销毁（密码不落盘留存）");
        inner.state = "running".to_string();
        inner.message = Some(format!(
            "校内 VPN 已连接（SOCKS 127.0.0.1:{}）",
            config.socks_port
        ));
        inner.last_error = None;
        self.logs.info(
            "vpn",
            format!("fdu-connect 就绪，SOCKS 端口 {}", config.socks_port),
        );
        self.emit(config, &inner);
        Ok(self.snapshot(config, &inner))
    }

    /// 停止校内 VPN。
    pub async fn stop(&self, config: &CampusVpnConfig) -> VpnStatus {
        let mut inner = self.inner.lock().await;
        let _ = self.kill_child(&mut inner).await;
        inner.existing = false;
        inner.state = "stopped".to_string();
        inner.pid = None;
        inner.message = None;
        inner.last_error = None;
        self.emit(config, &inner);
        self.logs.info("vpn", "校内 VPN 已停止");
        self.snapshot(config, &inner)
    }

    /// 测试登录（临时端口，不污染正式配置）。
    pub async fn test(
        &self,
        config: &CampusVpnConfig,
        secrets: &SecretStore,
        paths: &AppPaths,
    ) -> CmdResult<VpnTestResult> {
        let binary = resolve_binary(&self.app, config, paths)
            .ok_or_else(|| CommandError::new("vpn", "找不到 fdu-connect 可执行文件"))?;
        let password = secrets
            .get(config.password_ref.as_deref().unwrap_or(""))
            .or_else(|| secrets.get(labvpn_core::config::secret_keys::CAMPUS_PASSWORD))
            .ok_or_else(|| CommandError::new("vpn", "请先填写复旦上网密码再测试"))?;
        let totp = config
            .totp_secret_ref
            .as_deref()
            .and_then(|key| secrets.get(key));
        let test_port = find_free_port(config.socks_port + 1).await?;
        let server_address = resolve_server_address(config).await;
        let config_text = build_config_toml(
            config,
            &password,
            totp.as_deref(),
            &server_address,
            test_port,
        );
        let test_config = paths.vpn_dir.join("config.test.toml");
        std::fs::write(&test_config, config_text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&test_config, std::fs::Permissions::from_mode(0o600));
        }

        let mut child = fdu_command(&binary)
            .arg("-config")
            .arg(&test_config)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|err| CommandError::new("vpn", format!("启动 fdu-connect 失败: {err}")))?;

        let mut log_lines: Vec<String> = Vec::new();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let lines_stdout = Arc::clone(&lines);
        let stdout_task = tokio::spawn(async move {
            if let Some(stdout) = stdout {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    lines_stdout.lock().await.push(line);
                }
            }
        });
        let lines_stderr = Arc::clone(&lines);
        let stderr_task = tokio::spawn(async move {
            if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    lines_stderr.lock().await.push(line);
                }
            }
        });

        let deadline = Instant::now() + Duration::from_secs(40);
        let mut ok = false;
        let mut failure: Option<String> = None;
        while Instant::now() < deadline {
            if let Ok(Some(status)) = child.try_wait() {
                failure = Some(format!("fdu-connect 退出（{status}）"));
                break;
            }
            {
                let buffer = lines.lock().await;
                if buffer
                    .iter()
                    .any(|line| READY_MARKERS.iter().any(|marker| line.contains(marker)))
                {
                    ok = true;
                    break;
                }
                if let Some(line) = buffer
                    .iter()
                    .find(|line| ERROR_MARKERS.iter().any(|marker| line.contains(marker)))
                {
                    failure = Some(format!("登录失败：{line}"));
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(400)).await;
        }
        let _ = child.kill().await;
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        secure_remove(&test_config);
        log_lines.extend(lines.lock().await.clone());
        let recent: Vec<String> = log_lines.iter().rev().take(10).cloned().collect();

        if ok {
            Ok(VpnTestResult {
                ok: true,
                message: "登录成功：账号密码有效，VPN 服务可用".to_string(),
                log_lines: recent,
            })
        } else {
            Ok(VpnTestResult {
                ok: false,
                message: failure.unwrap_or_else(|| "测试超时：未在 40s 内登录成功".to_string()),
                log_lines: recent,
            })
        }
    }

    async fn port_open(&self, port: u16) -> bool {
        tokio::time::timeout(
            Duration::from_millis(600),
            tokio::net::TcpStream::connect(("127.0.0.1", port)),
        )
        .await
        .map(|result| result.is_ok())
        .unwrap_or(false)
    }

    async fn kill_child(&self, inner: &mut Inner) -> CmdResult<()> {
        if let Some(task) = inner.log_task.take() {
            task.abort();
        }
        if let Some(mut child) = inner.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        inner.pid = None;
        Ok(())
    }

    fn snapshot(&self, config: &CampusVpnConfig, inner: &Inner) -> VpnStatus {
        VpnStatus {
            enabled: config.enabled,
            state: inner.state.clone(),
            managed: config.managed,
            socks_port: config.socks_port,
            binary_path: inner
                .binary_path
                .as_ref()
                .map(|path| path.display().to_string()),
            pid: inner.pid,
            message: inner.message.clone(),
            last_error: inner.last_error.clone(),
            existing_instance: inner.existing,
        }
    }

    fn emit(&self, config: &CampusVpnConfig, inner: &Inner) {
        events::emit(&self.app, events::VPN_STATUS, self.snapshot(config, inner));
    }
}

/// 解析 fdu-connect 可执行文件。
///
/// 优先级：用户显式指定 → App 内置（打包） → 应用数据目录 → 系统常见路径 → PATH。
pub fn resolve_binary(
    app: &AppHandle,
    config: &CampusVpnConfig,
    paths: &AppPaths,
) -> Option<PathBuf> {
    let file_name = if cfg!(windows) {
        "fdu-connect.exe"
    } else {
        "fdu-connect"
    };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = config.binary_path.as_deref() {
        let path = path.trim();
        if !path.is_empty() {
            candidates.push(PathBuf::from(path));
        }
    }
    // App 内置（Release 打包）：优先于系统安装，保证开箱即用
    if let Some(bundled) = crate::services::core_bin::bundled_sidecar(app, "fdu-connect") {
        candidates.push(bundled);
    }
    candidates.push(paths.bin_dir.join(file_name));
    if let Some(home) = home_dir() {
        candidates.push(home.join("LocalDocuments/fdu-connect").join(file_name));
        candidates.push(home.join(".local/bin").join(file_name));
        candidates.push(home.join("fdu-connect").join(file_name));
    }
    candidates.push(PathBuf::from("/usr/local/bin").join(file_name));
    candidates.push(PathBuf::from("/opt/fdu-connect").join(file_name));
    candidates.push(PathBuf::from("/usr/bin").join(file_name));
    for candidate in candidates {
        if is_usable(&candidate) {
            return Some(candidate);
        }
    }
    // PATH 搜索
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(file_name))
        .find(|candidate| is_usable(candidate))
}

/// 探测环境中已有的 fdu-connect。
pub fn detect_existing(app: &AppHandle, config: &CampusVpnConfig, paths: &AppPaths) -> ExistingVpn {
    let binary = resolve_binary(app, config, paths);
    let ports = [config.socks_port, 1080];
    let socks_port = ports.into_iter().find(|port| port_open_blocking(*port));
    let service_label = {
        #[cfg(target_os = "macos")]
        {
            let output = crate::platform::hidden_command("launchctl")
                .arg("list")
                .output()
                .ok();
            output.and_then(|output| {
                let text = String::from_utf8_lossy(&output.stdout).to_string();
                text.lines()
                    .find(|line| line.contains("com.fdu-connect"))
                    .map(|_| "com.fdu-connect".to_string())
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    };
    ExistingVpn {
        binary_path: binary.as_ref().map(|path| path.display().to_string()),
        socks_port,
        running: socks_port.is_some(),
        service_label,
    }
}

fn port_open_blocking(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(300),
    )
    .is_ok()
}

fn is_usable(path: &Path) -> bool {
    path.is_file()
        && std::fs::metadata(path)
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
}

/// 覆盖并删除敏感文件，降低磁盘残留风险。
fn secure_remove(path: &Path) {
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.is_file() && metadata.len() > 0 {
            let len = metadata.len() as usize;
            if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(path) {
                let _ = std::io::Write::write_all(&mut file, &vec![0u8; len.min(4096)]);
                let _ = file.sync_all();
            }
        }
    }
    let _ = std::fs::remove_file(path);
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

async fn find_free_port(start: u16) -> CmdResult<u16> {
    for port in start..start.saturating_add(50) {
        if tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return Ok(port);
        }
    }
    Err(CommandError::new("vpn", "找不到可用端口用于登录测试"))
}

/// 解析 VPN 服务器地址：优先用户指定 IP，其次系统解析（避开 fake-ip）。
async fn resolve_server_address(config: &CampusVpnConfig) -> String {
    if let Some(override_ip) = config.server_ip_override.as_deref() {
        let trimmed = override_ip.trim();
        if !trimmed.is_empty() && trimmed.parse::<std::net::IpAddr>().is_ok() {
            return trimmed.to_string();
        }
    }
    let host = config.server.trim();
    if host.is_empty() {
        return host.to_string();
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_string();
    }
    match tokio::net::lookup_host((host, config.port)).await {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) => {
                let ip = addr.ip();
                let fake = match ip {
                    std::net::IpAddr::V4(v4) => {
                        let octets = v4.octets();
                        octets[0] == 198 && (octets[1] == 18 || octets[1] == 19)
                    }
                    std::net::IpAddr::V6(_) => false,
                };
                if fake {
                    log::warn!("服务器 {host} 解析到 fake-ip {ip}，回退使用域名");
                    host.to_string()
                } else {
                    ip.to_string()
                }
            }
            None => host.to_string(),
        },
        Err(err) => {
            log::warn!("解析 VPN 服务器域名失败（{err}），使用域名");
            host.to_string()
        }
    }
}

/// 生成 fdu-connect 的 config.toml。
pub fn build_config_toml(
    config: &CampusVpnConfig,
    password: &str,
    totp: Option<&str>,
    server_address: &str,
    socks_port: u16,
) -> String {
    let custom_domains = config
        .custom_proxy_domains
        .iter()
        .map(|domain| format!("    \"{}\",", escape_toml(domain)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"username = "{username}"
password = "{password}"
totp_secret = "{totp}"
cert_file = ""
cert_password = ""

# 进阶设置（由 LabVPN 生成）
server_address = "{server_address}"
server_port = {server_port}
disable_server_config = false
skip_domain_resource = false
disable_fdu_config = {disable_fdu_config}
disable_fdu_dns = {disable_fdu_dns}
disable_multi_line = false
proxy_all = {proxy_all}
# LabVPN 补丁：可取消握手 + SOCKS5 宽限应答（见 vendor/fdu-connect/PATCH.md）
socks_connect_timeout = {socks_connect_timeout}
socks_early_reply_ms = {socks_early_reply_ms}
socks_bind = "127.0.0.1:{socks_port}"
socks_user = ""
socks_passwd = ""
http_bind = ""
shadowsocks_url = ""
dial_direct_proxy = ""
tun_mode = false
add_route = false
dns_ttl = 3600
disable_keep_alive = false
fdu_dns_server = "202.120.224.26"
secondary_dns_server = "223.5.5.5"
dns_server_bind = ""
dns_hijack = false
debug_dump = false

port_forwarding = []

custom_dns = []

custom_proxy_domain = [
{custom_domains}
]
"#,
        username = escape_toml(&config.username),
        password = escape_toml(password),
        totp = escape_toml(totp.unwrap_or("")),
        server_address = escape_toml(server_address),
        server_port = config.port,
        proxy_all = config.proxy_all,
        socks_connect_timeout = SOCKS_CONNECT_TIMEOUT_SECS,
        socks_early_reply_ms = SOCKS_EARLY_REPLY_MS,
        disable_fdu_config = config.disable_fdu_config,
        disable_fdu_dns = config.disable_fdu_dns,
        socks_port = socks_port,
        custom_domains = custom_domains,
    )
}

/// TOML 基本字符串转义。
fn escape_toml(input: &str) -> String {
    input
        .chars()
        .flat_map(|ch| match ch {
            '\\' => vec!['\\', '\\'],
            '"' => vec!['\\', '"'],
            '\n' => vec!['\\', 'n'],
            '\r' => vec!['\\', 'r'],
            '\t' => vec!['\\', 't'],
            other => vec![other],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_toml_strings() {
        assert_eq!(escape_toml("a\\b\"c"), "a\\\\b\\\"c");
        assert_eq!(escape_toml("line\nbreak"), "line\\nbreak");
    }

    #[test]
    fn classifies_vpn_log_lines() {
        assert_eq!(
            vpn_log_level(
                "2026/10/27 12:52:44 SOCKS5: tcp -> 10.134.142.235:22 attempt 4 failed \
                 (context deadline exceeded), retrying"
            ),
            "warn"
        );
        assert_eq!(
            vpn_log_level(
                "KeepAlive: lookup www.baidu.com on 223.5.5.5:53: read udp \
                 10.230.33.220:43982: i/o timeout"
            ),
            "warn"
        );
        assert_eq!(
            vpn_log_level(
                "[SOCKS5] 2026/10/27 12:52:48 [E]: server: readfrom tcp \
                 127.0.0.1:11080->127.0.0.1:59245: read tcp 10.230.33.220:57625: \
                 connection reset by peer"
            ),
            "warn"
        );
        assert_eq!(
            vpn_log_level(
                "2026/10/27 12:52:35 SOCKS5: tcp -> 10.134.142.145:22 still connecting \
                 after 2s, replying and waiting up to 20s"
            ),
            "info"
        );
        assert_eq!(vpn_log_level("KeepAlive: OK"), "info");
        assert_eq!(
            vpn_log_level("SOCKS5 server listening on 127.0.0.1:11080"),
            "info"
        );
    }

    #[test]
    fn builds_config_toml_with_credentials() {
        let config = CampusVpnConfig {
            username: "21300000000".into(),
            custom_proxy_domains: vec!["science.org".into()],
            ..Default::default()
        };
        let toml = build_config_toml(&config, "p@ss\"word", None, "202.120.224.58", 11080);
        assert!(toml.contains("username = \"21300000000\""));
        assert!(toml.contains("password = \"p@ss\\\"word\""));
        assert!(toml.contains("server_address = \"202.120.224.58\""));
        assert!(toml.contains("socks_bind = \"127.0.0.1:11080\""));
        assert!(toml.contains("science.org"));
        assert!(toml.contains("dial_direct_proxy = \"\""));
        // 默认路由默认开启（校园 VPN 全走 RVPN 隧道）
        assert!(toml.contains("proxy_all = true"));
        // 补丁版 fdu-connect 的 SOCKS5 建连宽限
        assert!(toml.contains("socks_connect_timeout = 20"));
        assert!(toml.contains("socks_early_reply_ms = 2000"));
    }

    #[test]
    fn writes_proxy_all_toggle() {
        let config = CampusVpnConfig {
            proxy_all: false,
            ..Default::default()
        };
        let toml = build_config_toml(&config, "pw", None, "stuvpn.fudan.edu.cn", 11080);
        assert!(toml.contains("proxy_all = false"));

        let config = CampusVpnConfig {
            proxy_all: true,
            ..Default::default()
        };
        let toml = build_config_toml(&config, "pw", None, "stuvpn.fudan.edu.cn", 11080);
        assert!(toml.contains("proxy_all = true"));
    }
}
