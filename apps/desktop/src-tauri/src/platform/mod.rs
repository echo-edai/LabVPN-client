//! 平台差异能力：系统代理、默认网卡、Wi-Fi SSID、开机自启。
//!
//! 约定：业务层只调用本模块的跨平台函数，平台细节收敛在各自子模块。

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

mod startup;
pub use startup::{
    can_disable_proxy_startup, disable_proxy_startup, open_startup_settings, startup_items,
};

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

#[cfg(target_os = "linux")]
use linux as native;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(windows)]
use windows as native;

pub use native::{capture_proxy_settings, proxy_summary, read_proxy_setting, write_proxy_setting};

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

/// 自动代理可能覆盖应用预期的出口。
pub fn proxy_warnings() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        macos::proxy_warnings()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
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

/// 是否接受 TCP 连接；绑定权限和连接回收状态不等于活跃监听。
pub fn port_in_use(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(200),
    )
    .is_ok()
}

/// 普通权限核心的绑定探测；Unix 允许复用回收中的地址，仍须确认能够监听。
pub(crate) fn probe_local_listener(port: u16) -> std::io::Result<()> {
    let socket = tokio::net::TcpSocket::new_v4()?;
    #[cfg(unix)]
    socket.set_reuseaddr(true)?;
    socket.bind(std::net::SocketAddr::from(([127, 0, 0, 1], port)))?;
    socket.listen(1).map(drop)
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
    use std::io::Read;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let mut child = hidden_command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        stdout
            .take(2 * 1024 * 1024)
            .read_to_string(&mut text)
            .ok()?;
        Some(text)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < Duration::from_secs(8) => {
                std::thread::sleep(Duration::from_millis(25))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let output = reader.join().ok().flatten();
    status.filter(|status| status.success()).and(output)
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
