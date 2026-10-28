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
use tauri::Manager;

use crate::state::AppState;
use labvpn_core::vpn_log::EVENT_PREFIX;
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
    const ERROR_MARKERS: &[&str] = &[
        "[e]:",
        "error:",
        " fatal",
        "panic",
        "登录失败",
        "认证失败",
        "cannot login",
        "permission denied",
    ];
    const WARN_MARKERS: &[&str] = &[
        "failed",
        "timeout",
        "deadline exceeded",
        "reset by peer",
        "broken pipe",
        "connection refused",
        "no route to host",
        "retrying",
    ];
    let lower = line.to_ascii_lowercase();
    if ERROR_MARKERS.iter().any(|marker| lower.contains(marker)) {
        "error"
    } else if WARN_MARKERS.iter().any(|marker| lower.contains(marker)) {
        "warn"
    } else {
        "info"
    }
}

/// 会话级健康计数（供结构化事件与 60s 巡检使用）。
#[derive(Default)]
struct VpnHealth {
    keepalive_ok: u64,
    keepalive_fail: u64,
    fail_streak: u64,
    tunnel_down_since: Option<Instant>,
    session_started: Option<Instant>,
    twfid: Option<String>,
    session_id: Option<String>,
    socks_retry: u64,
    tunnel_reset: u64,
}

impl VpnHealth {
    fn reset_session(&mut self) {
        *self = Self {
            session_started: Some(Instant::now()),
            ..Self::default()
        };
    }

    fn snapshot_line(&self) -> String {
        let state = if self.fail_streak >= 3 {
            "down"
        } else if self.fail_streak > 0 {
            "degraded"
        } else {
            "ok"
        };
        let down = self
            .tunnel_down_since
            .map(|since| format!(" 中断持续={}s", since.elapsed().as_secs()))
            .unwrap_or_default();
        format!(
            "{EVENT_PREFIX} 健康巡检 keepalive_ok={} keepalive_fail={} 连续失败={} 状态={state}{down}",
            self.keepalive_ok, self.keepalive_fail, self.fail_streak
        )
    }

    fn end_line(&self, reason: &str) -> String {
        let uptime = self
            .session_started
            .map(|start| start.elapsed().as_secs())
            .unwrap_or(0);
        format!(
            "{EVENT_PREFIX} 会话结束（{reason}）时长={uptime}s keepalive_ok={} keepalive_fail={} 数据重连={} SOCKS重试={}",
            self.keepalive_ok, self.keepalive_fail, self.tunnel_reset, self.socks_retry
        )
    }
}

/// 去掉 fdu-connect 行内时间前缀（`2026/10/27 10:16:43 `）。
fn strip_inner_ts(message: &str) -> &str {
    let mut parts = message.splitn(3, ' ');
    let (Some(first), Some(second)) = (parts.next(), parts.next()) else {
        return message;
    };
    if first.len() >= 8 && first.contains('/') && second.matches(':').count() == 2 {
        return parts.next().unwrap_or("");
    }
    message
}

