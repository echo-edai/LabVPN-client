//! 平台差异能力：系统代理、默认网卡、Wi-Fi SSID、开机自启。
//!
//! 约定：业务层只调用本模块的跨平台函数，平台细节收敛在各自子模块。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// 默认出口网卡名。
pub fn default_interface() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::default_interface()
    }
    #[cfg(target_os = "linux")]
    {
        linux::default_interface()
    }
    #[cfg(windows)]
    {
        windows::default_interface()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        None
    }
}

/// 当前 Wi-Fi SSID。
pub fn wifi_ssid() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::wifi_ssid()
    }
    #[cfg(target_os = "linux")]
    {
        linux::wifi_ssid()
    }
    #[cfg(windows)]
    {
        windows::wifi_ssid()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        None
    }
}

/// 设置/取消系统代理（指向本机混合端口）。
pub fn set_system_proxy(port: u16, enable: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::set_system_proxy(port, enable)
    }
    #[cfg(target_os = "linux")]
    {
        linux::set_system_proxy(port, enable)
    }
    #[cfg(windows)]
    {
        windows::set_system_proxy(port, enable)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (port, enable);
        Err("当前平台不支持自动设置系统代理".to_string())
    }
}

/// 读取当前系统代理端口（无则 None）。
pub fn system_proxy_port() -> Option<u16> {
    #[cfg(target_os = "macos")]
    {
        macos::system_proxy_port()
    }
    #[cfg(target_os = "linux")]
    {
        linux::system_proxy_port()
    }
    #[cfg(windows)]
    {
        windows::system_proxy_port()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        None
    }
}

/// 是否已开启开机自启。
pub fn is_autostart_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// 设置开机自启。
pub fn set_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager
            .enable()
            .map_err(|err| format!("开启开机自启失败: {err}"))
    } else {
        manager
            .disable()
            .map_err(|err| format!("关闭开机自启失败: {err}"))
    }
}

/// 端口是否可绑定（用于冲突检测）。
pub fn port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// 端口是否已被监听。
pub fn port_in_use(port: u16) -> bool {
    !port_available(port)
}

/// 创建子进程命令：Windows 下隐藏控制台窗口（避免 GUI 应用闪现黑框/抢焦点）。
pub(crate) fn hidden_command<P: AsRef<std::ffi::OsStr>>(program: P) -> std::process::Command {
    #[allow(unused_mut)]
    let mut command = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// 执行命令并返回 stdout（失败返回 None）。
#[allow(dead_code)]
pub(crate) fn run_command(program: &str, args: &[&str]) -> Option<String> {
    let output = hidden_command(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// 用系统默认程序打开路径。
pub fn open_path(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    let program = "open";
    hidden_command(program)
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|err| format!("打开 {} 失败: {err}", path.display()))
}
