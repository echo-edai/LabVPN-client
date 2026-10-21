//! Linux 平台实现：ip route / nmcli / gsettings。

use super::run_command;

/// 默认出口网卡。
pub fn default_interface() -> Option<String> {
    let output = run_command("ip", &["route", "show", "default"])?;
    output.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        while let Some(part) = parts.next() {
            if part == "dev" {
                return parts.next().map(str::to_string);
            }
        }
        None
    })
}

/// Wi-Fi SSID。
pub fn wifi_ssid() -> Option<String> {
    if let Some(output) = run_command("nmcli", &["-t", "-f", "ACTIVE,SSID", "dev", "wifi"]) {
        for line in output.lines() {
            if let Some(ssid) = line.strip_prefix("yes:") {
                let ssid = ssid.trim();
                if !ssid.is_empty() {
                    return Some(ssid.to_string());
                }
            }
        }
    }
    if let Some(output) = run_command("iwgetid", &["-r"]) {
        let ssid = output.trim();
        if !ssid.is_empty() {
            return Some(ssid.to_string());
        }
    }
    None
}

/// 设置/取消 GNOME 系统代理。
pub fn set_system_proxy(port: u16, enable: bool) -> Result<(), String> {
    if run_command("gsettings", &["get", "org.gnome.system.proxy", "mode"]).is_none() {
        return Err(
            "当前桌面环境不支持自动设置系统代理（未找到 gsettings），请在系统设置中手动配置"
                .to_string(),
        );
    }
    let port = port.to_string();
    let commands: Vec<Vec<&str>> = if enable {
        vec![
            vec!["set", "org.gnome.system.proxy", "mode", "manual"],
            vec!["set", "org.gnome.system.proxy.http", "host", "127.0.0.1"],
            vec!["set", "org.gnome.system.proxy.http", "port", &port],
            vec!["set", "org.gnome.system.proxy.https", "host", "127.0.0.1"],
            vec!["set", "org.gnome.system.proxy.https", "port", &port],
            vec!["set", "org.gnome.system.proxy.socks", "host", "127.0.0.1"],
            vec!["set", "org.gnome.system.proxy.socks", "port", &port],
        ]
    } else {
        vec![vec!["set", "org.gnome.system.proxy", "mode", "none"]]
    };
    for command in commands {
        if run_command("gsettings", &command).is_none() {
            return Err(format!("gsettings {} 执行失败", command.join(" ")));
        }
    }
    Ok(())
}

/// 当前系统代理端口（仅识别 manual + 127.0.0.1）。
pub fn system_proxy_port() -> Option<u16> {
    let mode = run_command("gsettings", &["get", "org.gnome.system.proxy", "mode"])?;
    if !mode.contains("manual") {
        return None;
    }
    let host = run_command("gsettings", &["get", "org.gnome.system.proxy.http", "host"])?;
    if !host.contains("127.0.0.1") {
        return None;
    }
    let port = run_command("gsettings", &["get", "org.gnome.system.proxy.http", "port"])?;
    port.trim().trim_matches('\'').parse::<u16>().ok()
}
