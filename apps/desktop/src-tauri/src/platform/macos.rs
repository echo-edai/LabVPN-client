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
fn network_services() -> Vec<String> {
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

/// 设置/取消系统代理。
pub fn set_system_proxy(port: u16, enable: bool) -> Result<(), String> {
    let services = network_services();
    if services.is_empty() {
        return Err("未找到可配置的网络服务".to_string());
    }
    let port = port.to_string();
    let mut failures = Vec::new();
    for service in &services {
        let commands: Vec<Vec<&str>> = if enable {
            vec![
                vec!["-setwebproxy", service, "127.0.0.1", &port],
                vec!["-setsecurewebproxy", service, "127.0.0.1", &port],
                vec!["-setsocksfirewallproxy", service, "127.0.0.1", &port],
            ]
        } else {
            vec![
                vec!["-setwebproxystate", service, "off"],
                vec!["-setsecurewebproxystate", service, "off"],
                vec!["-setsocksfirewallproxystate", service, "off"],
            ]
        };
        for command in commands {
            if run_command("networksetup", &command).is_none() {
                failures.push(format!("{} {}", service, command[0]));
            }
        }
        if enable {
            let _ = run_command(
                "networksetup",
                &[
                    "-setproxybypassdomains",
                    service,
                    "localhost",
                    "127.0.0.1",
                    "::1",
                    "*.local",
                ],
            );
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!("部分网络服务设置失败：{}", failures.join("; ")))
    }
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
