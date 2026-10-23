//! 更新检查相关命令。

use tauri::State;

use crate::dto::ConfigEnvelope;
use crate::error::CmdResult;
use crate::services::update as update_service;
use crate::state::AppState;

/// 检查 GitHub Release 是否有比当前版本更新的发布。
#[tauri::command]
pub async fn check_update(state: State<'_, AppState>) -> CmdResult<update_service::UpdateInfo> {
    let info = update_service::check(env!("CARGO_PKG_VERSION")).await?;
    if info.has_update {
        state.logs.info(
            "update",
            format!("发现新版本 v{}（当前 v{}）", info.latest, info.current),
        );
    } else {
        state
            .logs
            .info("update", format!("已是最新版本 v{}", info.current));
    }
    Ok(info)
}

/// 记住「忽略此版本」，之后不再主动弹更新提示。
#[tauri::command]
pub fn skip_update_version(
    state: State<'_, AppState>,
    version: String,
) -> CmdResult<ConfigEnvelope> {
    let version = version.trim().to_string();
    let mut config = state.config_snapshot();
    config.update.skipped_version = if version.is_empty() {
        None
    } else {
        Some(version.clone())
    };
    state.save_config(config)?;
    if version.is_empty() {
        state.logs.info("update", "已恢复更新提示");
    } else {
        state
            .logs
            .info("update", format!("已忽略版本 v{version} 的更新提示"));
    }
    Ok(state.config_envelope())
}
