//! 应用层小工具。

/// 对 URL 做脱敏（复用核心库实现）。
pub fn redact_url(url: &str) -> String {
    labvpn_core::util::redact_url(url)
}

/// 读取文件内容（限制大小，用于 SSH 私钥等）。
pub fn read_small_file(path: &std::path::Path, max_bytes: usize) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|err| format!("读取文件失败: {err}"))?;
    if metadata.len() as usize > max_bytes {
        return Err(format!(
            "文件过大（{} 字节，上限 {max_bytes}）",
            metadata.len()
        ));
    }
    std::fs::read_to_string(path).map_err(|err| format!("读取文件失败: {err}"))
}
