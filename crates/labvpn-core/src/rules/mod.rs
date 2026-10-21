//! 分流规则模型与规则计划构建。
//!
//! 规则顺序即优先级，由 [`build`] 唯一生成。前端只展示与编辑「自定义片段」。
//! 详细设计见 `docs/RULES.md`。

pub mod custom;
pub mod matcher;
pub mod presets;
pub mod providers;

use serde::{Deserialize, Serialize};

use crate::config::{AppConfig, RoutingPreset};
use crate::util;
use providers::enhanced_providers;
use providers::{ProviderSpec, AD_BLOCK, CHINA_DOMAIN_FALLBACK, FDU_DOMAIN, FDU_IP, LAN};

/// 固定名称常量（mihomo 配置与前端同源）。
pub mod names {
    /// 国外流量总出口（订阅）
    pub const GROUP_PROXY: &str = "🚀 代理";
    /// 延迟自动择优
    pub const GROUP_AUTO: &str = "♻️ 自动选择";
    /// 校内分流组（DIRECT / FDU VPN 切换）
    pub const GROUP_CAMPUS: &str = "🎓 校内";
    /// SSH 出站组
    pub const GROUP_SSH: &str = "🐧 SSH 转发";
    /// 广告拦截组
    pub const GROUP_ADBLOCK: &str = "🛑 广告拦截";
    /// 兜底组
    pub const GROUP_FINAL: &str = "🐟 漏网之鱼";
    /// 校内 VPN 出口（mihomo proxy 名）
    pub const PROXY_FDU: &str = "🏫 FDU-RVPN";
    /// 直连
    pub const DIRECT: &str = "DIRECT";
    /// 拒绝
    pub const REJECT: &str = "REJECT";
    /// 订阅 proxy-provider 名称
    pub const PROVIDER_SUB: &str = "sub";

    /// SSH 出口代理名：`🖥 SSH-<host>`
    pub fn ssh_proxy_name(host: &str) -> String {
        format!("🖥 SSH-{host}")
    }

    /// 全部内置代理组名称
    pub fn all_groups() -> Vec<String> {
        vec![
            GROUP_PROXY.to_string(),
            GROUP_AUTO.to_string(),
            GROUP_CAMPUS.to_string(),
            GROUP_SSH.to_string(),
            GROUP_ADBLOCK.to_string(),
            GROUP_FINAL.to_string(),
        ]
    }
}

/// 代理组类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupKind {
    /// 手动选择
    Select,
    /// 延迟自动测试
    UrlTest,
    /// 主备回退
    Fallback,
}

impl GroupKind {
    /// mihomo 配置值
    pub fn as_mihomo(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::UrlTest => "url-test",
            Self::Fallback => "fallback",
        }
    }
}

/// 生成到 mihomo 配置中的代理组规格。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSpec {
    /// 组名
    pub name: String,
    /// 类型
    pub kind: GroupKind,
    /// 固定成员
    pub members: Vec<String>,
    /// 引用的 proxy-provider
    pub providers: Vec<String>,
    /// 健康检查 URL（url-test/fallback）
    pub url: Option<String>,
    /// 测速间隔（秒）
    pub interval: u32,
    /// 容差（毫秒）
    pub tolerance: u32,
}

impl GroupSpec {
    /// select 组。
    pub fn select(name: impl Into<String>, members: &[&str]) -> Self {
        Self {
            name: name.into(),
            kind: GroupKind::Select,
            members: members.iter().map(|member| (*member).to_string()).collect(),
            providers: Vec::new(),
            url: None,
            interval: 0,
            tolerance: 0,
        }
    }

    /// 带 provider 的 select 组。
    pub fn select_with_providers(
        name: impl Into<String>,
        members: &[&str],
        providers: &[&str],
    ) -> Self {
        let mut spec = Self::select(name, members);
        spec.providers = providers
            .iter()
            .map(|provider| (*provider).to_string())
            .collect();
        spec
    }

