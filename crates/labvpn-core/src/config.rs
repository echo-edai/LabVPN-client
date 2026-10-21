//! 应用配置模型、默认值、校验与版本迁移。
//!
//! 设计原则：
//! - 全部 `camelCase` JSON，前后端一致；
//! - 敏感字段只存「引用」(SecretRef)，真实值由应用层密钥库保管；
//! - 每个结构体都实现 `Default` 并带 `#[serde(default)]`，便于向后兼容升级。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::util;

/// 当前配置版本，结构变更时 +1 并在 [`AppConfig::migrate`] 中补迁移。
pub const CONFIG_VERSION: u32 = 1;

/// 默认参数（与 demand.md §9 保持一致）。
pub mod defaults {
    /// mihomo 混合代理端口
    pub const MIXED_PORT: u16 = 7897;
    /// mihomo REST 控制端口
    pub const CONTROLLER_PORT: u16 = 9097;
    /// fdu-connect SOCKS 端口（托管模式）
    pub const VPN_SOCKS_PORT: u16 = 11080;
    /// 校内 VPN 服务器
    pub const VPN_SERVER: &str = "stuvpn.fudan.edu.cn";
    /// 校内 VPN 端口
    pub const VPN_PORT: u16 = 443;
    /// 复旦校内 DNS
    pub const CAMPUS_DNS: &str = "202.120.224.26";
    /// 订阅请求 UA（多数机场按 Clash UA 返回 YAML）
    pub const SUBSCRIPTION_UA: &str = "clash-verge/v2.4.2";
    /// 订阅默认更新周期（小时）
    pub const UPDATE_INTERVAL_HOURS: u32 = 12;
    /// 校园网探测默认超时（毫秒）
    pub const PROBE_TIMEOUT_MS: u64 = 1500;
    /// 校园网关 Ping 探测目标（ping 通即 100% 校内）
    pub const CAMPUS_PING_TARGET: &str = "10.134.140.1";
    /// Ping 探测默认超时（毫秒）
    pub const PING_TIMEOUT_MS: u64 = 2000;
    /// 节点测速 URL（自动选最低延迟时使用）
    pub const NODE_TEST_URL: &str = "http://www.gstatic.com/generate_204";
    /// 节点自动测速间隔（秒）
    pub const NODE_TEST_INTERVAL_SECS: u64 = 300;
    /// 节点切换容差（毫秒）
    pub const NODE_TOLERANCE_MS: u64 = 50;
}

/// 密钥库中使用的键名常量。配置里只保存这些键。
pub mod secret_keys {
    /// 校内 VPN 密码
    pub const CAMPUS_PASSWORD: &str = "campus_vpn.password";
    /// 校内 VPN TOTP Secret
    pub const CAMPUS_TOTP: &str = "campus_vpn.totp_secret";
    /// SSH 密码
    pub const SSH_PASSWORD: &str = "ssh.password";
    /// SSH 私钥口令
    pub const SSH_KEY_PASSPHRASE: &str = "ssh.key_passphrase";
    /// mihomo 控制端口 secret
    pub const CONTROLLER_SECRET: &str = "engine.controller_secret";
}

/// 密钥引用：密钥库中的键名。
pub type SecretRef = String;

/// 订阅流量信息（来自 `subscription-userinfo` 响应头）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TrafficInfo {
    /// 已上传字节
    pub upload: u64,
    /// 已下载字节
    pub download: u64,
    /// 总流量字节（0 表示未知/不限）
    pub total: u64,
    /// 到期时间戳（秒，0 表示未知/长期）
    pub expire: Option<i64>,
}

impl TrafficInfo {
    /// 已用流量
    pub fn used(&self) -> u64 {
        self.upload.saturating_add(self.download)
    }
}

// ---------------------------------------------------------------------------
// 订阅
// ---------------------------------------------------------------------------

/// 订阅配置（配置页 1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SubscriptionConfig {
    /// 是否启用订阅
    pub enabled: bool,
    /// 订阅地址（http(s) 或 file:// 本地文件）
    pub url: String,
    /// 显示名称
    pub name: String,
    /// 自动更新周期（小时），0 表示不自动更新
    pub update_interval_hours: u32,
    /// 请求 UA
    pub user_agent: String,
    /// 是否让 mihomo 直接以 proxy-provider 形式拉取（否则用本地缓存 file provider）
    pub use_remote_provider: bool,
    /// 最近一次成功更新时间（应用层维护）
    pub last_updated: Option<DateTime<Utc>>,
    /// 最近一次解析出的流量信息
    pub traffic: Option<TrafficInfo>,
    /// 最近一次解析出的节点数（应用层维护，用于 UI 展示）
    pub proxy_count: Option<usize>,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            name: "我的订阅".to_string(),
            update_interval_hours: defaults::UPDATE_INTERVAL_HOURS,
            user_agent: defaults::SUBSCRIPTION_UA.to_string(),
            use_remote_provider: true,
            last_updated: None,
            traffic: None,
            proxy_count: None,
        }
    }
}

