//! 连接与引擎控制命令。

use tauri::State;

use labvpn_core::config::NodeSelectionMode;
use labvpn_core::probe::CampusStatus;
use labvpn_core::rules::names;

use crate::dto::{ConnectOutcome, EngineStatus, OperationResult, ProxyGroupInfo};
use crate::error::{CmdResult, CommandError};
use crate::services::{campus, orchestrator};
use crate::state::AppState;

fn outcome(result: orchestrator::ConnectResult, campus: Option<CampusStatus>) -> ConnectOutcome {
    ConnectOutcome {
        engine: result.engine,
        vpn: result.vpn,
        campus,
        mode: result.mode,
        warnings: result.warnings,
    }
}

/// 连接（完整编排）。
#[tauri::command]
pub async fn start_engine(state: State<'_, AppState>) -> CmdResult<ConnectOutcome> {
    let campus = campus::current(state.inner()).await;
    let result = orchestrator::connect(state.inner()).await?;
    Ok(outcome(result, Some(campus)))
}

/// 断开连接。
#[tauri::command]
pub async fn stop_engine(state: State<'_, AppState>) -> CmdResult<EngineStatus> {
    orchestrator::disconnect(state.inner()).await
}

/// 重启连接。
#[tauri::command]
pub async fn restart_engine(state: State<'_, AppState>) -> CmdResult<ConnectOutcome> {
    let _ = orchestrator::disconnect(state.inner()).await;
    let campus = campus::current(state.inner()).await;
    let result = orchestrator::connect(state.inner()).await?;
    Ok(outcome(result, Some(campus)))
}

/// 应用配置变更（运行中则重启核心）。
#[tauri::command]
pub async fn apply_config(state: State<'_, AppState>) -> CmdResult<Option<ConnectOutcome>> {
    let campus = campus::current(state.inner()).await;
    match orchestrator::apply_config(state.inner()).await? {
        Some(result) => Ok(Some(outcome(result, Some(campus)))),
        None => Ok(None),
    }
}

/// 引擎状态。
#[tauri::command]
pub async fn engine_status(state: State<'_, AppState>) -> CmdResult<EngineStatus> {
    Ok(state.engine.status().await)
}

/// 代理组列表。
#[tauri::command]
pub async fn get_proxy_groups(state: State<'_, AppState>) -> CmdResult<Vec<ProxyGroupInfo>> {
    state.engine.proxy_groups().await
}

/// 切换代理组。
#[tauri::command]
pub async fn select_proxy(
    state: State<'_, AppState>,
    group: String,
    name: String,
) -> CmdResult<OperationResult> {
    state.engine.select_proxy(&group, &name).await?;
    state
        .logs
        .info("engine", format!("切换代理组：{group} -> {name}"));
    Ok(OperationResult::ok(format!("已切换到 {name}")))
}

/// 延迟测试。
#[tauri::command]
pub async fn test_proxy_delay(state: State<'_, AppState>, name: String) -> CmdResult<u32> {
    state.engine.delay_test(&name).await
}

/// 校园网状态（refresh=true 强制重新检测）。
#[tauri::command]
pub async fn get_campus_status(
    state: State<'_, AppState>,
    refresh: Option<bool>,
) -> CmdResult<CampusStatus> {
    if refresh.unwrap_or(false) {
        let status = campus::detect(state.inner()).await;
        let _ = campus::update(state.inner(), status.clone()).await;
        Ok(status)
    } else {
        Ok(campus::current(state.inner()).await)
    }
}

/// 设置校园网手动覆盖（None=自动）。
#[tauri::command]
pub async fn set_campus_override(
    state: State<'_, AppState>,
    value: Option<bool>,
) -> CmdResult<CampusStatus> {
    state.set_campus_override(value);
    let status = campus::detect(state.inner()).await;
    let _ = campus::update(state.inner(), status.clone()).await;
    if state.engine.is_running().await {
        orchestrator::on_campus_changed(&state.app).await;
    }
    Ok(status)
}

/// 校园网探测所需信息（用于设置页展示）。
#[tauri::command]
pub fn campus_override(state: State<'_, AppState>) -> Option<bool> {
    state.campus_override()
}

/// 手动触发一次完整校园网检测并联动。
#[tauri::command]
pub async fn refresh_campus(state: State<'_, AppState>) -> CmdResult<CampusStatus> {
    let status = campus::detect(state.inner()).await;
    let changed = campus::update(state.inner(), status.clone()).await;
    if changed {
        orchestrator::on_campus_changed(&state.app).await;
    }
    Ok(status)
}

/// 订阅节点候选列表（读取本地缓存解析，用于手动固定节点）。
#[tauri::command]
pub fn get_node_candidates(state: State<'_, AppState>) -> CmdResult<Vec<String>> {
    let path = &state.paths.profile_file;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    match labvpn_core::subscription::parse_clash(&content) {
        Ok(profile) => Ok(profile.proxy_names),
        Err(_) => Ok(Vec::new()),
    }
}

/// 手动固定节点：持久化到配置并立即热切换（无需重启核心）。
#[tauri::command]
pub async fn select_node(state: State<'_, AppState>, name: String) -> CmdResult<OperationResult> {
    let selected = name.trim().to_string();
    if selected.is_empty() {
        return Err(CommandError::new("engine", "节点名称不能为空"));
    }
    let mut config = state.config_snapshot();
    config.node_selection.mode = NodeSelectionMode::Manual;
    config.node_selection.selected = Some(selected.clone());
    state.save_config(config)?;
    if state.engine.is_running().await {
        state
            .engine
            .select_proxy(names::GROUP_PROXY, &selected)
            .await?;
    }
    state
        .logs
        .info("engine", format!("代理节点已固定为：{selected}"));
    Ok(OperationResult::ok(format!("已固定节点 {selected}")))
}