    /// url-test 组。
    pub fn url_test(
        name: impl Into<String>,
        providers: &[&str],
        url: &str,
        interval: u32,
        tolerance: u32,
    ) -> Self {
        Self {
            name: name.into(),
            kind: GroupKind::UrlTest,
            members: Vec::new(),
            providers: providers
                .iter()
                .map(|provider| (*provider).to_string())
                .collect(),
            url: Some(url.to_string()),
            interval,
            tolerance,
        }
    }
}

/// 规则构建时的运行时上下文（不影响持久化配置）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuleContext {
    /// 当前是否在校园网
    pub on_campus: bool,
    /// 是否配置了校内 VPN
    pub vpn_configured: bool,
    /// 校内 VPN 是否正在运行
    pub vpn_running: bool,
    /// 校内 VPN 服务器主机名
    pub vpn_server_host: String,
    /// 校内 VPN 服务器解析出的 IP
    pub vpn_server_ips: Vec<String>,
    /// fdu-connect 进程名
    pub vpn_process_name: String,
    /// mihomo 进程名
    pub core_process_name: String,
    /// SSH 代理是否可用（启用且已绑定目标）
    pub ssh_configured: bool,
    /// 本机直连子网（自动直连，避免被 TUN/校内规则误捕获）
    pub local_subnets: Vec<String>,
    /// 订阅是否可用
    pub subscription_available: bool,
}

impl Default for RuleContext {
    fn default() -> Self {
        Self {
            on_campus: false,
            vpn_configured: false,
            vpn_running: false,
            vpn_server_host: String::new(),
            vpn_server_ips: Vec::new(),
            vpn_process_name: "fdu-connect".to_string(),
            core_process_name: "mihomo".to_string(),
            ssh_configured: false,
            local_subnets: Vec::new(),
            subscription_available: false,
        }
    }
}

impl RuleContext {
    /// 订阅 provider 名称（仅当订阅可用时）。
    pub fn subscription_provider(&self) -> Option<&'static str> {
        self.subscription_available.then_some(names::PROVIDER_SUB)
    }
}

/// 完整规则计划：代理组 + 规则集 + 有序规则 + 警告。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulePlan {
    /// 代理组
    pub groups: Vec<GroupSpec>,
    /// 规则集
    pub providers: Vec<ProviderSpec>,
    /// 有序规则
    pub rules: Vec<String>,
    /// 生成过程中的警告
    pub warnings: Vec<String>,
}

impl RulePlan {
    /// 规则中引用到的全部出口名称（供自定义规则校验）。
    pub fn known_targets(&self) -> Vec<String> {
        let mut targets: Vec<String> = self.groups.iter().map(|group| group.name.clone()).collect();
        targets.push(names::DIRECT.to_string());
        targets.push(names::REJECT.to_string());
        targets.push(names::PROXY_FDU.to_string());
        targets
    }
}

/// 构建规则计划（纯函数）。
pub fn build(config: &AppConfig, ctx: &RuleContext) -> RulePlan {
    let providers = build_providers(config);
    let groups = build_groups(config, ctx);
    let known_targets: Vec<String> = groups
        .iter()
        .map(|group| group.name.clone())
        .chain([
            names::DIRECT.to_string(),
            names::REJECT.to_string(),
            names::PROXY_FDU.to_string(),
        ])
        .collect();

    let adblock_enabled = adblock_enabled(config);
    let mut rules: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    push_prelude(&mut rules, config, ctx);
    push_custom(&mut rules, &mut warnings, config, &known_targets, true);
    push_unique(&mut rules, format!("RULE-SET,{LAN},{}", names::DIRECT));
    push_local_subnets(&mut rules, ctx);
    push_campus(&mut rules, config, ctx);
    push_ssh(&mut rules, config, ctx);
    push_custom(&mut rules, &mut warnings, config, &known_targets, false);

    let preset = config.routing.preset;
    let minimal = preset == RoutingPreset::Minimal;

    if !minimal {
        if adblock_enabled {
            push_unique(
                &mut rules,
                format!("RULE-SET,{AD_BLOCK},{}", names::GROUP_ADBLOCK),
            );
        }
        push_china(&mut rules, config, &mut warnings);
    }

    let final_group = if minimal && !ctx.subscription_available {
        names::DIRECT
    } else {
        names::GROUP_FINAL
    };
    push_unique(&mut rules, format!("MATCH,{final_group}"));

    collect_warnings(config, ctx, &mut warnings);
    let mut plan = RulePlan {
        groups,
        providers,
        rules,
        warnings,
    };
    plan.warnings.sort();
    plan.warnings.dedup();
    plan
}

