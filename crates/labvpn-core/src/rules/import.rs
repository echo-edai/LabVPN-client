//! 分流文件的离线解析与策略转换；不读文件、不发请求。

mod format;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use super::names;
use crate::util;

/// 单个源文件的大小上限。
pub const MAX_SOURCE_BYTES: usize = 3 * 1024 * 1024;
/// 单个档案的规则数上限。
pub const MAX_IMPORTED_RULES: usize = 100_000;
const MAX_SECTIONS: usize = 64;

/// 导入规则可落到的 LabVPN 策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outlet {
    /// 订阅代理组。
    Proxy,
    /// 直连。
    Direct,
    /// 拒绝连接。
    Reject,
    /// 校内策略组。
    Campus,
    /// SSH 转发组。
    Ssh,
}

impl Outlet {
    /// mihomo 内置出口或 LabVPN 组名。
    pub fn target(self) -> &'static str {
        match self {
            Self::Proxy => names::GROUP_PROXY,
            Self::Direct => names::DIRECT,
            Self::Reject => names::REJECT,
            Self::Campus => names::GROUP_CAMPUS,
            Self::Ssh => names::GROUP_SSH,
        }
    }
}

/// 一个有序规则项；外部集合转换为同一策略的本地快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// 该条或该集合的策略。
    pub outlet: Outlet,
    /// 规范化后不含策略字段的匹配条件。
    pub matchers: Vec<String>,
}

/// 可持久化档案。只保存已转换的匹配条件，不保存源配置或 URL。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// 文件显示名，不包含路径。
    pub name: String,
    /// 按源顺序排列的规则项。
    pub entries: Vec<Entry>,
    /// 源文件指定的兜底策略。
    pub fallback: Option<Outlet>,
}

impl Profile {
    /// 逐项拒绝不安全或损坏的持久化规则，不信任 IPC/JSON 输入。
    pub fn valid(&self) -> bool {
        if self.name.is_empty() || self.name.len() > 240 || self.name.chars().any(char::is_control)
        {
            return false;
        }
        if self.entries.is_empty() && self.fallback.is_none() {
            return false;
        }
        let mut count = 0;
        let mut weight = 0;
        for entry in &self.entries {
            if entry.matchers.is_empty() {
                return false;
            }
            for condition in &entry.matchers {
                count += 1;
                // 字符串和结构在 JSON/YAML 中的额外开销也受限。
                weight += condition.len() + 16;
                if count > MAX_IMPORTED_RULES
                    || weight > MAX_SOURCE_BYTES
                    || canonical_condition(condition).as_deref() != Ok(condition.as_str())
                {
                    return false;
                }
            }
        }
        true
    }
}

/// 预览中可本地化的问题分类，不回显原始行或敏感 URL。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Issue {
    /// 引擎不支持的匹配类型或选项。
    UnsupportedRule,
    /// 格式或匹配值不合法。
    InvalidRule,
    /// 没有可以替代的策略。
    UnknownOutlet,
    /// 未取得外部集合。
    RemoteSkipped,
    /// 集合没有兼容记录。
    EmptyRuleSet,
    /// 出现在源文件首条兜底之后。
    AfterFallback,
}

/// 按原因聚合的跳过条目；行号属于主文件。
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    /// 跳过原因。
    pub reason: Issue,
    /// 该原因的出现次数。
    pub count: usize,
    /// 最多八个源规则位置。
    pub lines: Vec<usize>,
}

/// 源策略与实际出口的对照。
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mapping {
    /// 源策略名。
    pub source: String,
    /// 目标策略，无法映射时为空。
    pub outlet: Option<Outlet>,
}

/// 导入预览与可保存的结构化结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// 有可导入内容时返回档案。
    pub profile: Option<Profile>,
    /// 兼容匹配条件数，不含兜底。
    pub rule_count: usize,
    /// 已转换的外链集合数。
    pub set_count: usize,
    /// 源策略的映射结果。
    pub mappings: Vec<Mapping>,
    /// 跳过项汇总。
    pub skipped: Vec<Skipped>,
    /// 最多八条规则样例。
    pub samples: Vec<String>,
}

