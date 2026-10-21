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
}

/// 拉取订阅内容（支持 http(s) 与 file://）。
pub async fn fetch(
    client: &reqwest::Client,
    config: &SubscriptionConfig,
) -> CmdResult<FetchResult> {
    let url = config.url.trim();
    if url.is_empty() {
        return Err(CommandError::new("subscription", "订阅地址为空"));
    }
    if let Some(path) = url.strip_prefix("file://") {
        let content = std::fs::read_to_string(path)
            .map_err(|err| CommandError::new("subscription", format!("读取本地订阅失败: {err}")))?;
        return Ok(FetchResult {
            content,
            traffic: None,
        });
    }
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, &config.user_agent)
        .header(reqwest::header::ACCEPT, "*/*")
        .send()
        .await
        .map_err(|err| CommandError::new("subscription", format!("请求订阅失败: {err}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(CommandError::new(
            "subscription",
            format!("订阅服务器返回 HTTP {status}"),
        ));
    }
    let traffic = response
        .headers()
        .get("subscription-userinfo")
        .and_then(|value| value.to_str().ok())
        .and_then(core_sub::parse_userinfo);
    let content = response
        .text()
        .await
        .map_err(|err| CommandError::new("subscription", format!("读取订阅内容失败: {err}")))?;
    Ok(FetchResult { content, traffic })
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

    let result = match fetch(&state.http, &config.subscription).await {
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
    if config.subscription.use_remote_provider && url.trim().starts_with("file://") {
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
    let result = fetch(&state.http, &config.subscription).await?;
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
