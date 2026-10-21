//! 事件名常量与发送辅助。

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// 引擎状态变化
pub const ENGINE_STATUS: &str = "engine://status";
/// 校内 VPN 状态变化
pub const VPN_STATUS: &str = "vpn://status";
/// 校园网检测结果变化
pub const CAMPUS_STATUS: &str = "campus://status";
/// 实时流量
pub const TRAFFIC: &str = "traffic://update";
/// 发送事件（失败仅记录日志，不影响主流程）。
pub fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(err) = app.emit(event, payload) {
        log::warn!("发送事件 {event} 失败: {err}");
    }
}
