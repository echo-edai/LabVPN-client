//! 通用工具：脱敏、哈希、IP/CIDR 解析、进程名归一化等。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use sha2::{Digest, Sha256};

/// 计算字符串的 SHA-256 十六进制摘要。
pub fn sha256_hex(input: impl AsRef<[u8]>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_ref());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// 对 URL 中的敏感 query 参数做脱敏，用于日志与诊断包。
///
/// 例如 `https://x/sub?token=abc&name=a` -> `https://x/sub?token=***&name=a`
pub fn redact_url(input: &str) -> String {
    let Some((base, query)) = input.split_once('?') else {
        return input.to_string();
    };
    let redacted: Vec<String> = query
        .split('&')
        .map(|pair| {
            let Some((key, _)) = pair.split_once('=') else {
                return pair.to_string();
            };
            let lower = key.to_ascii_lowercase();
            if lower.contains("token")
                || lower.contains("key")
                || lower.contains("secret")
                || lower.contains("passwd")
                || lower.contains("password")
                || lower.contains("auth")
            {
                format!("{key}=***")
            } else {
                pair.to_string()
            }
        })
        .collect();
    format!("{base}?{}", redacted.join("&"))
}

/// 对任意密钥字符串脱敏，仅保留首尾各 2 个字符。
pub fn mask_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    match chars.len() {
        0 => String::new(),
        1..=4 => "***".to_string(),
        n => {
            let head: String = chars[..2].iter().collect();
            let tail: String = chars[n - 2..].iter().collect();
            format!("{head}***{tail}")
        }
    }
}

/// 人类可读字节数。
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// 解析 IP 字面量（v4/v6），失败返回 `None`。
pub fn parse_ip(input: &str) -> Option<IpAddr> {
    input.trim().parse::<IpAddr>().ok()
}

/// 简单 IPv4 CIDR 解析：返回 (网络地址, 前缀长度)。
///
/// 只接受合法前缀与规范网络地址（主机位为 0），避免生成 mihomo 无法识别的规则。
pub fn parse_cidr_v4(input: &str) -> Option<(Ipv4Addr, u8)> {
    let (addr, prefix) = input.trim().split_once('/')?;
    let addr: Ipv4Addr = addr.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    if prefix > 32 {
        return None;
    }
    let mask = if prefix == 0 {
        0u32
    } else {
        u32::MAX << (32 - prefix)
    };
    let net = u32::from(addr) & mask;
    Some((Ipv4Addr::from(net), prefix))
}

/// 简单 IPv6 CIDR 解析：返回 (网络地址, 前缀长度)。
pub fn parse_cidr_v6(input: &str) -> Option<(Ipv6Addr, u8)> {
    let (addr, prefix) = input.trim().split_once('/')?;
    let addr: Ipv6Addr = addr.parse().ok()?;
    let prefix: u8 = prefix.parse().ok()?;
    if prefix > 128 {
        return None;
    }
    let mask = if prefix == 0 {
        0u128
    } else {
        u128::MAX << (128 - prefix)
    };
    let net = u128::from(addr) & mask;
    Some((Ipv6Addr::from(net), prefix))
}

/// 判断字符串是否为合法 IP 或 CIDR（v4/v6 均可）。
pub fn is_ip_or_cidr(input: &str) -> bool {
    input.contains('/')
        && (parse_cidr_v4(input).is_some() || parse_cidr_v6(input).is_some())
        || parse_ip(input).is_some()
}

/// 判断 CIDR 是否属于私有/本地保留网段（v4 简易判断）。
pub fn is_private_v4(addr: Ipv4Addr) -> bool {
    addr.is_private() || addr.is_loopback() || addr.is_link_local() || addr.is_unspecified()
}

/// 校验域名（允许通配 `*.` 前缀）。
pub fn is_domain(input: &str) -> bool {
    let domain = input.trim().trim_start_matches("*.").trim_end_matches('.');
    if domain.is_empty() || domain.len() > 253 || !domain.contains('.') {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

/// 从可执行文件路径中提取进程名（去目录、去 `.exe`）。
pub fn process_name(binary: &str) -> String {
    let file = binary
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(binary)
        .trim();
    let file = file.strip_suffix(".exe").unwrap_or(file);
    file.to_string()
}

/// 规范化域名（小写、去尾部点）。
pub fn normalize_domain(input: &str) -> String {
    input.trim().to_ascii_lowercase().trim_end_matches('.').to_string()
}

/// 生成「规则目标」显示名：把内部组名转成人类可读描述。
pub fn outbound_display(outbound: &str) -> String {
    match outbound {
        "DIRECT" => "直连".to_string(),
        "REJECT" => "拦截".to_string(),
        other => other.to_string(),
    }
}

/// 截断长字符串用于日志。
pub fn truncate(input: &str, max: usize) -> String {
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= max {
        return input.to_string();
    }
    format!("{}…", chars[..max].iter().collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_token_query() {
        let url = "https://api.example.com/sub?token=abc123&name=XFLTD";
        let out = redact_url(url);
        assert!(out.contains("token=***"));
        assert!(out.contains("name=XFLTD"));
        assert!(!out.contains("abc123"));
    }

    #[test]
    fn masks_short_and_long_secrets() {
        assert_eq!(mask_secret(""), "");
        assert_eq!(mask_secret("abc"), "***");
        assert_eq!(mask_secret("abcdefgh"), "ab***gh");
    }

    #[test]
    fn parses_cidrs() {
        assert!(parse_cidr_v4("10.0.0.0/8").is_some());
        assert!(parse_cidr_v4("10.1.2.3/8").is_some());
        assert!(parse_cidr_v4("10.0.0.0/33").is_none());
        assert!(parse_cidr_v6("2001:da8:8001::/48").is_some());
        assert!(parse_ip("202.120.224.26").is_some());
        assert!(!is_ip_or_cidr("not-an-ip"));
    }

    #[test]
    fn validates_domains() {
        assert!(is_domain("fudan.edu.cn"));
        assert!(is_domain("*.fudan.edu.cn"));
        assert!(!is_domain("no-dot"));
        assert!(!is_domain("-bad.com"));
    }

    #[test]
    fn extracts_process_names() {
        assert_eq!(process_name("/usr/local/bin/fdu-connect"), "fdu-connect");
        assert_eq!(process_name("C:\\LabVPN\\fdu-connect.exe"), "fdu-connect");
    }
}