/// 是否启用广告拦截（严格预设强制开启）。
pub fn adblock_enabled(config: &AppConfig) -> bool {
    config.routing.adblock || config.routing.preset == RoutingPreset::Strict
}

fn build_providers(config: &AppConfig) -> Vec<ProviderSpec> {
    let mut providers = vec![ProviderSpec::local(LAN, "classical", "./rules/lan.yaml")];
    providers.push(ProviderSpec::local(
        FDU_IP,
        "classical",
        "./rules/fdu-ip.yaml",
    ));
    providers.push(ProviderSpec::local(
        FDU_DOMAIN,
        "classical",
        "./rules/fdu-domain.yaml",
    ));
    if adblock_enabled(config) {
        providers.push(ProviderSpec::local(
            AD_BLOCK,
            "classical",
            "./rules/adblock.yaml",
        ));
    }
    if !config.routing.geo_rules && config.routing.preset != RoutingPreset::Minimal {
        providers.push(ProviderSpec::local(
            CHINA_DOMAIN_FALLBACK,
            "classical",
            "./rules/china-domain-fallback.yaml",
        ));
    }
    if config.routing.enhanced_rules {
        providers.extend(enhanced_providers());
    }
    providers
}

fn build_groups(config: &AppConfig, ctx: &RuleContext) -> Vec<GroupSpec> {
    let mut groups = Vec::new();
    let has_sub = ctx.subscription_available;

    // 订阅自动选择
    if has_sub {
        groups.push(GroupSpec::url_test(
            names::GROUP_AUTO,
            &[names::PROVIDER_SUB],
            "http://www.gstatic.com/generate_204",
            300,
            50,
        ));
    }
    // 国外总出口
    if has_sub {
        groups.push(GroupSpec::select_with_providers(
            names::GROUP_PROXY,
            &[names::GROUP_AUTO, names::DIRECT],
            &[names::PROVIDER_SUB],
        ));
    } else {
        groups.push(GroupSpec::select(names::GROUP_PROXY, &[names::DIRECT]));
    }

    // 校内分流组：默认出口顺序由当前网络位置决定
    let mut campus_members: Vec<&str> = Vec::new();
    if ctx.vpn_configured {
        if ctx.on_campus {
            campus_members.push(names::DIRECT);
            campus_members.push(names::PROXY_FDU);
        } else {
            campus_members.push(names::PROXY_FDU);
            campus_members.push(names::DIRECT);
        }
    } else {
        campus_members.push(names::DIRECT);
    }
    groups.push(GroupSpec::select(names::GROUP_CAMPUS, &campus_members));

    // SSH 出站组
    if ctx.ssh_configured {
        let ssh_name = names::ssh_proxy_name(config.ssh_proxy.host.trim());
        groups.push(GroupSpec::select(
            names::GROUP_SSH,
            &[ssh_name.as_str(), names::DIRECT, names::GROUP_PROXY],
        ));
    }

    // 广告拦截组
    if adblock_enabled(config) {
        groups.push(GroupSpec::select(
            names::GROUP_ADBLOCK,
            &[names::REJECT, names::DIRECT],
        ));
    }

    // 兜底组
    groups.push(GroupSpec::select(
        names::GROUP_FINAL,
        &[names::GROUP_PROXY, names::DIRECT],
    ));

    groups
}

