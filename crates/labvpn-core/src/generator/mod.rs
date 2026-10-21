//! mihomo 配置生成器（纯函数）。
//!
//! 输入 [`AppConfig`] + [`RuntimeContext`]，输出合法 YAML、规则计划与警告。
//! 相同输入必定产生相同输出，可用快照测试锁定。

pub mod model;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::{AppConfig, RoutingPreset};
use crate::error::CoreResult;
use crate::generator::model::{
    DnsSection, GroupSection, HealthCheck, MihomoConfig, ProfileSection, ProxyEntry, ProxyProvider,
    RuleProvider, SniffPorts, SnifferSection, SocksProxy, SshProxy, TunSection,
};
use crate::rules::{self, names, providers, RuleContext, RulePlan};

/// 订阅来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileSource {
    /// 无订阅
    #[default]
    None,
    /// mihomo 直接以 http provider 拉取
    Remote,
    /// 使用应用缓存的本地订阅文件
    Local,
}

/// SSH 运行时凭据（由应用层从密钥库解析，只在内存中）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshRuntime {
    /// 密码
    pub password: Option<String>,
    /// 私钥路径（相对 mihomo 工作目录的安全路径，如 `./keys/ssh_id`）
    pub private_key_path: Option<String>,
    /// 私钥内容（如已直接读到）
    pub private_key_content: Option<String>,
    /// 私钥口令
    pub private_key_passphrase: Option<String>,
}

/// 生成配置所需的运行时上下文（不持久化）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuntimeContext {
    /// 是否在校园网
    pub on_campus: bool,
    /// 是否配置了校内 VPN
    pub vpn_configured: bool,
    /// 校内 VPN 是否正在运行
    pub vpn_running: bool,
    /// 校内 VPN 服务器 IP（用于直连规则与路由排除）
    pub vpn_server_ips: Vec<String>,
    /// fdu-connect 进程名
    pub vpn_process_name: String,
    /// mihomo 进程名
    pub core_process_name: String,
    /// 本机直连子网
    pub local_subnets: Vec<String>,
    /// 特权 helper 是否可用（决定 TUN 是否真正启用）
    pub tun_available: bool,
    /// 控制端口密钥
    pub controller_secret: String,
    /// 订阅来源
    pub profile: ProfileSource,
    /// SSH 运行时凭据
    pub ssh_runtime: Option<SshRuntime>,
    /// 额外路由排除地址
    pub extra_route_exclude: Vec<String>,
    /// 应用层追加的警告
    pub warnings_in: Vec<String>,
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self {
            on_campus: false,
            vpn_configured: false,
            vpn_running: false,
            vpn_server_ips: Vec::new(),
            vpn_process_name: "fdu-connect".to_string(),
            core_process_name: "mihomo".to_string(),
            local_subnets: Vec::new(),
            tun_available: false,
            controller_secret: String::new(),
            profile: ProfileSource::None,
            ssh_runtime: None,
            extra_route_exclude: Vec::new(),
            warnings_in: Vec::new(),
        }
    }
}

/// 生成统计。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationStats {
    /// 手工代理数
    pub proxy_count: usize,
    /// 代理组数
    pub group_count: usize,
    /// 规则集数
    pub provider_count: usize,
    /// 规则条数
    pub rule_count: usize,
    /// TUN 是否启用
    pub tun_enabled: bool,
}

/// 生成结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedConfig {
    /// mihomo YAML 文本
    pub yaml: String,
    /// 警告
    pub warnings: Vec<String>,
    /// 统计
    pub stats: GenerationStats,
    /// 规则计划（用于诊断与展示）
    pub plan: RulePlan,
}

