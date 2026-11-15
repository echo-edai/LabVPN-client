//! 校内 VPN 相关命令。

use tauri::State;

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
    let app = state.app.clone();
    let result = tokio::task::spawn_blocking(move || vpn::detect_existing(&app, &campus, &paths))
        .await
        .unwrap_or_default();
    Ok(result)
}

/// 启动校内 VPN（独立于代理核心）。
#[tauri::command]
pub async fn start_vpn(state: State<'_, AppState>) -> CmdResult<VpnStatus> {
    orchestrator::start_vpn(state.inner()).await
}

/// 停止校内 VPN。
#[tauri::command]
pub async fn stop_vpn(state: State<'_, AppState>) -> CmdResult<VpnStatus> {
    Ok(orchestrator::stop_vpn(state.inner()).await)
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

/// SSH 数据面探测（欢迎行，不校验凭据）。
#[tauri::command]
pub async fn test_ssh(host: String, port: u16) -> CmdResult<SshTestResult> {
    let host = host.trim().to_string();
    if host.is_empty() {
        return Err(CommandError::new("ssh", "请填写 SSH 主机地址"));
    }
    if port == 0 {
        return Err(CommandError::new("ssh", "SSH 端口不能为 0"));
    }
    Ok(crate::services::ssh_probe::test(&host, port).await)
}

/// 解析并分析校内 VPN 日志（会话 / KeepAlive / 隧道中断 / 拨号失败）。
#[tauri::command]
pub async fn analyze_vpn_logs(
    state: State<'_, AppState>,
) -> CmdResult<labvpn_core::vpn_log::VpnLogAnalysis> {
    let path = state.paths.app_log_file.clone();
    let text = tokio::task::spawn_blocking(move || std::fs::read_to_string(path))
        .await
        .map_err(|err| CommandError::new("vpn", format!("读取日志任务失败: {err}")))?
        .map_err(|err| CommandError::new("vpn", format!("读取日志文件失败: {err}")))?;
    // 只看最近一次连接，避免多天日志混在一起干扰判断
    let slice = labvpn_core::vpn_log::latest_session_slice(&text);
    Ok(labvpn_core::vpn_log::analyze_log(slice))
}
