//! 系统环境、helper、日志等命令。

use std::path::PathBuf;

use chrono::Local;
use tauri::State;

use crate::dto::{AppSnapshot, LogEntry, OperationResult, SystemInfo};
use crate::error::{CmdResult, CommandError};
use crate::platform;
use crate::services::{campus, core_bin, helper, orchestrator, subscription, vpn};
use crate::state::AppState;

/// 应用快照（首屏一次拉取）。
#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>) -> CmdResult<AppSnapshot> {
    let config = state.config_snapshot();
    let engine = state.engine.status().await;
    let vpn_status = state.vpn.status(&config.campus_vpn).await;
    let campus_status = state.last_campus().await;
    let subscription_info = subscription::info_from_config(&state.paths, &config.subscription);
    let mut core = state.core_info();
    if core.version.is_none() {
        core = state.refresh_core_info();
    }
    let system = system_info(&state).await;
    Ok(AppSnapshot {
        config: state.config_envelope(),
        engine,
        vpn: vpn_status,
        campus: campus_status,
        subscription: subscription_info,
        core,
        system,
        presets: labvpn_core::rules::presets::all(),
        logs: state.logs.recent(120),
    })
}

/// 系统环境信息。
#[tauri::command]
pub async fn get_system_info(state: State<'_, AppState>) -> CmdResult<SystemInfo> {
    Ok(system_info(&state).await)
}

async fn system_info(state: &AppState) -> SystemInfo {
    let config = state.config_snapshot();
    let port = helper::default_port();
    let token = helper::ensure_token(&state.paths.token_file).ok();
    let (helper_running, helper_version) = match token {
        Some(token) => match helper::ping(port, &token).await {
            Some(response) => (true, Some(response.helper_version)),
            None => (false, None),
        },
        None => (false, None),
    };
    let helper_installed = helper::is_installed() || helper_running;

    let interface = tokio::task::spawn_blocking(platform::default_interface)
        .await
        .unwrap_or(None);
    let ssid = tokio::task::spawn_blocking(platform::wifi_ssid)
        .await
        .unwrap_or(None);
    let existing_vpn = {
        let paths = state.paths.clone();
        let campus = config.campus_vpn.clone();
        let app = state.app.clone();
        tokio::task::spawn_blocking(move || vpn::detect_existing(&app, &campus, &paths))
            .await
            .unwrap_or_default()
    };

    let mut port_conflicts = Vec::new();
    let engine_running = state.engine.is_running().await;
    if !engine_running {
        for port in [config.engine.mixed_port, config.engine.controller_port] {
            if platform::port_in_use(port) {
                port_conflicts.push(format!("127.0.0.1:{port} 已被占用"));
            }
        }
    }
    if platform::port_in_use(config.campus_vpn.socks_port) && !existing_vpn.running {
        port_conflicts.push(format!(
            "校内 VPN 端口 127.0.0.1:{} 已被占用",
            config.campus_vpn.socks_port
        ));
    }

    SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        helper_installed,
        helper_running,
        helper_version,
        default_interface: interface,
        wifi_ssid: ssid,
        system_proxy_enabled: platform::system_proxy_port() == Some(config.engine.mixed_port),
        existing_vpn,
        port_conflicts,
        campus_override: state.campus_override(),
        core_version: state.core_info().version,
        app_uptime_secs: state.uptime_secs(),
    }
}

