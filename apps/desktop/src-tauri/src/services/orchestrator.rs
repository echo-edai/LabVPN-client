//! 编排层：把配置、校园网检测、校内 VPN、代理核心、系统代理串成一次「连接」。

use labvpn_core::config::{secret_keys, AppConfig, IssueLevel, NodeSelectionMode, SshAuth};
use labvpn_core::generator::{self, ProfileSource, RuntimeContext, SshRuntime};
use labvpn_core::rules::names;
use tauri::{AppHandle, Manager};

use crate::dto::{EngineStatus, VpnStatus};
use crate::error::{CmdResult, CommandError};
use crate::events;
use crate::platform;
use crate::services::config as config_service;
use crate::services::core_bin;
use crate::services::engine::{EngineMode, StartRequest};
use crate::services::{campus, helper, subscription};
use crate::state::AppState;

/// 连接结果。
pub struct ConnectResult {
    /// 引擎状态
    pub engine: EngineStatus,
    /// 校内 VPN 状态
    pub vpn: VpnStatus,
    /// 运行模式（tun / systemProxy）
    pub mode: String,
    /// 警告
    pub warnings: Vec<String>,
}

struct BuiltRuntime {
    context: RuntimeContext,
    assets: Vec<(String, String)>,
    warnings: Vec<String>,
}

/// 连接：检测校园网 → 启动 VPN（必要时）→ 生成配置 → 启动核心 → 切换分组 → 系统代理。
pub async fn connect(state: &AppState) -> CmdResult<ConnectResult> {
    let mut config = state.config_snapshot();
    let errors: Vec<String> = config
        .validate()
        .into_iter()
        .filter(|issue| issue.level == IssueLevel::Error)
        .map(|issue| format!("{}：{}", issue.field, issue.message))
        .collect();
    if !errors.is_empty() {
        return Err(
            CommandError::new("config", "配置不完整，无法连接").with_detail(errors.join("；"))
        );
    }

    let mut warnings: Vec<String> = Vec::new();

    // 0) 清理上次异常退出遗留的核心（避免端口被自己的旧进程占用）
    crate::services::engine::cleanup_stale_core(&state.paths.core_dir);

    // 0.5) 端口冲突自动避让（Clash Verge / Shadowrocket / 其他 mihomo 常占用 7897）
    if !state.engine.is_running().await {
        choose_ports(&mut config, &mut warnings);
    }

    // 1) 校园网检测
    let campus_status = if config.campus_vpn.auto_detect_campus {
        campus::detect(state).await
    } else {
        campus::fallback_status()
    };
    state.set_last_campus(campus_status.clone()).await;
    events::emit(&state.app, events::CAMPUS_STATUS, campus_status.clone());
    let on_campus = campus_status.on_campus;
    let skip_vpn = config.campus_vpn.skip_when_on_campus && on_campus;

    // 2) 校内 VPN
    let vpn_status = if !config.campus_vpn.enabled {
        state.vpn.status(&config.campus_vpn).await
    } else if skip_vpn {
        state
            .logs
            .info("orchestrator", "检测到校园网：跳过 VPN 连接");
        state.vpn.stop(&config.campus_vpn).await
    } else {
        match state
            .vpn
            .start(&config.campus_vpn, &state.secrets, &state.paths)
            .await
        {
            Ok(status) => status,
            Err(err) => {
                // VPN 失败不阻断整体连接：相关流量会不可达，但其它分流仍可用
                warnings.push(format!("校内 VPN 启动失败：{err}"));
                state.vpn.status(&config.campus_vpn).await
            }
        }
    };

    // 3) helper 可用性决定 TUN 是否启用
    let (helper_port, helper_token, helper_ready) = helper_readiness(state).await;
    let want_tun = config.engine.tun.enabled;
    let mode = if want_tun && helper_ready {
        EngineMode::Tun
    } else {
        EngineMode::SystemProxy
    };
    if want_tun && !helper_ready {
        warnings.push(
            "TUN 模式需要安装特权 helper（设置页可一键安装）。当前已回退系统代理模式".to_string(),
        );
    }

    // 4) 运行时上下文 + 生成配置
    let built = build_runtime(state, &config, on_campus, mode == EngineMode::Tun).await?;
    warnings.extend(built.warnings.clone());
    if vpn_status.state == "error" {
        if let Some(error) = &vpn_status.last_error {
            warnings.push(format!("校内 VPN 状态异常：{error}"));
        }
    }
    let generated = generator::generate(&config, &built.context)?;
    warnings.extend(generated.warnings.clone());

    // 5) 启动核心
    let resolved = core_bin::resolve_core(&state.app, &state.paths);
    let controller_secret = built.context.controller_secret.clone();
    let request = StartRequest {
        generated,
        mode,
        core_path: resolved.path,
        workdir: if mode == EngineMode::Tun {
            state.paths.helper_workdir.clone()
        } else {
            state.paths.core_dir.clone()
        },
        config_path: state.paths.core_config.clone(),
        helper_port,
        helper_token,
        controller_secret,
        controller_port: config.engine.controller_port,
        mixed_port: config.engine.mixed_port,
        assets: built.assets,
    };
    let engine_status = state.engine.start(request).await?;

    // 6) 校内分组热切换（不重启核心）
    let _ = state.engine.apply_campus_target(on_campus).await;

    // 6.5) 手动固定节点：热切换到用户指定节点（自动模式由 mihomo url-test 按自定义间隔测速）
    if config.subscription.enabled && config.node_selection.mode == NodeSelectionMode::Manual {
        let selected = config
            .node_selection
            .selected
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        if let Some(name) = selected {
            match state.engine.select_proxy(names::GROUP_PROXY, name).await {
                Ok(()) => state.logs.info("engine", format!("已固定代理节点：{name}")),
                Err(err) => warnings.push(format!("切换代理节点失败（{name}）：{err}")),
            }
        }
    }

    // 7) 系统代理
    if mode == EngineMode::SystemProxy && config.engine.system_proxy {
        if let Err(err) = platform::set_system_proxy(config.engine.mixed_port, true) {
            warnings.push(format!("设置系统代理失败：{err}"));
        }
    } else {
        let _ = platform::set_system_proxy(config.engine.mixed_port, false);
    }

    state.logs.info(
        "orchestrator",
        format!(
            "连接完成：mode={} 校内={} vpn={} mixed-port={}",
            mode.as_str(),
            on_campus,
            vpn_status.state,
            config.engine.mixed_port
        ),
    );
    Ok(ConnectResult {
        engine: engine_status,
        vpn: vpn_status,
        mode: mode.as_str().to_string(),
        warnings,
    })
}

