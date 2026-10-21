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
    let mut http_port = None;
    let mut socks_port = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("HTTPPort :") {
            http_port = value.trim().parse::<u16>().ok();
        }
        if let Some(value) = line.strip_prefix("SOCKSPort :") {
            socks_port = value.trim().parse::<u16>().ok();
        }
    }
    http_port.or(socks_port)
}