/// 输入文件无法安全处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    /// 超出输入或转换结果的限额。
    TooLarge,
    /// 无可识别的规则容器。
    InvalidFormat,
}

#[derive(Debug)]
enum Rule<'a> {
    Match {
        condition: &'a [&'a str],
        policy: Option<&'a str>,
        options: &'a [&'a str],
    },
    Fallback(&'a str),
}

/// 公开集合引用。网络层仍须独立检查 URL 白名单。
pub fn referenced_sets(text: &str) -> Result<Vec<String>, ImportError> {
    let file = format::parse(text)?;
    let mut sets = BTreeSet::new();
    for (_, line) in &file.rules {
        let parts = split(line);
        let (condition, policy) = match classify(&parts) {
            Ok(Rule::Fallback(_)) => break,
            Ok(Rule::Match {
                condition,
                policy: Some(policy),
                options: [],
            }) => (condition, policy),
            _ => continue,
        };
        if is_set(condition[0]) && parse_outlet(policy, &file.groups).is_some() {
            sets.insert(condition[1].to_owned());
            if sets.len() > MAX_SECTIONS {
                return Err(ImportError::TooLarge);
            }
        }
    }
    Ok(sets.into_iter().collect())
}

/// 转换文件。`sets` 是已经取得的外部公开文本；未取得的引用只报告，不生成网络配置。
pub fn preview(
    name: &str,
    text: &str,
    default_outlet: Outlet,
    sets: &BTreeMap<String, String>,
) -> Result<Preview, ImportError> {
    let file = format::parse(text)?;
    let mut profile = Profile {
        name: clean_label(name).unwrap_or_else(|| "rules".into()),
        entries: Vec::new(),
        fallback: None,
    };
    let mut result = Preview {
        profile: None,
        rule_count: 0,
        set_count: 0,
        mappings: Vec::new(),
        skipped: Vec::new(),
        samples: Vec::new(),
    };
    let mut skipped = BTreeMap::new();
    let mut mapped = BTreeMap::new();
    let mut weight = 0;
    let mut reached_fallback = false;
    for (number, line) in file.rules {
        if reached_fallback {
            note(&mut skipped, Issue::AfterFallback, number);
            continue;
        }
        let parts = split(&line);
        let rule = match classify(&parts) {
            Ok(rule) => rule,
            Err(reason) => {
                note(&mut skipped, reason, number);
                continue;
            }
        };
        let policy = match rule {
            Rule::Fallback(policy) => {
                reached_fallback = true;
                Some(policy)
            }
            Rule::Match { policy, .. } => policy,
        };
        let outlet = match policy {
            None => Some(default_outlet),
            Some(policy) => {
                let outlet = parse_outlet(policy, &file.groups);
                if let Some(key) = clean_label(policy) {
                    if mapped.len() < 128 {
                        mapped.insert(key, outlet);
                    }
                }
                outlet
            }
        };
        let Some(outlet) = outlet else {
            note(&mut skipped, Issue::UnknownOutlet, number);
            continue;
        };
        let Rule::Match {
            condition, options, ..
        } = rule
        else {
            profile.fallback = Some(outlet);
            continue;
        };
        let matchers = if is_set(condition[0]) {
            if !options.is_empty() {
                note(&mut skipped, Issue::UnsupportedRule, number);
                continue;
            }
            let Some(source) = sets.get(condition[1]) else {
                note(&mut skipped, Issue::RemoteSkipped, number);
                continue;
            };
            let matches = convert_set(source, &mut skipped, number);
            if matches.is_empty() {
                note(&mut skipped, Issue::EmptyRuleSet, number);
                continue;
            }
            result.set_count += 1;
            matches
        } else {
            match normalize(condition[0], condition[1], options) {
                Ok(condition) => vec![condition],
                Err(reason) => {
                    note(&mut skipped, reason, number);
                    continue;
                }
            }
        };
        result.rule_count += matchers.len();
        weight += matchers
            .iter()
            .map(|condition| condition.len() + 16)
            .sum::<usize>();
        if result.rule_count > MAX_IMPORTED_RULES || weight > MAX_SOURCE_BYTES {
            return Err(ImportError::TooLarge);
        }
        for condition in matchers.iter().take(8 - result.samples.len()) {
            result.samples.push(core_rule(condition, outlet.target()));
        }
        profile.entries.push(Entry { outlet, matchers });
    }
    if !profile.entries.is_empty() || profile.fallback.is_some() {
        result.profile = Some(profile);
    }
    result.skipped = skipped.into_values().collect();
    result.mappings = mapped
        .into_iter()
        .map(|(source, outlet)| Mapping { source, outlet })
        .collect();
    Ok(result)
}

