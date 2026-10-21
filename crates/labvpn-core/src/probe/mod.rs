//! 校园网多信号探测。
//!
//! 信号来源（按可信度）：
//! 1. **ICMP Ping 校内网关**（默认 10.134.140.1）：ping 通即 **100% 在校内**，
//!    自动跳过 fdu-connect 并让校内流量全部直连
//! 2. 本地物理网卡 IP 是否命中校内网段（强信号）
//! 3. 内网 TCP / UDP DNS 探针是否可达（强信号，且**绑定物理网卡**以绕过第三方 TUN）
//! 4. Wi-Fi SSID 是否为复旦校园网（辅助信号）
//!
//! 输出带置信度的 [`CampusStatus`]，供 UI 展示每个信号的结果，并支持手动覆盖。

pub mod socket;

use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::ProbeTarget;
use crate::util;

/// 单次探测结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeAttempt {
    /// 是否成功
    pub ok: bool,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
    /// 是否成功绑定网卡
    pub bound: bool,
    /// 细节（展示给用户）
    pub detail: String,
}

impl ProbeAttempt {
    /// 构造失败结果。
    pub fn failed(detail: impl Into<String>) -> Self {
        Self {
            ok: false,
            elapsed_ms: 0,
            bound: false,
            detail: detail.into(),
        }
    }
}

/// 本机网卡信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceInfo {
    /// 网卡名
    pub name: String,
    /// 系统网卡索引
    pub index: Option<u32>,
    /// IP 地址
    pub ip: Option<IpAddr>,
    /// 前缀长度
    pub prefix: Option<u8>,
    /// 是否虚拟网卡（TUN/TAP/回环/桥接等）
    pub is_virtual: bool,
}

impl InterfaceInfo {
    /// 计算 CIDR 表示，如 `192.168.1.0/24`。
    pub fn cidr(&self) -> Option<String> {
        let ip = self.ip?;
        let prefix = self.prefix?;
        match ip {
            IpAddr::V4(v4) => {
                let raw = u32::from(v4);
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix as u32)
                };
                Some(format!(
                    "{}/{}",
                    std::net::Ipv4Addr::from(raw & mask),
                    prefix
                ))
            }
            IpAddr::V6(v6) => {
                let raw = u128::from(v6);
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix as u32)
                };
                Some(format!(
                    "{}/{}",
                    std::net::Ipv6Addr::from(raw & mask),
                    prefix
                ))
            }
        }
    }
}

