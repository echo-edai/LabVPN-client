//! 订阅拉取、解析、缓存与配置写回。

use crate::dto::SubscriptionInfo;
use crate::error::{CmdResult, CommandError};
use crate::paths::AppPaths;
use crate::state::AppState;
use crate::util::redact_url;
use chrono::Utc;
use labvpn_core::config::{SubscriptionConfig, TrafficInfo};
use labvpn_core::subscription as core_sub;

/// 拉取结果。
pub struct FetchResult {
    /// 原始内容
    pub content: String,
    /// 流量信息（响应头）
    pub traffic: Option<TrafficInfo>,
    /// 是否通过代理拉取（IP 鉴权机场需要保持本地缓存，避免 mihomo 直连拉取再次 403）
    pub via_proxy: bool,
}

/// 解析 file:// URL 为本地路径。
///
/// 兼容：
/// - `file:///C:/Users/x/sub.yaml`（RFC 8089 三斜杠，Windows）
/// - `file://C:/Users/x/sub.yaml`（非标准两斜杠）
/// - `file:///home/user/sub.yaml`（Unix）
/// - `file://localhost/...`
/// - 百分号编码（空格/中文）
fn file_url_to_path(url: &str) -> Result<std::path::PathBuf, String> {
    let rest = url
        .strip_prefix("file://")
        .ok_or_else(|| "不是 file:// 地址".to_string())?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    // Windows 三斜杠：/C:/... -> C:/...
    let rest = if rest.len() >= 3
        && rest.as_bytes()[0] == b'/'
        && rest.as_bytes()[1].is_ascii_alphabetic()
        && rest.as_bytes()[2] == b':'
    {
        &rest[1..]
    } else {
        rest
    };
    if rest.trim().is_empty() {
        return Err("file:// 地址缺少文件路径".to_string());
    }
    let decoded = percent_decode(rest)?;
    Ok(std::path::PathBuf::from(decoded))
}

/// 解码 URL 中的百分号编码（UTF-8）。
fn percent_decode(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("file:// 地址包含无效的百分号编码".to_string());
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|_| "file:// 地址包含无效的百分号编码".to_string())?;
            let value = u8::from_str_radix(hex, 16)
                .map_err(|_| "file:// 地址包含无效的百分号编码".to_string())?;
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "file:// 地址不是有效的 UTF-8 路径".to_string())
}

/// 规范化自定义代理地址（支持纯端口）。
fn normalize_proxy(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(port) = value.parse::<u16>() {
        return Some(format!("http://127.0.0.1:{port}"));
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        Some(value.to_string())
    } else {
        None
    }
}

/// 候选拉取通道（标签, 代理地址）。
async fn candidate_proxies(
    state: &AppState,
    config: &SubscriptionConfig,
) -> Vec<(String, Option<String>)> {
    use labvpn_core::config::SubscriptionFetchMode;
    let engine_proxy = {
        let status = state.engine.status().await;
        if status.running {
            Some(format!("http://127.0.0.1:{}", status.mixed_port))
        } else {
            None
        }
    };
    let system_proxy =
        crate::platform::system_proxy_port().map(|port| format!("http://127.0.0.1:{port}"));
    let custom_proxy = config.fetch_proxy.as_deref().and_then(normalize_proxy);
    let env_proxy = [
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
    ]
    .iter()
    .find_map(|key| std::env::var(key).ok())
    .and_then(|value| normalize_proxy(&value));

    let mut candidates: Vec<(String, Option<String>)> = Vec::new();
    let mut push = |label: &str, proxy: Option<String>| {
        if candidates
            .iter()
            .all(|(existing, existing_proxy)| existing != label && *existing_proxy != proxy)
        {
            candidates.push((label.to_string(), proxy));
        }
    };

    match config.fetch_mode {
        SubscriptionFetchMode::Direct => push("直连", None),
        SubscriptionFetchMode::SystemProxy => push("系统代理", system_proxy),
        SubscriptionFetchMode::Custom => push("自定义代理", custom_proxy),
        SubscriptionFetchMode::Auto => {
            push("直连", None);
            if let Some(proxy) = engine_proxy.clone() {
                push("本机 mihomo", Some(proxy));
            }
            if let Some(proxy) = system_proxy.clone() {
                push("系统代理", Some(proxy));
            }
            if let Some(proxy) = custom_proxy.clone() {
                push("自定义代理", Some(proxy));
            }
            if let Some(proxy) = env_proxy {
                push("环境变量代理", Some(proxy));
            }
        }
    }
    candidates
}

fn client_for(proxy: Option<&str>) -> CmdResult<reqwest::Client> {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(25));
    builder = match proxy {
        Some(proxy) => builder
            .proxy(reqwest::Proxy::all(proxy).map_err(|err| {
                CommandError::new("subscription", format!("代理地址无效: {err}"))
            })?),
        None => builder.no_proxy(),
    };
    builder
        .build()
        .map_err(|err| CommandError::new("subscription", format!("初始化 HTTP 客户端失败: {err}")))
}