// ---------------------------------------------------------------------------
// 校园网探测
// ---------------------------------------------------------------------------

/// 探测协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProbeProtocol {
    /// TCP 连接
    #[default]
    Tcp,
    /// UDP DNS 查询（要求目标为 DNS 服务器）
    UdpDns,
    /// ICMP Ping（端口无意义，置 0；绑定物理网卡执行）
    Icmp,
}

/// 校园网探测目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProbeTarget {
    /// 展示名称
    pub name: String,
    /// 主机（IP 优先）
    pub host: String,
    /// 端口
    pub port: u16,
    /// 协议
    pub protocol: ProbeProtocol,
    /// 权重（越大越可信，0-100）
    pub weight: u8,
    /// 超时（毫秒）
    pub timeout_ms: u64,
}

impl Default for ProbeTarget {
    fn default() -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            port: 443,
            protocol: ProbeProtocol::Tcp,
            weight: 60,
            timeout_ms: defaults::PROBE_TIMEOUT_MS,
        }
    }
}

/// 默认 Ping 校内网关探测目标（ping 通即 100% 校内）。
pub fn campus_ping_target() -> ProbeTarget {
    ProbeTarget {
        name: "校内网关(Ping)".into(),
        host: defaults::CAMPUS_PING_TARGET.into(),
        port: 0,
        protocol: ProbeProtocol::Icmp,
        weight: 100,
        timeout_ms: defaults::PING_TIMEOUT_MS,
    }
}

/// 默认探测列表（2026-10 实测可用）。
pub fn default_probe_targets() -> Vec<ProbeTarget> {
    vec![
        campus_ping_target(),
        ProbeTarget {
            name: "校内镜像站".into(),
            host: "10.176.52.2".into(),
            port: 443,
            protocol: ProbeProtocol::Tcp,
            weight: 100,
            timeout_ms: defaults::PROBE_TIMEOUT_MS,
        },
        ProbeTarget {
            name: "校内 DNS(UDP)".into(),
            host: defaults::CAMPUS_DNS.into(),
            port: 53,
            protocol: ProbeProtocol::UdpDns,
            weight: 90,
            timeout_ms: defaults::PROBE_TIMEOUT_MS,
        },
        ProbeTarget {
            name: "校内 DNS(TCP)".into(),
            host: defaults::CAMPUS_DNS.into(),
            port: 53,
            protocol: ProbeProtocol::Tcp,
            weight: 70,
            timeout_ms: defaults::PROBE_TIMEOUT_MS,
        },
        ProbeTarget {
            name: "校内应用服务".into(),
            host: "10.203.8.198".into(),
            port: 443,
            protocol: ProbeProtocol::Tcp,
            weight: 50,
            timeout_ms: defaults::PROBE_TIMEOUT_MS,
        },
    ]
}

/// 默认校内网段（本地 IP 信号）。
pub fn default_campus_cidrs() -> Vec<String> {
    vec![
        "10.188.0.0/16".into(),
        "202.120.0.0/16".into(),
        "2001:da8:8001::/48".into(),
    ]
}

// ---------------------------------------------------------------------------
// 校内 VPN
// ---------------------------------------------------------------------------

/// 校内 VPN（fdu-connect）配置（配置页 2）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CampusVpnConfig {
    /// 是否启用校内 VPN
    pub enabled: bool,
    /// true：由 LabVPN 启动并托管 fdu-connect；false：复用已运行的实例
    pub managed: bool,
    /// 上网账号
    pub username: String,
    /// 密码的密钥引用
    pub password_ref: Option<SecretRef>,
    /// TOTP secret 的密钥引用
    pub totp_secret_ref: Option<SecretRef>,
    /// 服务器地址
    pub server: String,
    /// 服务器端口
    pub port: u16,
    /// 服务器 IP 覆盖（留空则自动解析；校园网下建议用 IP 避免 fake-ip 干扰）
    pub server_ip_override: Option<String>,
    /// SOCKS 监听端口（托管模式下由 LabVPN 指定；复用模式下为用户已有端口）
    pub socks_port: u16,
    /// fdu-connect 可执行文件路径（留空则自动发现）
    pub binary_path: Option<String>,
    /// 传给 fdu-connect 的额外参数（高级）
    pub extra_args: Vec<String>,
    /// 是否自动检测校园网
    pub auto_detect_campus: bool,
    /// 校内时是否跳过 VPN（需求核心行为）
    pub skip_when_on_campus: bool,
    /// 探测目标
    pub probe_targets: Vec<ProbeTarget>,
    /// 校园网 IP 段信号
    pub campus_cidrs: Vec<String>,
    /// 强制走 RVPN 的额外域名（会同时写入 mihomo 校内规则）
    pub custom_proxy_domains: Vec<String>,
    /// 禁用复旦专用配置（透传 fdu-connect）
    pub disable_fdu_config: bool,
    /// 禁用复旦 DNS（透传 fdu-connect）
    pub disable_fdu_dns: bool,
}