fn push_prelude(rules: &mut Vec<String>, config: &AppConfig, ctx: &RuleContext) {
    if ctx.vpn_configured {
        let server = ctx.vpn_server_host.trim();
        if !server.is_empty() {
            if util::parse_ip(server).is_some() {
                push_unique(
                    rules,
                    format!("IP-CIDR,{server}/32,{},no-resolve", names::DIRECT),
                );
            } else {
                push_unique(rules, format!("DOMAIN,{server},{}", names::DIRECT));
            }
        }
        for ip in &ctx.vpn_server_ips {
            push_unique(
                rules,
                format!("IP-CIDR,{ip}/32,{},no-resolve", names::DIRECT),
            );
        }
        for process in process_names(&ctx.vpn_process_name) {
            push_unique(rules, format!("PROCESS-NAME,{process},{}", names::DIRECT));
        }
        let _ = config;
    }
    for process in process_names(&ctx.core_process_name) {
        push_unique(rules, format!("PROCESS-NAME,{process},{}", names::DIRECT));
    }
    push_unique(
        rules,
        format!("IP-CIDR,127.0.0.0/8,{},no-resolve", names::DIRECT),
    );
}

fn process_names(process: &str) -> Vec<String> {
    let base = util::process_name(process);
    if base.is_empty() {
        return Vec::new();
    }
    vec![base.clone(), format!("{base}.exe")]
}

fn push_local_subnets(rules: &mut Vec<String>, ctx: &RuleContext) {
    let mut subnets = ctx.local_subnets.clone();
    subnets.sort();
    subnets.dedup();
    for subnet in subnets {
        let trimmed = subnet.trim();
        if trimmed.is_empty() || trimmed == "0.0.0.0/0" || trimmed == "::/0" {
            continue;
        }
        if util::parse_cidr_v4(trimmed).is_some() {
            push_unique(
                rules,
                format!("IP-CIDR,{trimmed},{},no-resolve", names::DIRECT),
            );
        } else if util::parse_cidr_v6(trimmed).is_some() {
            push_unique(
                rules,
                format!("IP-CIDR6,{trimmed},{},no-resolve", names::DIRECT),
            );
        }
    }
}

fn push_campus(rules: &mut Vec<String>, config: &AppConfig, ctx: &RuleContext) {
    let target = if ctx.vpn_configured {
        names::GROUP_CAMPUS
    } else {
        names::DIRECT
    };
    push_unique(rules, format!("RULE-SET,{FDU_IP},{target}"));
    push_unique(rules, format!("RULE-SET,{FDU_DOMAIN},{target}"));

    // 额外校内域名：路由配置 + VPN 配置里的强制 RVPN 域名
    let mut domains: Vec<String> = config
        .routing
        .extra_campus_domains
        .iter()
        .chain(config.campus_vpn.custom_proxy_domains.iter())
        .filter(|domain| util::is_domain(domain))
        .map(|domain| util::normalize_domain(domain))
        .collect();
    domains.sort();
    domains.dedup();
    for domain in domains {
        let domain = domain.trim_start_matches("*.").trim_start_matches('.');
        if domain.is_empty() {
            continue;
        }
        push_unique(rules, format!("DOMAIN-SUFFIX,{domain},{target}"));
    }
}