fn clean_label(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()
        && !text.contains(['/', '\\', ':', '=', '?', '#'])
        && !text.chars().any(char::is_control))
    .then(|| text.chars().take(60).collect())
}

fn note(skipped: &mut BTreeMap<Issue, Skipped>, reason: Issue, number: usize) {
    let row = skipped.entry(reason).or_insert_with(|| Skipped {
        reason,
        count: 0,
        lines: Vec::new(),
    });
    row.count += 1;
    if row.lines.len() < 8 && !row.lines.contains(&number) {
        row.lines.push(number);
    }
}

fn is_set(kind: &str) -> bool {
    kind.eq_ignore_ascii_case("RULE-SET") || kind.eq_ignore_ascii_case("DOMAIN-SET")
}

fn split(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}

fn parse_outlet(policy: &str, groups: &BTreeMap<String, String>) -> Option<Outlet> {
    let mut label = policy;
    // 上限同时覆盖循环引用和无意义的超长策略链。
    for _ in 0..12 {
        match label.to_ascii_uppercase().as_str() {
            "PROXY" => return Some(Outlet::Proxy),
            "DIRECT" => return Some(Outlet::Direct),
            "REJECT" | "REJECT-DROP" | "REJECT-NO-DROP" => return Some(Outlet::Reject),
            _ if label == names::GROUP_PROXY => return Some(Outlet::Proxy),
            _ if label == names::GROUP_CAMPUS => return Some(Outlet::Campus),
            _ if label == names::GROUP_SSH => return Some(Outlet::Ssh),
            _ => label = groups.get(&label.to_ascii_lowercase())?,
        }
    }
    None
}

