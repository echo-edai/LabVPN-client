//! 特权 helper 的通信协议（应用层与 `labvpn-helper` 共用）。
//!
//! 传输：回环 TCP（默认 `127.0.0.1:38471`），JSON Lines（一行一个 JSON 对象）。
//! 安全：每个请求必须携带令牌；helper 只接受回环连接、固定命令集、固定核心路径。

use serde::{Deserialize, Serialize};

/// 默认 IPC 端口。
pub const DEFAULT_PORT: u16 = 38471;
/// 协议版本（helper 与 App 不匹配时提示升级）。
pub const PROTOCOL_VERSION: u32 = 1;
/// 单次请求最大字节数（含配置 YAML）。
pub const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
/// helper 服务标签（macOS launchd label / Linux service 名）。
pub const SERVICE_LABEL: &str = "app.labvpn.helper";

/// 请求命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Command {
    /// 存活探测
    Ping,
    /// 启动代理核心
    StartCore {
        /// 生成好的 mihomo 配置 YAML
        config_yaml: String,
        /// 需要写入工作目录的附加文件（相对路径 -> 内容），如订阅缓存、SSH 私钥
        #[serde(default)]
        assets: std::collections::BTreeMap<String, String>,
    },
    /// 停止代理核心
    StopCore,
    /// 重启代理核心
    RestartCore {
        /// 新配置
        config_yaml: String,
        /// 附加文件
        #[serde(default)]
        assets: std::collections::BTreeMap<String, String>,
    },
    /// 查询状态
    Status,
    /// 停止 helper 自身
    Shutdown,
}

/// 请求信封（令牌 + 命令）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// 认证令牌
    pub token: String,
    /// 命令
    #[serde(flatten)]
    pub command: Command,
}

/// 核心运行状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CoreStatus {
    /// 核心是否运行
    pub running: bool,
    /// 核心进程 PID
    pub pid: Option<u32>,
    /// 运行时长（秒）
    pub uptime_secs: Option<u64>,
    /// 最近一次错误
    pub last_error: Option<String>,
    /// 配置版本（App 侧生成时间戳，便于排查）
    pub config_generation: Option<u64>,
    /// 核心日志文件路径（供诊断）
    pub log_path: Option<String>,
}

/// 响应。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Response {
    /// 是否成功
    pub ok: bool,
    /// 错误信息（失败时）
    pub error: Option<String>,
    /// helper 版本
    pub helper_version: String,
    /// 协议版本
    pub protocol_version: u32,
    /// helper PID
    pub helper_pid: u32,
    /// 平台（macos/windows/linux）
    pub platform: String,
    /// 核心状态
    pub status: Option<CoreStatus>,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            ok: false,
            error: None,
            helper_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION,
            helper_pid: std::process::id(),
            platform: std::env::consts::OS.to_string(),
            status: None,
        }
    }
}

impl Response {
    /// 成功响应。
    pub fn success() -> Self {
        Self {
            ok: true,
            ..Self::default()
        }
    }

    /// 失败响应。
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
            ..Self::default()
        }
    }

    /// 附加状态。
    pub fn with_status(mut self, status: CoreStatus) -> Self {
        self.status = Some(status);
        self
    }
}

/// 协议版本是否兼容。
pub fn is_compatible(helper_protocol: u32) -> bool {
    helper_protocol == PROTOCOL_VERSION
}

/// 校验令牌（长度一致 + 常量时间比较，降低时序泄漏）。
pub fn verify_token(expected: &str, actual: &str) -> bool {
    if expected.is_empty() || expected.len() != actual.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(actual.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_request_with_tag() {
        let request = Request {
            token: "t".into(),
            command: Command::Ping,
        };
        let json = serde_json::to_string(&request).expect("serialize");
        assert!(json.contains("\"cmd\":\"ping\""));
        assert!(json.contains("\"token\":\"t\""));
        let back: Request = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, request);
    }

    #[test]
    fn start_core_roundtrip() {
        let request = Request {
            token: "abc".into(),
            command: Command::StartCore {
                config_yaml: "mixed-port: 7897\n".into(),
                assets: Default::default(),
            },
        };
        let json = serde_json::to_string(&request).expect("serialize");
        let back: Request = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, request);
    }

    #[test]
    fn token_verification() {
        assert!(verify_token("secret-token", "secret-token"));
        assert!(!verify_token("secret-token", "secret-toke"));
        assert!(!verify_token("", ""));
        assert!(!verify_token("abc", "abcd"));
    }

    #[test]
    fn protocol_compatibility() {
        assert!(is_compatible(PROTOCOL_VERSION));
        assert!(!is_compatible(PROTOCOL_VERSION + 1));
    }
}
