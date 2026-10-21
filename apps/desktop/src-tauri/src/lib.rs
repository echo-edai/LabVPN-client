//! LabVPN 桌面应用入口。
//!
//! 组装 Tauri 插件、全局状态、系统托盘、后台任务与全部命令。

mod commands;
mod dto;
mod error;
mod events;
mod paths;
mod platform;
mod services;
mod state;
mod util;

use std::sync::atomic::AtomicBool;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

/// 退出清理只执行一次。
static EXIT_CLEANED: AtomicBool = AtomicBool::new(false);

use crate::state::AppState;

/// 启动 LabVPN。
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            let state = AppState::new(&handle)?;
            app.manage(state);
            setup_tray(app)?;
            services::campus::spawn_polling(handle.clone());
            services::stats::spawn(handle.clone());
            spawn_startup_tasks(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                let tray_enabled = state.config_snapshot().ui.tray;
                if tray_enabled {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::get_config,
            commands::config::save_config,
            commands::config::reset_config,
            commands::config::export_config,
            commands::config::import_config,
            commands::config::validate_config,
            commands::config::preview_config,
            commands::config::get_presets,
            commands::config::validate_rules,
            commands::config::diagnose_target,
            commands::connect::start_engine,
            commands::connect::stop_engine,
            commands::connect::restart_engine,
            commands::connect::apply_config,
            commands::connect::engine_status,
            commands::connect::get_proxy_groups,
            commands::connect::select_proxy,
            commands::connect::get_node_candidates,
            commands::connect::select_node,
            commands::connect::test_proxy_delay,
            commands::analytics::get_analytics,
            commands::analytics::set_ai_domains,
            commands::connect::get_campus_status,
            commands::connect::set_campus_override,
            commands::connect::campus_override,
            commands::connect::refresh_campus,
            commands::subscription::import_subscription,
            commands::subscription::test_subscription,
            commands::subscription::refresh_subscription,
            commands::subscription::get_subscription,
            commands::vpn::detect_vpn_binary,
            commands::vpn::start_vpn,
            commands::vpn::stop_vpn,
            commands::vpn::vpn_status,
            commands::vpn::test_vpn_connection,
            commands::vpn::test_ssh,
            commands::system::get_snapshot,
            commands::system::get_system_info,
            commands::system::install_helper,
            commands::system::uninstall_helper,
            commands::system::start_helper_elevated,
            commands::system::set_system_proxy,
            commands::system::get_autostart,
            commands::system::set_autostart,
            commands::system::get_logs,
            commands::system::clear_logs,
            commands::system::open_path,
            commands::system::export_diagnostics,
            commands::system::sync_subscription,
            commands::system::detect_campus_now,
        ])
        .build(tauri::generate_context!())
        .expect("构建 LabVPN 失败")
        .run(|app, event| {
            // 退出时务必断开核心并关闭系统代理，避免留下指向已停止端口的代理设置；
            // 同时覆盖托盘「退出」（app.exit）与窗口全部关闭两种路径，只执行一次。
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                if EXIT_CLEANED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if let Some(state) = app.try_state::<AppState>() {
                    tauri::async_runtime::block_on(async {
                        let _ = services::orchestrator::disconnect(state.inner()).await;
                        // Windows 等「提权临时启动」模式：退出时关闭 helper
                        if !services::helper::is_installed() {
                            if let Ok(token) =
                                services::helper::ensure_token(&state.paths.token_file)
                            {
                                let _ = services::helper::shutdown(
                                    services::helper::default_port(),
                                    &token,
                                )
                                .await;
                            }
                        }
                    });
                }
            }
        });
}

/// 系统托盘。
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示 LabVPN", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "连接 / 断开", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "打开设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 LabVPN", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &settings, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "settings" => {
                show_main_window(app);
                let _ = app.emit_to("main", "navigate://settings", ());
            }
            "toggle" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state = app.state::<AppState>();
                    if state.engine.is_running().await {
                        let _ = services::orchestrator::disconnect(&state).await;
                    } else if let Err(err) = services::orchestrator::connect(&state).await {
                        state.logs.error("tray", format!("托盘连接失败：{err}"));
                    }
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    let _tray = builder.build(app)?;
    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// 启动后的后台任务：自动连接、订阅刷新。
fn spawn_startup_tasks(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        let state = app.state::<AppState>();
        let config = state.config_snapshot();

        if config.subscription.enabled
            && config.subscription.update_interval_hours > 0
            && !config.subscription.url.trim().is_empty()
        {
            let stale = config
                .subscription
                .last_updated
                .map(|time| {
                    let hours = chrono::Utc::now().signed_duration_since(time).num_hours();
                    hours >= i64::from(config.subscription.update_interval_hours)
                })
                .unwrap_or(true);
            if stale || !state.paths.profile_file.exists() {
                match services::subscription::refresh(&state).await {
                    Ok(info) => state.logs.info(
                        "subscription",
                        format!("启动时刷新订阅：{} 个节点", info.proxy_count),
                    ),
                    Err(err) => state
                        .logs
                        .warn("subscription", format!("启动刷新订阅失败：{err}")),
                }
            }
        }

        if config.engine.connect_on_launch {
            if let Err(err) = services::orchestrator::connect(&state).await {
                state.logs.error("startup", format!("自动连接失败：{err}"));
            }
        }
    });
}
