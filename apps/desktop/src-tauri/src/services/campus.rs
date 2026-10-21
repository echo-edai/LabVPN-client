//! 校园网检测服务：调用核心探测 + 平台信息 + 定时轮询。

use std::time::Duration;

use labvpn_core::probe::{self, CampusStatus, ProbeInput};
use tauri::{AppHandle, Manager};

use crate::events;
use crate::platform;
use crate::state::AppState;

/// 异步执行一次检测。
pub async fn detect(state: &AppState) -> CampusStatus {
    let config = state.config_snapshot();
    let input = ProbeInput {
        targets: config.campus_vpn.probe_targets.clone(),
        campus_cidrs: config.campus_vpn.campus_cidrs.clone(),
        interface: platform::default_interface(),
        ssid: platform::wifi_ssid(),
    };
    let status = tokio::task::spawn_blocking(move || probe::detect(&input))
        .await
        .unwrap_or_else(|err| {
            log::warn!("校园网探测任务失败: {err}");
            fallback_status()
        });
    status.apply_manual_override(state.campus_override())
}

/// 更新并广播检测结果；返回是否发生变化（用于触发联动）。
pub async fn update(state: &AppState, status: CampusStatus) -> bool {
    let changed = {
        let mut campus = state.campus.lock().await;
        let changed = campus
            .last
            .as_ref()
            .map(|previous| previous.on_campus != status.on_campus)
            .unwrap_or(true);
        campus.last = Some(status.clone());
        changed
    };
    events::emit(&state.app, events::CAMPUS_STATUS, status);
    changed
}

/// 启动定时轮询（每 60 秒；网络变化时由前端/系统事件触发即时检测）。
pub fn spawn_polling(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let state = app.state::<AppState>();
            if state.config_snapshot().campus_vpn.auto_detect_campus {
                let status = detect(&state).await;
                let changed = update(&state, status).await;
                if changed {
                    crate::services::orchestrator::on_campus_changed(&app).await;
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

/// 获取最近一次检测结果（无则实时检测一次）。
pub async fn current(state: &AppState) -> CampusStatus {
    if let Some(status) = state.campus.lock().await.last.clone() {
        return status;
    }
    let status = detect(state).await;
    let _ = update(state, status.clone()).await;
    status
}

/// 兜底状态（探测任务 panic 时）。
pub fn fallback_status() -> CampusStatus {
    CampusStatus {
        on_campus: false,
        confidence: 0.0,
        signals: Vec::new(),
        checked_at: chrono::Utc::now(),
        interface: None,
        manual_override: None,
    }
}