/// 生成 mihomo 配置。
pub fn generate(config: &AppConfig, ctx: &RuntimeContext) -> CoreResult<GeneratedConfig> {
    let rule_ctx = build_rule_context(config, ctx);
    let plan = rules::build(config, &rule_ctx);

    let mut warnings = plan.warnings.clone();
    warnings.extend(ctx.warnings_in.iter().cloned());
    warnings.extend(validate_runtime(config, ctx));

    let proxies = build_proxies(config, ctx);
    let proxy_providers = build_proxy_providers(config, ctx);
    let mode = match config.routing.preset {
        RoutingPreset::Global => "global",
        RoutingPreset::Direct => "direct",
        _ => "rule",
    };
    let tun = build_tun(config, ctx);
    if config.engine.tun.enabled && tun.is_none() {
        warnings.push(
            "TUN 模式不可用（特权 helper 未安装或未授权），已回退为系统代理模式：裸 IP 流量不会进入分流"
                .to_string(),
        );
    }

    let mihomo = MihomoConfig {
        mixed_port: config.engine.mixed_port,
        allow_lan: config.engine.allow_lan,
        mode: mode.to_string(),
        log_level: config.engine.log_level.clone(),
        ipv6: config.engine.ipv6,
        unified_delay: true,
        tcp_concurrent: true,
        find_process_mode: "strict".to_string(),
        external_controller: format!("127.0.0.1:{}", config.engine.controller_port),
        secret: ctx.controller_secret.clone(),
        profile: ProfileSection {
            store_selected: true,
            store_fake_ip: true,
        },
        dns: build_dns(config),
        tun,
        proxies,
        proxy_providers,
        proxy_groups: plan.groups.iter().map(group_to_section).collect(),
        rule_providers: plan
            .providers
            .iter()
            .map(|provider| (provider.name.clone(), provider_to_section(provider)))
            .collect(),
        rules: plan.rules.clone(),
        sniffer: build_sniffer(),
    };

    let yaml = serde_yaml_ng::to_string(&mihomo)?;
    warnings.sort();
    warnings.dedup();

    let stats = GenerationStats {
        proxy_count: mihomo.proxies.len(),
        group_count: mihomo.proxy_groups.len(),
        provider_count: mihomo.rule_providers.len(),
        rule_count: mihomo.rules.len(),
        tun_enabled: mihomo.tun.is_some(),
    };

    Ok(GeneratedConfig {
        yaml,
        warnings,
        stats,
        plan,
    })
}

fn build_rule_context(config: &AppConfig, ctx: &RuntimeContext) -> RuleContext {
    RuleContext {
        on_campus: ctx.on_campus,
        vpn_configured: ctx.vpn_configured,
        vpn_running: ctx.vpn_running,
        vpn_server_host: config.campus_vpn.server.clone(),
        vpn_server_ips: ctx.vpn_server_ips.clone(),
        vpn_process_name: ctx.vpn_process_name.clone(),
        core_process_name: ctx.core_process_name.clone(),
        ssh_configured: ctx.ssh_runtime.is_some() && ssh_routes_configured(config),
        local_subnets: ctx.local_subnets.clone(),
        subscription_available: ctx.profile != ProfileSource::None,
    }
}

fn ssh_routes_configured(config: &AppConfig) -> bool {
    config.ssh_proxy.enabled
        && config
            .ssh_proxy
            .routes
            .iter()
            .any(|route| !route.value.trim().is_empty())
}

fn build_dns(config: &AppConfig) -> DnsSection {
    let dns = &config.routing.dns;
    let mut fake_ip_filter = vec![
        "*.lan".to_string(),
        "*.local".to_string(),
        "+.localhost".to_string(),
        "*.msftconnecttest.com".to_string(),
        "*.msftncsi.com".to_string(),
    ];
    fake_ip_filter.extend(dns.fake_ip_filter_extra.iter().cloned());
    fake_ip_filter.sort();
    fake_ip_filter.dedup();

    let mut nameserver_policy = BTreeMap::new();
    if !dns.campus_dns.trim().is_empty() {
        for domain in campus_domains(config) {
            nameserver_policy.insert(format!("+.{domain}"), vec![dns.campus_dns.clone()]);
        }
    }

    DnsSection {
        enable: true,
        ipv6: config.engine.ipv6,
        enhanced_mode: if dns.fake_ip { "fake-ip" } else { "redir-host" }.to_string(),
        fake_ip_range: "198.18.0.1/16".to_string(),
        fake_ip_filter,
        default_nameserver: vec!["223.5.5.5".to_string(), "119.29.29.29".to_string()],
        nameserver: dns.nameservers.clone(),
        proxy_server_nameserver: dns.proxy_server_nameservers.clone(),
        nameserver_policy,
    }
}

