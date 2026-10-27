//! mihomo 连接日志分类（纯逻辑）。
//!
//! 把核心连接日志（`[TCP]/[UDP]/[ICMP] ... using ...`）归类成日志页的四个分类：
//!
//! - **外网连接**：走 `🚀 代理` 组（订阅节点）
//! - **校园网连接**：走 `🎓 校内` 组（fdu-connect RVPN）
//! - **直连**：`using DIRECT`
//! - **SSH 连接**：目标端口 22 / 进程名为 ssh / mihomo `🖥 SSH-*` 出站
//!
//! 分类优先级：SSH > 校园网 > 代理 > 直连。这样「ssh 到校内主机」会出现在 SSH 页，
//! 方便单独排查 SSH 场景；普通校内流量仍在校园网页。

use serde::{Deserialize, Serialize};

/// 连接日志分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnCategory {
    /// 外网连接（代理节点）
    Proxy,
    /// 校园网连接（校内 VPN）
    Campus,
    /// 直连
    Direct,
    /// SSH 连接
    Ssh,
}

impl ConnCategory {
    /// 写入日志 `source` 字段的稳定标识。
    pub fn source(self) -> &'static str {
        match self {
            Self::Proxy => "conn-proxy",
            Self::Campus => "conn-campus",
            Self::Direct => "conn-direct",
            Self::Ssh => "conn-ssh",
        }
    }
}

/// 是否为连接日志行（TCP/UDP/ICMP）。
pub fn is_connection_line(line: &str) -> bool {
    line.contains("[TCP]") || line.contains("[UDP]") || line.contains("[ICMP]")
}

/// 分类一条 mihomo 连接日志；不是连接日志时返回 `None`。
pub fn classify(line: &str) -> Option<ConnCategory> {
    if !is_connection_line(line) {
        return None;
    }
    if is_ssh(line) {
        return Some(ConnCategory::Ssh);
    }
    if line.contains("🎓 校内") || line.contains("[🏫 校内 VPN]") {
        return Some(ConnCategory::Campus);
    }
    if line.contains("🚀 代理") {
        return Some(ConnCategory::Proxy);
    }
    if line.contains("using DIRECT") || line.contains("[DIRECT]") {
        return Some(ConnCategory::Direct);
    }
    // 其他命名策略组（用户自定义）按代理处理
    line.contains("using ").then_some(ConnCategory::Proxy)
}

/// 判断是否 SSH 相关连接。
fn is_ssh(line: &str) -> bool {
    if line.contains("(ssh)") || line.contains("🖥 SSH") {
        return true;
    }
    // 解析 `--> host:port` 中的端口是否为 22
    let Some((_, rest)) = line.split_once("-->") else {
        return false;
    };
    let target = rest.split_whitespace().next().unwrap_or("");
    matches!(target.rsplit_once(':'), Some((_, port)) if port == "22")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_proxy_connection() {
        let line = r#"time="2026-10-27T16:02:24.422810000+08:00" level=warning msg="[TCP] dial 🚀 代理 (match GeoSite(geolocation-!cn)) 198.18.0.1:54970(Codex (Service)) --> chatgpt.com:443 error: i/o timeout""#;
        assert_eq!(classify(line), Some(ConnCategory::Proxy));
    }

    #[test]
    fn classifies_campus_connection() {
        let line = r#"time="2026-10-27T10:39:12.388693000+08:00" level=info msg="[TCP] 198.18.0.1:54481(Browser Helper) --> 10.134.142.226:9090 match RuleSet(FDUIP) using 🎓 校内[🏫 校内 VPN]""#;
        assert_eq!(classify(line), Some(ConnCategory::Campus));
    }

    #[test]
    fn classifies_direct_connection() {
        let line = r#"time="2026-10-27T16:02:24.429423000+08:00" level=info msg="[TCP] 198.18.0.1:54972(WeChat) --> szextshort.weixin.qq.com:80 match RuleSet(ChinaDomainFallback) using DIRECT""#;
        assert_eq!(classify(line), Some(ConnCategory::Direct));
    }

    #[test]
    fn ssh_wins_over_route_category() {
        let campus_ssh = r#"msg="[TCP] 198.18.0.1:54366(ssh) --> 10.134.142.145:22 match RuleSet(FDUIP) using 🎓 校内[🏫 校内 VPN]""#;
        assert_eq!(classify(campus_ssh), Some(ConnCategory::Ssh));
        let port_22 = r#"msg="[TCP] 198.18.0.1:50000(code) --> 10.134.142.226:22 match Match using 🚀 代理[node]""#;
        assert_eq!(classify(port_22), Some(ConnCategory::Ssh));
        let ssh_outbound = r#"msg="[TCP] 198.18.0.1:50001(curl) --> 203.0.113.9:22 match Match using 🖥 SSH-203.0.113.9""#;
        assert_eq!(classify(ssh_outbound), Some(ConnCategory::Ssh));
    }

    #[test]
    fn ignores_non_connection_lines() {
        assert_eq!(classify("Start initial configuration in progress"), None);
        assert_eq!(classify("[TUN] Tun adapter listening at: utun11"), None);
    }

    #[test]
    fn sources_are_stable() {
        assert_eq!(ConnCategory::Proxy.source(), "conn-proxy");
        assert_eq!(ConnCategory::Campus.source(), "conn-campus");
        assert_eq!(ConnCategory::Direct.source(), "conn-direct");
        assert_eq!(ConnCategory::Ssh.source(), "conn-ssh");
    }
}
