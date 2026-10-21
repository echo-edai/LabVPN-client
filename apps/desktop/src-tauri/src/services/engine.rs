//! 代理引擎生命周期管理（直接子进程 / 特权 helper 两种模式）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use labvpn_core::generator::GeneratedConfig;
use labvpn_core::ipc::Command as HelperCommand;
use labvpn_core::rules::names;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::dto::{EngineStatus, ProxyGroupInfo};
use crate::error::{CmdResult, CommandError};
use crate::events;
use crate::services::helper;
use crate::services::logs::LogStore;
use crate::services::mihomo_api::{traffic_point, MihomoApi};

/// 引擎运行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMode {
    /// TUN 全流量（需要 helper）
    Tun,
    /// 系统代理（用户态核心）
    SystemProxy,
}

impl EngineMode {
    /// 稳定标识。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tun => "tun",
            Self::SystemProxy => "systemProxy",
        }
    }
}

/// 启动请求。
pub struct StartRequest {
    /// 生成的配置
    pub generated: GeneratedConfig,
    /// 运行模式
    pub mode: EngineMode,
    /// 直接模式下的核心路径
    pub core_path: Option<PathBuf>,
    /// 直接模式工作目录（TUN 模式由 helper 使用自己的工作目录）
    pub workdir: PathBuf,
    /// 直接模式配置文件路径
    pub config_path: PathBuf,
    /// helper 端口
    pub helper_port: u16,
    /// helper 令牌
    pub helper_token: String,
    /// 控制端口密钥
    pub controller_secret: String,
    /// 控制端口
    pub controller_port: u16,
    /// 混合端口
    pub mixed_port: u16,
    /// 附加资源（相对路径 -> 内容：订阅缓存、SSH 私钥）
    pub assets: Vec<(String, String)>,
}

struct Inner {
    status: EngineStatus,
    child: Option<Child>,
    pid_file: Option<PathBuf>,
    api: Option<MihomoApi>,
    traffic_task: Option<tokio::task::JoinHandle<()>>,
    log_task: Option<tokio::task::JoinHandle<()>>,
    attached: bool,
    helper_port: u16,
    helper_token: String,
}

/// 引擎管理器。
pub struct EngineManager {
    app: AppHandle,
    logs: Arc<LogStore>,
    inner: Mutex<Inner>,
}

impl EngineManager {
    /// 创建管理器。
    pub fn new(app: AppHandle, logs: Arc<LogStore>, mixed_port: u16, controller_port: u16) -> Self {
        Self {
            app,
            logs,
            inner: Mutex::new(Inner {
                status: EngineStatus::idle(mixed_port, controller_port),
                child: None,
                pid_file: None,
                api: None,
                traffic_task: None,
                log_task: None,
                attached: false,
                helper_port: 0,
                helper_token: String::new(),
            }),
        }
    }