/// 识别连接层关键事件并转成可检索的 `[VPN事件]` 日志。
fn classify_vpn_line(line: &str, health: &mut VpnHealth) -> Option<(&'static str, String)> {
    let message = strip_inner_ts(line.trim());
    if message.starts_with("Start FDU Connect") {
        health.reset_session();
        return Some(("info", format!("{EVENT_PREFIX} 会话开始：{message}")));
    }
    if let Some(value) = message.strip_prefix("TWFID: ") {
        health.twfid = Some(value.trim().to_string());
        return None;
    }
    if message.starts_with("Update TWFID") {
        if let Some((_, value)) = message.split_once(':') {
            health.twfid = Some(value.trim().to_string());
        }
        return Some((
            "info",
            format!(
                "{EVENT_PREFIX} TWFID 授权 twfid={}",
                health.twfid.clone().unwrap_or_default()
            ),
        ));
    }
    if message.starts_with("TWFID has been authorized") {
        return Some((
            "info",
            format!(
                "{EVENT_PREFIX} 登录成功 twfid={}",
                health.twfid.clone().unwrap_or_default()
            ),
        ));
    }
    if let Some(value) = message.strip_prefix("Server session ID: ") {
        health.session_id = Some(value.trim().to_string());
        return Some((
            "info",
            format!("{EVENT_PREFIX} 会话建立 session_id={}", value.trim()),
        ));
    }
    if message.starts_with("KeepAlive: OK") {
        health.keepalive_ok += 1;
        health.fail_streak = 0;
        if let Some(since) = health.tunnel_down_since.take() {
            return Some((
                "info",
                format!(
                    "{EVENT_PREFIX} 隧道恢复 中断时长={}s keepalive_ok={} keepalive_fail={}",
                    since.elapsed().as_secs(),
                    health.keepalive_ok,
                    health.keepalive_fail
                ),
            ));
        }
        return None;
    }
    if message.starts_with("KeepAlive:") {
        health.keepalive_fail += 1;
        health.fail_streak += 1;
        if health.tunnel_down_since.is_none() {
            health.tunnel_down_since = Some(Instant::now());
        }
        return match health.fail_streak {
            3 => Some((
                "warn",
                format!(
                    "{EVENT_PREFIX} 隧道中断：KeepAlive 连续失败 3 次（{message}）；\
                     fdu-connect 不会自动重新登录，只能等待隧道恢复或重启校内 VPN"
                ),
            )),
            10 => Some((
                "warn",
                format!(
                    "{EVENT_PREFIX} 隧道持续中断：KeepAlive 连续失败 {} 次，建议重启校内 VPN",
                    health.fail_streak
                ),
            )),
            _ => None,
        };
    }
    if message.starts_with("Error occurred while receiving")
        || message.starts_with("Error occurred while sending")
    {
        health.tunnel_reset += 1;
        return Some((
            "warn",
            format!("{EVENT_PREFIX} RVPN 数据连接异常，fdu-connect 正在重新握手：{message}"),
        ));
    }
    if message.contains("still connecting after")
        || (message.contains("attempt") && message.contains("failed"))
    {
        health.socks_retry += 1;
    }
    None
}

/// 自动重启策略常量：KeepAlive 连续失败达到阈值即重启（等效重新授权）。
const AUTO_RESTART_FAIL_THRESHOLD: u64 = 3;
/// 自动重启频率保护窗口。
const AUTO_RESTART_WINDOW: Duration = Duration::from_secs(30 * 60);
/// 窗口内最多自动重启次数。
const AUTO_RESTART_MAX_PER_WINDOW: usize = 3;

/// TCP KeepAlive 探测状态（通过 fdu-connect SOCKS5 做真实 TCP 探测）。
#[derive(Default)]
struct TcpHealth {
    /// 探测成功次数
    ok: u64,
    /// 探测失败次数
    fail: u64,
    /// 连续失败次数
    fail_streak: u64,
    /// 上次探测时间
    last_probe: Option<Instant>,
    /// 连续失败开始时间
    down_since: Option<Instant>,
    /// 最近一次错误
    last_error: Option<String>,
}

/// TCP KeepAlive 探测间隔。
const TCP_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(60);
/// 单次探测总超时（fdu-connect 内部 SOCKS 宽限最长 20s，外加余量）。
const TCP_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(25);

/// 构造 DNS A 查询（TCP DNS 探测用，纯函数便于单测）。
fn build_dns_query(host: &str) -> Vec<u8> {
    let mut query = Vec::with_capacity(64);
    query.extend_from_slice(&[0x4c, 0x56]); // ID
    query.extend_from_slice(&[0x01, 0x00]); // 标准查询，递归期望
    query.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
    query.extend_from_slice(&[0x00, 0x00]); // ANCOUNT
    query.extend_from_slice(&[0x00, 0x00]); // NSCOUNT
    query.extend_from_slice(&[0x00, 0x00]); // ARCOUNT
    for label in host.split('.') {
        let bytes = label.as_bytes();
        query.push(bytes.len() as u8);
        query.extend_from_slice(bytes);
    }
    query.push(0);
    query.extend_from_slice(&[0x00, 0x01]); // QTYPE A
    query.extend_from_slice(&[0x00, 0x01]); // QCLASS IN
    query
}

