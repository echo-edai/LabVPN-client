//! 订阅相关命令。

use tauri::State;

use crate::dto::{ImportSubscriptionRequest, SubscriptionInfo};
use crate::error::CmdResult;
use crate::services::subscription;
use crate::state::AppState;

/// 导入（或更新）订阅。
#[tauri::command]
pub async fn import_subscription(
    state: State<'_, AppState>,
    request: ImportSubscriptionRequest,
) -> CmdResult<SubscriptionInfo> {
    subscription::import(
        state.inner(),
        &request.url,
        request.name.as_deref(),
        request.persist,
    )
    .await
}

/// 测试订阅地址（不写入配置）。
#[tauri::command]
pub async fn test_subscription(
    state: State<'_, AppState>,
    url: String,
) -> CmdResult<SubscriptionInfo> {
    subscription::test_url(state.inner(), &url).await
}

/// 刷新当前订阅。
#[tauri::command]
pub async fn refresh_subscription(state: State<'_, AppState>) -> CmdResult<SubscriptionInfo> {
    subscription::refresh(state.inner()).await
}

/// 当前订阅展示信息。
#[tauri::command]
pub fn get_subscription(state: State<'_, AppState>) -> SubscriptionInfo {
    let config = state.config_snapshot();
    subscription::info_from_config(&state.paths, &config.subscription)
}
