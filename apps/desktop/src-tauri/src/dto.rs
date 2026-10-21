//! 前后端传输对象（DTO）：全部 `camelCase` 序列化。

use std::collections::BTreeMap;

use labvpn_core::config::{AppConfig, TrafficInfo, ValidationIssue};
use labvpn_core::generator::GenerationStats;
use labvpn_core::probe::CampusStatus;
use labvpn_core::rules::presets::PresetInfo;
use serde::{Deserialize, Serialize};

/// 路径信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathsInfo {
    /// 数据根目录
    pub data_dir: String,
    /// 配置文件
    pub config_file: String,
    /// 核心工作目录
    pub core_dir: String,
    /// helper 工作目录
    pub helper_workdir: String,
    /// 日志目录
    pub logs_dir: String,
    /// 二进制目录
    pub bin_dir: String,
}

/// 配置信封：配置 + 密钥存在性 + 校验结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEnvelope {
    /// 配置
    pub config: AppConfig,
    /// 密钥键 -> 是否存在
    pub secrets_present: BTreeMap<String, bool>,
    /// 校验问题
    pub validation: Vec<ValidationIssue>,
    /// 路径信息
    pub paths: PathsInfo,
}

/// mihomo 核心信息。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreInfo {
    /// 核心二进制路径
    pub binary_path: Option<String>,
    /// 核心版本（运行时探测）
    pub version: Option<String>,
    /// 来源：resource/appData/dev/path/missing
    pub source: String,
}

/// 引擎运行状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// 状态：idle / starting / running / stopping / error
    pub state: String,
    /// 运行模式：tun / systemProxy
    pub mode: Option<String>,
    /// 是否正在运行
    pub running: bool,
    /// TUN 是否实际启用
    pub tun_enabled: bool,
    /// 是否由 helper 托管
    pub helper_mode: bool,
    /// 混合代理端口
    pub mixed_port: u16,
    /// 控制端口
    pub controller_port: u16,
    /// 核心 PID
    pub pid: Option<u32>,
    /// 启动时间
    pub started_at: Option<String>,
    /// 最近错误
    pub last_error: Option<String>,
    /// 生成配置时的警告
    pub warnings: Vec<String>,
}

impl EngineStatus {
    /// 空闲状态。
    pub fn idle(mixed_port: u16, controller_port: u16) -> Self {
        Self {
            state: "idle".to_string(),
            mode: None,
            running: false,
            tun_enabled: false,
            helper_mode: false,
            mixed_port,
            controller_port,
            pid: None,
            started_at: None,
            last_error: None,
            warnings: Vec::new(),
        }
    }
}

/// 校内 VPN 状态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnStatus {
    /// 配置是否启用
    pub enabled: bool,
    /// 状态：disabled / stopped / detecting / starting / running / reusing / error
    pub state: String,
    /// 是否托管模式
    pub managed: bool,
    /// SOCKS 端口
    pub socks_port: u16,
    /// 二进制路径
    pub binary_path: Option<String>,
    /// PID
    pub pid: Option<u32>,
    /// 状态说明
    pub message: Option<String>,
    /// 最近错误
    pub last_error: Option<String>,
    /// 是否复用已运行的实例
    pub existing_instance: bool,
}

/// 订阅信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    /// 是否启用
    pub enabled: bool,
    /// 名称
    pub name: String,
    /// 脱敏后的 URL
    pub url_masked: String,
    /// 节点数量
    pub proxy_count: usize,
    /// 代理组数量
    pub group_count: usize,
    /// 最近更新时间
    pub last_updated: Option<String>,
    /// 流量信息
    pub traffic: Option<TrafficInfo>,
    /// 解析警告
    pub warnings: Vec<String>,
    /// 识别到的格式
    pub format: Option<String>,
    /// 节点名称预览
    pub proxy_names: Vec<String>,
}

/// 代理组信息（来自 mihomo API）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyGroupInfo {
    /// 组名
    pub name: String,
    /// 类型
    pub kind: String,
    /// 当前选中
    pub now: Option<String>,
    /// 可选项
    pub all: Vec<String>,
    /// 当前延迟
    pub delay: Option<u32>,
}

/// 实时流量（每秒）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficPoint {
    /// 上行（B/s）
    pub up: u64,
    /// 下行（B/s）
    pub down: u64,
    /// 累计上行
    pub up_total: u64,
    /// 累计下行
    pub down_total: u64,
    /// 时间
    pub ts: String,
}

/// 日志条目。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// 自增 ID
    pub id: u64,
    /// 时间
    pub ts: String,
    /// 级别
    pub level: String,
    /// 来源（app / core / vpn / helper）
    pub source: String,
    /// 内容
    pub message: String,
}

/// 配置预览结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    /// 完整 YAML
    pub yaml: String,
    /// 警告
    pub warnings: Vec<String>,
    /// 统计
    pub stats: GenerationStats,
    /// 最终规则列表
    pub rules: Vec<String>,
}

/// 分流诊断结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsResult {
    /// 输入
    pub input: String,
    /// 是否命中
    pub matched: bool,
    /// 命中的规则
    pub rule: String,
    /// 出口
    pub outbound: String,
    /// 诊断备注
    pub notes: Vec<String>,
}

/// 自定义规则校验结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleValidation {
    /// 合法规则（归一化后）
    pub valid: Vec<String>,
    /// 非法规则（原始行）
    pub invalid: Vec<String>,
    /// 警告
    pub warnings: Vec<String>,
}

