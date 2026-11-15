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

const INTERNET_SETTINGS: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

use crate::services::proxy_lease::ProxySetting;

pub fn capture_proxy_settings(port: u16) -> Result<Vec<ProxySetting>, String> {
    let mut settings = Vec::new();
    for (key, applied) in [
        ("ProxyEnable", "1".to_string()),
        ("ProxyServer", format!("127.0.0.1:{port}")),
        ("AutoConfigURL", String::new()),
    ] {
        let mut setting = ProxySetting {
            scope: INTERNET_SETTINGS.into(),
            key: key.into(),
            before: String::new(),
            applied: serde_json::to_string(&Some(applied)).map_err(|err| err.to_string())?,
        };
        setting.before = read_proxy_setting(&setting)?;
        if setting.before != setting.applied {
            settings.push(setting);
        }
    }
    Ok(settings)
}

pub fn read_proxy_setting(setting: &ProxySetting) -> Result<String, String> {
    // PowerShell 区分键缺失与读取失败，避免覆盖未知原值。
    let script = format!(
        r#"$ErrorActionPreference='Stop'; $p=Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'; $v=$p.PSObject.Properties['{}']; if ($null -eq $v) {{ 'null' }} else {{ [string]$v.Value | ConvertTo-Json -Compress }}"#,
        setting.key
    );
    run_command(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", &script],
    )
    .map(|v| v.trim().to_string())
    .ok_or_else(|| "读取 Windows 系统代理失败".into())
}

pub fn write_proxy_setting(setting: &ProxySetting, value: &str) -> Result<(), String> {
    let data: Option<String> = serde_json::from_str(value).map_err(|err| err.to_string())?;
    let output = if let Some(value) = data {
        let kind = if setting.key == "ProxyEnable" {
            "REG_DWORD"
        } else {
            "REG_SZ"
        };
        run_command(
            "reg",
            &[
                "add",
                INTERNET_SETTINGS,
                "/v",
                &setting.key,
                "/t",
                kind,
                "/d",
                &value,
                "/f",
            ],
        )
    } else {
        run_command(
            "reg",
            &["delete", INTERNET_SETTINGS, "/v", &setting.key, "/f"],
        )
    };
    output.ok_or_else(|| "写入 Windows 系统代理失败".to_string())?;
    let script = r#"Add-Type -Namespace LabVPN -Name WinInet -MemberDefinition '[DllImport("wininet.dll")] public static extern bool InternetSetOption(IntPtr hInternet, int option, IntPtr buffer, int length);'; [void][LabVPN.WinInet]::InternetSetOption([IntPtr]::Zero,39,[IntPtr]::Zero,0); [void][LabVPN.WinInet]::InternetSetOption([IntPtr]::Zero,37,[IntPtr]::Zero,0)"#;
    run_command(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )
    .ok_or_else(|| "设置已写入，但通知系统刷新失败".to_string())?;
    Ok(())
}

pub fn proxy_summary() -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for key in ["ProxyEnable", "ProxyServer", "AutoConfigURL"] {
        let setting = ProxySetting {
            scope: INTERNET_SETTINGS.into(),
            key: key.into(),
            before: String::new(),
            applied: String::new(),
        };
        let value: Option<String> =
            serde_json::from_str(&read_proxy_setting(&setting)?).map_err(|err| err.to_string())?;
        lines.push(format!(
            "{key}: {}",
            if key == "AutoConfigURL" {
                if value.as_deref().unwrap_or_default().is_empty() {
                    "OFF".into()
                } else {
                    "ON".into()
                }
            } else {
                value.unwrap_or_else(|| "OFF".into())
            }
        ));
    }
    lines.push("WPAD / WinHTTP / VPN: 请在系统网络设置中核对".into());
    Ok(lines)
}

/// 当前系统代理端口（仅识别 127.0.0.1）。
pub fn system_proxy_port() -> Option<u16> {
    let output = run_command("reg", &["query", INTERNET_SETTINGS, "/v", "ProxyEnable"])?;
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
