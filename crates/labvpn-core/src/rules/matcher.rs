//! 规则命中的离线模拟（用于「分流诊断」）。
//!
//! 说明：DOMAIN / IP-CIDR / RULE-SET 可以离线精确判断；
//! GEOIP / GEOSITE / PROCESS-NAME 依赖 mihomo 内核数据，此处只给出提示不做断言。

use std::net::IpAddr;

use crate::rules::providers::ProviderPayload;
use crate::rules::RulePlan;
use crate::util;

/// 诊断输入。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Input {
    Ip(IpAddr),
    Domain(String),
}

impl Input {
    fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        if let Some(ip) = util::parse_ip(trimmed) {
            return Self::Ip(ip);
        }
        Self::Domain(util::normalize_domain(
            trimmed
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or(trimmed)
                .split(':')
                .next()
                .unwrap_or(trimmed),
        ))
    }

    fn as_display(&self) -> String {
        match self {
            Self::Ip(ip) => ip.to_string(),
            Self::Domain(domain) => domain.clone(),
        }
    }
}

/// 规则匹配结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchOutcome {
    /// 输入（规范化后）
    pub input: String,
    /// 是否命中某条规则
    pub matched: bool,
    /// 命中的规则行（或 `MATCH` 兜底）
    pub rule: String,
    /// 出口策略 / 代理组
    pub outbound: String,
    /// 诊断备注
    pub notes: Vec<String>,
}

/// 对给定目标执行离线规则模拟。
pub fn match_target(input: &str, plan: &RulePlan, payloads: &[ProviderPayload]) -> MatchOutcome {
    let parsed = Input::parse(input);
    let mut notes = Vec::new();
    let mut seen_geo_note = false;

    for rule in &plan.rules {
        let parts: Vec<&str> = rule.split(',').map(str::trim).collect();
        if parts.is_empty() {
            continue;
        }
        let kind = parts[0].to_ascii_uppercase();
        match kind.as_str() {
            "MATCH" => {
                let outbound = parts.get(1).copied().unwrap_or("DIRECT").to_string();
                return MatchOutcome {
                    input: parsed.as_display(),
                    matched: true,
                    rule: rule.clone(),
                    outbound,
                    notes,
                };
            }
            "RULE-SET" => {
                let Some(provider_name) = parts.get(1) else {
                    continue;
                };
                let outbound = parts.get(2).copied().unwrap_or("DIRECT").to_string();
                let Some(payload) = payloads.iter().find(|p| p.name == *provider_name) else {
                    notes.push(format!("规则集 {provider_name} 为远程规则集，离线无法精确匹配"));
                    continue;
                };
                if let Some(pattern) = payload
                    .rules
                    .iter()
                    .find(|pattern| matches_pattern(pattern, &parsed))
                {
                    return MatchOutcome {
                        input: parsed.as_display(),
                        matched: true,
                        rule: format!("RULE-SET,{provider_name} ({pattern})"),
                        outbound,
                        notes,
                    };
                }
            }
            "DOMAIN" | "DOMAIN-SUFFIX" | "DOMAIN-KEYWORD" => {
                if matches_pattern(rule, &parsed) {
                    return MatchOutcome {
                        input: parsed.as_display(),
                        matched: true,
                        rule: rule.clone(),
                        outbound: parts.get(2).copied().unwrap_or("DIRECT").to_string(),
                        notes,
                    };
                }
            }
            "IP-CIDR" | "IP-CIDR6" => {
                if matches!(parsed, Input::Ip(_)) {
                    if matches_pattern(rule, &parsed) {
                        return MatchOutcome {
                            input: parsed.as_display(),
                            matched: true,
                            rule: rule.clone(),
                            outbound: parts.get(2).copied().unwrap_or("DIRECT").to_string(),
                            notes,
                        };
                    }
                } else if !rule.contains("no-resolve") {
                    notes.push(format!("规则 {rule} 需要解析域名后判定，离线模拟跳过"));
                }
            }
            "GEOIP" | "GEOIP6" | "GEOSITE" => {
                if !seen_geo_note {
                    notes.push(format!(
                        "{kind} 规则依赖 mihomo 的 geo 数据，离线模拟无法精确判定；以实际连接为准"
                    ));
                    seen_geo_note = true;
                }
            }
            "PROCESS-NAME" => {
                if !seen_geo_note {
                    notes.push("PROCESS-NAME 规则依赖实际进程信息，离线模拟跳过".to_string());
                    seen_geo_note = true;
                }
            }
            _ => {}
        }
    }

    MatchOutcome {
        input: parsed.as_display(),
        matched: false,
        rule: String::new(),
        outbound: "DIRECT".to_string(),
        notes,
    }
}

