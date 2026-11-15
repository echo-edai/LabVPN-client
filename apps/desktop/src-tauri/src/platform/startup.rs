//! 仅管理明确识别的代理启动项；不删除软件、不结束正在使用的连接。

use super::hidden_command;
#[cfg(target_os = "macos")]
use super::run_command;
use crate::services::environment::StartupItem;

#[cfg(target_os = "macos")]
const ITEMS: &[(&str, &str)] = &[
    ("com.fdu-connect", "user"),
    ("com.sangfor.ECAgentProxy", "agent"),
    ("com.arraynetworks.mhttpd", "agent"),
    ("com.sangfor.EasyMonitor", "system"),
    ("com.arraynetworks.vpnd", "system"),
    ("com.leigod.helper", "system"),
    ("io.github.clash-verge-rev.mihomo", "system"),
    ("io.github.clash-verge-rev.helper", "system"),
];

pub fn can_disable_proxy_startup() -> bool {
    cfg!(target_os = "macos")
}

pub fn startup_items() -> Vec<StartupItem> {
    #[cfg(target_os = "macos")]
    {
        let Some(uid) = run_command("id", &["-u"]) else {
            return Vec::new();
        };
        let gui = format!("gui/{}", uid.trim());
        let user = std::env::var("HOME").unwrap_or_default();
        let gui_disabled = run_command("launchctl", &["print-disabled", &gui]);
        let system_disabled = run_command("launchctl", &["print-disabled", "system"]);
        ITEMS
            .iter()
            .filter_map(|(label, kind)| {
                let directory = match *kind {
                    "user" => format!("{user}/Library/LaunchAgents"),
                    "agent" => "/Library/LaunchAgents".into(),
                    _ => "/Library/LaunchDaemons".into(),
                };
                if !std::path::Path::new(&directory)
                    .join(format!("{label}.plist"))
                    .exists()
                {
                    return None;
                }
                let disabled = if *kind == "system" {
                    &system_disabled
                } else {
                    &gui_disabled
                };
                Some(StartupItem {
                    label: (*label).into(),
                    domain: if *kind == "system" {
                        "system".into()
                    } else {
                        gui.clone()
                    },
                    disabled: disabled.as_ref().map(|text| {
                        text.lines().any(|line| {
                            [
                                format!("\"{label}\" => true"),
                                format!("\"{label}\" => disabled"),
                            ]
                            .iter()
                            .any(|disabled| line.trim() == disabled)
                        })
                    }),
                })
            })
            .collect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

pub fn disable_proxy_startup() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        let items = startup_items();
        let mut system = Vec::new();
        for item in items.iter().filter(|item| item.disabled != Some(true)) {
            if item.domain == "system" {
                // 标签来自固定白名单，脚本不接收前端输入。
                system.push(format!("/bin/launchctl disable system/{}", item.label));
            } else if run_command(
                "launchctl",
                &["disable", &format!("{}/{}", item.domain, item.label)],
            )
            .is_none()
            {
                return Err(format!("停用 {} 失败", item.label));
            }
        }
        if !system.is_empty() {
            let command = system.join(" && ");
            let script = format!("do shell script \"{command}\" with prompt \"LabVPN 将停用已识别代理服务的开机启动，不结束当前连接\" with administrator privileges");
            let output = hidden_command("osascript")
                .args(["-e", &script])
                .output()
                .map_err(|err| err.to_string())?;
            if !output.status.success() {
                return Err("管理员授权未完成；部分用户启动项可能已停用，请重新检查 / Administrator authorization was not completed".into());
            }
        }
        let remaining = startup_items()
            .into_iter()
            .filter(|item| item.disabled != Some(true))
            .count();
        if remaining != 0 {
            return Err(format!("仍有 {remaining} 个启动项未确认停用，请重新检查"));
        }
        Ok("已停用识别到的代理启动项；当前连接保持运行。其他登录项请在系统启动设置核对 / Known startup services disabled; review other login items in System Settings".into())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("当前平台请使用系统启动设置管理代理自启动 / Use system startup settings on this platform".into())
    }
}

pub fn open_startup_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let result = hidden_command("open")
        .arg("x-apple.systempreferences:com.apple.LoginItems-Settings.extension")
        .spawn();
    #[cfg(windows)]
    let result = hidden_command("cmd")
        .args(["/C", "start", "", "ms-settings:startupapps"])
        .spawn();
    #[cfg(target_os = "linux")]
    let result = hidden_command("xdg-open")
        .arg(format!(
            "{}/.config/autostart",
            std::env::var("HOME").unwrap_or_default()
        ))
        .spawn();
    result
        .map(|_| ())
        .map_err(|err| format!("无法打开启动设置：{err}"))
}
