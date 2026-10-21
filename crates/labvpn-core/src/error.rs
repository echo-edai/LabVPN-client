//! 统一错误类型。

/// LabVPN 核心错误。
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// 配置不合法（附带字段定位）。
    #[error("配置校验失败: {0}")]
    Validation(String),

    /// YAML 序列化/反序列化失败。
    #[error("YAML 处理失败: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),

    /// JSON 序列化/反序列化失败。
    #[error("JSON 处理失败: {0}")]
    Json(#[from] serde_json::Error),

    /// 订阅内容无法解析。
    #[error("订阅解析失败: {0}")]
    Subscription(String),

    /// 网络相关失败（由应用层包装）。
    #[error("网络失败: {0}")]
    Network(String),

    /// 校园网探测失败。
    #[error("校园网探测失败: {0}")]
    Probe(String),

    /// 规则非法。
    #[error("规则非法: {0}")]
    Rule(String),

    /// IO 失败。
    #[error("IO 失败: {0}")]
    Io(#[from] std::io::Error),
}

/// 核心库统一 Result。
pub type CoreResult<T> = std::result::Result<T, CoreError>;