impl Default for CampusVpnConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            managed: true,
            username: String::new(),
            password_ref: Some(secret_keys::CAMPUS_PASSWORD.to_string()),
            totp_secret_ref: None,
            server: defaults::VPN_SERVER.to_string(),
            port: defaults::VPN_PORT,
            server_ip_override: None,
            socks_port: defaults::VPN_SOCKS_PORT,
            binary_path: None,
            extra_args: Vec::new(),
            auto_detect_campus: true,
            skip_when_on_campus: true,
            probe_targets: default_probe_targets(),
            campus_cidrs: default_campus_cidrs(),
            custom_proxy_domains: Vec::new(),
            disable_fdu_config: false,
            disable_fdu_dns: false,
        }
    }
}

// ---------------------------------------------------------------------------
// SSH 出站代理
// ---------------------------------------------------------------------------

/// SSH 认证方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SshAuth {
    /// 密码认证
    #[default]
    Password,
    /// 私钥认证
    PrivateKey,
}

/// SSH 绑定目标（域名或 CIDR，内容自动识别）。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SshRoute {
    /// 域名（普通或 `*.` 前缀）或 IP/CIDR
    pub value: String,
}

/// 公网 SSH 转发代理配置（配置页 3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SshProxyConfig {
    /// 是否启用
    pub enabled: bool,
    /// 公网 IP / 域名
    pub host: String,
    /// SSH 端口
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 认证方式
    pub auth: SshAuth,
    /// 密码的密钥引用
    pub password_ref: Option<SecretRef>,
    /// 私钥文件路径
    pub private_key_path: Option<String>,
    /// 私钥口令的密钥引用
    pub private_key_passphrase_ref: Option<SecretRef>,
    /// 固定 host key（可选，mihomo `host-key` 字段）
    pub host_keys: Vec<String>,
    /// 绑定走 SSH 的域名/CIDR
    pub routes: Vec<SshRoute>,
}

impl Default for SshProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: String::new(),
            port: 22,
            username: String::new(),
            auth: SshAuth::Password,
            password_ref: Some(secret_keys::SSH_PASSWORD.to_string()),
            private_key_path: None,
            private_key_passphrase_ref: None,
            host_keys: Vec::new(),
            routes: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 路由 / 规则
// ---------------------------------------------------------------------------

/// 分流预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RoutingPreset {
    /// 智能分流（默认）：校内 → 校内，国内直连，国外走订阅，支持 SSH
    #[default]
    Smart,
    /// 极简：只保留三类出口（校内 / SSH / 订阅）
    Minimal,
    /// 严格：开启广告拦截与完整国内规则，其它全走订阅
    Strict,
    /// 全局代理
    Global,
    /// 全部直连（排障用）
    Direct,
}

impl RoutingPreset {
    /// 稳定 ID（前端同源常量）
    pub fn id(self) -> &'static str {
        match self {
            Self::Smart => "smart",
            Self::Minimal => "minimal",
            Self::Strict => "strict",
            Self::Global => "global",
            Self::Direct => "direct",
        }
    }

    /// 中文名称
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::Smart => "智能分流",
            Self::Minimal => "极简模式",
            Self::Strict => "严格分流",
            Self::Global => "全局代理",
            Self::Direct => "全部直连",
        }
    }

    /// 中文说明
    pub fn description_zh(self) -> &'static str {
        match self {
            Self::Smart => "校内走 VPN、国内直连、国外走订阅，自动处理 SSH",
            Self::Minimal => "仅区分校内 / SSH / 订阅三类出口，不做国内直连",
            Self::Strict => "开启广告拦截，国内域名与 IP 直连，其余全部走订阅",
            Self::Global => "所有流量都走代理（仍保留防环与局域网直连）",
            Self::Direct => "全部直连，仅用于排查网络问题",
        }
    }
}

