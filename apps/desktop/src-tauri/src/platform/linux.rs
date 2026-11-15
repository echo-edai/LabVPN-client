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

use crate::services::proxy_lease::ProxySetting;

pub fn capture_proxy_settings(port: u16) -> Result<Vec<ProxySetting>, String> {
    let mut settings = Vec::new();
    // 模式最后写入，恢复时也先恢复端点再启用原模式。
    for (schema, key, applied) in [
        ("org.gnome.system.proxy", "mode", "'manual'".to_string()),
        ("org.gnome.system.proxy.http", "host", "'127.0.0.1'".into()),
        ("org.gnome.system.proxy.http", "port", port.to_string()),
        ("org.gnome.system.proxy.https", "host", "'127.0.0.1'".into()),
        ("org.gnome.system.proxy.https", "port", port.to_string()),
        ("org.gnome.system.proxy.socks", "host", "'127.0.0.1'".into()),
        ("org.gnome.system.proxy.socks", "port", port.to_string()),
    ] {
        let mut setting = ProxySetting {
            scope: schema.into(),
            key: key.into(),
            before: String::new(),
            applied,
        };
        setting.before = read_proxy_setting(&setting)?;
        if setting.key == "host" {
            let auth = run_command("gsettings", &["get", schema, "use-authentication"]);
            if auth.is_some_and(|v| v.trim() == "true") {
                return Err("请先关闭系统代理认证，LabVPN 无法备份密码".into());
            }
        }
        if setting.before != setting.applied {
            settings.push(setting);
        }
    }
    Ok(settings)
}

pub fn read_proxy_setting(setting: &ProxySetting) -> Result<String, String> {
    run_command("gsettings", &["get", &setting.scope, &setting.key])
        .map(|v| v.trim().to_string())
        .ok_or_else(|| "当前桌面不支持 GNOME gsettings 系统代理，请在系统设置中配置".into())
}

pub fn write_proxy_setting(setting: &ProxySetting, value: &str) -> Result<(), String> {
    run_command("gsettings", &["set", &setting.scope, &setting.key, value])
        .map(|_| ())
        .ok_or_else(|| "写入系统代理失败".into())
}

pub fn proxy_summary() -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for (scope, key) in [
        ("org.gnome.system.proxy", "mode"),
        ("org.gnome.system.proxy.http", "host"),
        ("org.gnome.system.proxy.http", "port"),
    ] {
        let setting = ProxySetting {
            scope: scope.into(),
            key: key.into(),
            before: String::new(),
            applied: String::new(),
        };
        lines.push(format!("{scope}/{key}: {}", read_proxy_setting(&setting)?));
    }
    Ok(lines)
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
