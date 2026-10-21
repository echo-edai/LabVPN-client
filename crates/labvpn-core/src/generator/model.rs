//! mihomo 配置文件的数据模型（Serde 序列化，字段顺序即 YAML 输出顺序）。

use std::collections::BTreeMap;

use serde::Serialize;

/// mihomo 完整配置。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MihomoConfig {
    /// 混合代理端口（HTTP + SOCKS5）
    pub mixed_port: u16,
    /// 是否允许局域网接入
    pub allow_lan: bool,
    /// 运行模式：rule / global / direct
    pub mode: String,
    /// 日志级别
    pub log_level: String,
    /// 是否启用 IPv6
    pub ipv6: bool,
    /// 统一延迟
    pub unified_delay: bool,
    /// TCP 并发连接
    pub tcp_concurrent: bool,
    /// 进程匹配模式（PROCESS-NAME 规则需要）
    pub find_process_mode: String,
    /// REST 控制地址
    pub external_controller: String,
    /// REST 控制密钥
    pub secret: String,
    /// 持久化选项
    pub profile: ProfileSection,
    /// DNS
    pub dns: DnsSection,
    /// TUN（关闭时省略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tun: Option<TunSection>,
    /// 手工代理（校内 VPN、SSH）
    pub proxies: Vec<ProxyEntry>,
    /// 订阅 provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_providers: Option<BTreeMap<String, ProxyProvider>>,
    /// 代理组
    pub proxy_groups: Vec<GroupSection>,
    /// 规则集
    pub rule_providers: BTreeMap<String, RuleProvider>,
    /// 有序规则
    pub rules: Vec<String>,
    /// 域名嗅探
    pub sniffer: SnifferSection,
}

/// profile 持久化选项。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProfileSection {
    /// 记住手动选择的节点
    pub store_selected: bool,
    /// 记住 fake-ip 映射
    pub store_fake_ip: bool,
}

/// DNS 配置段。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DnsSection {
    /// 是否启用
    pub enable: bool,
    /// 是否解析 AAAA
    pub ipv6: bool,
    /// 增强模式：fake-ip / redir-host
    pub enhanced_mode: String,
    /// fake-ip 网段
    pub fake_ip_range: String,
    /// 保持真实解析的域名
    pub fake_ip_filter: Vec<String>,
    /// 用于解析 DNS 服务器域名的引导 DNS
    pub default_nameserver: Vec<String>,
    /// 默认 DNS
    pub nameserver: Vec<String>,
    /// 解析代理服务器域名用的 DNS（直连）
    pub proxy_server_nameserver: Vec<String>,
    /// 按域名指定 DNS
    pub nameserver_policy: BTreeMap<String, Vec<String>>,
}

/// TUN 配置段。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TunSection {
    /// 是否启用
    pub enable: bool,
    /// 协议栈：system / gvisor / mixed
    pub stack: String,
    /// 自动路由
    pub auto_route: bool,
    /// 自动探测出站网卡
    pub auto_detect_interface: bool,
    /// DNS 劫持
    pub dns_hijack: Vec<String>,
    /// 路由排除地址（防止环路，如 VPN 服务器）
    pub route_exclude_address: Vec<String>,
    /// MTU
    pub mtu: u16,
    /// 严格路由
    pub strict_route: bool,
}

/// 代理条目（socks5 / ssh）。
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ProxyEntry {
    /// SOCKS5（校内 VPN）
    Socks(SocksProxy),
    /// SSH 出站
    Ssh(SshProxy),
}

/// SOCKS5 代理。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SocksProxy {
    /// 名称
    pub name: String,
    /// 类型
    #[serde(rename = "type")]
    pub kind: String,
    /// 服务器
    pub server: String,
    /// 端口
    pub port: u16,
    /// UDP 中继
    pub udp: bool,
}

/// SSH 出站代理。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SshProxy {
    /// 名称
    pub name: String,
    /// 类型
    #[serde(rename = "type")]
    pub kind: String,
    /// 服务器
    pub server: String,
    /// 端口
    pub port: u16,
    /// 用户名
    pub username: String,
    /// 密码（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// 私钥内容或路径（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    /// 私钥口令（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key_passphrase: Option<String>,
    /// 固定 host key（可选）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub host_key: Vec<String>,
}

/// 订阅 proxy-provider。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProxyProvider {
    /// 类型：http / file
    #[serde(rename = "type")]
    pub kind: String,
    /// 远程地址（http 类型）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// 本地缓存路径（相对工作目录）
    pub path: String,
    /// 更新间隔（秒），file 类型为 0
    pub interval: u32,
    /// 健康检查
    pub health_check: HealthCheck,
}

/// provider 健康检查。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct HealthCheck {
    /// 是否启用
    pub enable: bool,
    /// 检查 URL
    pub url: String,
    /// 间隔（秒）
    pub interval: u32,
}

/// 代理组。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GroupSection {
    /// 组名
    pub name: String,
    /// 类型：select / url-test / fallback
    #[serde(rename = "type")]
    pub kind: String,
    /// 固定成员
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub proxies: Vec<String>,
    /// 引用 provider
    #[serde(rename = "use", skip_serializing_if = "Vec::is_empty")]
    pub use_providers: Vec<String>,
    /// 健康检查 URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// 测速间隔（秒）
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub interval: u32,
    /// 容差（毫秒）
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub tolerance: u32,
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

/// rule-provider。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RuleProvider {
    /// 类型：http / file
    #[serde(rename = "type")]
    pub kind: String,
    /// behavior
    pub behavior: String,
    /// 格式
    pub format: String,
    /// 远程地址
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// 本地路径
    pub path: String,
    /// 更新间隔（秒）
    #[serde(skip_serializing_if = "is_zero_u32")]
    pub interval: u32,
}

/// 域名嗅探。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SnifferSection {
    /// 是否启用
    pub enable: bool,
    /// 强制 DNS 映射
    pub force_dns_mapping: bool,
    /// 解析纯 IP 连接
    pub parse_pure_ip: bool,
    /// 覆盖目标地址
    pub override_destination: bool,
    /// 按协议嗅探端口
    pub sniff: BTreeMap<String, SniffPorts>,
}

/// 嗅探端口。
#[derive(Debug, Clone, Serialize)]
pub struct SniffPorts {
    /// 端口列表
    pub ports: Vec<String>,
}
