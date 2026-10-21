//! Windows 平台实现：netsh / registry / PowerShell。

use super::run_command;

/// 默认出口网卡（返回 InterfaceAlias，便于展示）。
pub fn default_interface() -> Option<String> {
    let output = run_command(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Select-Object -First 1).InterfaceAlias",
        ],
    )?;
    let alias = output.trim();
    if alias.is_empty() {
        None
    } else {
        Some(alias.to_string())
    }
}

/// Wi-Fi SSID。
pub fn wifi_ssid() -> Option<String> {
    let output = run_command("netsh", &["wlan", "show", "interfaces"])?;
    for line in output.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("SSID") {
            // 排除 BSSID 行
            if value.trim_start().starts_with(':') {
                let ssid = value.trim_start().trim_start_matches(':').trim();
                if !ssid.is_empty() {
                    return Some(ssid.to_string());
                }
            }
        }
    }
    None
}

const INTERNET_SETTINGS: &str =
    r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// 设置/取消系统代理（WinINet 注册表 + 通知刷新）。
pub fn set_system_proxy(port: u16, enable: bool) -> Result<(), String> {
    let (enable_value, server_value) = if enable {
        ("1", format!("127.0.0.1:{port}"))
    } else {
        ("0", String::new())
    };
    let set_enable = run_command(
        "reg",
        &[
            "add",
            INTERNET_SETTINGS,
            "/v",
            "ProxyEnable",
            "/t",
            "REG_DWORD",
            "/d",
            enable_value,
            "/f",
        ],
    );
    if set_enable.is_none() {
        return Err("写入 ProxyEnable 注册表失败".to_string());
    }
    if enable {
        let set_server = run_command(
            "reg",
            &[
                "add",
                INTERNET_SETTINGS,
                "/v",
                "ProxyServer",
                "/t",
                "REG_SZ",
                "/d",
                &server_value,
                "/f",
            ],
        );
        if set_server.is_none() {
            return Err("写入 ProxyServer 注册表失败".to_string());
        }
        let _ = run_command(
            "reg",
            &[
                "add",
                INTERNET_SETTINGS,
                "/v",
                "ProxyOverride",
                "/t",
                "REG_SZ",
                "/d",
                "<local>",
                "/f",
            ],
        );
    }
    // 通知 WinINet 刷新设置（否则部分应用需要重启才生效）
    let script = r#"
Add-Type -Namespace LabVPN -Name WinInet -MemberDefinition '[DllImport("wininet.dll", SetLastError = true)] public static extern bool InternetSetOption(IntPtr hInternet, int dwOption, IntPtr lpBuffer, int dwBufferLength);'
[void][LabVPN.WinInet]::InternetSetOption([IntPtr]::Zero, 39, [IntPtr]::Zero, 0)
[void][LabVPN.WinInet]::InternetSetOption([IntPtr]::Zero, 37, [IntPtr]::Zero, 0)
"#;
    let _ = run_command("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]);
    Ok(())
}

/// 当前系统代理端口（仅识别 127.0.0.1）。
pub fn system_proxy_port() -> Option<u16> {
    let output = run_command(
        "reg",
        &["query", INTERNET_SETTINGS, "/v", "ProxyEnable"],
    )?;
    if !output.to_ascii_lowercase().contains("0x1") {
        return None;
    }
    let server = run_command("reg", &["query", INTERNET_SETTINGS, "/v", "ProxyServer"])?;
    for token in server.split_whitespace() {
        if let Some((host, port)) = token.rsplit_once(':') {
            if host.contains("127.0.0.1") {
                return port.parse::<u16>().ok();
            }
        }
    }
    None
}
