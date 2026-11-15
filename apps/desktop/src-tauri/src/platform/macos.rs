//! macOS 平台实现：route / networksetup / scutil。

use super::run_command;

/// 默认出口网卡。
pub fn default_interface() -> Option<String> {
    let output = run_command("route", &["-n", "get", "default"])?;
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("interface:")
            .map(|value| value.trim().to_string())
    })
}

/// Wi-Fi SSID。
pub fn wifi_ssid() -> Option<String> {
    // 找出 Wi-Fi 硬件端口
    let ports = run_command("networksetup", &["-listallhardwareports"])?;
    let mut wifi_device: Option<String> = None;
    let mut lines = ports.lines();
    while let Some(line) = lines.next() {
        if line.contains("Wi-Fi") || line.contains("AirPort") {
            for _ in 0..3 {
                if let Some(next) = lines.next() {
                    if let Some(device) = next.trim().strip_prefix("Device:") {
                        wifi_device = Some(device.trim().to_string());
                        break;
                    }
                }
            }
        }
    }
    let device = wifi_device?;
    let output = run_command("networksetup", &["-getairportnetwork", &device])?;
    let ssid = output.trim();
    if let Some(value) = ssid.strip_prefix("Current Wi-Fi Network:") {
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    // 新系统可能返回 "You are not associated with an AirPort network."
    None
}

/// 列出所有网络服务名。
pub(super) fn network_services() -> Vec<String> {
    let Some(output) = run_command("networksetup", &["-listallnetworkservices"]) else {
        return Vec::new();
    };
    output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("An asterisk")
                && !line.starts_with('*')
                && !line.contains("denotes that a network service is disabled")
        })
        .map(str::to_string)
        .collect()
}

use crate::services::proxy_lease::ProxySetting;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Endpoint {
    enabled: bool,
    server: String,
    port: u16,
    authenticated: bool,
}

fn proxy_commands(key: &str) -> Result<(&str, &str, &str), String> {
    match key {
        "http" => Ok(("-getwebproxy", "-setwebproxy", "-setwebproxystate")),
        "https" => Ok((
            "-getsecurewebproxy",
            "-setsecurewebproxy",
            "-setsecurewebproxystate",
        )),
        "socks" => Ok((
            "-getsocksfirewallproxy",
            "-setsocksfirewallproxy",
            "-setsocksfirewallproxystate",
        )),
        "pac" => Ok(("-getautoproxyurl", "", "-setautoproxystate")),
        "wpad" => Ok(("-getproxyautodiscovery", "", "-setproxyautodiscovery")),
        _ => Err("未知代理字段".into()),
    }
}

pub fn capture_proxy_settings(port: u16) -> Result<Vec<ProxySetting>, String> {
    let mut settings = Vec::new();
    for service in network_services() {
        for key in ["pac", "wpad", "http", "https", "socks"] {
            let mut setting = ProxySetting {
                scope: service.clone(),
                key: key.into(),
                before: String::new(),
                applied: "off".into(),
            };
            setting.before = read_proxy_setting(&setting)?;
            if !matches!(key, "pac" | "wpad") {
                let original: Endpoint =
                    serde_json::from_str(&setting.before).map_err(|err| err.to_string())?;
                if original.authenticated {
                    return Err(format!(
                        "{service} 使用带认证的代理，请先在系统设置中切换，LabVPN 无法备份其密码"
                    ));
                }
                setting.applied = serde_json::to_string(&Endpoint {
                    enabled: true,
                    server: "127.0.0.1".into(),
                    port,
                    authenticated: false,
                })
                .map_err(|err| err.to_string())?;
            }
            if setting.before != setting.applied {
                settings.push(setting);
            }
        }
    }
    Ok(settings)
}

pub fn read_proxy_setting(setting: &ProxySetting) -> Result<String, String> {
    let (get, _, _) = proxy_commands(&setting.key)?;
    let output = run_command("networksetup", &[get, &setting.scope])
        .ok_or_else(|| format!("读取 {} 失败", setting.scope))?;
    let value = |name: &str| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .map(str::trim)
    };
    if setting.key == "wpad" {
        return match value("Auto Proxy Discovery:") {
            Some("On") => Ok("on".into()),
            Some("Off") => Ok("off".into()),
            _ => Err("无法解析 WPAD 状态".into()),
        };
    }
    let enabled = match value("Enabled:") {
        Some("Yes") => true,
        Some("No") => false,
        _ => return Err("无法解析代理状态".into()),
    };
    if setting.key == "pac" {
        return Ok(if enabled { "on" } else { "off" }.into());
    }
    let endpoint = Endpoint {
        enabled,
        server: value("Server:").unwrap_or_default().to_string(),
        port: value("Port:").and_then(|v| v.parse().ok()).unwrap_or(0),
        authenticated: value("Authenticated Proxy Enabled:") == Some("1"),
    };
    serde_json::to_string(&endpoint).map_err(|err| err.to_string())
}