fn classify<'a>(parts: &'a [&'a str]) -> Result<Rule<'a>, Issue> {
    match parts {
        [kind, policy]
            if kind.eq_ignore_ascii_case("FINAL") || kind.eq_ignore_ascii_case("MATCH") =>
        {
            if policy.is_empty() {
                Err(Issue::InvalidRule)
            } else {
                Ok(Rule::Fallback(policy))
            }
        }
        [value] => {
            // 裸域名/网段在格式层补上类型后才进入这里。
            if value.is_empty() {
                Err(Issue::InvalidRule)
            } else {
                Err(Issue::UnsupportedRule)
            }
        }
        [_, value, rest @ ..] if !value.is_empty() => {
            let (policy, options) = match rest {
                [] => (None, rest),
                [option, ..] if option.eq_ignore_ascii_case("no-resolve") => (None, rest),
                [policy, options @ ..] if !policy.is_empty() => (Some(*policy), options),
                _ => return Err(Issue::InvalidRule),
            };
            Ok(Rule::Match {
                condition: &parts[..2],
                policy,
                options,
            })
        }
        _ => Err(Issue::InvalidRule),
    }
}

fn convert_set(source: &str, skipped: &mut BTreeMap<Issue, Skipped>, number: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let is_qx = source.lines().any(|line| line.starts_with("HOST,"));
    for line in source
        .trim_start_matches('\u{feff}')
        .lines()
        .filter_map(format::data_line)
    {
        let line = format::with_plain_type(line);
        let parts = split(&line);
        let converted = match parts.as_slice() {
            [kind, value, options @ ..] if !is_set(kind) => {
                let options = if is_qx
                    && options
                        .first()
                        .is_some_and(|s| !s.eq_ignore_ascii_case("no-resolve"))
                {
                    // Quantumult X 的标签属于源规则集，出口取上层 RULE-SET。
                    &options[1..]
                } else {
                    options
                };
                normalize(kind, value, options)
            }
            _ => Err(Issue::UnsupportedRule),
        };
        match converted {
            Ok(condition) if seen.insert(condition.clone()) => result.push(condition),
            Ok(_) => {}
            Err(reason) => note(skipped, reason, number),
        }
    }
    result
}

fn canonical_condition(text: &str) -> Result<String, Issue> {
    match split(text).as_slice() {
        [kind, value, options @ ..] => normalize(kind, value, options),
        _ => Err(Issue::InvalidRule),
    }
}

fn normalize(kind: &str, value: &str, options: &[&str]) -> Result<String, Issue> {
    if value.is_empty() || value.len() > 253 || value.chars().any(char::is_control) {
        return Err(Issue::InvalidRule);
    }
    let input = kind.to_ascii_uppercase();
    let kind = match input.as_str() {
        "HOST" => "DOMAIN",
        "HOST-SUFFIX" => "DOMAIN-SUFFIX",
        "HOST-KEYWORD" => "DOMAIN-KEYWORD",
        "HOST-WILDCARD" => "DOMAIN-WILDCARD",
        "IP6-CIDR" => "IP-CIDR6",
        "IP-CIDR" if value.contains(':') => "IP-CIDR6",
        _ => &input,
    };
    let value = match kind {
        "DOMAIN" | "DOMAIN-SUFFIX" => {
            let value = if kind == "DOMAIN-SUFFIX" {
                value
                    .trim_start_matches('.')
                    .trim_start_matches("*.")
                    .trim_start_matches("+.")
            } else {
                value
            };
            if value.contains('*') || !(util::is_domain(value) || label(value)) {
                return Err(Issue::InvalidRule);
            }
            util::normalize_domain(value)
        }
        "DOMAIN-KEYWORD" | "DOMAIN-WILDCARD" => {
            if !value.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || b".-_".contains(&b)
                    || kind == "DOMAIN-WILDCARD" && b"*?".contains(&b)
            }) {
                return Err(Issue::InvalidRule);
            }
            util::normalize_domain(value)
        }
        "IP-CIDR" => util::parse_cidr_v4(value)
            .map(|(ip, bits)| format!("{ip}/{bits}"))
            .ok_or(Issue::InvalidRule)?,
        "IP-CIDR6" => util::parse_cidr_v6(value)
            .map(|(ip, bits)| format!("{ip}/{bits}"))
            .ok_or(Issue::InvalidRule)?,
        "GEOIP" | "GEOSITE" => {
            if !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"@!_-".contains(&b))
            {
                return Err(Issue::InvalidRule);
            }
            if kind == "GEOIP" {
                value.to_ascii_uppercase()
            } else {
                value.to_ascii_lowercase()
            }
        }
        "PROCESS-NAME" => {
            if value.contains(['/', '\\', '#', '(', ')', ':', '=']) {
                return Err(Issue::InvalidRule);
            }
            value.to_owned()
        }
        _ => return Err(Issue::UnsupportedRule),
    };
    let ip_kind = matches!(kind, "IP-CIDR" | "IP-CIDR6" | "GEOIP");
    let option = match options {
        [] => "",
        [opt] if ip_kind && opt.eq_ignore_ascii_case("no-resolve") => ",no-resolve",
        _ => return Err(Issue::UnsupportedRule),
    };
    Ok(format!("{kind},{value}{option}"))
}

fn label(value: &str) -> bool {
    let value = value.trim_end_matches('.');
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
}

/// 给已规范的匹配条件插入目标，保留尾部选项位置。
pub(super) fn core_rule(condition: &str, target: &str) -> String {
    match condition.strip_suffix(",no-resolve") {
        Some(base) => format!("{base},{target},no-resolve"),
        None => format!("{condition},{target}"),
    }
}
