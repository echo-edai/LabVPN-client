//! 更新检查的纯逻辑：版本号解析、比较与 GitHub Release 列表的挑选。
//!
//! 网络请求在应用层 [`services::update`]，这里只做可单测的纯计算。

use serde::Deserialize;
use serde_json::Value;

/// 语义化版本（只比较 `major.minor.patch`，忽略 `-pre`/`+build` 后缀）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub struct Version {
    /// 主版本号
    pub major: u64,
    /// 次版本号
    pub minor: u64,
    /// 修订号
    pub patch: u64,
}

impl Version {
    /// 解析版本号。接受 `1.2.3`、`v1.2.3`、`V1.2.3`、`1.2`、`1.2.3-pre.1` 等写法。
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        let stripped = trimmed
            .strip_prefix('v')
            .or_else(|| trimmed.strip_prefix('V'))
            .unwrap_or(trimmed);
        let core = stripped.split(['-', '+']).next().unwrap_or(stripped).trim();
        if core.is_empty() {
            return None;
        }
        let mut numbers = [0u64; 3];
        for (index, part) in core.split('.').enumerate() {
            if index >= numbers.len() || part.is_empty() {
                return None;
            }
            numbers[index] = part.parse::<u64>().ok()?;
        }
        Some(Self {
            major: numbers[0],
            minor: numbers[1],
            patch: numbers[2],
        })
    }

    /// 规范化为 `x.y.z` 字符串。
    pub fn to_normalized(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// `self` 是否比 `other` 新。
    pub fn is_newer_than(self, other: Self) -> bool {
        self > other
    }
}

/// 一个可供升级的 Release 候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseCandidate {
    /// 原始 tag，如 `v0.1.10`
    pub tag: String,
    /// 解析后的版本
    pub version: Version,
    /// Release 标题
    pub name: Option<String>,
    /// Release 页面地址
    pub url: String,
    /// Release 说明（Markdown 原文）
    pub notes: Option<String>,
    /// 发布时间（RFC3339）
    pub published_at: Option<String>,
    /// 是否为预发布
    pub prerelease: bool,
}

/// 解析单个 GitHub release JSON；draft 或版本号不可解析时返回 `None`。
pub fn parse_release(value: &Value) -> Option<ReleaseCandidate> {
    if value.get("draft").and_then(Value::as_bool).unwrap_or(false) {
        return None;
    }
    let tag = value.get("tag_name")?.as_str()?.trim().to_string();
    let version = Version::parse(&tag)?;
    Some(ReleaseCandidate {
        tag,
        version,
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string),
        url: value
            .get("html_url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        notes: value
            .get("body")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|body| !body.is_empty())
            .map(str::to_string),
        published_at: value
            .get("published_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        prerelease: value
            .get("prerelease")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// 从 GitHub `/releases` 返回的 JSON 数组里挑选**比当前版本新**的最新 Release。
///
/// 忽略 draft；预发布版本（本项目的 Release 都是 pre-release）也会参与比较。
pub fn pick_latest(releases: &Value, current: &str) -> Option<ReleaseCandidate> {
    let current = Version::parse(current)?;
    releases
        .as_array()?
        .iter()
        .filter_map(parse_release)
        .filter(|candidate| candidate.version.is_newer_than(current))
        .max_by(|left, right| left.version.cmp(&right.version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_common_version_formats() {
        assert_eq!(
            Version::parse("v1.2.3"),
            Some(Version {
                major: 1,
                minor: 2,
                patch: 3
            })
        );
        assert_eq!(
            Version::parse("1.10.0").map(Version::to_normalized),
            Some("1.10.0".into())
        );
        assert_eq!(
            Version::parse("0.1").map(Version::to_normalized),
            Some("0.1.0".into())
        );
        assert_eq!(
            Version::parse("v0.1.10-pre.1").map(Version::to_normalized),
            Some("0.1.10".into())
        );
        assert_eq!(Version::parse(""), None);
        assert_eq!(Version::parse("v"), None);
        assert_eq!(Version::parse("v1.x.0"), None);
        assert_eq!(Version::parse("1.2.3.4"), None);
    }

    #[test]
    fn compares_versions_numerically() {
        let newer = Version::parse("0.1.10").expect("parse");
        let older = Version::parse("0.1.9").expect("parse");
        assert!(newer.is_newer_than(older));
        assert!(!older.is_newer_than(newer));
        assert!(!newer.is_newer_than(newer));
    }

    #[test]
    fn pick_latest_skips_drafts_and_old_versions() {
        let releases = json!([
            {
                "tag_name": "v0.1.12",
                "draft": true,
                "prerelease": true,
                "html_url": "https://example.com/draft"
            },
            {
                "tag_name": "v0.1.11",
                "draft": false,
                "prerelease": true,
                "name": "LabVPN v0.1.11",
                "body": "notes 0.1.11",
                "html_url": "https://example.com/0.1.11",
                "published_at": "2026-10-24T00:00:00Z"
            },
            {
                "tag_name": "v0.1.10",
                "draft": false,
                "prerelease": true,
                "html_url": "https://example.com/0.1.10"
            },
            {
                "tag_name": "nightly",
                "draft": false,
                "prerelease": false
            }
        ]);
        let latest = pick_latest(&releases, "0.1.10").expect("latest");
        assert_eq!(latest.tag, "v0.1.11");
        assert_eq!(latest.version.to_normalized(), "0.1.11");
        assert_eq!(latest.url, "https://example.com/0.1.11");
        assert_eq!(latest.notes.as_deref(), Some("notes 0.1.11"));
        assert!(latest.prerelease);

        // 已是最新（或更新）时不返回候选。
        assert!(pick_latest(&releases, "0.1.11").is_none());
        assert!(pick_latest(&releases, "0.2.0").is_none());
    }

    #[test]
    fn pick_latest_handles_payload_errors() {
        assert!(pick_latest(&json!({"message": "rate limited"}), "0.1.0").is_none());
        assert!(pick_latest(&json!([]), "0.1.0").is_none());
        assert!(pick_latest(&json!([]), "not-a-version").is_none());
    }
}