/// 断开：停核心 → 停 VPN → 恢复系统代理。
pub async fn disconnect(state: &AppState) -> CmdResult<EngineStatus> {
    let config = state.config_snapshot();
    let status = state.engine.stop().await;
    if config.campus_vpn.managed {
        let _ = state.vpn.stop(&config.campus_vpn).await;
    }
    if let Err(err) = platform::set_system_proxy(config.engine.mixed_port, false) {
        state
            .logs
            .warn("orchestrator", format!("恢复系统代理失败：{err}"));
    }
    state.logs.info("orchestrator", "已断开连接");
    Ok(status)
}

/// 配置变更后应用（运行中则重启核心）。
pub async fn apply_config(state: &AppState) -> CmdResult<Option<ConnectResult>> {
    let running = state.engine.is_running().await;
    if running {
        let _ = disconnect(state).await;
        let result = connect(state).await?;
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

/// 校园网状态变化后的联动（由轮询任务调用）。
pub async fn on_campus_changed(app: &AppHandle) {
    let state = app.state::<AppState>();
    let config = state.config_snapshot();
    if !config.campus_vpn.auto_detect_campus {
        return;
    }
    let Some(status) = state.last_campus().await else {
        return;
    };
    if !state.engine.is_running().await {
        return;
    }
    let on_campus = status.on_campus;
    let skip_vpn = config.campus_vpn.skip_when_on_campus && on_campus;
    if skip_vpn {
        if config.campus_vpn.enabled && config.campus_vpn.managed {
            state
                .logs
                .info("orchestrator", "进入校园网：停止校内 VPN，切为直连");
            let _ = state.vpn.stop(&config.campus_vpn).await;
        }
        let _ = state.engine.apply_campus_target(true).await;
    } else {
        if config.campus_vpn.enabled {
            let current = state.vpn.status(&config.campus_vpn).await;
            if !matches!(current.state.as_str(), "running" | "reusing") {
                state
                    .logs
                    .info("orchestrator", "离开校园网：重新连接校内 VPN");
                let _ = state
                    .vpn
                    .start(&config.campus_vpn, &state.secrets, &state.paths)
                    .await;
            }
        }
        let _ = state.engine.apply_campus_target(false).await;
    }
}

async fn helper_readiness(state: &AppState) -> (u16, String, bool) {
    let port = helper::default_port();
    let token = match helper::ensure_token(&state.paths.token_file) {
        Ok(token) => token,
        Err(err) => {
            state
                .logs
                .warn("helper", format!("准备 helper 令牌失败：{err}"));
            return (port, String::new(), false);
        }
    };
    for _ in 0..3 {
        if helper::ping(port, &token).await.is_some() {
            return (port, token, true);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    (port, token, false)
}

async fn build_runtime(
    state: &AppState,
    config: &AppConfig,
    on_campus: bool,
    tun_available: bool,
) -> CmdResult<BuiltRuntime> {
    let mut warnings = Vec::new();
    let mut assets: Vec<(String, String)> = Vec::new();

    // 订阅来源
    let profile_source =
        if config.subscription.enabled && !config.subscription.url.trim().is_empty() {
            let remote_possible = config.subscription.use_remote_provider
                && (config.subscription.url.starts_with("http://")
                    || config.subscription.url.starts_with("https://"));
            if remote_possible {
                ProfileSource::Remote
            } else if state.paths.profile_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&state.paths.profile_file) {
                    assets.push(("providers/sub.yaml".to_string(), content));
                }
                ProfileSource::Local
            } else {
                // 首次使用本地缓存：同步拉取一次
                match subscription::refresh(state).await {
                    Ok(_) => {
                        if let Ok(content) = std::fs::read_to_string(&state.paths.profile_file) {
                            assets.push(("providers/sub.yaml".to_string(), content));
                        }
                        ProfileSource::Local
                    }
                    Err(err) => {
                        warnings.push(format!("订阅内容不可用：{err}"));
                        ProfileSource::None
                    }
                }
            }
        } else {
            ProfileSource::None
        };

    // SSH 运行凭据
    let ssh_runtime = if config.ssh_proxy.enabled
        && config
            .ssh_proxy
            .routes
            .iter()
            .any(|route| !route.value.trim().is_empty())
    {
        match config.ssh_proxy.auth {
            SshAuth::Password => {
                let password = config
                    .ssh_proxy
                    .password_ref
                    .as_deref()
                    .and_then(|key| state.secrets.get(key))
                    .or_else(|| state.secrets.get(secret_keys::SSH_PASSWORD));
                match password {
                    Some(password) => Some(SshRuntime {
                        password: Some(password),
                        ..Default::default()
                    }),
                    None => {
                        warnings.push("SSH 密码未配置，SSH 出口已跳过".to_string());
                        None
                    }
                }
            }
            SshAuth::PrivateKey => {
                let path = config
                    .ssh_proxy
                    .private_key_path
                    .as_deref()
                    .unwrap_or_default();
                match crate::util::read_small_file(std::path::Path::new(path), 256 * 1024) {
                    Ok(content) => {
                        assets.push(("keys/ssh_key".to_string(), content));
                        let passphrase = config
                            .ssh_proxy
                            .private_key_passphrase_ref
                            .as_deref()
                            .and_then(|key| state.secrets.get(key));
                        Some(SshRuntime {
                            private_key_path: Some("./keys/ssh_key".to_string()),
                            private_key_passphrase: passphrase,
                            ..Default::default()
                        })
                    }
                    Err(err) => {
                        warnings.push(format!("读取 SSH 私钥失败：{err}"));
                        None
                    }
                }
            }
        }
    } else {
        None
    };

    // 校内 VPN 服务器 IP（用于直连规则 + TUN 路由排除）
    let vpn_server_ips = resolve_vpn_server_ips(config).await;
    let controller_secret = config_service::ensure_controller_secret(&state.secrets, config)?;
    let interfaces = labvpn_core::probe::collect_interfaces();
    let local_subnets = labvpn_core::probe::local_subnets(&interfaces);

    let context = RuntimeContext {
        on_campus,
        vpn_configured: config.campus_vpn.enabled,
        vpn_running: false,
        vpn_server_ips,
        vpn_process_name: "fdu-connect".to_string(),
        core_process_name: "mihomo".to_string(),
        local_subnets,
        tun_available,
        controller_secret,
        profile: profile_source,
        ssh_runtime,
        extra_route_exclude: Vec::new(),
        warnings_in: Vec::new(),
    };
    Ok(BuiltRuntime {
        context,
        assets,
        warnings,
    })
}

async fn resolve_vpn_server_ips(config: &AppConfig) -> Vec<String> {
    if let Some(override_ip) = config.campus_vpn.server_ip_override.as_deref() {
        let trimmed = override_ip.trim();
        if trimmed.parse::<std::net::IpAddr>().is_ok() {
            return vec![trimmed.to_string()];
        }
    }
    let host = config.campus_vpn.server.trim();
    if host.is_empty() {
        return Vec::new();
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return vec![host.to_string()];
    }
    match tokio::net::lookup_host((host, config.campus_vpn.port)).await {
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
                    Vec::new()
                } else {
                    vec![ip.to_string()]
                }
            }
            None => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

/// 解析引擎模式（供 UI 展示 helper 可用性）。
pub async fn helper_available(state: &AppState) -> bool {
    let token = match helper::ensure_token(&state.paths.token_file) {
        Ok(token) => token,
        Err(_) => return false,
    };
    helper::ping(helper::default_port(), &token).await.is_some()
}

/// 生成预览（不启动）。
pub async fn preview(state: &AppState) -> CmdResult<crate::dto::PreviewResult> {
    let config = state.config_snapshot();
    let campus = state
        .last_campus()
        .await
        .map(|status| status.on_campus)
        .unwrap_or(false);
    let helper_ready = helper_available(state).await;
    let built = build_runtime(
        state,
        &config,
        campus,
        config.engine.tun.enabled && helper_ready,
    )
    .await?;
    let generated = generator::generate(&config, &built.context)?;
    // 预览内容脱敏：控制端口 secret / SSH 密码与私钥口令
    let mut yaml = generated.yaml.clone();
    if !built.context.controller_secret.is_empty() {
        yaml = yaml.replace(&built.context.controller_secret, "***REDACTED***");
    }
    if let Some(ssh) = &built.context.ssh_runtime {
        if let Some(password) = &ssh.password {
            yaml = yaml.replace(password, "***REDACTED***");
        }
        if let Some(passphrase) = &ssh.private_key_passphrase {
            yaml = yaml.replace(passphrase, "***REDACTED***");
        }
    }
    let mut warnings = built.warnings;
    warnings.extend(generated.warnings.clone());
    warnings.sort();
    warnings.dedup();
    Ok(crate::dto::PreviewResult {
        yaml,
        warnings,
        stats: generated.stats,
        rules: generated.plan.rules,
    })
}

/// 构建当前规则计划（用于分流诊断）。
pub async fn rule_plan(state: &AppState) -> CmdResult<labvpn_core::rules::RulePlan> {
    let config = state.config_snapshot();
    let campus = state
        .last_campus()
        .await
        .map(|status| status.on_campus)
        .unwrap_or(false);
    let helper_ready = helper_available(state).await;
    let built = build_runtime(
        state,
        &config,
        campus,
        config.engine.tun.enabled && helper_ready,
    )
    .await?;
    let generated = generator::generate(&config, &built.context)?;
    Ok(generated.plan)
}

/// 端口被其他程序占用时自动切换到邻近空闲端口，并给出提示。
fn choose_ports(config: &mut AppConfig, warnings: &mut Vec<String>) {
    config.engine.mixed_port = pick_free_port(config.engine.mixed_port, "混合代理端口", warnings);
    config.engine.controller_port =
        pick_free_port(config.engine.controller_port, "控制端口", warnings);
}

fn pick_free_port(preferred: u16, label: &str, warnings: &mut Vec<String>) -> u16 {
    if !platform::port_in_use(preferred) {
        return preferred;
    }
    let upper = preferred.saturating_add(50).min(u16::MAX - 1);
    for candidate in preferred.saturating_add(1)..=upper {
        if candidate > 1024 && !platform::port_in_use(candidate) {
            warnings.push(format!(
                "{label} {preferred} 已被其他程序占用，本次连接自动使用 {candidate}（可在设置中修改）"
            ));
            return candidate;
        }
    }
    warnings.push(format!(
        "{label} {preferred} 被占用且附近没有可用端口，请手动修改端口"
    ));
    preferred
}