    /// 当前状态（顺带回收意外退出的子进程）。
    pub async fn status(&self) -> EngineStatus {
        let mut inner = self.inner.lock().await;
        if !inner.attached {
            if let Some(child) = inner.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        inner.status.state = "error".to_string();
                        inner.status.running = false;
                        inner.status.pid = None;
                        inner.status.last_error = Some(format!("核心进程已退出（{status}）"));
                    }
                    Ok(None) => {}
                    Err(err) => {
                        inner.status.last_error = Some(format!("查询核心状态失败: {err}"));
                    }
                }
            }
        }
        inner.status.clone()
    }

    /// 启动引擎。
    pub async fn start(&self, request: StartRequest) -> CmdResult<EngineStatus> {
        let mut inner = self.inner.lock().await;
        if inner.status.running {
            return Ok(inner.status.clone());
        }
        self.cleanup_locked(&mut inner).await;

        inner.status = EngineStatus {
            state: "starting".to_string(),
            mode: Some(request.mode.as_str().to_string()),
            running: false,
            tun_enabled: request.mode == EngineMode::Tun,
            helper_mode: request.mode == EngineMode::Tun,
            mixed_port: request.mixed_port,
            controller_port: request.controller_port,
            pid: None,
            started_at: None,
            last_error: None,
            warnings: request.generated.warnings.clone(),
        };
        self.emit_status(&inner.status);

        let api = MihomoApi::new(request.controller_port, request.controller_secret.clone());
        let yaml = request.generated.yaml.clone();
        inner.helper_port = request.helper_port;
        inner.helper_token = request.helper_token.clone();

        let asset_paths: Vec<String> = request
            .assets
            .iter()
            .map(|(path, _)| path.clone())
            .collect();
        if request.mode == EngineMode::Tun {
            let assets: std::collections::BTreeMap<String, String> =
                request.assets.iter().cloned().collect();
            if let Err(err) = helper::request(
                request.helper_port,
                &request.helper_token,
                HelperCommand::StartCore {
                    config_yaml: yaml,
                    assets,
                },
            )
            .await
            {
                inner.status.state = "error".to_string();
                inner.status.last_error = Some(err.to_string());
                self.emit_status(&inner.status);
                return Err(err);
            }
            inner.attached = true;
            self.logs.info("core", "已通过特权 helper 启动 TUN 核心");
        } else {
            let core_path = request.core_path.clone().ok_or_else(|| {
                CommandError::new(
                    "engine",
                    "找不到 mihomo 核心二进制，请先运行 `npm run sidecars` 或安装核心",
                )
            })?;
            // 清理上一次异常退出遗留的核心进程与含密码的配置
            cleanup_stale_core(&request.workdir);
            self.prepare_direct_workdir(
                &request.workdir,
                &request.config_path,
                &yaml,
                &request.assets,
            )?;
            let mut child = Command::new(&core_path)
                .arg("-d")
                .arg(&request.workdir)
                .arg("-f")
                .arg(&request.config_path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(false)
                .spawn()
                .map_err(|err| CommandError::new("engine", format!("启动核心失败: {err}")))?;
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();
            let logs = Arc::clone(&self.logs);
            inner.log_task = Some(tokio::spawn(async move {
                if let Some(stdout) = stdout {
                    let mut lines = BufReader::new(stdout).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        logs.info("core", line);
                    }
                }
            }));
            let logs_err = Arc::clone(&self.logs);
            tokio::spawn(async move {
                if let Some(stderr) = stderr {
                    let mut lines = BufReader::new(stderr).lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        logs_err.warn("core", line);
                    }
                }
            });
            let pid = child.id();
            let pid_file = request.workdir.join("mihomo.pid");
            if let Some(pid_value) = pid {
                let _ = std::fs::write(&pid_file, format!("{pid_value}\n"));
            }
            inner.pid_file = Some(pid_file);
            inner.child = Some(child);
            inner.status.pid = pid;
            self.logs.info(
                "core",
                format!(
                    "已启动核心 pid={} 端口={}",
                    pid.map(|value| value.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    request.mixed_port
                ),
            );
        }

        // 等待控制 API 就绪（最多 60s；首次启动可能需要加载 geodata）
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut version = None;
        while Instant::now() < deadline {
            if let Some(child) = inner.child.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    let message = format!("核心启动失败（{status}），请查看日志");
                    inner.status.state = "error".to_string();
                    inner.status.last_error = Some(message.clone());
                    inner.status.running = false;
                    self.emit_status(&inner.status);
                    return Err(CommandError::new("engine", message));
                }
            }
            match api.version().await {
                Ok(found) => {
                    version = Some(found);
                    break;
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(300)).await,
            }
        }
        let Some(version) = version else {
            let recent: Vec<String> = self
                .logs
                .recent(6)
                .iter()
                .map(|entry| entry.message.clone())
                .collect();
            let message = format!(
                "核心控制端口 {} 未就绪（60s 超时）：可能被其他代理软件占用或核心启动失败。\
                 请先关闭 Clash Verge / Shadowrocket 等代理软件后重试。最近日志：{}",
                request.controller_port,
                if recent.is_empty() {
                    "（无）".to_string()
                } else {
                    recent.join(" | ")
                }
            );
            inner.status.state = "error".to_string();
            inner.status.last_error = Some(message.clone());
            self.emit_status(&inner.status);
            self.cleanup_locked(&mut inner).await;
            return Err(CommandError::new("engine", message));
        };
        self.logs.info(
            "core",
            format!(
                "核心就绪：{version}（控制端口 {}）",
                request.controller_port
            ),
        );

        // 混合端口必须真正监听，否则流量无法走代理（常见于端口被 Clash Verge 等占用）
        if !wait_port_open(request.mixed_port, Duration::from_secs(5)).await {
            let message = format!(
                "混合代理端口 {} 未监听：大概率被其他代理软件占用（请关闭 Clash Verge / Shadowrocket / 其他 mihomo），\
                 或在「设置」中换一个端口",
                request.mixed_port
            );
            self.logs.error("core", message.clone());
            inner.status.state = "error".to_string();
            inner.status.last_error = Some(message.clone());
            inner.status.running = false;
            self.emit_status(&inner.status);
            self.cleanup_locked(&mut inner).await;
            return Err(CommandError::new("engine", message));
        }

        // 安全清理：核心已完成配置解析，销毁含敏感字段的配置与私钥资产
        if request.mode == EngineMode::SystemProxy {
            secure_remove(&request.config_path);
        }
        for relative in &asset_paths {
            if relative.starts_with("keys/") {
                secure_remove(&request.workdir.join(relative));
            }
        }
        self.logs
            .info("engine", "敏感配置已从磁盘清理（仅保留在内存中）");

        let app = self.app.clone();
        let api_for_stream = api.clone();
        inner.traffic_task = Some(tokio::spawn(async move {
            let (sender, mut receiver) = tokio::sync::mpsc::channel::<(u64, u64)>(32);
            let stream_api = api_for_stream.clone();
            tokio::spawn(async move {
                stream_api.stream_traffic(sender).await;
            });
            let mut up_total = 0u64;
            let mut down_total = 0u64;
            while let Some((up, down)) = receiver.recv().await {
                up_total = up_total.saturating_add(up);
                down_total = down_total.saturating_add(down);
                events::emit(
                    &app,
                    events::TRAFFIC,
                    traffic_point(up, down, up_total, down_total),
                );
            }
        }));

        inner.api = Some(api);
        inner.status.state = "running".to_string();
        inner.status.running = true;
        inner.status.started_at =
            Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
        self.emit_status(&inner.status);
        self.logs.info("engine", "引擎已启动");
        Ok(inner.status.clone())
    }

    /// 停止引擎。
    pub async fn stop(&self) -> EngineStatus {
        let mut inner = self.inner.lock().await;
        self.cleanup_locked(&mut inner).await;
        inner.status.state = "idle".to_string();
        inner.status.running = false;
        inner.status.mode = None;
        inner.status.tun_enabled = false;
        inner.status.helper_mode = false;
        inner.status.pid = None;
        inner.status.started_at = None;
        self.emit_status(&inner.status);
        self.logs.info("engine", "引擎已停止");
        inner.status.clone()
    }

    async fn cleanup_locked(&self, inner: &mut Inner) {
        if let Some(task) = inner.traffic_task.take() {
            task.abort();
        }
        if let Some(task) = inner.log_task.take() {
            task.abort();
        }
        if let Some(pid_file) = inner.pid_file.take() {
            let _ = std::fs::remove_file(pid_file);
        }
        if inner.attached && !inner.helper_token.is_empty() && inner.helper_port != 0 {
            let port = inner.helper_port;
            let token = inner.helper_token.clone();
            if let Err(err) = helper::request(port, &token, HelperCommand::StopCore).await {
                log::warn!("通过 helper 停止核心失败: {err}");
            }
        }
        inner.attached = false;
        if let Some(mut child) = inner.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        inner.api = None;
    }

    /// 切换代理组。
    pub async fn select_proxy(&self, group: &str, name: &str) -> CmdResult<()> {
        let api = self
            .api()
            .await
            .ok_or_else(|| CommandError::new("engine", "引擎未运行，无法切换代理组"))?;
        api.select_proxy(group, name).await
    }

    /// 读取代理组。
    pub async fn proxy_groups(&self) -> CmdResult<Vec<ProxyGroupInfo>> {
        let api = self
            .api()
            .await
            .ok_or_else(|| CommandError::new("engine", "引擎未运行"))?;
        api.proxy_groups().await
    }

    /// 延迟测试。
    pub async fn delay_test(&self, name: &str) -> CmdResult<u32> {
        let api = self
            .api()
            .await
            .ok_or_else(|| CommandError::new("engine", "引擎未运行"))?;
        api.delay(name, "http://www.gstatic.com/generate_204", 5000)
            .await
    }

    /// 当前 API 客户端。
    pub async fn api(&self) -> Option<MihomoApi> {
        self.inner.lock().await.api.clone()
    }

    /// 是否运行中。
    pub async fn is_running(&self) -> bool {
        self.inner.lock().await.status.running
    }

    /// 设置校内外分流组选中项（不重启内核）。
    pub async fn apply_campus_target(&self, on_campus: bool) -> CmdResult<()> {
        if !self.is_running().await {
            return Ok(());
        }
        let target = if on_campus {
            names::DIRECT
        } else {
            names::PROXY_FDU
        };
        if let Err(err) = self.select_proxy(names::GROUP_CAMPUS, target).await {
            log::warn!("切换校内组到 {target} 失败（回退 DIRECT）: {err}");
            self.select_proxy(names::GROUP_CAMPUS, names::DIRECT)
                .await?;
        }
        Ok(())
    }

    fn prepare_direct_workdir(
        &self,
        workdir: &Path,
        config_path: &Path,
        yaml: &str,
        assets: &[(String, String)],
    ) -> CmdResult<()> {
        std::fs::create_dir_all(workdir.join("rules"))?;
        for (file, content) in labvpn_core::rules::providers::builtin_rule_files() {
            std::fs::write(workdir.join("rules").join(file), content)?;
        }
        // 内置 geodata：避免 mihomo 首次启动联网下载数十 MB 导致就绪超时
        if let Ok(resource_dir) = self.app.path().resource_dir() {
            for name in ["GeoSite.dat", "geoip.metadb"] {
                let source = resource_dir.join("bin").join(name);
                let target = workdir.join(name);
                if source.is_file() && !target.exists() {
                    let _ = std::fs::copy(&source, &target);
                }
            }
        }
        for (rel, content) in assets {
            let rel_path = Path::new(rel);
            if rel_path.is_absolute() || rel.split(['/', '\\']).any(|segment| segment == "..") {
                return Err(CommandError::new(
                    "engine",
                    format!("非法附加资源路径: {rel}"),
                ));
            }
            let target = workdir.join(rel_path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&target, content)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600));
            }
        }
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(config_path, yaml)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(config_path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    fn emit_status(&self, status: &EngineStatus) {
        events::emit(&self.app, events::ENGINE_STATUS, status.clone());
    }
}

