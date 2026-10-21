//! 内置规则集（rule-provider 数据）与 provider 规格定义。
//!
//! 规则数据文件位于仓库 `resources/rules/`，通过 `include_str!` 编译进库，
//! 这样应用层在首次启动时可以把它们写入 mihomo 工作目录，无需额外资源打包。

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

/// 局域网 / 保留网段
pub const LAN: &str = "LAN";
/// 复旦域名
pub const FDU_DOMAIN: &str = "FDUDomain";
/// 复旦内网 IP
pub const FDU_IP: &str = "FDUIP";
/// 广告拦截
pub const AD_BLOCK: &str = "AdBlock";
/// 中国大陆域名离线兜底
pub const CHINA_DOMAIN_FALLBACK: &str = "ChinaDomainFallback";

const FDU_DOMAIN_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/rules/fdu-domain.yaml"
));
const FDU_IP_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/rules/fdu-ip.yaml"
));
const LAN_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/rules/lan.yaml"
));
const ADBLOCK_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/rules/adblock.yaml"
));
const CHINA_FALLBACK_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/rules/china-domain-fallback.yaml"
));

/// 一个内置规则集。
pub struct BuiltinProvider {
    /// provider 名称（规则里 `RULE-SET,<name>` 引用）
    pub name: &'static str,
    /// 写入工作目录时使用的文件名（位于 `core/rules/` 下）
    pub file: &'static str,
    /// mihomo behavior，目前均为 classical
    pub behavior: &'static str,
    /// 说明（用于文档/UI）
    pub description: &'static str,
    /// YAML 内容
    pub content: &'static str,
}

static BUILTIN: [BuiltinProvider; 5] = [
    BuiltinProvider {
        name: LAN,
        file: "lan.yaml",
        behavior: "classical",
        description: "局域网 / 保留网段直连",
        content: LAN_YAML,
    },
    BuiltinProvider {
        name: FDU_DOMAIN,
        file: "fdu-domain.yaml",
        behavior: "classical",
        description: "复旦及附属组织域名",
        content: FDU_DOMAIN_YAML,
    },
    BuiltinProvider {
        name: FDU_IP,
        file: "fdu-ip.yaml",
        behavior: "classical",
        description: "复旦内网 IP（10.0.0.0/8）",
        content: FDU_IP_YAML,
    },
    BuiltinProvider {
        name: AD_BLOCK,
        file: "adblock.yaml",
        behavior: "classical",
        description: "广告 / 追踪域名",
        content: ADBLOCK_YAML,
    },
    BuiltinProvider {
        name: CHINA_DOMAIN_FALLBACK,
        file: "china-domain-fallback.yaml",
        behavior: "classical",
        description: "中国大陆常见域名（离线兜底）",
        content: CHINA_FALLBACK_YAML,
    },
];

/// 所有内置规则集。
pub fn builtin_providers() -> &'static [BuiltinProvider] {
    &BUILTIN
}

/// 按名称查找内置规则集。
pub fn builtin_provider(name: &str) -> Option<&'static BuiltinProvider> {
    BUILTIN.iter().find(|provider| provider.name == name)
}

/// provider 的加载方式。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ProviderSource {
    /// 本地文件（相对 mihomo 工作目录）
    File {
        /// 相对路径，如 `./rules/lan.yaml`
        path: String,
    },
    /// 远程 HTTP 规则集
    Http {
        /// URL
        url: String,
        /// 相对工作目录的缓存路径
        path: String,
    },
}

/// 生成到 mihomo 配置中的 rule-provider 规格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSpec {
    /// provider 名称
    pub name: String,
    /// 来源
    pub source: ProviderSource,
    /// behavior：classical / domain / ipcidr
    pub behavior: String,
    /// 格式：yaml / text
    pub format: String,
    /// 远程更新间隔（秒），本地文件为 0
    pub interval: u32,
}

