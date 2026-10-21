//! 订阅内容解析：格式识别、Clash YAML 概要、流量信息、分享链接检测。
//!
//! 本模块只做**纯解析**；网络拉取在应用层完成。

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::config::TrafficInfo;
use crate::error::{CoreError, CoreResult};

/// 订阅格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubscriptionFormat {
    /// Clash / mihomo YAML
    ClashYaml,
    /// Base64 编码的分享链接集合（v2rayN 等机场常见）
    Base64Links,
    /// 明文分享链接（每行一条）
    PlainLinks,
    /// 无法识别
    Unknown,
}

impl SubscriptionFormat {
    /// 中文展示名。
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::ClashYaml => "Clash / mihomo 配置",
            Self::Base64Links => "Base64 分享链接",
            Self::PlainLinks => "分享链接",
            Self::Unknown => "未知格式",
        }
    }
}

/// Clash 订阅概要。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClashProfile {
    /// 节点名称（保留顺序）
    pub proxy_names: Vec<String>,
    /// 代理组名称
    pub group_names: Vec<String>,
    /// proxy-provider 名称
    pub provider_names: Vec<String>,
    /// 规则条数
    pub rule_count: usize,
}

impl ClashProfile {
    /// 节点数量。
    pub fn proxy_count(&self) -> usize {
        self.proxy_names.len()
    }
}

/// 订阅概要（应用层用于展示）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionSummary {
    /// 识别到的格式
    pub format: SubscriptionFormat,
    /// 节点数量
    pub proxy_count: usize,
    /// 代理组数量
    pub group_count: usize,
    /// 节点名称（最多 200 个，供 UI 预览）
    pub proxy_names: Vec<String>,
    /// 流量信息
    pub traffic: Option<TrafficInfo>,
    /// 解析警告
    pub warnings: Vec<String>,
}

/// 识别订阅内容格式。
pub fn detect_format(content: &str) -> SubscriptionFormat {
    let trimmed = content.trim().trim_start_matches('\u{feff}');
    if trimmed.is_empty() {
        return SubscriptionFormat::Unknown;
    }
    // 优先尝试 YAML（Clash）
    if looks_like_clash_yaml(trimmed) {
        return SubscriptionFormat::ClashYaml;
    }
    // Base64（去掉空白后解码，内容含 "://"）
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.len() > 24
        && compact
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'))
    {
        if let Ok(decoded) = decode_base64(&compact) {
            if decoded.contains("://") {
                return SubscriptionFormat::Base64Links;
            }
        }
    }
    // 明文分享链接
    let has_link = trimmed
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .any(|line| line.contains("://"));
    if has_link {
        return SubscriptionFormat::PlainLinks;
    }
    SubscriptionFormat::Unknown
}

fn looks_like_clash_yaml(content: &str) -> bool {
    let first_meaningful = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'));
    let Some(first) = first_meaningful else {
        return false;
    };
    let key_hit = [
        "proxies",
        "proxy-groups",
        "proxy-providers",
        "rules",
        "mixed-port",
    ]
    .iter()
    .any(|key| first.starts_with(&format!("{key}:")) || first.starts_with(&format!("\"{key}\":")));
    if key_hit {
        return true;
    }
    // 兜底：能解析且包含 proxies / proxy-groups 键
    serde_yaml_ng::from_str::<serde_yaml_ng::Value>(content)
        .map(|value| {
            value.get("proxies").is_some()
                || value.get("proxy-groups").is_some()
                || value.get("proxy-providers").is_some()
        })
        .unwrap_or(false)
}

/// 解析 Clash 订阅概要。
pub fn parse_clash(content: &str) -> CoreResult<ClashProfile> {
    let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(content)?;
    let mut profile = ClashProfile::default();
    if let Some(proxies) = value
        .get("proxies")
        .and_then(serde_yaml_ng::Value::as_sequence)
    {
        profile.proxy_names = proxies
            .iter()
            .filter_map(|proxy| {
                proxy
                    .get("name")
                    .and_then(serde_yaml_ng::Value::as_str)
                    .map(str::to_string)
            })
            .collect();
    }
    if let Some(groups) = value
        .get("proxy-groups")
        .and_then(serde_yaml_ng::Value::as_sequence)
    {
        profile.group_names = groups
            .iter()
            .filter_map(|group| {
                group
                    .get("name")
                    .and_then(serde_yaml_ng::Value::as_str)
                    .map(str::to_string)
            })
            .collect();
    }
    if let Some(providers) = value
        .get("proxy-providers")
        .and_then(serde_yaml_ng::Value::as_mapping)
    {
        profile.provider_names = providers
            .keys()
            .filter_map(serde_yaml_ng::Value::as_str)
            .map(str::to_string)
            .collect();
    }
    profile.rule_count = value
        .get("rules")
        .and_then(serde_yaml_ng::Value::as_sequence)
        .map(|rules| rules.len())
        .unwrap_or(0);
    Ok(profile)
}