/// DNS 配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DnsConfig {
    /// 是否启用 fake-ip（推荐开启，TUN 模式必需）
    pub fake_ip: bool,
    /// 主 DNS 列表
    pub nameservers: Vec<String>,
    /// 解析订阅/SSH 服务器域名用的 DNS（直连）
    pub proxy_server_nameservers: Vec<String>,
    /// 校内 DNS
    pub campus_dns: String,
    /// 额外 fake-ip 过滤域名（保持真实解析）
    pub fake_ip_filter_extra: Vec<String>,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            fake_ip: true,
            nameservers: vec!["223.5.5.5".into(), "119.29.29.29".into()],
            proxy_server_nameservers: vec!["223.5.5.5".into()],
            campus_dns: defaults::CAMPUS_DNS.into(),
            fake_ip_filter_extra: Vec::new(),
        }
    }
}

/// 路由与规则配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RoutingConfig {
    /// 预设
    pub preset: RoutingPreset,
    /// 自定义规则（mihomo rules 语法，逐行一条）
    pub custom_rules: Vec<String>,
    /// 自定义规则是否插到最高优先级（10/8 规则之前）
    pub custom_rules_high_priority: bool,
    /// 是否拦截广告域名
    pub adblock: bool,
    /// 是否使用 GEOSITE/GEOIP（需要 geo 数据文件）
    pub geo_rules: bool,
    /// 是否启用增强规则集（ACL4SSR 远程 rule-provider，需要联网）
    pub enhanced_rules: bool,
    /// DNS 配置
    pub dns: DnsConfig,
    /// 额外走校内的域名（同时透传给 fdu-connect 的 custom_proxy_domain）
    pub extra_campus_domains: Vec<String>,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            preset: RoutingPreset::Smart,
            custom_rules: Vec::new(),
            custom_rules_high_priority: false,
            adblock: false,
            geo_rules: true,
            enhanced_rules: false,
            dns: DnsConfig::default(),
            extra_campus_domains: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// 网络分析
// ---------------------------------------------------------------------------

/// 默认 AI 平台域名（用于出口 IP 长期跟踪）。
pub fn default_ai_domains() -> Vec<String> {
    [
        "chatgpt.com",
        "openai.com",
        "oaistatic.com",
        "oaiusercontent.com",
        "claude.ai",
        "anthropic.com",
        "gemini.google.com",
        "bard.google.com",
        "perplexity.ai",
        "copilot.microsoft.com",
    ]
    .iter()
    .map(|domain| (*domain).to_string())
    .collect()
}

/// 网络分析（流量历史 / 出口 IP / AI 平台）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AnalyticsConfig {
    /// 是否启用后台采样
    pub enabled: bool,
    /// 流量采样间隔（秒）
    pub sample_interval_seconds: u64,
    /// 出口 IP 采样间隔（秒）
    pub egress_interval_seconds: u64,
    /// 数据保留天数
    pub retention_days: u32,
    /// AI 平台域名（含用户自定义，可加前缀）
    pub ai_domains: Vec<String>,
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_interval_seconds: 15,
            egress_interval_seconds: 300,
            retention_days: 90,
            ai_domains: default_ai_domains(),
        }
    }
}

// ---------------------------------------------------------------------------
// 代理节点选择
// ---------------------------------------------------------------------------

/// 节点选择模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeSelectionMode {
    /// 自动：按自定义测速 URL 与间隔选择延迟最低的节点
    #[default]
    Auto,
    /// 手动：固定使用指定节点（热切换，无需重启核心）
    Manual,
}

/// 国外订阅节点的选择策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NodeSelectionConfig {
    /// 选择模式
    pub mode: NodeSelectionMode,
    /// 手动模式下固定的节点名称
    pub selected: Option<String>,
    /// 自动测速使用的 URL（经过代理）
    pub test_url: String,
    /// 自动测速间隔（秒）
    pub interval_seconds: u64,
    /// 切换容差（毫秒，延迟差小于该值不切换）
    pub tolerance_ms: u64,
}

impl Default for NodeSelectionConfig {
    fn default() -> Self {
        Self {
            mode: NodeSelectionMode::Auto,
            selected: None,
            test_url: defaults::NODE_TEST_URL.to_string(),
            interval_seconds: defaults::NODE_TEST_INTERVAL_SECS,
            tolerance_ms: defaults::NODE_TOLERANCE_MS,
        }
    }
}

// ---------------------------------------------------------------------------
// 引擎
// ---------------------------------------------------------------------------