/// 需要校内 DNS 的域名（内置复旦规则集 + 用户例外）。
fn campus_domains(config: &AppConfig) -> Vec<String> {
    let mut domains: Vec<String> = Vec::new();
    if let Ok(payloads) = providers::builtin_payloads() {
        if let Some(payload) = payloads.iter().find(|p| p.name == providers::FDU_DOMAIN) {
            for rule in &payload.rules {
                if let Some(value) = rule.strip_prefix("DOMAIN-SUFFIX,") {
                    domains.push(value.to_string());
                }
            }
        }
    }
    for domain in config
        .routing
        .extra_campus_domains
        .iter()
        .chain(config.campus_vpn.custom_proxy_domains.iter())
    {
        let normalized = domain
            .trim()
            .trim_start_matches("*.")
            .trim_start_matches('.')
            .to_ascii_lowercase();
        if !normalized.is_empty() {
            domains.push(normalized);
        }
    }
    domains.sort();
    domains.dedup();
    domains
}

fn build_tun(config: &AppConfig, ctx: &RuntimeContext) -> Option<TunSection> {
    if !config.engine.tun.enabled || !ctx.tun_available {
        return None;
    }
    let mut exclude: Vec<String> = ctx
        .vpn_server_ips
        .iter()
        .filter_map(|ip| {
            let ip = ip.trim();
            if ip.is_empty() {
                None
            } else if ip.contains('/') {
                Some(ip.to_string())
            } else {
                Some(format!("{ip}/32"))
            }
        })
        .collect();
    exclude.extend(config.engine.tun.route_exclude_extra.iter().cloned());
    exclude.extend(ctx.extra_route_exclude.iter().cloned());
    exclude.sort();
    exclude.dedup();

    let dns_hijack = if config.engine.tun.dns_hijack {
        vec!["any:53".to_string(), "tcp://any:53".to_string()]
    } else {
        Vec::new()
    };

    Some(TunSection {
        enable: true,
        stack: config.engine.tun.stack.as_mihomo().to_string(),
        auto_route: true,
        auto_detect_interface: true,
        dns_hijack,
        route_exclude_address: exclude,
        mtu: config.engine.tun.mtu.max(576),
        strict_route: config.engine.tun.strict_route,
    })
}

fn build_proxies(config: &AppConfig, ctx: &RuntimeContext) -> Vec<ProxyEntry> {
    let mut proxies = Vec::new();
    if ctx.vpn_configured {
        proxies.push(ProxyEntry::Socks(SocksProxy {
            name: names::PROXY_FDU.to_string(),
            kind: "socks5".to_string(),
            server: "127.0.0.1".to_string(),
            port: config.campus_vpn.socks_port,
            udp: false,
        }));
    }
    if let Some(ssh) = &ctx.ssh_runtime {
        if ssh_routes_configured(config) && !config.ssh_proxy.host.trim().is_empty() {
            let private_key = ssh
                .private_key_content
                .clone()
                .or_else(|| ssh.private_key_path.clone());
            proxies.push(ProxyEntry::Ssh(SshProxy {
                name: names::ssh_proxy_name(config.ssh_proxy.host.trim()),
                kind: "ssh".to_string(),
                server: config.ssh_proxy.host.trim().to_string(),
                port: config.ssh_proxy.port,
                username: config.ssh_proxy.username.trim().to_string(),
                password: ssh.password.clone(),
                private_key,
                private_key_passphrase: ssh.private_key_passphrase.clone(),
                host_key: config.ssh_proxy.host_keys.clone(),
            }));
        }
    }
    proxies
}