/// 解析订阅内容，生成 UI 展示用概要。
pub fn summarize(content: &str) -> CoreResult<SubscriptionSummary> {
    let format = detect_format(content);
    let mut warnings = Vec::new();
    match format {
        SubscriptionFormat::ClashYaml => {
            let profile = parse_clash(content)?;
            if profile.proxy_count() == 0 && profile.provider_names.is_empty() {
                warnings.push("订阅中没有找到节点（proxies 为空）".to_string());
            }
            Ok(SubscriptionSummary {
                format,
                proxy_count: profile.proxy_count(),
                group_count: profile.group_names.len(),
                proxy_names: profile.proxy_names.into_iter().take(200).collect(),
                traffic: None,
                warnings,
            })
        }
        SubscriptionFormat::Base64Links | SubscriptionFormat::PlainLinks => {
            let links = if format == SubscriptionFormat::Base64Links {
                let compact: String = content.chars().filter(|c| !c.is_whitespace()).collect();
                decode_base64(&compact).unwrap_or_default()
            } else {
                content.to_string()
            };
            let count = links.lines().filter(|line| line.contains("://")).count();
            warnings.push(
                "检测到分享链接订阅：当前版本仅支持 Clash/mihomo YAML，请使用机场的 Clash 订阅链接"
                    .to_string(),
            );
            Ok(SubscriptionSummary {
                format,
                proxy_count: count,
                group_count: 0,
                proxy_names: Vec::new(),
                traffic: None,
                warnings,
            })
        }
        SubscriptionFormat::Unknown => Err(CoreError::Subscription(
            "无法识别订阅格式：既不是 Clash YAML，也不是分享链接".to_string(),
        )),
    }
}

/// 解析 `subscription-userinfo` 响应头。
///
/// 形如 `upload=123; download=456; total=789; expire=1700000000`
pub fn parse_userinfo(header: &str) -> Option<TrafficInfo> {
    let mut traffic = TrafficInfo::default();
    let mut found_any = false;
    for pair in header.split([';', ',']) {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = value.trim().parse::<i64>().ok()?;
        match key.trim().to_ascii_lowercase().as_str() {
            "upload" => {
                traffic.upload = value.max(0) as u64;
                found_any = true;
            }
            "download" => {
                traffic.download = value.max(0) as u64;
                found_any = true;
            }
            "total" => {
                traffic.total = value.max(0) as u64;
                found_any = true;
            }
            "expire" => {
                traffic.expire = (value > 0).then_some(value);
                found_any = true;
            }
            _ => {}
        }
    }
    found_any.then_some(traffic)
}

/// 解码 Base64（兼容 URL-safe、有无 padding）。
pub fn decode_base64(input: &str) -> Result<String, base64::DecodeError> {
    let normalized: String = input
        .trim()
        .replace('-', "+")
        .replace('_', "/")
        .trim_end_matches('=')
        .to_string();
    let bytes = base64::engine::general_purpose::STANDARD_NO_PAD.decode(normalized)?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

/// 判断内容是否像 Clash 配置（供应用层做格式提示）。
pub fn is_clash_yaml(content: &str) -> bool {
    detect_format(content) == SubscriptionFormat::ClashYaml
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLASH_FIXTURE: &str = include_str!("../../tests/fixtures/clash-sub.yaml");
    const BASE64_FIXTURE: &str = include_str!("../../tests/fixtures/base64-sub.txt");

    #[test]
    fn detects_clash_yaml() {
        assert_eq!(detect_format(CLASH_FIXTURE), SubscriptionFormat::ClashYaml);
    }

    #[test]
    fn detects_base64_links() {
        assert_eq!(
            detect_format(BASE64_FIXTURE),
            SubscriptionFormat::Base64Links
        );
    }

    #[test]
    fn parses_clash_profile() {
        let profile = parse_clash(CLASH_FIXTURE).expect("parse");
        assert_eq!(profile.proxy_names.len(), 3);
        assert_eq!(profile.proxy_names[0], "🇭🇰 香港 01");
        assert_eq!(profile.group_names.len(), 1);
        assert_eq!(profile.rule_count, 4);
    }

    #[test]
    fn summarizes_clash_without_warnings() {
        let summary = summarize(CLASH_FIXTURE).expect("summarize");
        assert_eq!(summary.proxy_count, 3);
        assert!(summary.warnings.is_empty());
    }

    #[test]
    fn rejects_unknown_format() {
        assert!(summarize("hello world").is_err());
    }

    #[test]
    fn parses_userinfo_header() {
        let traffic =
            parse_userinfo("upload=1024; download=2048; total=10737418240; expire=1800000000")
                .expect("traffic");
        assert_eq!(traffic.upload, 1024);
        assert_eq!(traffic.download, 2048);
        assert_eq!(traffic.total, 10_737_418_240);
        assert_eq!(traffic.expire, Some(1_800_000_000));
        assert_eq!(traffic.used(), 3072);
    }

    #[test]
    fn parses_negative_expire_as_none() {
        let traffic = parse_userinfo("expire=-1; total=0").expect("traffic");
        assert_eq!(traffic.expire, None);
    }

    #[test]
    fn base64_decode_supports_urlsafe() {
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("ss://abc");
        assert_eq!(decode_base64(&encoded).expect("decode"), "ss://abc");
    }

    #[test]
    fn format_labels_are_human_readable() {
        assert!(SubscriptionFormat::ClashYaml.label_zh().contains("Clash"));
        assert!(SubscriptionFormat::Unknown.label_zh().contains("未知"));
    }
}