fn push_ssh(rules: &mut Vec<String>, config: &AppConfig, ctx: &RuleContext) {
    if !ctx.ssh_configured {
        return;
    }
    for route in &config.ssh_proxy.routes {
        let value = route.value.trim();
        if value.is_empty() {
            continue;
        }
        if util::parse_ip(value).is_some() {
            push_unique(
                rules,
                format!("IP-CIDR,{value}/32,{},no-resolve", names::GROUP_SSH),
            );
        } else if util::parse_cidr_v4(value).is_some() {
            push_unique(
                rules,
                format!("IP-CIDR,{value},{},no-resolve", names::GROUP_SSH),
            );
        } else if util::parse_cidr_v6(value).is_some() {
            push_unique(
                rules,
                format!("IP-CIDR6,{value},{},no-resolve", names::GROUP_SSH),
            );
        } else if util::is_domain(value) {
            push_unique(
                rules,
                format!(
                    "DOMAIN-SUFFIX,{},{}",
                    util::normalize_domain(value),
                    names::GROUP_SSH
                ),
            );
        }
    }
}

fn push_custom(
    rules: &mut Vec<String>,
    warnings: &mut Vec<String>,
    config: &AppConfig,
    known_targets: &[String],
    high_priority: bool,
) {
    if config.routing.custom_rules_high_priority != high_priority {
        return;
    }
    let lines = custom::split_lines(&config.routing.custom_rules.join("\n"));
    let (normalized, custom_warnings) = custom::normalize_rules(&lines, known_targets);
    warnings.extend(custom_warnings);
    for rule in normalized {
        push_unique(rules, rule);
    }
}

fn push_china(rules: &mut Vec<String>, config: &AppConfig, warnings: &mut Vec<String>) {
    if config.routing.enhanced_rules {
        push_unique(
            rules,
            format!("RULE-SET,ChinaDomainRemote,{}", names::DIRECT),
        );
    }
    if config.routing.geo_rules {
        push_unique(rules, format!("GEOSITE,cn,{}", names::DIRECT));
        push_unique(rules, format!("GEOIP,CN,{},no-resolve", names::DIRECT));
    } else {
        push_unique(
            rules,
            format!("RULE-SET,{CHINA_DOMAIN_FALLBACK},{}", names::DIRECT),
        );
        warnings.push("已关闭 geo 规则：国内 IP 直连能力受限，仅使用内置域名兜底".to_string());
    }
}

fn collect_warnings(config: &AppConfig, ctx: &RuleContext, warnings: &mut Vec<String>) {
    if !ctx.subscription_available {
        warnings.push("未配置订阅：国外流量将没有可用出口".to_string());
    }
    if !config.campus_vpn.enabled {
        warnings.push("校内 VPN 未启用：10.0.0.0/8 将直连".to_string());
    }
    if config.ssh_proxy.enabled && !ctx.ssh_configured {
        warnings.push("SSH 代理已启用但没有绑定任何域名/IP，暂不生效".to_string());
    }
    if ctx.on_campus && config.campus_vpn.skip_when_on_campus {
        warnings.push("检测到校园网：已跳过 VPN，校内流量直连".to_string());
    }
    if !config.campus_vpn.auto_detect_campus {
        warnings.push("校园网自动检测已关闭".to_string());
    }
}