/// 采集本机所有网卡信息。
pub fn collect_interfaces() -> Vec<InterfaceInfo> {
    let mut result = Vec::new();
    let Ok(ifaces) = if_addrs::get_if_addrs() else {
        return result;
    };
    for iface in ifaces {
        let (ip, prefix) = match iface.addr {
            if_addrs::IfAddr::V4(addr) => (Some(IpAddr::V4(addr.ip)), Some(addr.prefixlen)),
            if_addrs::IfAddr::V6(addr) => (Some(IpAddr::V6(addr.ip)), Some(addr.prefixlen)),
        };
        let is_virtual = ip
            .map(|ip| is_virtual_interface(&iface.name, ip))
            .unwrap_or(true);
        result.push(InterfaceInfo {
            name: iface.name,
            index: iface.index,
            ip,
            prefix,
            is_virtual,
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

/// 判断虚拟网卡（TUN/TAP/回环/桥接/容器等）。
pub fn is_virtual_interface(name: &str, ip: IpAddr) -> bool {
    let lower = name.to_ascii_lowercase();
    const VIRTUAL_PREFIXES: &[&str] = &[
        "utun",
        "tun",
        "tap",
        "ppp",
        "ipsec",
        "wg",
        "wintun",
        "tailscale",
        "zerotier",
        "lo",
        "loopback",
        "bridge",
        "br-",
        "docker",
        "veth",
        "virbr",
        "vmnet",
        "awdl",
        "llw",
        "gif",
        "stf",
        "anpi",
        "clash",
        "mihomo",
        "singbox",
        "sing-box",
        "shadowrocket",
        "vpn",
    ];
    if name == "lo0" || name == "lo" {
        return true;
    }
    if VIRTUAL_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    // Fake-IP 网段（第三方代理 TUN 常见）
    if let IpAddr::V4(v4) = ip {
        let octets = v4.octets();
        if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
            return true;
        }
        if v4.is_loopback() {
            return true;
        }
    }
    false
}

/// 挑选主用物理网卡：优先非虚拟、IPv4 非环回/链路本地。
pub fn pick_primary_interface(ifaces: &[InterfaceInfo]) -> Option<&InterfaceInfo> {
    ifaces
        .iter()
        .filter(|iface| !iface.is_virtual)
        .filter(|iface| match iface.ip {
            Some(IpAddr::V4(v4)) => !v4.is_loopback() && !v4.is_link_local(),
            Some(IpAddr::V6(_)) => false,
            None => false,
        })
        .max_by_key(|iface| iface.prefix.unwrap_or(0) as u32)
}

/// 本机直连子网（仅 IPv4，非虚拟网卡），用于生成 `DIRECT` 规则。
pub fn local_subnets(ifaces: &[InterfaceInfo]) -> Vec<String> {
    let mut subnets: Vec<String> = ifaces
        .iter()
        .filter(|iface| !iface.is_virtual)
        .filter_map(InterfaceInfo::cidr)
        .filter(|cidr| cidr.contains('.'))
        .collect();
    subnets.sort();
    subnets.dedup();
    subnets
}

/// 信号类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignalKind {
    /// 本地 IP 段
    IpRange,
    /// TCP 探针
    TcpProbe,
    /// UDP DNS 探针
    DnsProbe,
    /// ICMP Ping 探针
    PingProbe,
    /// Wi-Fi SSID
    Wifi,
    /// 手动覆盖
    Manual,
}

/// 单个信号结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignalResult {
    /// 信号类型
    pub kind: SignalKind,
    /// 信号名称（UI 展示）
    pub name: String,
    /// 是否命中
    pub ok: bool,
    /// 权重
    pub weight: u8,
    /// 细节
    pub detail: String,
}

/// 校园网检测结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CampusStatus {
    /// 是否在校园网
    pub on_campus: bool,
    /// 置信度 0.0 - 1.0
    pub confidence: f32,
    /// 各信号明细
    pub signals: Vec<SignalResult>,
    /// 检测时间
    pub checked_at: DateTime<Utc>,
    /// 使用的物理网卡
    pub interface: Option<String>,
    /// 手动覆盖值（None 表示自动检测）
    pub manual_override: Option<bool>,
}

impl CampusStatus {
    /// 应用手动覆盖（用户强制"在校内/在校外"）。
    pub fn apply_manual_override(mut self, override_value: Option<bool>) -> Self {
        self.manual_override = override_value;
        if let Some(value) = override_value {
            self.on_campus = value;
            self.confidence = 1.0;
            self.signals.push(SignalResult {
                kind: SignalKind::Manual,
                name: "手动设置".to_string(),
                ok: value,
                weight: 255,
                detail: if value {
                    "用户强制指定为校内".to_string()
                } else {
                    "用户强制指定为校外".to_string()
                },
            });
        }
        self
    }

    /// 一行摘要，用于日志与 UI 提示。
    pub fn summary(&self) -> String {
        if self.on_campus {
            format!("校内网络（置信度 {:.0}%）", self.confidence * 100.0)
        } else {
            format!("校外网络（置信度 {:.0}%）", (1.0 - self.confidence) * 100.0)
        }
    }
}

/// 探测输入。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProbeInput {
    /// 探测目标
    pub targets: Vec<ProbeTarget>,
    /// 校内 IP 段
    pub campus_cidrs: Vec<String>,
    /// 指定物理网卡（None 则自动挑选）
    pub interface: Option<String>,
    /// Wi-Fi SSID（可选，由平台层采集）
    pub ssid: Option<String>,
}