/// 通用操作结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    /// 是否成功
    pub ok: bool,
    /// 说明
    pub message: String,
    /// 详情
    pub detail: Option<String>,
}

impl OperationResult {
    /// 成功。
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
            detail: None,
        }
    }
}

/// 现存 fdu-connect 安装探测结果。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExistingVpn {
    /// 二进制路径
    pub binary_path: Option<String>,
    /// 正在监听的 SOCKS 端口
    pub socks_port: Option<u16>,
    /// 是否正在运行
    pub running: bool,
    /// macOS launchd 标签
    pub service_label: Option<String>,
}

/// VPN 连接测试结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnTestResult {
    /// 是否成功
    pub ok: bool,
    /// 结论
    pub message: String,
    /// 日志细节
    pub log_lines: Vec<String>,
}

/// SSH 连通性测试结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTestResult {
    /// 是否连通
    pub ok: bool,
    /// 说明
    pub message: String,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
}

/// 环境信息（设置页/排障）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    /// 操作系统
    pub os: String,
    /// 架构
    pub arch: String,
    /// App 版本
    pub app_version: String,
    /// helper 是否已安装（服务存在）
    pub helper_installed: bool,
    /// helper 是否正在运行
    pub helper_running: bool,
    /// helper 版本
    pub helper_version: Option<String>,
    /// 默认出口网卡
    pub default_interface: Option<String>,
    /// Wi-Fi SSID
    pub wifi_ssid: Option<String>,
    /// 系统代理是否指向本应用
    pub system_proxy_enabled: bool,
    /// 已存在的 fdu-connect
    pub existing_vpn: ExistingVpn,
    /// 端口占用冲突
    pub port_conflicts: Vec<String>,
    /// 校园网手动覆盖
    pub campus_override: Option<bool>,
    /// 当前核心版本
    pub core_version: Option<String>,
    /// 应用已运行时长（秒）
    pub app_uptime_secs: u64,
}

/// 连接结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectOutcome {
    /// 引擎状态
    pub engine: EngineStatus,
    /// 校内 VPN 状态
    pub vpn: VpnStatus,
    /// 校园网检测
    pub campus: Option<CampusStatus>,
    /// 运行模式：tun / systemProxy
    pub mode: String,
    /// 警告
    pub warnings: Vec<String>,
}

/// 应用快照（前端首屏一次拉取）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    /// 配置
    pub config: ConfigEnvelope,
    /// 引擎
    pub engine: EngineStatus,
    /// 校内 VPN
    pub vpn: VpnStatus,
    /// 校园网检测
    pub campus: Option<CampusStatus>,
    /// 订阅
    pub subscription: SubscriptionInfo,
    /// 核心
    pub core: CoreInfo,
    /// 系统环境
    pub system: SystemInfo,
    /// 预设
    pub presets: Vec<PresetInfo>,
    /// 最近日志
    pub logs: Vec<LogEntry>,
}

/// 前端提交配置时附带的密钥更新（键 -> 明文，后端立即写入密钥库）。
pub type SecretUpdates = BTreeMap<String, String>;

/// 订阅导入请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSubscriptionRequest {
    /// 订阅地址
    pub url: String,
    /// 名称（可空）
    pub name: Option<String>,
    /// 是否保存到配置
    #[serde(default = "default_true")]
    pub persist: bool,
}

fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// 网络分析
// ---------------------------------------------------------------------------

/// 分类流量字节。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryBytes {
    /// 上行
    pub up: u64,
    /// 下行
    pub down: u64,
}

/// 分类流量总计。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryTotals {
    /// 校内流量
    pub school: CategoryBytes,
    /// 海外（订阅节点）流量
    pub overseas: CategoryBytes,
    /// 国内直连
    pub direct: CategoryBytes,
    /// SSH 转发
    pub ssh: CategoryBytes,
    /// 其他
    pub other: CategoryBytes,
    /// AI 平台流量
    pub ai: CategoryBytes,
}

/// 一个时间桶的分类流量。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficBucketDto {
    /// 桶起始时间（Unix 秒）
    pub ts: i64,
    /// 校内
    pub school: CategoryBytes,
    /// 海外
    pub overseas: CategoryBytes,
    /// 直连
    pub direct: CategoryBytes,
    /// SSH
    pub ssh: CategoryBytes,
    /// 其他
    pub other: CategoryBytes,
    /// AI 平台
    pub ai: CategoryBytes,
}

/// 出口 IP 记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EgressSampleDto {
    /// 时间
    pub ts: i64,
    /// 路由：direct / proxy
    pub route: String,
    /// 出口 IP
    pub ip: String,
    /// 代理节点（proxy 路由）
    pub node: Option<String>,
}

/// 校园网状态点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CampusPointDto {
    /// 时间
    pub ts: i64,
    /// 是否在校内
    pub on_campus: bool,
}

/// 网络分析数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsData {
    /// 时间范围：24h / 7d / 30d
    pub range: String,
    /// 桶粒度：hour / day
    pub granularity: String,
    /// 总计
    pub totals: CategoryTotals,
    /// 流量时间桶
    pub buckets: Vec<TrafficBucketDto>,
    /// 出口 IP 历史
    pub egress: Vec<EgressSampleDto>,
    /// 校园网状态时间线
    pub campus: Vec<CampusPointDto>,
    /// 配置的 AI 平台域名
    pub ai_domains: Vec<String>,
    /// 数据更新时间
    pub updated_at: String,
}