/// 单次 HTTP 拉取；非 2xx 时把响应体摘要一起返回，便于识别 IP 鉴权等问题。
async fn fetch_once(
    config: &SubscriptionConfig,
    url: &str,
    proxy: Option<&str>,
) -> Result<FetchResult, String> {
    let client = client_for(proxy).map_err(|err| err.to_string())?;
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, &config.user_agent)
        .header(reqwest::header::ACCEPT, "*/*")
        .send()
        .await
        .map_err(|err| format!("请求失败: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let snippet: String = body
            .trim()
            .replace(['\n', '\r'], " ")
            .chars()
            .take(240)
            .collect();
        return Err(if snippet.is_empty() {
            format!("HTTP {status}")
        } else {
            format!("HTTP {status}：{snippet}")
        });
    }
    let traffic = response
        .headers()
        .get("subscription-userinfo")
        .and_then(|value| value.to_str().ok())
        .and_then(core_sub::parse_userinfo);
    let content = response
        .text()
        .await
        .map_err(|err| format!("读取内容失败: {err}"))?;
    Ok(FetchResult {
        content,
        traffic,
        via_proxy: proxy.is_some(),
    })
}

/// 拉取订阅内容（支持 http(s) 与 file://）。
///
/// `fetch_mode` 控制 HTTP 拉取通道；自动模式下依次尝试直连 / 本机 mihomo / 系统代理 / 自定义代理，
/// 以解决「机场启用 IP 鉴权，直连返回 403」的问题。
pub async fn fetch(state: &AppState, config: &SubscriptionConfig) -> CmdResult<FetchResult> {
    let url = config.url.trim();
    if url.is_empty() {
        return Err(CommandError::new("subscription", "订阅地址为空"));
    }
    if url.starts_with("file://") {
        let path = file_url_to_path(url).map_err(|err| {
            CommandError::new("subscription", format!("file:// 地址格式不合法：{err}"))
        })?;
        let content = std::fs::read_to_string(&path).map_err(|err| {
            CommandError::new(
                "subscription",
                format!("读取本地订阅失败（{}）：{err}", path.display()),
            )
        })?;
        return Ok(FetchResult {
            content,
            traffic: None,
            via_proxy: false,
        });
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(CommandError::new(
            "subscription",
            "订阅地址必须以 http(s):// 或 file:// 开头",
        ));
    }

    let candidates = candidate_proxies(state, config).await;
    let mut failures: Vec<String> = Vec::new();
    for (label, proxy) in &candidates {
        match fetch_once(config, url, proxy.as_deref()).await {
            Ok(result) => {
                if result.via_proxy {
                    state
                        .logs
                        .info("subscription", format!("订阅已通过代理拉取（{label}）"));
                }
                return Ok(result);
            }
            Err(err) => failures.push(format!("{label}：{err}")),
        }
    }

    let mut detail = failures.join("；");
    if !detail.is_empty() {
        detail.push('。');
    }
    detail.push_str(
        "如果响应包含「IP鉴权 / AUTH_FAIL」，说明该机场对订阅来源 IP 做了白名单：\
         请把「订阅拉取方式」改为自定义代理，并填写你本机代理的端口（如 Clash Verge 的 7897）。",
    );
    Err(CommandError::new("subscription", "订阅拉取失败").with_detail(detail))
}

/// 保存订阅内容到缓存文件（0600）。
pub fn save_profile(paths: &AppPaths, content: &str) -> CmdResult<()> {
    std::fs::create_dir_all(&paths.profiles_dir)?;
    let tmp = paths.profile_file.with_extension("yaml.tmp");
    std::fs::write(&tmp, content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &paths.profile_file)?;
    Ok(())
}

/// 从配置生成展示信息。
pub fn info_from_config(paths: &AppPaths, config: &SubscriptionConfig) -> SubscriptionInfo {
    let cache_exists = paths.profile_file.exists();
    SubscriptionInfo {
        enabled: config.enabled,
        name: config.name.clone(),
        url_masked: redact_url(&config.url),
        proxy_count: config.proxy_count.unwrap_or(0),
        group_count: 0,
        last_updated: config
            .last_updated
            .map(|time| time.format("%Y-%m-%d %H:%M").to_string()),
        traffic: config.traffic.clone(),
        warnings: if cache_exists {
            Vec::new()
        } else {
            vec!["尚未缓存订阅内容".to_string()]
        },
        format: None,
        proxy_names: Vec::new(),
    }
}

