//! 应用更新检查：请求 GitHub Releases 列表并挑选比当前版本新的 Release。
//!
//! 只读网络请求，不写任何文件；版本比较逻辑在 [`labvpn_core::update`]（可单测）。

use std::time::Duration;

use serde::Serialize;

use crate::error::{CmdResult, CommandError};

/// 项目 Releases 列表（本项目所有 Release 均为 pre-release，因此不用 `/latest`）。
const RELEASES_URL: &str =
    "https://api.github.com/repos/echo-edai/LabVPN-client/releases?per_page=30";
/// Release 说明最多回传的字符数（避免超长 Markdown 撑爆前端）。
const MAX_NOTES_CHARS: usize = 4000;

/// 一次更新检查的结果（前端用于展示更新提示）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// 当前应用版本
    pub current: String,
    /// 远端最新版本（`x.y.z`）
    pub latest: String,
    /// 是否存在比当前版本更新的发布
    pub has_update: bool,
    /// Release 页面地址
    pub release_url: String,
    /// Release 标题
    pub release_name: Option<String>,
    /// Release 说明（Markdown 原文，已截断）
    pub notes: Option<String>,
    /// 发布时间（RFC3339）
    pub published_at: Option<String>,
    /// 是否为预发布
    pub prerelease: bool,
}

/// 检查更新。
///
/// - `current`：当前应用版本（由命令层传入 `env!("CARGO_PKG_VERSION")`）。
/// - 无更新时返回 `has_update = false`，并带上最新版本号。
pub async fn check(current: &str) -> CmdResult<UpdateInfo> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("LabVPN/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|err| CommandError::new("update", format!("初始化更新客户端失败: {err}")))?;

    let response = client.get(RELEASES_URL).send().await.map_err(|err| {
        CommandError::new(
            "update",
            format!("检查更新失败（网络不可达，或 GitHub 被阻断）: {err}"),
        )
    })?;

    let status = response.status();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err(CommandError::new(
            "update",
            "检查更新失败：GitHub API 请求过于频繁，请稍后再试",
        ));
    }
    if !status.is_success() {
        return Err(CommandError::new(
            "update",
            format!("检查更新失败：GitHub API 返回 HTTP {status}"),
        ));
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|err| CommandError::new("update", format!("解析更新信息失败: {err}")))?;

    let latest = labvpn_core::update::pick_latest(&payload, current);
    let Some(candidate) = latest else {
        // 没有比当前更新的 Release：把远端最新（若有）也告诉前端，便于展示。
        let remote_latest = payload.as_array().and_then(|items| {
            items
                .iter()
                .filter_map(labvpn_core::update::parse_release)
                .max_by(|left, right| left.version.cmp(&right.version))
        });
        return Ok(UpdateInfo {
            current: current.to_string(),
            latest: remote_latest
                .as_ref()
                .map(|item| item.version.to_normalized())
                .unwrap_or_else(|| current.to_string()),
            has_update: false,
            release_url: remote_latest
                .as_ref()
                .map(|item| item.url.clone())
                .unwrap_or_default(),
            release_name: remote_latest.and_then(|item| item.name),
            notes: None,
            published_at: None,
            prerelease: false,
        });
    };

    let notes = candidate.notes.map(|notes| truncate_notes(&notes));

    Ok(UpdateInfo {
        current: current.to_string(),
        latest: candidate.version.to_normalized(),
        has_update: true,
        release_url: candidate.url,
        release_name: candidate.name,
        notes,
        published_at: candidate.published_at,
        prerelease: candidate.prerelease,
    })
}

/// 截断过长的 Release 说明（按字符而非字节，避免切坏中文）。
fn truncate_notes(notes: &str) -> String {
    if notes.chars().count() <= MAX_NOTES_CHARS {
        return notes.to_string();
    }
    let mut truncated: String = notes.chars().take(MAX_NOTES_CHARS).collect();
    truncated.push_str("\n\n…（完整说明见 Release 页面）");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_notes_without_breaking_chars() {
        let short = "短说明";
        assert_eq!(truncate_notes(short), short);

        let long = "更".repeat(MAX_NOTES_CHARS + 10);
        let truncated = truncate_notes(&long);
        assert!(truncated.contains("完整说明见 Release 页面"));
        assert_eq!(
            truncated.chars().filter(|ch| *ch == '更').count(),
            MAX_NOTES_CHARS
        );
    }
}