fn build_proxy_providers(
    config: &AppConfig,
    ctx: &RuntimeContext,
) -> Option<BTreeMap<String, ProxyProvider>> {
    let path = "./providers/sub.yaml".to_string();
    let health_check = HealthCheck {
        enable: true,
        url: "http://www.gstatic.com/generate_204".to_string(),
        interval: 600,
    };
    let provider = match ctx.profile {
        ProfileSource::None => return None,
        ProfileSource::Local => ProxyProvider {
            kind: "file".to_string(),
            url: None,
            path,
            interval: 0,
            health_check,
        },
        ProfileSource::Remote => {
            let url = config.subscription.url.trim().to_string();
            if url.is_empty() {
                return None;
            }
            let interval = config.subscription.update_interval_hours.max(1) * 3600;
            ProxyProvider {
                kind: "http".to_string(),
                url: Some(url),
                path,
                interval,
                health_check,
            }
        }
    };
    let mut map = BTreeMap::new();
    map.insert(names::PROVIDER_SUB.to_string(), provider);
    Some(map)
}

fn group_to_section(group: &rules::GroupSpec) -> GroupSection {
    GroupSection {
        name: group.name.clone(),
        kind: group.kind.as_mihomo().to_string(),
        proxies: group.members.clone(),
        use_providers: group.providers.clone(),
        url: group.url.clone(),
        interval: group.interval,
        tolerance: group.tolerance,
    }
}

fn provider_to_section(provider: &rules::providers::ProviderSpec) -> RuleProvider {
    match &provider.source {
        rules::providers::ProviderSource::File { path } => RuleProvider {
            kind: "file".to_string(),
            behavior: provider.behavior.clone(),
            format: provider.format.clone(),
            url: None,
            path: path.clone(),
            interval: 0,
        },
        rules::providers::ProviderSource::Http { url, path } => RuleProvider {
            kind: "http".to_string(),
            behavior: provider.behavior.clone(),
            format: provider.format.clone(),
            url: Some(url.clone()),
            path: path.clone(),
            interval: provider.interval,
        },
    }
}

fn build_sniffer() -> SnifferSection {
    let mut sniff = BTreeMap::new();
    sniff.insert(
        "HTTP".to_string(),
        SniffPorts {
            ports: vec!["80".into(), "8080".into(), "8880".into()],
        },
    );
    sniff.insert(
        "TLS".to_string(),
        SniffPorts {
            ports: vec!["443".into(), "8443".into()],
        },
    );
    SnifferSection {
        enable: true,
        force_dns_mapping: true,
        parse_pure_ip: true,
        override_destination: true,
        sniff,
    }
}