/// 安装特权 helper（弹出系统授权）。
#[tauri::command]
pub async fn install_helper(state: State<'_, AppState>) -> CmdResult<OperationResult> {
    let helper_path = core_bin::resolve_helper(&state.app, &state.paths).ok_or_else(|| {
        CommandError::new("helper", "找不到 labvpn-helper，请先运行 sidecars 脚本")
    })?;
    let core = core_bin::resolve_core(&state.app, &state.paths)
        .path
        .ok_or_else(|| CommandError::new("helper", "找不到 mihomo 核心"))?;
    let token = helper::ensure_token(&state.paths.token_file)?;
    let port = helper::default_port();
    let workdir = state.paths.helper_workdir.clone();
    let token_file = state.paths.token_file.clone();
    let result = tokio::task::spawn_blocking(move || {
        helper::install(&helper_path, &core, &token_file, &workdir, port)
    })
    .await
    .map_err(|err| CommandError::new("helper", format!("安装任务失败: {err}")))??;
    // 等待 helper 就绪
    for _ in 0..20 {
        if helper::ping(port, &token).await.is_some() {
            state.logs.info("helper", "特权 helper 已就绪");
            return Ok(result);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Ok(OperationResult {
        ok: true,
        message: "安装请求已完成，但 helper 尚未就绪，请稍后在设置页重试连接".to_string(),
        detail: None,
    })
}

/// 卸载特权 helper。
#[tauri::command]
pub async fn uninstall_helper(state: State<'_, AppState>) -> CmdResult<OperationResult> {
    let helper_path = core_bin::resolve_helper(&state.app, &state.paths)
        .ok_or_else(|| CommandError::new("helper", "找不到 labvpn-helper"))?;
    let result = tokio::task::spawn_blocking(move || helper::uninstall(&helper_path))
        .await
        .map_err(|err| CommandError::new("helper", format!("卸载任务失败: {err}")))??;
    Ok(result)
}

/// 以管理员权限临时启动 helper（Windows / 未安装服务时）。
#[tauri::command]
pub async fn start_helper_elevated(state: State<'_, AppState>) -> CmdResult<OperationResult> {
    let helper_path = core_bin::resolve_helper(&state.app, &state.paths)
        .ok_or_else(|| CommandError::new("helper", "找不到 labvpn-helper"))?;
    let core = core_bin::resolve_core(&state.app, &state.paths)
        .path
        .ok_or_else(|| CommandError::new("helper", "找不到 mihomo 核心"))?;
    let token = helper::ensure_token(&state.paths.token_file)?;
    let port = helper::default_port();
    let workdir = state.paths.helper_workdir.clone();
    let token_file = state.paths.token_file.clone();
    let result = tokio::task::spawn_blocking(move || {
        helper::start_elevated(&helper_path, &token_file, &workdir, &core, port)
    })
    .await
    .map_err(|err| CommandError::new("helper", format!("启动任务失败: {err}")))??;
    for _ in 0..20 {
        if helper::ping(port, &token).await.is_some() {
            return Ok(result);
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Ok(result)
}

/// 设置/关闭系统代理。
#[tauri::command]
pub fn set_system_proxy(state: State<'_, AppState>, enabled: bool) -> CmdResult<OperationResult> {
    let config = state.config_snapshot();
    match platform::set_system_proxy(config.engine.mixed_port, enabled) {
        Ok(()) => Ok(OperationResult::ok(if enabled {
            "系统代理已开启"
        } else {
            "系统代理已关闭"
        })),
        Err(err) => Err(CommandError::new("systemProxy", err)),
    }
}

/// 读取开机自启状态。
#[tauri::command]
pub fn get_autostart(state: State<'_, AppState>) -> bool {
    platform::is_autostart_enabled(&state.app)
}

/// 设置开机自启。
#[tauri::command]
pub fn set_autostart(state: State<'_, AppState>, enabled: bool) -> CmdResult<OperationResult> {
    match platform::set_autostart(&state.app, enabled) {
        Ok(()) => Ok(OperationResult::ok(if enabled {
            "已开启开机自启"
        } else {
            "已关闭开机自启"
        })),
        Err(err) => Err(CommandError::new("autostart", err)),
    }
}

/// 读取最近日志。
#[tauri::command]
pub fn get_logs(state: State<'_, AppState>, limit: Option<usize>) -> Vec<LogEntry> {
    state.logs.recent(limit.unwrap_or(200))
}

/// 清空内存日志。
#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>) -> OperationResult {
    state.logs.clear();
    OperationResult::ok("日志已清空（文件保留）")
}

/// 打开数据目录/日志目录。
#[tauri::command]
pub fn open_path(state: State<'_, AppState>, target: String) -> CmdResult<OperationResult> {
    let path: PathBuf = match target.as_str() {
        "data" => state.paths.data_dir.clone(),
        "logs" => state.paths.logs_dir.clone(),
        "core" => state.paths.core_dir.clone(),
        other => PathBuf::from(other),
    };
    platform::open_path(&path).map_err(|err| CommandError::new("open", err))?;
    Ok(OperationResult::ok(format!("已打开 {}", path.display())))
}

/// 导出诊断包（脱敏配置 + 日志 + 环境信息），返回文件路径。
#[tauri::command]
pub async fn export_diagnostics(state: State<'_, AppState>) -> CmdResult<OperationResult> {
    let config = state.config_snapshot();
    let mut redacted = config.clone();
    redacted.subscription.url = crate::util::redact_url(&redacted.subscription.url);
    let system = system_info(&state).await;
    let payload = serde_json::json!({
        "generatedAt": Local::now().to_rfc3339(),
        "appVersion": env!("CARGO_PKG_VERSION"),
        "configVersion": config.version,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "config": redacted,
        "system": system,
        "engine": state.engine.status().await,
        "vpn": state.vpn.status(&config.campus_vpn).await,
        "campus": state.last_campus().await,
        "logs": state.logs.recent(300),
    });
    let file_name = format!("diagnostics-{}.json", Local::now().format("%Y%m%d-%H%M%S"));
    let path = state.paths.logs_dir.join(file_name);
    let text = serde_json::to_string_pretty(&payload)?;
    std::fs::write(&path, text)?;
    state
        .logs
        .info("app", format!("诊断包已导出：{}", path.display()));
    Ok(OperationResult {
        ok: true,
        message: "诊断包已导出".to_string(),
        detail: Some(path.display().to_string()),
    })
}

/// 触发订阅刷新（托盘/定时任务使用）。
#[tauri::command]
pub async fn sync_subscription(state: State<'_, AppState>) -> CmdResult<OperationResult> {
    match subscription::refresh(&state).await {
        Ok(info) => Ok(OperationResult::ok(format!(
            "订阅已更新：{} 个节点",
            info.proxy_count
        ))),
        Err(err) => Err(err),
    }
}

/// 立即刷新校园网检测并联动。
#[tauri::command]
pub async fn detect_campus_now(
    state: State<'_, AppState>,
) -> CmdResult<crate::dto::OperationResult> {
    let status = campus::detect(state.inner()).await;
    let changed = campus::update(state.inner(), status).await;
    if changed {
        orchestrator::on_campus_changed(&state.app).await;
    }
    Ok(OperationResult::ok("已刷新校园网状态"))
}
