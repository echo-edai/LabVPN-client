//! 校内 VPN 相关命令。

use tauri::State;

use crate::dto::{ExistingVpn, VpnStatus, VpnTestResult};
use crate::error::CmdResult;
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
