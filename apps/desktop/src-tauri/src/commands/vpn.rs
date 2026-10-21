//! 校内 VPN 相关命令。

use tauri::State;

use std::time::{Duration, Instant};

use crate::dto::{ExistingVpn, SshTestResult, VpnStatus, VpnTestResult};
use crate::error::{CmdResult, CommandError};
use crate::services::{orchestrator, vpn};
use crate::state::AppState;

/// 探测本机已有的 fdu-connect 安装/运行状态。
#[tauri::command]
pub async fn detect_vpn_binary(state: State<'_, AppState>) -> CmdResult<ExistingVpn> {
    let config = state.config_snapshot();
    let paths = state.paths.clone();
    let campus = config.campus_vpn.clone();
    let result = tokio::task::spawn_blocking(move || vpn::detect_existing(&campus, &paths))
        .await
        .unwrap_or_default();
    Ok(result)
}

/// 启动校内 VPN（独立于代理核心）。
#[tauri::command]
pub async fn start_vpn(state: State<'_, AppState>) -> CmdResult<VpnStatus> {
    let config = state.config_snapshot();
    let status = state
        .vpn
        .start(&config.campus_vpn, &state.secrets, &state.paths)
        .await?;
    if state.engine.is_running().await {
        let _ = orchestrator::apply_config(state.inner()).await;
    }
    Ok(status)
}

/// 停止校内 VPN。
#[tauri::command]
pub async fn stop_vpn(state: State<'_, AppState>) -> CmdResult<VpnStatus> {
    let config = state.config_snapshot();
    Ok(state.vpn.stop(&config.campus_vpn).await)
}

/// 当前校内 VPN 状态。
#[tauri::command]
pub async fn vpn_status(state: State<'_, AppState>) -> CmdResult<VpnStatus> {
    let config = state.config_snapshot();
    Ok(state.vpn.status(&config.campus_vpn).await)
}

/// 登录测试（使用临时端口，不影响正式连接）。
#[tauri::command]
pub async fn test_vpn_connection(state: State<'_, AppState>) -> CmdResult<VpnTestResult> {
    let config = state.config_snapshot();
    state
        .vpn
        .test(&config.campus_vpn, &state.secrets, &state.paths)
        .await
}

/// SSH 端口连通性测试（TCP 握手，不校验凭据）。
#[tauri::command]
pub async fn test_ssh(host: String, port: u16) -> CmdResult<SshTestResult> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err(CommandError::new("ssh", "请填写 SSH 主机地址"));
    }
    if port == 0 {
        return Err(CommandError::new("ssh", "SSH 端口不能为 0"));
    }
    let start = Instant::now();
    let timeout = Duration::from_secs(6);
    match tokio::time::timeout(
        timeout,
        tokio::net::TcpStream::connect((host.as_str(), port)),
    )
    .await
    {
        Ok(Ok(_stream)) => Ok(SshTestResult {
            ok: true,
            message: format!("TCP 连接成功（{host}:{port}），凭据将在应用配置后由 mihomo 校验"),
            elapsed_ms: start.elapsed().as_millis() as u64,
        }),
        Ok(Err(err)) => Ok(SshTestResult {
            ok: false,
            message: format!("连接失败：{err}"),
            elapsed_ms: start.elapsed().as_millis() as u64,
        }),
        Err(_) => Ok(SshTestResult {
            ok: false,
            message: format!("连接超时（{}s）", timeout.as_secs()),
            elapsed_ms: timeout.as_millis() as u64,
        }),
    }
}