/// 导入订阅：拉取 → 校验格式 → 缓存 → 写回配置。
pub async fn import(
    state: &AppState,
    url: &str,
    name: Option<&str>,
    persist: bool,
) -> CmdResult<SubscriptionInfo> {
    let mut config = state.config_snapshot();
    let previous_url = config.subscription.url.clone();
    config.subscription.url = url.trim().to_string();
    if let Some(name) = name {
        let name = name.trim();
        if !name.is_empty() {
            config.subscription.name = name.to_string();
        }
    }
    config.subscription.enabled = true;

    let result = match fetch(state, &config.subscription).await {
        Ok(result) => result,
        Err(err) => {
            config.subscription.url = previous_url;
            return Err(err);
        }
    };
    let format = core_sub::detect_format(&result.content);
    let summary = core_sub::summarize(&result.content).map_err(|err| {
        CommandError::new("subscription", err.to_string()).with_detail(format!(
            "识别为：{}。当前版本仅支持 Clash/mihomo YAML 订阅",
            format.label_zh()
        ))
    })?;
    if format != core_sub::SubscriptionFormat::ClashYaml {
        return Err(CommandError::new(
            "subscription",
            format!(
                "订阅格式为「{}」，当前版本仅支持 Clash/mihomo YAML。请在机场后台选择 Clash 订阅链接",
                format.label_zh()
            ),
        ));
    }
    if persist {
        save_profile(&state.paths, &result.content)?;
    }

    config.subscription.last_updated = Some(Utc::now());
    config.subscription.traffic = result.traffic.or(summary.traffic);
    config.subscription.proxy_count = Some(summary.proxy_count);
    // 本地文件 / 通过代理拉取的订阅：让 mihomo 使用本地缓存，避免内核直连拉取再次失败
    if config.subscription.use_remote_provider
        && (url.trim().starts_with("file://") || result.via_proxy)
    {
        config.subscription.use_remote_provider = false;
    }
    if persist {
        state.save_config(config.clone())?;
    }

    let mut info = info_from_config(&state.paths, &config.subscription);
    info.proxy_count = summary.proxy_count;
    info.group_count = summary.group_count;
    info.proxy_names = summary.proxy_names;
    info.format = Some(format.label_zh().to_string());
    info.warnings = summary.warnings;
    state.logs.info(
        "subscription",
        format!(
            "订阅已导入：{} 个节点，{} 个代理组",
            info.proxy_count, info.group_count
        ),
    );
    Ok(info)
}

/// 刷新当前订阅（应用启动/定时任务调用）。
pub async fn refresh(state: &AppState) -> CmdResult<SubscriptionInfo> {
    let config = state.config_snapshot();
    if !config.subscription.enabled || config.subscription.url.trim().is_empty() {
        return Ok(info_from_config(&state.paths, &config.subscription));
    }
    import(state, &config.subscription.url.clone(), None, true).await
}

/// 仅探测订阅可用性（不写配置）。
pub async fn test_url(state: &AppState, url: &str) -> CmdResult<SubscriptionInfo> {
    let mut config = state.config_snapshot();
    config.subscription.url = url.trim().to_string();
    config.subscription.enabled = true;
    let result = fetch(state, &config.subscription).await?;
    let summary = core_sub::summarize(&result.content)?;
    let format = core_sub::detect_format(&result.content);
    Ok(SubscriptionInfo {
        enabled: true,
        name: config.subscription.name,
        url_masked: redact_url(url),
        proxy_count: summary.proxy_count,
        group_count: summary.group_count,
        last_updated: None,
        traffic: result.traffic,
        warnings: summary.warnings,
        format: Some(format.label_zh().to_string()),
        proxy_names: summary.proxy_names,
    })
}

#[cfg(test)]
mod tests {
    use super::{file_url_to_path, normalize_proxy, percent_decode};

    #[test]
    fn parses_file_urls() {
        // Windows 标准三斜杠
        assert_eq!(
            file_url_to_path("file:///C:/Users/me/sub.yaml")
                .expect("triple slash")
                .to_string_lossy(),
            "C:/Users/me/sub.yaml"
        );
        // Windows 非标准两斜杠
        assert_eq!(
            file_url_to_path("file://C:/Users/me/sub.yaml")
                .expect("double slash")
                .to_string_lossy(),
            "C:/Users/me/sub.yaml"
        );
        // Unix 绝对路径
        assert_eq!(
            file_url_to_path("file:///tmp/sub.yaml")
                .expect("unix")
                .to_string_lossy(),
            "/tmp/sub.yaml"
        );
        // localhost + 百分号编码
        assert_eq!(
            file_url_to_path("file://localhost/tmp/ci%20la%20sub.yaml")
                .expect("localhost")
                .to_string_lossy(),
            "/tmp/ci la sub.yaml"
        );
        assert!(file_url_to_path("file://").is_err());
    }

    #[test]
    fn decodes_percent_and_rejects_bad_encoding() {
        assert_eq!(percent_decode("a%20b%E4%B8%AD").expect("decode"), "a b中");
        assert!(percent_decode("bad%2").is_err());
        assert!(percent_decode("bad%ZZ").is_err());
    }

    #[test]
    fn normalizes_proxy_addresses() {
        assert_eq!(
            normalize_proxy("7897").as_deref(),
            Some("http://127.0.0.1:7897")
        );
        assert_eq!(
            normalize_proxy("http://127.0.0.1:7897").as_deref(),
            Some("http://127.0.0.1:7897")
        );
        assert!(normalize_proxy("socks5://127.0.0.1:1080").is_none());
        assert!(normalize_proxy("").is_none());
    }
}