/// 覆盖并删除敏感文件，降低磁盘残留风险。
fn secure_remove(path: &Path) {
    use std::io::Write;
    if let Ok(metadata) = std::fs::metadata(path) {
        if metadata.is_file() && metadata.len() > 0 {
            let len = metadata.len() as usize;
            if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(path) {
                let _ = file.write_all(&vec![0u8; len.min(4096)]);
                let _ = file.sync_all();
            }
        }
    }
    let _ = std::fs::remove_file(path);
}

/// 清理上次异常退出遗留的核心进程（读取工作目录下的 PID 文件）与含密码的配置。
pub fn cleanup_stale_core(workdir: &Path) {
    let pid_file = workdir.join("mihomo.pid");
    if let Ok(text) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = text.trim().parse::<u32>() {
            if pid_alive(pid) && process_is_core(pid) {
                kill_pid(pid);
            }
        }
        let _ = std::fs::remove_file(&pid_file);
    }
    let config = workdir.join("config.yaml");
    if config.exists() {
        secure_remove(&config);
    }
}

/// 等待本地端口可连接。
async fn wait_port_open(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    false
}

#[cfg(unix)]
fn process_is_core(pid: u32) -> bool {
    std::process::Command::new("ps")
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
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(unix)]
fn kill_pid(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
}

#[cfg(windows)]
fn kill_pid(pid: u32) {
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status();
}