/// TUN 协议栈。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TunStack {
    /// 系统栈（性能好，Unix 推荐）
    System,
    /// gVisor（兼容性好，Windows 推荐）
    Gvisor,
    /// 混合（默认）
    #[default]
    Mixed,
}

impl TunStack {
    /// mihomo 配置值
    pub fn as_mihomo(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Gvisor => "gvisor",
            Self::Mixed => "mixed",
        }
    }
}

/// TUN 配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TunConfig {
    /// 是否启用 TUN（关闭则使用系统代理模式）
    pub enabled: bool,
    /// 协议栈
    pub stack: TunStack,
    /// MTU
    pub mtu: u16,
    /// 是否劫持 DNS
    pub dns_hijack: bool,
    /// 严格路由（Windows 下可作为排障开关）
    pub strict_route: bool,
    /// 路由排除地址（额外，例如其他 VPN 服务器）
    pub route_exclude_extra: Vec<String>,
}

impl Default for TunConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            stack: TunStack::Mixed,
            mtu: 1500,
            dns_hijack: true,
            strict_route: false,
            route_exclude_extra: Vec::new(),
        }
    }
}

/// 代理引擎配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EngineConfig {
    /// 混合代理端口（HTTP + SOCKS）
    pub mixed_port: u16,
    /// REST 控制端口
    pub controller_port: u16,
    /// 控制端口 secret 的密钥引用
    pub controller_secret_ref: SecretRef,
    /// TUN 配置
    pub tun: TunConfig,
    /// 是否设置系统代理（TUN 模式下通常关闭）
    pub system_proxy: bool,
    /// 是否允许局域网设备接入
    pub allow_lan: bool,
    /// 是否启用 IPv6
    pub ipv6: bool,
    /// 日志级别：silent/error/warning/info/debug
    pub log_level: String,
    /// 启动时自动连接
    pub connect_on_launch: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            mixed_port: defaults::MIXED_PORT,
            controller_port: defaults::CONTROLLER_PORT,
            controller_secret_ref: secret_keys::CONTROLLER_SECRET.to_string(),
            tun: TunConfig::default(),
            system_proxy: true,
            allow_lan: false,
            ipv6: false,
            log_level: "info".to_string(),
            connect_on_launch: false,
        }
    }
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

/// 主题。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    /// 跟随系统
    #[default]
    System,
    /// 浅色
    Light,
    /// 深色
    Dark,
}

/// 语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    /// 简体中文
    #[default]
    ZhCn,
    /// English
    EnUs,
}

/// 界面配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UiConfig {
    /// 主题
    pub theme: Theme,
    /// 语言
    pub language: Language,
    /// 是否显示托盘图标
    pub tray: bool,
    /// 是否开机自启
    pub auto_launch: bool,
    /// 是否完成首次配置向导
    pub setup_completed: bool,
    /// 向导步骤（0-3）
    pub setup_step: u8,
    /// 是否显示实时流量图
    pub traffic_graph: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            language: Language::ZhCn,
            tray: true,
            auto_launch: false,
            setup_completed: false,
            setup_step: 0,
            traffic_graph: true,
        }
    }
}

// ---------------------------------------------------------------------------
// 根配置
// ---------------------------------------------------------------------------

/// LabVPN 全部持久化配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppConfig {
    /// 配置版本
    pub version: u32,
    /// 订阅
    pub subscription: SubscriptionConfig,
    /// 校内 VPN
    pub campus_vpn: CampusVpnConfig,
    /// SSH 代理
    pub ssh_proxy: SshProxyConfig,
    /// 代理节点选择
    pub node_selection: NodeSelectionConfig,
    /// 网络分析
    pub analytics: AnalyticsConfig,
    /// 路由规则
    pub routing: RoutingConfig,
    /// 引擎
    pub engine: EngineConfig,
    /// 界面
    pub ui: UiConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            subscription: SubscriptionConfig::default(),
            campus_vpn: CampusVpnConfig::default(),
            ssh_proxy: SshProxyConfig::default(),
            node_selection: NodeSelectionConfig::default(),
            analytics: AnalyticsConfig::default(),
            routing: RoutingConfig::default(),
            engine: EngineConfig::default(),
            ui: UiConfig::default(),
        }
    }
}

/// 校验级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueLevel {
    /// 阻断性问题（无法连接）
    Error,
    /// 警告（可以连接但可能不符合预期）
    Warn,
}

/// 单条校验结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationIssue {
    /// 字段路径，如 `subscription.url`
    pub field: String,
    /// 级别
    pub level: IssueLevel,
    /// 说明（中文，直接展示给用户）
    pub message: String,
}