/// 通过 fdu-connect 的 SOCKS5 端口做 TCP KeepAlive 探测。
///
/// - `port == 53`：发送 TCP DNS 查询并等待应答（推荐，能真实验证隧道数据面）；
/// - 其它端口：完成 SOCKS5 CONNECT 后等待 2 秒，若对端未立即关闭则视为可用（近似判断）。
async fn probe_tcp_via_socks(socks_port: u16, host: &str, port: u16) -> Result<u64, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let started = Instant::now();
    let mut stream = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::net::TcpStream::connect(("127.0.0.1", socks_port)),
    )
    .await
    .map_err(|_| "连接本地 SOCKS5 超时".to_string())?
    .map_err(|err| format!("连接本地 SOCKS5 失败（fdu-connect 未就绪）：{err}"))?;

    // SOCKS5 无认证握手
    let mut write = stream.write_all(&[0x05, 0x01, 0x00]);
    tokio::time::timeout(Duration::from_secs(3), write)
        .await
        .map_err(|_| "SOCKS5 握手写超时".to_string())?
        .map_err(|err| format!("SOCKS5 握手写失败：{err}"))?;
    let mut greeting = [0u8; 2];
    tokio::time::timeout(Duration::from_secs(3), stream.read_exact(&mut greeting))
        .await
        .map_err(|_| "SOCKS5 握手读超时".to_string())?
        .map_err(|err| format!("SOCKS5 握手读失败：{err}"))?;
    if greeting != [0x05, 0x00] {
        return Err(format!("SOCKS5 不支持无认证（{greeting:02x?}）"));
    }

    // CONNECT 请求
    let mut request = vec![0x05, 0x01, 0x00];
    if let Ok(ipv4) = host.parse::<std::net::Ipv4Addr>() {
        request.push(0x01);
        request.extend_from_slice(&ipv4.octets());
    } else if let Ok(ipv6) = host.parse::<std::net::Ipv6Addr>() {
        request.push(0x04);
        request.extend_from_slice(&ipv6.octets());
    } else {
        let domain = host.as_bytes();
        if domain.is_empty() || domain.len() > 255 {
            return Err("探测终点不是合法 IP/域名".to_string());
        }
        request.push(0x03);
        request.push(domain.len() as u8);
        request.extend_from_slice(domain);
    }
    request.extend_from_slice(&port.to_be_bytes());
    write = stream.write_all(&request);
    tokio::time::timeout(Duration::from_secs(3), write)
        .await
        .map_err(|_| "SOCKS5 CONNECT 写超时".to_string())?
        .map_err(|err| format!("SOCKS5 CONNECT 写失败：{err}"))?;

    // 读 CONNECT 应答（fdu-connect 可能宽限 2s 后先回成功）
    let mut head = [0u8; 4];
    tokio::time::timeout(TCP_KEEPALIVE_TIMEOUT, stream.read_exact(&mut head))
        .await
        .map_err(|_| "SOCKS5 CONNECT 应答超时（隧道数据面可能中断）".to_string())?
        .map_err(|err| format!("SOCKS5 CONNECT 应答失败：{err}"))?;
    if head[0] != 0x05 || head[1] != 0x00 {
        return Err(format!("SOCKS5 CONNECT 被拒绝（rep={}）", head[1]));
    }
    let skip = match head[3] {
        0x01 => 4usize,
        0x04 => 16usize,
        0x03 => {
            let mut len = [0u8; 1];
            tokio::time::timeout(Duration::from_secs(3), stream.read_exact(&mut len))
                .await
                .map_err(|_| "SOCKS5 应答读超时".to_string())?
                .map_err(|err| format!("SOCKS5 应答读失败：{err}"))?;
            len[0] as usize
        }
        _ => return Err(format!("SOCKS5 应答地址类型未知（{}）", head[3])),
    };
    let mut tail = vec![0u8; skip + 2];
    tokio::time::timeout(Duration::from_secs(3), stream.read_exact(&mut tail))
        .await
        .map_err(|_| "SOCKS5 应答读超时".to_string())?
        .map_err(|err| format!("SOCKS5 应答读失败：{err}"))?;

    if port == 53 {
        // 真实数据往返：TCP DNS 查询
        let query = build_dns_query("www.baidu.com");
        let mut framed = Vec::with_capacity(query.len() + 2);
        framed.extend_from_slice(&(query.len() as u16).to_be_bytes());
        framed.extend_from_slice(&query);
        tokio::time::timeout(TCP_KEEPALIVE_TIMEOUT, stream.write_all(&framed))
            .await
            .map_err(|_| "TCP DNS 查询写超时（隧道数据面可能中断）".to_string())?
            .map_err(|err| format!("TCP DNS 查询写失败：{err}"))?;
        let mut len_buf = [0u8; 2];
        tokio::time::timeout(TCP_KEEPALIVE_TIMEOUT, stream.read_exact(&mut len_buf))
            .await
            .map_err(|_| "TCP DNS 应答超时（隧道数据面可能中断）".to_string())?
            .map_err(|err| format!("TCP DNS 应答读失败：{err}"))?;
        let length = u16::from_be_bytes(len_buf) as usize;
        if !(12..=4096).contains(&length) {
            return Err(format!("TCP DNS 应答长度异常（{length}）"));
        }
        let mut response = vec![0u8; length];
        tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut response))
            .await
            .map_err(|_| "TCP DNS 应答内容超时".to_string())?
            .map_err(|err| format!("TCP DNS 应答内容读失败：{err}"))?;
        let rcode = response[3] & 0x0f;
        let answers = u16::from_be_bytes([response[6], response[7]]);
        if rcode != 0 {
            return Err(format!("TCP DNS 返回错误码 {rcode}"));
        }
        if answers == 0 {
            return Err("TCP DNS 无应答记录".to_string());
        }
    } else {
        // 其它端口：等待 2 秒，若被关闭则视为失败
        let mut probe = [0u8; 1];
        match tokio::time::timeout(Duration::from_secs(2), stream.read(&mut probe)).await {
            Ok(Ok(0)) => return Err("TCP 探测目标已关闭连接".to_string()),
            Ok(Err(err)) => return Err(format!("TCP 探测读失败：{err}")),
            _ => {}
        }
    }
    Ok(started.elapsed().as_millis() as u64)
}