pub fn write_proxy_setting(setting: &ProxySetting, value: &str) -> Result<(), String> {
    let (_, set, state) = proxy_commands(&setting.key)?;
    let run = |args: &[&str]| match run_command("networksetup", args) {
        Some(output) if !output.contains("Error") && !output.contains("error") => Ok(()),
        _ => Err(format!(
            "设置 {} / {} 失败，请检查系统权限",
            setting.scope, setting.key
        )),
    };
    if matches!(setting.key.as_str(), "pac" | "wpad") {
        if value != "on" && value != "off" {
            return Err("无效代理开关".into());
        }
        return run(&[state, &setting.scope, value]);
    }
    let endpoint: Endpoint = serde_json::from_str(value).map_err(|err| err.to_string())?;
    if endpoint.authenticated {
        return Err("不能恢复带密码的代理".into());
    }
    if !endpoint.server.is_empty() && endpoint.port > 0 {
        run(&[
            set,
            &setting.scope,
            &endpoint.server,
            &endpoint.port.to_string(),
            "off",
        ])?;
    } else if endpoint.enabled {
        return Err("代理端点无效".into());
    }
    run(&[
        state,
        &setting.scope,
        if endpoint.enabled { "on" } else { "off" },
    ])
}

/// 展示端点和开关，不展示可能含令牌的 PAC 地址。
pub fn proxy_summary() -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    for service in network_services() {
        let mut values = Vec::new();
        for key in ["http", "https", "socks", "pac", "wpad"] {
            let setting = ProxySetting {
                scope: service.clone(),
                key: key.into(),
                before: String::new(),
                applied: String::new(),
            };
            let value = read_proxy_setting(&setting)?;
            if matches!(key, "pac" | "wpad") {
                if value == "on" {
                    values.push(key.to_uppercase());
                }
            } else {
                let endpoint: Endpoint =
                    serde_json::from_str(&value).map_err(|err| err.to_string())?;
                if endpoint.enabled {
                    values.push(format!(
                        "{} {}:{}",
                        key.to_uppercase(),
                        endpoint.server,
                        endpoint.port
                    ));
                }
            }
        }
        result.push(format!(
            "{service}: {}",
            if values.is_empty() {
                "OFF".into()
            } else {
                values.join(" / ")
            }
        ));
    }
    Ok(result)
}

/// 当前系统代理端口（优先 HTTP，其次 SOCKS）。
pub fn system_proxy_port() -> Option<u16> {
    let output = run_command("scutil", &["--proxy"])?;
    owned_proxy_port(&output)
}

fn proxy_value<'a>(output: &'a str, key: &str) -> Option<&'a str> {
    output
        .lines()
        .filter_map(|line| line.trim().split_once(" : "))
        .find_map(|(name, value)| (name == key).then_some(value.trim()))
}

fn owned_proxy_port(output: &str) -> Option<u16> {
    let mut found = None;
    for protocol in ["HTTP", "HTTPS", "SOCKS"] {
        if proxy_value(output, &format!("{protocol}Enable")) != Some("1") {
            continue;
        }
        let host = proxy_value(output, &format!("{protocol}Proxy"))?;
        if host != "127.0.0.1" && host != "::1" {
            return None;
        }
        let port = proxy_value(output, &format!("{protocol}Port"))?
            .parse::<u16>()
            .ok()?;
        if port == 0 || found.is_some_and(|previous| previous != port) {
            return None;
        }
        found = Some(port);
    }
    found
}

/// 只报告有效的自动代理状态，诊断日志不包含 PAC URL 中可能存在的令牌。
pub fn proxy_warnings() -> Vec<String> {
    let Some(output) = run_command("scutil", &["--proxy"]) else {
        return vec!["无法读取系统代理状态".to_string()];
    };
    let mut warnings = Vec::new();
    if proxy_value(&output, "ProxyAutoConfigEnable") == Some("1") {
        warnings.push(
            "系统启用了 PAC 自动代理，浏览器可能选择其他代理出口；请在网络设置中检查来源"
                .to_string(),
        );
    }
    if proxy_value(&output, "ProxyAutoDiscoveryEnable") == Some("1") {
        warnings.push("系统启用了 WPAD 自动代理发现，网络变化时代理出口可能改变".to_string());
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::owned_proxy_port;

    #[test]
    fn ignores_disabled_ports_and_remote_proxies() {
        assert_eq!(owned_proxy_port("HTTPEnable : 0\nHTTPPort : 7897"), None);
        assert_eq!(
            owned_proxy_port("HTTPEnable : 1\nHTTPProxy : proxy.example\nHTTPPort : 7897"),
            None
        );
    }

    #[test]
    fn requires_consistent_enabled_loopback_proxies() {
        let output = "HTTPEnable : 1\nHTTPProxy : 127.0.0.1\nHTTPPort : 7897\nHTTPSEnable : 1\nHTTPSProxy : 127.0.0.1\nHTTPSPort : 7897";
        assert_eq!(owned_proxy_port(output), Some(7897));
        assert_eq!(
            owned_proxy_port(&output.replace("HTTPSPort : 7897", "HTTPSPort : 7890")),
            None
        );
    }
}