impl ValidationIssue {
    fn error(field: &str, message: impl Into<String>) -> Self {
        Self {
            field: field.to_string(),
            level: IssueLevel::Error,
            message: message.into(),
        }
    }

    fn warn(field: &str, message: impl Into<String>) -> Self {
        Self {
            field: field.to_string(),
            level: IssueLevel::Warn,
            message: message.into(),
        }
    }
}

impl AppConfig {
    /// 完整校验，返回所有问题（按字段排序稳定）。
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        self.validate_ports(&mut issues);
        self.validate_subscription(&mut issues);
        self.validate_campus(&mut issues);
        self.validate_ssh(&mut issues);
        self.validate_node_selection(&mut issues);
        self.validate_routing(&mut issues);
        self.validate_engine(&mut issues);
        issues.sort_by(|a, b| a.field.cmp(&b.field));
        issues
    }

    /// 是否满足连接条件（无阻断性问题）。
    pub fn can_connect(&self) -> bool {
        !self
            .validate()
            .iter()
            .any(|issue| issue.level == IssueLevel::Error)
    }

    /// 仅错误列表。
    pub fn errors(&self) -> Vec<ValidationIssue> {
        self.validate()
            .into_iter()
            .filter(|issue| issue.level == IssueLevel::Error)
            .collect()
    }

    fn validate_ports(&self, issues: &mut Vec<ValidationIssue>) {
        for (field, port) in [
            ("engine.mixedPort", self.engine.mixed_port),
            ("engine.controllerPort", self.engine.controller_port),
            ("campusVpn.socksPort", self.campus_vpn.socks_port),
        ] {
            if port == 0 {
                issues.push(ValidationIssue::error(field, "端口不能为 0"));
            } else if port < 1024 {
                issues.push(ValidationIssue::warn(
                    field,
                    "低于 1024 的端口可能需要管理员权限",
                ));
            }
        }
        if self.engine.mixed_port == self.engine.controller_port {
            issues.push(ValidationIssue::error(
                "engine.mixedPort",
                "混合代理端口不能与控制端口相同",
            ));
        }
        if !self.campus_vpn.managed && self.campus_vpn.socks_port == self.engine.mixed_port {
            issues.push(ValidationIssue::error(
                "campusVpn.socksPort",
                "复用模式的 SOCKS 端口不能与混合代理端口相同",
            ));
        }
    }

    fn validate_subscription(&self, issues: &mut Vec<ValidationIssue>) {
        let sub = &self.subscription;
        if !sub.enabled {
            if self.routing.preset == RoutingPreset::Smart && self.campus_vpn.enabled {
                issues.push(ValidationIssue::warn(
                    "subscription.url",
                    "未启用订阅时，国外流量将没有可用出口（仅能直连/校内）",
                ));
            }
            return;
        }
        let url = sub.url.trim();
        if url.is_empty() {
            issues.push(ValidationIssue::error(
                "subscription.url",
                "已启用订阅，但订阅地址为空",
            ));
        } else if !(url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("file://"))
        {
            issues.push(ValidationIssue::error(
                "subscription.url",
                "订阅地址必须是 http(s):// 或 file://",
            ));
        }
    }

    fn validate_campus(&self, issues: &mut Vec<ValidationIssue>) {
        let vpn = &self.campus_vpn;
        if !vpn.enabled {
            return;
        }
        if vpn.username.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "campusVpn.username",
                "请填写复旦上网账号",
            ));
        }
        if vpn.password_ref.is_none() {
            issues.push(ValidationIssue::error(
                "campusVpn.password",
                "请填写复旦上网密码",
            ));
        }
        if vpn.server.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "campusVpn.server",
                "校内 VPN 服务器地址不能为空",
            ));
        }
        if let Some(ip) = vpn.server_ip_override.as_deref() {
            if !ip.trim().is_empty() && util::parse_ip(ip).is_none() {
                issues.push(ValidationIssue::warn(
                    "campusVpn.serverIpOverride",
                    "服务器 IP 覆盖不是合法 IP，将忽略",
                ));
            }
        }
        for target in &vpn.probe_targets {
            let port_required = !matches!(target.protocol, ProbeProtocol::Icmp);
            if target.host.trim().is_empty() || (port_required && target.port == 0) {
                issues.push(ValidationIssue::warn(
                    "campusVpn.probeTargets",
                    format!("探测目标「{}」配置不完整，已忽略", target.name),
                ));
            }
        }
        if vpn.probe_targets.is_empty() && vpn.auto_detect_campus {
            issues.push(ValidationIssue::warn(
                "campusVpn.probeTargets",
                "没有任何探测目标，校园网检测只能依赖本地 IP 段",
            ));
        }
        for cidr in &vpn.campus_cidrs {
            if !util::is_ip_or_cidr(cidr) {
                issues.push(ValidationIssue::warn(
                    "campusVpn.campusCidrs",
                    format!("无效的校内网段：{cidr}"),
                ));
            }
        }
        for domain in &vpn.custom_proxy_domains {
            if !util::is_domain(domain) {
                issues.push(ValidationIssue::warn(
                    "campusVpn.customProxyDomains",
                    format!("无效的域名：{domain}"),
                ));
            }
        }
        if !vpn.auto_detect_campus {
            issues.push(ValidationIssue::warn(
                "campusVpn.autoDetectCampus",
                "已关闭校园网自动检测：在校内时也会尝试连接 VPN",
            ));
        }
    }

    fn validate_node_selection(&self, issues: &mut Vec<ValidationIssue>) {
        let node = &self.node_selection;
        let test_url = node.test_url.trim();
        if node.mode == NodeSelectionMode::Auto {
            if test_url.is_empty() {
                issues.push(ValidationIssue::error(
                    "nodeSelection.testUrl",
                    "自动选择节点需要填写测速 URL",
                ));
            } else if !test_url.starts_with("http://") && !test_url.starts_with("https://") {
                issues.push(ValidationIssue::error(
                    "nodeSelection.testUrl",
                    "测速 URL 必须以 http:// 或 https:// 开头",
                ));
            }
            if !(30..=3600).contains(&node.interval_seconds) {
                issues.push(ValidationIssue::error(
                    "nodeSelection.intervalSeconds",
                    "自动测速间隔应在 30–3600 秒之间",
                ));
            }
        } else if node
            .selected
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            issues.push(ValidationIssue::warn(
                "nodeSelection.selected",
                "手动模式未指定节点，将使用第一个可用节点",
            ));
        }
    }

    fn validate_ssh(&self, issues: &mut Vec<ValidationIssue>) {
        let ssh = &self.ssh_proxy;
        if !ssh.enabled {
            return;
        }
        if ssh.host.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "sshProxy.host",
                "请填写 SSH 公网地址",
            ));
        }
        if ssh.username.trim().is_empty() {
            issues.push(ValidationIssue::error(
                "sshProxy.username",
                "请填写 SSH 用户名",
            ));
        }
        match ssh.auth {
            SshAuth::Password => {
                if ssh.password_ref.is_none() {
                    issues.push(ValidationIssue::error(
                        "sshProxy.password",
                        "密码认证需要填写 SSH 密码",
                    ));
                }
            }
            SshAuth::PrivateKey => {
                let path = ssh.private_key_path.as_deref().unwrap_or_default();
                if path.trim().is_empty() {
                    issues.push(ValidationIssue::error(
                        "sshProxy.privateKeyPath",
                        "私钥认证需要选择私钥文件",
                    ));
                }
            }
        }
        for route in &ssh.routes {
            let value = route.value.trim();
            if value.is_empty() {
                issues.push(ValidationIssue::warn(
                    "sshProxy.routes",
                    "存在空的 SSH 绑定目标，已忽略",
                ));
            } else if !util::is_domain(value) && !util::is_ip_or_cidr(value) {
                issues.push(ValidationIssue::warn(
                    "sshProxy.routes",
                    format!("无法识别的绑定目标：{value}"),
                ));
            }
        }
        if ssh.routes.iter().all(|route| route.value.trim().is_empty()) {
            issues.push(ValidationIssue::warn(
                "sshProxy.routes",
                "SSH 未绑定任何域名/IP，配置不会生效",
            ));
        }
    }

    fn validate_routing(&self, issues: &mut Vec<ValidationIssue>) {
        for (index, rule) in self.routing.custom_rules.iter().enumerate() {
            if rule.trim().is_empty() {
                continue;
            }
            if let Err(err) = crate::rules::custom::validate_syntax(rule) {
                issues.push(ValidationIssue::warn(
                    "routing.customRules",
                    format!("第 {} 条自定义规则被忽略：{err}", index + 1),
                ));
            }
        }
        for domain in &self.routing.extra_campus_domains {
            if !util::is_domain(domain) {
                issues.push(ValidationIssue::warn(
                    "routing.extraCampusDomains",
                    format!("无效的校内例外域名：{domain}"),
                ));
            }
        }
    }

    fn validate_engine(&self, issues: &mut Vec<ValidationIssue>) {
        if self.engine.log_level.trim().is_empty() {
            issues.push(ValidationIssue::warn(
                "engine.logLevel",
                "日志级别为空，将使用 info",
            ));
        }
        if self.engine.tun.mtu < 576 {
            issues.push(ValidationIssue::warn(
                "engine.tun.mtu",
                "MTU 过小，可能导致连接异常（建议 1500）",
            ));
        }
    }

    /// 从 JSON 迁移到当前版本。
    pub fn migrate_json(mut value: serde_json::Value) -> CoreResult<Self> {
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        if version > CONFIG_VERSION {
            return Err(CoreError::Validation(format!(
                "配置版本 {version} 高于当前程序支持的 {CONFIG_VERSION}，请升级 LabVPN"
            )));
        }
        if version < CONFIG_VERSION {
            // v0 -> v1：补全版本号并回填默认值（serde default 已覆盖大部分场景）
            value["version"] = serde_json::json!(CONFIG_VERSION);
        }
        let mut config: AppConfig = serde_json::from_value(value)?;
        config.version = CONFIG_VERSION;
        config.ensure_new_defaults();
        Ok(config)
    }

    /// 旧配置迁移时补齐新增的默认信号（不覆盖用户自定义探测目标）。
    pub fn ensure_new_defaults(&mut self) {
        let has_ping = self.campus_vpn.probe_targets.iter().any(|target| {
            target.protocol == ProbeProtocol::Icmp
                && target.host.trim() == defaults::CAMPUS_PING_TARGET
        });
        if !has_ping {
            self.campus_vpn
                .probe_targets
                .insert(0, campus_ping_target());
        }
    }

    /// 序列化为 JSON 字符串（格式化）。
    pub fn to_json(&self) -> CoreResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid_enough() {
        let config = AppConfig::default();
        // 默认未配置账号，必然有错误，但不应 panic 或产生空消息
        let issues = config.validate();
        assert!(issues.iter().any(|i| i.field == "campusVpn.username"));
        assert!(issues.iter().all(|i| !i.message.is_empty()));
    }

    #[test]
    fn configured_config_can_connect() {
        let mut config = AppConfig::default();
        config.campus_vpn.username = "21300000000".into();
        config.subscription.enabled = true;
        config.subscription.url = "https://example.com/sub".into();
        assert!(config.can_connect(), "{:?}", config.errors());
    }

    #[test]
    fn rejects_bad_subscription_url() {
        let mut config = AppConfig::default();
        config.campus_vpn.username = "u".into();
        config.campus_vpn.enabled = false;
        config.subscription.enabled = true;
        config.subscription.url = "ftp://x".into();
        let errors = config.errors();
        assert!(errors.iter().any(|e| e.field == "subscription.url"));
    }

    #[test]
    fn duplicate_ports_rejected() {
        let mut config = AppConfig::default();
        config.engine.controller_port = config.engine.mixed_port;
        let errors = config.errors();
        assert!(errors.iter().any(|e| e.field == "engine.mixedPort"));
    }

    #[test]
    fn migrates_old_probe_targets_to_include_ping() {
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize");
        // 模拟旧版本配置：删除 Ping 目标、删除 nodeSelection 字段
        value["campusVpn"]["probeTargets"] = serde_json::json!([
            {"name": "校内镜像站", "host": "10.176.52.2", "port": 443, "protocol": "tcp", "weight": 100, "timeoutMs": 1500}
        ]);
        value
            .as_object_mut()
            .expect("object")
            .remove("nodeSelection");
        let config = AppConfig::migrate_json(value).expect("migrate");
        assert!(config
            .campus_vpn
            .probe_targets
            .iter()
            .any(|target| target.protocol == ProbeProtocol::Icmp
                && target.host == defaults::CAMPUS_PING_TARGET));
        // 用户自定义的探针保留
        assert!(config
            .campus_vpn
            .probe_targets
            .iter()
            .any(|target| target.host == "10.176.52.2"));
    }

    #[test]
    fn json_roundtrip() {
        let mut config = AppConfig::default();
        config.campus_vpn.username = "tester".into();
        let json = serde_json::to_value(&config).expect("serialize");
        let restored = AppConfig::migrate_json(json).expect("migrate");
        assert_eq!(config, restored);
    }

    #[test]
    fn migrates_v0_json() {
        let json = serde_json::json!({
            "version": 0,
            "campusVpn": {"username": "old-user"}
        });
        let config = AppConfig::migrate_json(json).expect("migrate");
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.campus_vpn.username, "old-user");
        assert_eq!(config.engine.mixed_port, defaults::MIXED_PORT);
    }

    #[test]
    fn rejects_future_version() {
        let json = serde_json::json!({"version": CONFIG_VERSION + 1});
        assert!(AppConfig::migrate_json(json).is_err());
    }
}
