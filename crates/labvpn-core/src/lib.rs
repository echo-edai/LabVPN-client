//! # labvpn-core
//!
//! LabVPN 的纯逻辑核心库。本 crate **不依赖** Tauri / tokio / 网络请求 / 文件系统，
//! 便于单元测试与在三平台一致地复用。
//!
//! 模块划分：
//! - [`config`]：应用配置模型、默认值、校验与迁移
//! - [`rules`]：分流规则模型、代理组拓扑、规则预设、自定义规则校验、命中模拟
//! - [`generator`]：`AppConfig + RuntimeContext -> mihomo YAML`（纯函数）
//! - [`ipc`]：特权 helper 通信协议（App 与 helper 共用）
//! - [`subscription`]：Clash 订阅内容解析、格式识别、流量信息
//! - [`probe`]：校园网多信号探测（含按网卡绑定以绕过第三方 TUN）
//! - [`util`]：通用工具（脱敏、哈希、IP/CIDR 解析等）

#![deny(clippy::unwrap_used)]
#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod generator;
pub mod ipc;
pub mod probe;
pub mod rules;
pub mod subscription;
pub mod util;

pub use config::AppConfig;
pub use error::{CoreError, CoreResult};

/// 应用显示名（同时用于日志与路径）
pub const APP_NAME: &str = "LabVPN";

/// 应用标识（bundle identifier）
pub const APP_ID: &str = "app.labvpn.desktop";

/// 数据目录下的子目录名
pub mod dirs {
    /// 生成的 mihomo 配置目录
    pub const CORE_DIR: &str = "core";
    /// 订阅缓存目录
    pub const PROFILES_DIR: &str = "profiles";
    /// 日志目录
    pub const LOGS_DIR: &str = "logs";
    /// sidecar 二进制目录
    pub const BIN_DIR: &str = "bin";
}

/// mihomo 生成的配置文件在 [`dirs::CORE_DIR`] 下的文件名
pub const CORE_CONFIG_FILE: &str = "config.yaml";

/// 订阅缓存文件名
pub const PROFILE_FILE: &str = "profile.yaml";