/// 自动重启的并发保护与频率记录。
#[derive(Default)]
struct RestartGuard {
    in_progress: bool,
    history: Vec<Instant>,
    cap_logged: bool,
}

/// 是否应该触发自动重启（纯函数，便于单测）。
fn should_auto_restart(
    fail_streak: u64,
    state: &str,
    history_len: usize,
    in_progress: bool,
) -> bool {
    fail_streak >= AUTO_RESTART_FAIL_THRESHOLD
        && state == "running"
        && !in_progress
        && history_len < AUTO_RESTART_MAX_PER_WINDOW
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
    health: Option<Arc<std::sync::Mutex<VpnHealth>>>,
    health_task: Option<tokio::task::JoinHandle<()>>,
    /// TCP KeepAlive 探测状态（fdu-connect 的 UDP KeepAlive 已禁用）
    tcp_health: Arc<std::sync::Mutex<TcpHealth>>,
}

/// 校内 VPN 管理器。
pub struct VpnManager {
    app: AppHandle,
    logs: Arc<LogStore>,
    inner: Mutex<Inner>,
    restart_guard: Mutex<RestartGuard>,
}

impl VpnManager {
    /// 创建管理器。
    pub fn new(app: AppHandle, logs: Arc<LogStore>) -> Self {
        Self {
            app,
            logs,
            restart_guard: Mutex::new(RestartGuard::default()),
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
                health: None,
                health_task: None,
                tcp_health: Arc::new(std::sync::Mutex::new(TcpHealth::default())),
            }),
        }
    }

    /// 当前状态。
    pub async fn status(&self, config: &CampusVpnConfig) -> VpnStatus {
        let mut inner = self.inner.lock().await;
        if let Some(child) = inner.child.as_mut() {
            if let Ok(Some(status)) = child.try_wait() {
                let code = status
                    .code()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "signal".to_string());
                self.logs.warn(
                    "vpn",
                    format!("{EVENT_PREFIX} fdu-connect 进程退出 code={code}"),
                );
                inner.state = "error".to_string();
                inner.last_error = Some(format!("fdu-connect 进程退出（{status}）"));
                inner.child = None;
                inner.pid = None;
            }
        }
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

    /// TCP KeepAlive 探测状态快照（ok, fail, fail_streak, down_since）。
    async fn tcp_health_snapshot(&self) -> (u64, u64, u64, Option<Instant>) {
        let inner = self.inner.lock().await;
        let health = match inner.tcp_health.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        (
            health.ok,
            health.fail,
            health.fail_streak,
            health.down_since,
        )
    }

    /// 每 60 秒做一次 TCP KeepAlive 探测（通过 fdu-connect SOCKS5）。
    ///
    /// 只在 VPN 处于 running 状态时探测；结果写入日志并驱动自动重启。
    pub async fn tcp_keepalive_tick(&self) {
        let (config, health, socks_port) = {
            let inner = self.inner.lock().await;
            if inner.state != "running" {
                return;
            }
            let Some(config) = inner.config.clone() else {
                return;
            };
            (
                config.clone(),
                Arc::clone(&inner.tcp_health),
                config.socks_port,
            )
        };
        {
            let mut guard = match health.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(last) = guard.last_probe {
                if last.elapsed() < TCP_KEEPALIVE_INTERVAL {
                    return;
                }
            }
            guard.last_probe = Some(Instant::now());
        }
        let target = format!("{}:{}", config.keepalive_host, config.keepalive_port);
        let result =
            probe_tcp_via_socks(socks_port, &config.keepalive_host, config.keepalive_port).await;
        let mut guard = match health.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        match result {
            Ok(elapsed_ms) => {
                guard.ok += 1;
                guard.fail_streak = 0;
                guard.down_since = None;
                guard.last_error = None;
                drop(guard);
                self.logs.info(
                    "vpn",
                    format!("{EVENT_PREFIX} TCP KeepAlive 成功（{target}，{elapsed_ms}ms）"),
                );
            }
            Err(err) => {
                guard.fail += 1;
                guard.fail_streak += 1;
                if guard.down_since.is_none() {
                    guard.down_since = Some(Instant::now());
                }
                guard.last_error = Some(err.clone());
                let streak = guard.fail_streak;
                let fail = guard.fail;
                let ok = guard.ok;
                drop(guard);
                self.logs.warn(
                    "vpn",
                    format!(
                        "{EVENT_PREFIX} TCP KeepAlive 失败（{target}，连续 {streak} 次，ok={ok} fail={fail}）：{err}"
                    ),
                );
            }
        }
    }

    /// KeepAlive 连续失败时自动重启 fdu-connect（等效重新授权）。
    ///
    /// 返回是否真的执行了重启。带 30 分钟窗口内最多 3 次的频率保护，避免网络长时间
    /// 不可用时反复杀进程。
    pub async fn auto_restart_if_needed(
        &self,
        config: &CampusVpnConfig,
        secrets: &SecretStore,
        paths: &AppPaths,
    ) -> bool {
        let (tcp_ok, tcp_fail, tcp_streak, down_since) = self.tcp_health_snapshot().await;
        let mut guard = self.restart_guard.lock().await;
        if tcp_streak == 0 {
            guard.cap_logged = false;
            return false;
        }
        let now = Instant::now();
        guard
            .history
            .retain(|when| now.duration_since(*when) < AUTO_RESTART_WINDOW);
        let running = {
            let inner = self.inner.lock().await;
            inner.state == "running" && inner.child.is_some()
        };
        if !should_auto_restart(
            tcp_streak,
            if running { "running" } else { "stopped" },
            guard.history.len(),
            guard.in_progress,
        ) {
            if tcp_streak >= AUTO_RESTART_FAIL_THRESHOLD
                && guard.history.len() >= AUTO_RESTART_MAX_PER_WINDOW
                && !guard.cap_logged
            {
                guard.cap_logged = true;
                let message = format!(
                    "{EVENT_PREFIX} TCP KeepAlive 连续失败 {} 次，但 30 分钟内已自动重启 {} 次，暂停自动重连；请手动重连或检查网络",
                    tcp_streak,
                    guard.history.len()
                );
                drop(guard);
                self.logs.error("vpn", message);
            }
            return false;
        }
        guard.in_progress = true;
        guard.history.push(now);
        drop(guard);

        self.logs.warn(
            "vpn",
            format!(
                "{EVENT_PREFIX} TCP KeepAlive 连续失败 {} 次（探测 {}:{}，ok={} fail={}，中断约 {} 秒），自动重启 fdu-connect（等效重新授权）",
                tcp_streak,
                config.keepalive_host,
                config.keepalive_port,
                tcp_ok,
                tcp_fail,
                down_since.map(|since| since.elapsed().as_secs()).unwrap_or(0)
            ),
        );
        let _ = self.stop(config).await;
        let result = self.start(config, secrets, paths).await;
        {
            let mut guard = self.restart_guard.lock().await;
            guard.in_progress = false;
        }
        match result {
            Ok(_) => {
                self.logs.info(
                    "vpn",
                    format!("{EVENT_PREFIX} 自动重启完成，等待 TCP KeepAlive 恢复"),
                );
                true
            }
            Err(err) => {
                self.logs
                    .error("vpn", format!("{EVENT_PREFIX} 自动重启失败：{err}"));
                false
            }
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
        let health: Arc<std::sync::Mutex<VpnHealth>> =
            Arc::new(std::sync::Mutex::new(VpnHealth::default()));
        inner.health = Some(Arc::clone(&health));
        let health_stdout = Arc::clone(&health);
        inner.log_task = Some(tokio::spawn(async move {
            if let Some(stdout) = stdout {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let event = {
                        let mut guard = match health_stdout.lock() {
                            Ok(guard) => guard,
                            Err(poisoned) => poisoned.into_inner(),
                        };
                        classify_vpn_line(&line, &mut guard)
                    };
                    if let Some((level, message)) = event {
                        if level == "warn" {
                            logs.warn("vpn", message);
                        } else {
                            logs.info("vpn", message);
                        }
                    }
                    match vpn_log_level(&line) {
                        "error" => logs.error("vpn", &line),
                        "warn" => logs.warn("vpn", &line),
                        _ => logs.info("vpn", &line),
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

        // 每 60s 输出一次连接健康快照，便于日志页/离线分析直接看到 KeepAlive 状态
        let health_snapshot = Arc::clone(&health);
        let logs_snapshot = Arc::clone(&self.logs);
        inner.health_task = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                let message = {
                    let guard = match health_snapshot.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    guard.snapshot_line()
                };
                logs_snapshot.info("vpn", message);
            }
        }));

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
        if let Ok(mut health) = inner.tcp_health.lock() {
            *health = TcpHealth::default();
        }
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
        if let Some(health) = inner.health.as_ref() {
            let guard = match health.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            self.logs.info("vpn", guard.end_line("停止/重连"));
        }
        let _ = self.kill_child(&mut inner).await;
        inner.health = None;
        if let Ok(mut health) = inner.tcp_health.lock() {
            *health = TcpHealth::default();
        }
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
        if let Some(task) = inner.health_task.take() {
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
disable_keep_alive = true
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

/// 启动自动重连监控（每 15 秒检查一次 KeepAlive 连续失败次数）。
pub fn spawn_health_monitor(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            let state = app.state::<AppState>();
            let config = state.config_snapshot();
            if !config.campus_vpn.enabled || !config.campus_vpn.managed {
                continue;
            }
            state.vpn.tcp_keepalive_tick().await;
            let _ = state
                .vpn
                .auto_restart_if_needed(&config.campus_vpn, &state.secrets, &state.paths)
                .await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_dns_query_for_tcp_probe() {
        let query = build_dns_query("www.baidu.com");
        // header(12) + qname(1+3 + 1+5 + 1+3 + 1 = 15) + qtype/qclass(4)
        assert_eq!(query.len(), 31);
        assert_eq!(query[0..2], [0x4c, 0x56]);
        assert_eq!(query[2..4], [0x01, 0x00]);
        assert_eq!(query[4..6], [0x00, 0x01]);
        assert_eq!(query[12], 3);
        assert_eq!(&query[13..16], b"www");
        assert_eq!(query[16], 5);
        assert_eq!(&query[17..22], b"baidu");
        assert_eq!(query[22], 3);
        assert_eq!(&query[23..26], b"com");
        assert_eq!(query[26], 0);
        assert_eq!(&query[27..31], [0x00, 0x01, 0x00, 0x01]);
    }

    #[test]
    fn auto_restart_policy_thresholds() {
        assert!(!should_auto_restart(2, "running", 0, false));
        assert!(!should_auto_restart(3, "stopped", 0, false));
        assert!(!should_auto_restart(3, "running", 0, true));
        assert!(!should_auto_restart(3, "running", 3, false));
        assert!(should_auto_restart(3, "running", 0, false));
        assert!(should_auto_restart(10, "running", 2, false));
    }

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
            "error"
        );
        assert_eq!(
            vpn_log_level("2026/10/27 16:00:00 Initial FDU-Connect failed: cannot login"),
            "error"
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
