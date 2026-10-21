//! 命令层统一错误类型（可直接序列化给前端）。

use serde::Serialize;

/// 统一的命令错误。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    /// 机器可读的错误码（如 `config_invalid`、`network`）
    pub code: String,
    /// 用户可见的错误信息
    pub message: String,
    /// 详细上下文（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl CommandError {
    /// 构造错误。
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    /// 追加详细上下文。
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.detail {
            Some(detail) => write!(formatter, "{}（{}）", self.message, detail),
            None => write!(formatter, "{}", self.message),
        }
    }
}

impl std::error::Error for CommandError {}

impl From<labvpn_core::CoreError> for CommandError {
    fn from(error: labvpn_core::CoreError) -> Self {
        Self::new("core", error.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(error: std::io::Error) -> Self {
        Self::new("io", error.to_string())
    }
}

impl From<serde_json::Error> for CommandError {
    fn from(error: serde_json::Error) -> Self {
        Self::new("json", error.to_string())
    }
}

impl From<serde_yaml_ng::Error> for CommandError {
    fn from(error: serde_yaml_ng::Error) -> Self {
        Self::new("yaml", error.to_string())
    }
}

impl From<reqwest::Error> for CommandError {
    fn from(error: reqwest::Error) -> Self {
        Self::new("network", format!("网络请求失败: {error}"))
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::new("error", message)
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self::new("error", message)
    }
}

/// 命令层 Result。
pub type CmdResult<T> = std::result::Result<T, CommandError>;