fn matches_pattern(pattern: &str, input: &Input) -> bool {
    let parts: Vec<&str> = pattern.split(',').map(str::trim).collect();
    if parts.len() < 2 {
        return false;
    }
    let kind = parts[0].to_ascii_uppercase();
    let value = parts[1];
    match kind.as_str() {
        "DOMAIN" => match input {
            Input::Domain(domain) => domain == &util::normalize_domain(value),
            Input::Ip(_) => false,
        },
        "DOMAIN-SUFFIX" => match input {
            Input::Domain(domain) => {
                let suffix = util::normalize_domain(value.trim_start_matches('.'));
                domain == &suffix || domain.ends_with(&format!(".{suffix}"))
            }
            Input::Ip(_) => false,
        },
        "DOMAIN-KEYWORD" => match input {
            Input::Domain(domain) => domain.contains(&util::normalize_domain(value)),
            Input::Ip(_) => false,
        },
        "IP-CIDR" | "IP-CIDR6" => match input {
            Input::Ip(ip) => {
                if ip.is_ipv4() {
                    match (util::parse_cidr_v4(value), ip) {
                        (Some((net, prefix)), IpAddr::V4(ip)) => {
                            let mask = if prefix == 0 {
                                0u32
                            } else {
                                u32::MAX << (32 - prefix)
                            };
                            (u32::from(ip) & mask) == u32::from(net)
                        }
                        _ => false,
                    }
                } else {
                    match (util::parse_cidr_v6(value), ip) {
                        (Some((net, prefix)), IpAddr::V6(ip)) => {
                            let mask = if prefix == 0 {
                                0u128
                            } else {
                                u128::MAX << (128 - prefix)
                            };
                            (u128::from(ip) & mask) == u128::from(net)
                        }
                        _ => false,
                    }
                }
            }
            Input::Domain(_) => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, defaults};
    use crate::rules::{RuleContext, build};

    fn plan_fixture() -> RulePlan {
        let mut config = AppConfig::default();
        config.campus_vpn.enabled = true;
        config.campus_vpn.username = "tester".into();
        config.subscription.enabled = true;
        config.subscription.url = "https://example.com/sub".into();
        let ctx = RuleContext {
            on_campus: false,
            vpn_configured: true,
            vpn_running: true,
            vpn_server_host: defaults::VPN_SERVER.into(),
            vpn_server_ips: vec!["202.120.224.58".into()],
            vpn_process_name: "fdu-connect".into(),
            core_process_name: "mihomo".into(),
            ssh_configured: true,
            local_subnets: vec!["192.168.1.0/24".into()],
            subscription_available: true,
        };
        build(&config, &ctx)
    }

    #[test]
    fn matches_campus_domain_via_provider() {
        let plan = plan_fixture();
        let payloads = crate::rules::providers::builtin_payloads().expect("payloads");
        let outcome = match_target("https://www.fudan.edu.cn/", &plan, &payloads);
        assert!(outcome.matched);
        assert!(outcome.rule.contains("FDUDomain"));
        assert_eq!(outcome.outbound, crate::rules::names::GROUP_CAMPUS);
    }

    #[test]
    fn matches_campus_ip() {
        let plan = plan_fixture();
        let payloads = crate::rules::providers::builtin_payloads().expect("payloads");
        let outcome = match_target("10.203.8.198", &plan, &payloads);
        assert!(outcome.matched);
        assert_eq!(outcome.outbound, crate::rules::names::GROUP_CAMPUS);
    }

    #[test]
    fn matches_vpn_server_direct() {
        let plan = plan_fixture();
        let payloads = crate::rules::providers::builtin_payloads().expect("payloads");
        let outcome = match_target("stuvpn.fudan.edu.cn", &plan, &payloads);
        assert!(outcome.matched);
        assert_eq!(outcome.outbound, "DIRECT");
    }

    #[test]
    fn falls_through_to_match() {
        let plan = plan_fixture();
        let payloads = crate::rules::providers::builtin_payloads().expect("payloads");
        let outcome = match_target("example.org", &plan, &payloads);
        assert!(outcome.matched);
        assert_eq!(outcome.rule, format!("MATCH,{}", crate::rules::names::GROUP_FINAL));
    }
}