fn push_unique(rules: &mut Vec<String>, rule: String) {
    if !rules.contains(&rule) {
        rules.push(rule);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{defaults, AppConfig};

    fn base_config() -> AppConfig {
        let mut config = AppConfig::default();
        config.campus_vpn.username = "21300000000".into();
        config.subscription.enabled = true;
        config.subscription.url = "https://example.com/sub".into();
        config.ssh_proxy.enabled = true;
        config.ssh_proxy.host = "1.2.3.4".into();
        config.ssh_proxy.username = "root".into();
        config.ssh_proxy.routes.push(crate::config::SshRoute {
            value: "example.net".into(),
        });
        config
    }

    fn context() -> RuleContext {
        RuleContext {
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
        }
    }

    #[test]
    fn smart_plan_contains_required_rules_in_order() {
        let plan = build(&base_config(), &context());
        let rules = &plan.rules;
        let pos = |needle: &str| {
            rules
                .iter()
                .position(|rule| rule.contains(needle))
                .unwrap_or_else(|| panic!("缺少规则 {needle}: {rules:#?}"))
        };
        // VPN 服务器直连必须在最前
        assert!(pos(defaults::VPN_SERVER) < pos("RULE-SET,FDUIP"));
        // 10/8 在校内规则中
        assert!(pos("RULE-SET,FDUIP") < pos("RULE-SET,FDUDomain"));
        // 校内规则先于国内直连
        assert!(pos("RULE-SET,FDUDomain") < pos("GEOIP,CN"));
        // 国内直连先于兜底
        assert!(pos("GEOIP,CN") < pos("MATCH,"));
        assert_eq!(
            rules.last().expect("last rule"),
            &format!("MATCH,{}", names::GROUP_FINAL)
        );
    }

    #[test]
    fn groups_have_expected_topology() {
        let plan = build(&base_config(), &context());
        let group = |name: &str| {
            plan.groups
                .iter()
                .find(|group| group.name == name)
                .unwrap_or_else(|| panic!("缺少组 {name}"))
        };
        assert!(group(names::GROUP_PROXY)
            .providers
            .contains(&"sub".to_string()));
        assert!(group(names::GROUP_AUTO).kind == GroupKind::UrlTest);
        assert!(group(names::GROUP_CAMPUS)
            .members
            .contains(&names::PROXY_FDU.to_string()));
        assert!(group(names::GROUP_SSH)
            .members
            .iter()
            .any(|member| member.starts_with("🖥 SSH-")));
        if adblock_enabled(&base_config()) {
            assert!(group(names::GROUP_ADBLOCK)
                .members
                .contains(&names::REJECT.to_string()));
        }
    }

    #[test]
    fn on_campus_switches_default_campus_target() {
        let mut ctx = context();
        ctx.on_campus = true;
        let plan = build(&base_config(), &ctx);
        let campus = plan
            .groups
            .iter()
            .find(|group| group.name == names::GROUP_CAMPUS)
            .expect("campus group");
        assert_eq!(campus.members.first().expect("first member"), names::DIRECT);
    }

    #[test]
    fn ssh_routes_become_rules() {
        let plan = build(&base_config(), &context());
        assert!(plan
            .rules
            .iter()
            .any(|rule| rule == &format!("DOMAIN-SUFFIX,example.net,{}", names::GROUP_SSH)));
    }

    #[test]
    fn multiple_provider_ips_and_processes_are_deduped() {
        let mut ctx = context();
        ctx.vpn_server_ips = vec!["202.120.224.58".into(), "202.120.224.58".into()];
        let plan = build(&base_config(), &ctx);
        let count = plan
            .rules
            .iter()
            .filter(|rule| rule.contains("202.120.224.58/32"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn minimal_preset_has_no_china_rules() {
        let mut config = base_config();
        config.routing.preset = RoutingPreset::Minimal;
        let plan = build(&config, &context());
        assert!(!plan.rules.iter().any(|rule| rule.starts_with("GEOIP,CN")));
        assert!(!plan
            .rules
            .iter()
            .any(|rule| rule.contains("ChinaDomainFallback")));
    }

    #[test]
    fn custom_rules_are_inserted_and_validated() {
        let mut config = base_config();
        config.routing.custom_rules =
            vec!["DOMAIN,git.example.com,🚀 代理".into(), "BAD,RULE".into()];
        let plan = build(&config, &context());
        assert!(plan
            .rules
            .iter()
            .any(|rule| rule == "DOMAIN,git.example.com,🚀 代理"));
        assert!(!plan.warnings.is_empty());
    }

    #[test]
    fn disabled_vpn_makes_campus_direct() {
        let mut config = base_config();
        config.campus_vpn.enabled = false;
        let mut ctx = context();
        ctx.vpn_configured = false;
        let plan = build(&config, &ctx);
        assert!(plan
            .rules
            .iter()
            .any(|rule| rule == &format!("RULE-SET,{FDU_IP},{}", names::DIRECT)));
    }
}