impl ProviderSpec {
    /// 本地文件 provider。
    pub fn local(name: impl Into<String>, behavior: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: ProviderSource::File { path: path.into() },
            behavior: behavior.into(),
            format: "yaml".to_string(),
            interval: 0,
        }
    }

    /// 远程 provider。
    pub fn remote(
        name: impl Into<String>,
        behavior: impl Into<String>,
        url: impl Into<String>,
        path: impl Into<String>,
        interval: u32,
    ) -> Self {
        Self {
            name: name.into(),
            source: ProviderSource::Http {
                url: url.into(),
                path: path.into(),
            },
            behavior: behavior.into(),
            format: "yaml".to_string(),
            interval,
        }
    }

    /// 规则集在 mihomo 工作目录内的相对路径。
    pub fn rel_path(&self) -> &str {
        match &self.source {
            ProviderSource::File { path } | ProviderSource::Http { path, .. } => path,
        }
    }
}

/// 解析 YAML 规则集内容为 payload 行列表（用于离线命中模拟）。
pub fn payload_from_yaml(content: &str) -> CoreResult<Vec<String>> {
    #[derive(Deserialize)]
    struct RuleFile {
        payload: Vec<String>,
    }
    let parsed: RuleFile = serde_yaml_ng::from_str(content)
        .map_err(|err| CoreError::Rule(format!("规则集 YAML 解析失败: {err}")))?;
    Ok(parsed.payload)
}

/// 某个规则集的实际规则列表（用于命中模拟）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPayload {
    /// provider 名称
    pub name: String,
    /// payload 行，例如 `DOMAIN-SUFFIX,fudan.edu.cn`
    pub rules: Vec<String>,
}

/// 加载全部内置规则集的 payload。
pub fn builtin_payloads() -> CoreResult<Vec<ProviderPayload>> {
    let mut payloads = Vec::with_capacity(BUILTIN.len());
    for provider in &BUILTIN {
        payloads.push(ProviderPayload {
            name: provider.name.to_string(),
            rules: payload_from_yaml(provider.content)?,
        });
    }
    Ok(payloads)
}

/// 常用远程规则集（增强规则集开关打开时启用）。
pub fn enhanced_providers() -> Vec<ProviderSpec> {
    vec![
        ProviderSpec::remote(
            "ChinaDomainRemote",
            "classical",
            "https://testingcf.jsdelivr.net/gh/ACL4SSR/ACL4SSR@master/Clash/ChinaDomain.list",
            "./rules/ChinaDomain.list",
            24 * 3600,
        ),
        ProviderSpec::remote(
            "BanADRemote",
            "classical",
            "https://testingcf.jsdelivr.net/gh/ACL4SSR/ACL4SSR@master/Clash/BanAD.list",
            "./rules/BanAD.list",
            24 * 3600,
        ),
    ]
}

/// mihomo 工作目录内规则文件清单：`(文件名, 内容)`。
pub fn builtin_rule_files() -> Vec<(&'static str, &'static str)> {
    BUILTIN.iter().map(|provider| (provider.file, provider.content)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_payloads_parse() {
        let payloads = builtin_payloads().expect("parse builtin payloads");
        assert_eq!(payloads.len(), BUILTIN.len());
        for payload in payloads {
            assert!(!payload.rules.is_empty(), "{} 规则为空", payload.name);
        }
    }

    #[test]
    fn fdu_rules_cover_requirement() {
        let payloads = builtin_payloads().expect("parse");
        let fdu_domain = payloads
            .iter()
            .find(|p| p.name == FDU_DOMAIN)
            .expect("fdu domain");
        assert!(fdu_domain
            .rules
            .iter()
            .any(|rule| rule == "DOMAIN-SUFFIX,fudan.edu.cn"));
        let fdu_ip = payloads.iter().find(|p| p.name == FDU_IP).expect("fdu ip");
        assert!(fdu_ip.rules.iter().any(|rule| rule == "IP-CIDR,10.0.0.0/8"));
    }
}