fn validate_runtime(config: &AppConfig, ctx: &RuntimeContext) -> Vec<String> {
    let mut warnings = Vec::new();
    if ctx.vpn_configured && !ctx.vpn_running && !ctx.on_campus {
        warnings.push("校内 VPN 尚未连接：10.0.0.0/8 的访问可能失败".to_string());
    }
    if config.ssh_proxy.enabled && ctx.ssh_runtime.is_none() {
        warnings.push("SSH 代理缺少可用凭据，已跳过该出口".to_string());
    }
    if config.subscription.enabled && ctx.profile == ProfileSource::None {
        warnings.push("订阅尚未就绪（未导入或缓存丢失）".to_string());
    }
    if ctx.controller_secret.trim().is_empty() {
        warnings.push("控制端口密钥为空：外部控制 API 将不可用".to_string());
    }
    if config.routing.geo_rules {
        warnings.push("已启用 geo 规则：首次运行需下载 geosite/geoip 数据（约 20MB）".to_string());
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    fn demo_config() -> AppConfig {
        let mut config = AppConfig::default();
        config.campus_vpn.username = "21300000000".into();
        config.campus_vpn.managed = true;
        config.subscription.enabled = true;
        config.subscription.url = "https://sub.example.com/api?token=SECRET".into();
        config.ssh_proxy.enabled = true;
        config.ssh_proxy.host = "203.0.113.9".into();
        config.ssh_proxy.username = "lab".into();
        config.ssh_proxy.routes.push(crate::config::SshRoute {
            value: "*.lab.example.com".into(),
        });
        config.routing.custom_rules = vec!["DOMAIN,git.example.com,🚀 代理".into()];
        config
    }

    fn demo_context() -> RuntimeContext {
        RuntimeContext {
            on_campus: false,
            vpn_configured: true,
            vpn_running: true,
            vpn_server_ips: vec!["202.120.224.58".into()],
            vpn_process_name: "fdu-connect".into(),
            core_process_name: "mihomo".into(),
            local_subnets: vec!["192.168.1.0/24".into()],
            tun_available: true,
            controller_secret: "test-secret".into(),
            profile: ProfileSource::Remote,
            ssh_runtime: Some(SshRuntime {
                password: Some("ssh-pass".into()),
                ..Default::default()
            }),
            extra_route_exclude: Vec::new(),
            warnings_in: Vec::new(),
        }
    }

    #[test]
    fn generates_valid_yaml_for_full_setup() {
        let config = demo_config();
        let generated = generate(&config, &demo_context()).expect("generate");
        assert!(generated.yaml.contains("mixed-port: 7897"));
        assert!(generated
            .yaml
            .contains("external-controller: 127.0.0.1:9097"));
        assert!(generated.yaml.contains("name: 🏫 FDU-RVPN"));
        assert!(generated.yaml.contains("type: ssh"));
        assert!(generated.yaml.contains("name: 🎓 校内"));
        assert!(generated.yaml.contains("RULE-SET,FDUIP,🎓 校内"));
        assert!(generated.yaml.contains("RULE-SET,FDUIP"));
        assert!(generated.yaml.contains("rules:"));
        assert!(generated.stats.tun_enabled);
        assert!(generated.stats.proxy_count >= 2);
        // YAML 可被再次解析
        let value: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&generated.yaml).expect("valid yaml");
        assert!(value.get("proxy-groups").is_some());
        assert!(value.get("rule-providers").is_some());
    }

    #[test]
    fn tun_disabled_when_helper_unavailable() {
        let config = demo_config();
        let mut ctx = demo_context();
        ctx.tun_available = false;
        let generated = generate(&config, &ctx).expect("generate");
        assert!(!generated.yaml.contains("\ntun:"));
        assert!(!generated.stats.tun_enabled);
        assert!(generated
            .warnings
            .iter()
            .any(|warning| warning.contains("回退为系统代理模式")));
    }

    #[test]
    fn on_campus_switches_campus_group_default() {
        let config = demo_config();
        let mut ctx = demo_context();
        ctx.on_campus = true;
        let generated = generate(&config, &ctx).expect("generate");
        assert!(generated.yaml.contains("name: 🎓 校内"));
        let value: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&generated.yaml).expect("valid yaml");
        let groups = value["proxy-groups"].as_sequence().expect("groups");
        let campus = groups
            .iter()
            .find(|group| group["name"].as_str() == Some(names::GROUP_CAMPUS))
            .expect("campus group");
        assert_eq!(campus["proxies"][0].as_str(), Some(names::DIRECT));
    }

    #[test]
    fn local_profile_uses_file_provider() {
        let config = demo_config();
        let mut ctx = demo_context();
        ctx.profile = ProfileSource::Local;
        let generated = generate(&config, &ctx).expect("generate");
        assert!(generated.yaml.contains("type: file"));
        assert!(!generated.yaml.contains("sub.example.com/api"));
    }

    #[test]
    fn direct_preset_sets_mode() {
        let mut config = demo_config();
        config.routing.preset = RoutingPreset::Direct;
        let generated = generate(&config, &demo_context()).expect("generate");
        assert!(generated.yaml.contains("mode: direct"));
    }

    #[test]
    fn golden_snapshot() {
        let config = demo_config();
        let generated = generate(&config, &demo_context()).expect("generate");
        let actual = generated.yaml.clone();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/mihomo-smart.yaml"
        );
        if std::env::var("UPDATE_FIXTURES").is_ok() {
            std::fs::write(path, &actual).expect("write fixture");
            return;
        }
        let expected = std::fs::read_to_string(path).unwrap_or_else(|_| {
            panic!("缺少快照文件，请运行 UPDATE_FIXTURES=1 cargo test -p labvpn-core 生成")
        });
        assert_eq!(expected, actual, "生成结果与快照不一致");
    }
}