/// 探测执行器（便于测试注入 mock）。
pub trait ProbeRunner {
    /// 执行一次探测。
    fn probe(&self, target: &ProbeTarget, iface: Option<socket::IfaceRef<'_>>) -> ProbeAttempt;
}

/// 系统真实探测执行器。
pub struct SystemProbeRunner;

impl ProbeRunner for SystemProbeRunner {
    fn probe(&self, target: &ProbeTarget, iface: Option<socket::IfaceRef<'_>>) -> ProbeAttempt {
        socket::probe(target, iface)
    }
}

/// 采集本机接口并执行自动检测。
pub fn detect(input: &ProbeInput) -> CampusStatus {
    let ifaces = collect_interfaces();
    detect_with(&SystemProbeRunner, &ifaces, input)
}

/// 使用指定网卡信息与执行器做检测（纯函数，便于测试）。
pub fn detect_with(
    runner: &dyn ProbeRunner,
    ifaces: &[InterfaceInfo],
    input: &ProbeInput,
) -> CampusStatus {
    let primary = input
        .interface
        .as_deref()
        .and_then(|name| ifaces.iter().find(|iface| iface.name == name))
        .or_else(|| pick_primary_interface(ifaces));

    let mut signals = Vec::new();

    // 1) 本地 IP 段
    if !input.campus_cidrs.is_empty() {
        let mut matched = Vec::new();
        for iface in ifaces.iter().filter(|iface| !iface.is_virtual) {
            let Some(ip) = iface.ip else { continue };
            for cidr in &input.campus_cidrs {
                if ip_in_cidr(ip, cidr) {
                    matched.push(format!("{ip}（{}）命中 {cidr}", iface.name));
                }
            }
        }
        signals.push(SignalResult {
            kind: SignalKind::IpRange,
            name: "本地 IP 段".to_string(),
            ok: !matched.is_empty(),
            weight: 80,
            detail: if matched.is_empty() {
                "本机 IP 不属于校内网段".to_string()
            } else {
                matched.join("；")
            },
        });
    }

    // 2) 网络探针
    for target in &input.targets {
        let port_required = !matches!(target.protocol, crate::config::ProbeProtocol::Icmp);
        if target.host.trim().is_empty() || (port_required && target.port == 0) {
            continue;
        }
        let iface_ref = primary.map(|iface| socket::IfaceRef {
            name: iface.name.as_str(),
            index: iface.index,
        });
        let attempt = runner.probe(target, iface_ref);
        let (kind, name) = match target.protocol {
            crate::config::ProbeProtocol::Tcp => {
                (SignalKind::TcpProbe, format!("TCP {}", target.name))
            }
            crate::config::ProbeProtocol::UdpDns => {
                (SignalKind::DnsProbe, format!("DNS {}", target.name))
            }
            crate::config::ProbeProtocol::Icmp => {
                (SignalKind::PingProbe, format!("Ping {}", target.name))
            }
        };
        let bind_note = if let Some(iface) = primary {
            if attempt.bound {
                format!("已绑定 {}", iface.name)
            } else {
                format!("未绑定 {}", iface.name)
            }
        } else {
            "未找到物理网卡".to_string()
        };
        let detail = if attempt.detail.contains("已绑定") {
            attempt.detail.clone()
        } else {
            format!("{}；{}", attempt.detail, bind_note)
        };
        signals.push(SignalResult {
            kind,
            name,
            ok: attempt.ok,
            weight: target.weight.min(100),
            detail,
        });
    }

    // 3) Wi-Fi SSID
    if let Some(ssid) = &input.ssid {
        let lower = ssid.to_ascii_lowercase();
        let ok = lower.contains("fdu") || lower.contains("fudan");
        signals.push(SignalResult {
            kind: SignalKind::Wifi,
            name: "Wi-Fi SSID".to_string(),
            ok,
            weight: if ok { 70 } else { 20 },
            detail: if ok {
                format!("已连接复旦校园网：{ssid}")
            } else {
                format!("当前 SSID：{ssid}")
            },
        });
    }

    let (on_campus, confidence) = score(&signals);

    CampusStatus {
        on_campus,
        confidence,
        signals,
        checked_at: Utc::now(),
        interface: primary.map(|iface| iface.name.clone()),
        manual_override: None,
    }
}

fn score(signals: &[SignalResult]) -> (bool, f32) {
    if signals.is_empty() {
        return (false, 0.0);
    }
    // 校内网关 ping 通 ⇒ 确定性处于校园内网：100% 置信度，直接跳过 VPN、校内流量全直连
    if signals
        .iter()
        .any(|signal| signal.ok && signal.kind == SignalKind::PingProbe)
    {
        return (true, 1.0);
    }
    let total: u32 = signals.iter().map(|signal| u32::from(signal.weight)).sum();
    let positive: u32 = signals
        .iter()
        .filter(|signal| signal.ok)
        .map(|signal| u32::from(signal.weight))
        .sum();
    let ratio = if total == 0 {
        0.0
    } else {
        positive as f32 / total as f32
    };
    // 强信号：本地 IP 段命中 / 复旦 SSID / 高权重内网探针成功
    let local_evidence = signals
        .iter()
        .any(|signal| signal.ok && matches!(signal.kind, SignalKind::IpRange | SignalKind::Wifi));
    let probe_evidence = signals
        .iter()
        .any(|signal| signal.ok && signal.weight >= 90);
    let strong = local_evidence || probe_evidence;
    let on_campus = strong || ratio >= 0.5;
    let confidence = if on_campus {
        if strong {
            // 已有确定性证据（校内 IP/复旦 SSID/可达的校园内网探针），
            // 即使其余信号失败（如经 NAT 路由器上网、魔改 TUN 干扰）也保持高置信度。
            ratio.clamp(0.9, 1.0)
        } else {
            ratio.clamp(0.6, 0.89)
        }
    } else {
        ratio.min(0.5)
    };
    (on_campus, confidence)
}

/// 判断 IP 是否落在 CIDR 内（v4/v6）。
pub fn ip_in_cidr(ip: IpAddr, cidr: &str) -> bool {
    match ip {
        IpAddr::V4(v4) => match util::parse_cidr_v4(cidr) {
            Some((net, prefix)) => {
                let mask = if prefix == 0 {
                    0u32
                } else {
                    u32::MAX << (32 - prefix as u32)
                };
                (u32::from(v4) & mask) == u32::from(net)
            }
            None => false,
        },
        IpAddr::V6(v6) => match util::parse_cidr_v6(cidr) {
            Some((net, prefix)) => {
                let mask = if prefix == 0 {
                    0u128
                } else {
                    u128::MAX << (128 - prefix as u32)
                };
                (u128::from(v6) & mask) == u128::from(net)
            }
            None => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{default_campus_cidrs, default_probe_targets, ProbeProtocol};
    use std::collections::HashMap;

    struct MockRunner {
        answers: HashMap<String, bool>,
    }

    impl ProbeRunner for MockRunner {
        fn probe(
            &self,
            target: &ProbeTarget,
            _iface: Option<socket::IfaceRef<'_>>,
        ) -> ProbeAttempt {
            let ok = self.answers.get(&target.host).copied().unwrap_or(false);
            ProbeAttempt {
                ok,
                elapsed_ms: 5,
                bound: true,
                detail: if ok { "5ms".into() } else { "timeout".into() },
            }
        }
    }

    fn campus_iface() -> InterfaceInfo {
        InterfaceInfo::wifi(
            "en0",
            Some(12),
            Some(IpAddr::V4("10.188.1.10".parse().unwrap())),
            Some(19),
            false,
        )
    }

    impl InterfaceInfo {
        fn wifi(
            name: &str,
            index: Option<u32>,
            ip: Option<IpAddr>,
            prefix: Option<u8>,
            is_virtual: bool,
        ) -> Self {
            Self {
                name: name.to_string(),
                index,
                ip,
                prefix,
                is_virtual,
            }
        }
    }

    #[test]
    fn detects_campus_by_ip_range() {
        let ifaces = vec![
            campus_iface(),
            InterfaceInfo::wifi(
                "utun11",
                Some(20),
                Some("198.18.0.1".parse().unwrap()),
                Some(24),
                true,
            ),
        ];
        let runner = MockRunner {
            answers: HashMap::new(),
        };
        let input = ProbeInput {
            targets: default_probe_targets(),
            campus_cidrs: default_campus_cidrs(),
            ..Default::default()
        };
        let status = detect_with(&runner, &ifaces, &input);
        assert!(status.on_campus, "{:#?}", status.signals);
        assert_eq!(status.interface.as_deref(), Some("en0"));
        assert!(status.confidence >= 0.6);
    }

    #[test]
    fn detects_campus_by_probe_off_campus_ip() {
        let ifaces = vec![InterfaceInfo::wifi(
            "en0",
            Some(12),
            Some("192.168.1.10".parse().unwrap()),
            Some(24),
            false,
        )];
        let mut answers = HashMap::new();
        answers.insert("10.176.52.2".to_string(), true);
        let runner = MockRunner { answers };
        let input = ProbeInput {
            targets: default_probe_targets(),
            campus_cidrs: default_campus_cidrs(),
            ..Default::default()
        };
        let status = detect_with(&runner, &ifaces, &input);
        assert!(status.on_campus);
        assert!(
            status.confidence >= 0.9,
            "校园内网探针可达应给出高置信度：{:#?}",
            status.signals
        );
        assert!(status
            .signals
            .iter()
            .any(|signal| signal.kind == SignalKind::TcpProbe && signal.ok));
    }

    #[test]
    fn ping_campus_gateway_means_100_percent_campus() {
        // 经 NAT 路由器上网（本地 IP 非校内段），但能 ping 通校内网关 10.134.140.1
        let ifaces = vec![InterfaceInfo::wifi(
            "en0",
            Some(12),
            Some("192.168.1.10".parse().unwrap()),
            Some(24),
            false,
        )];
        let mut answers = HashMap::new();
        answers.insert("10.134.140.1".to_string(), true);
        let runner = MockRunner { answers };
        let input = ProbeInput {
            targets: default_probe_targets(),
            campus_cidrs: default_campus_cidrs(),
            ..Default::default()
        };
        let status = detect_with(&runner, &ifaces, &input);
        assert!(status.on_campus);
        assert_eq!(status.confidence, 1.0);
        assert!(status
            .signals
            .iter()
            .any(|signal| signal.kind == SignalKind::PingProbe && signal.ok));
    }

    #[test]
    fn off_campus_when_nothing_matches() {
        let ifaces = vec![InterfaceInfo::wifi(
            "en0",
            Some(12),
            Some("192.168.1.10".parse().unwrap()),
            Some(24),
            false,
        )];
        let runner = MockRunner {
            answers: HashMap::new(),
        };
        let input = ProbeInput {
            targets: default_probe_targets(),
            campus_cidrs: default_campus_cidrs(),
            ..Default::default()
        };
        let status = detect_with(&runner, &ifaces, &input);
        assert!(!status.on_campus);
        assert!(status.confidence <= 0.5);
    }

    #[test]
    fn manual_override_wins() {
        let status = CampusStatus {
            on_campus: false,
            confidence: 0.0,
            signals: vec![],
            checked_at: Utc::now(),
            interface: None,
            manual_override: None,
        };
        let forced = status.clone().apply_manual_override(Some(true));
        assert!(forced.on_campus);
        assert_eq!(forced.confidence, 1.0);
        assert!(forced.signals.iter().any(|s| s.kind == SignalKind::Manual));
    }

    #[test]
    fn computes_local_subnets() {
        let ifaces = vec![
            campus_iface(),
            InterfaceInfo::wifi(
                "en0",
                None,
                Some("10.188.1.10".parse().unwrap()),
                Some(19),
                false,
            ),
            InterfaceInfo::wifi(
                "utun11",
                None,
                Some("198.18.0.1".parse().unwrap()),
                Some(24),
                true,
            ),
        ];
        let subnets = local_subnets(&ifaces);
        assert_eq!(subnets, vec!["10.188.0.0/19"]);
    }

    #[test]
    fn wifi_ssid_signal() {
        let runner = MockRunner {
            answers: HashMap::new(),
        };
        let input = ProbeInput {
            targets: vec![],
            campus_cidrs: vec![],
            interface: None,
            ssid: Some("fduwireless".to_string()),
        };
        let status = detect_with(&runner, &[], &input);
        assert!(status.on_campus);
        let other = ProbeInput {
            ssid: Some("HomeWiFi".to_string()),
            ..Default::default()
        };
        let status = detect_with(&runner, &[], &other);
        assert!(!status.on_campus);
    }

    #[test]
    fn ip_in_cidr_works_for_both_families() {
        assert!(ip_in_cidr("10.188.1.10".parse().unwrap(), "10.188.0.0/16"));
        assert!(!ip_in_cidr("10.230.1.10".parse().unwrap(), "10.188.0.0/16"));
        assert!(ip_in_cidr(
            "2001:da8:8001:844::1".parse().unwrap(),
            "2001:da8:8001::/48"
        ));
    }

    #[test]
    fn proto_default_is_tcp() {
        assert_eq!(ProbeProtocol::default(), ProbeProtocol::Tcp);
    }
}
