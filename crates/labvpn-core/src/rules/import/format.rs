//! 提取 Shadowrocket 段落与 Clash 的规则列表，忽略其余字段。

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::net::IpAddr;

use super::{ImportError, MAX_IMPORTED_RULES, MAX_SOURCE_BYTES};

pub(super) struct Input {
    pub rules: Vec<(usize, String)>,
    pub groups: BTreeMap<String, String>,
}

pub(super) fn parse(text: &str) -> Result<Input, ImportError> {
    if text.len() > MAX_SOURCE_BYTES {
        return Err(ImportError::TooLarge);
    }
    let text = text.trim_start_matches('\u{feff}');
    let mut input = Input {
        rules: Vec::new(),
        groups: BTreeMap::new(),
    };
    if is_yaml(text) {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Document {
            Map {
                rules: Option<Vec<String>>,
                payload: Option<Vec<String>>,
            },
            List(Vec<String>),
        }
        let parsed: Document =
            serde_yaml_ng::from_str(text).map_err(|_| ImportError::InvalidFormat)?;
        let items = match parsed {
            Document::Map { rules, payload } => {
                rules.or(payload).ok_or(ImportError::InvalidFormat)?
            }
            Document::List(rules) => rules,
        };
        if items.len() > MAX_IMPORTED_RULES {
            return Err(ImportError::TooLarge);
        }
        input.rules = items
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                data_line(line).map(|value| (i + 1, with_plain_type(value).into_owned()))
            })
            .collect();
        return Ok(input);
    }
    let sections = text.lines().any(|line| section(line.trim()).is_some());
    let mut current = "".to_owned();
    let mut has_rules = !sections;
    for (i, line) in text.lines().enumerate() {
        let Some(line) = data_line(line) else {
            continue;
        };
        if let Some(head) = section(line) {
            current = head.to_ascii_lowercase();
            has_rules |= current == "rule";
            continue;
        }
        if current == "proxy group" && input.groups.len() < 256 {
            if let Some((name, definition)) = line.split_once('=') {
                let fields: Vec<_> = definition.split(',').map(str::trim).collect();
                if fields
                    .first()
                    .is_some_and(|field| field.eq_ignore_ascii_case("select"))
                {
                    let chosen = fields
                        .iter()
                        .skip(1)
                        .find_map(|part| part.strip_prefix("policy-select-name=").map(str::trim))
                        .or_else(|| {
                            fields
                                .get(1)
                                .copied()
                                .filter(|member| !member.contains('='))
                        });
                    if let Some(chosen) = chosen.filter(|choice| !choice.is_empty()) {
                        input
                            .groups
                            .insert(name.trim().to_ascii_lowercase(), chosen.to_owned());
                    }
                }
            }
        } else if !sections || current == "rule" {
            if input.rules.len() >= MAX_IMPORTED_RULES {
                return Err(ImportError::TooLarge);
            }
            input
                .rules
                .push((i + 1, with_plain_type(line).into_owned()));
        }
    }
    if !has_rules {
        return Err(ImportError::InvalidFormat);
    }
    Ok(input)
}

fn is_yaml(text: &str) -> bool {
    if text.lines().any(|line| section(line.trim()).is_some()) {
        return false;
    }
    text.lines()
        .any(|line| line.starts_with("rules:") || line.starts_with("payload:"))
        || text
            .lines()
            .find_map(data_line)
            .is_some_and(|line| line.starts_with("- "))
}

fn section(line: &str) -> Option<&str> {
    let body = line.strip_prefix('[')?.strip_suffix(']')?.trim();
    (!body.is_empty() && !body.contains(['[', ']', ','])).then_some(body)
}

pub(super) fn data_line(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(['#', ';']) || line.starts_with("//") {
        return None;
    }
    let line = line
        .split_once(" #")
        .map_or(line, |(head, _)| head)
        .trim_end();
    Some(line)
}

pub(super) fn with_plain_type(line: &str) -> Cow<'_, str> {
    if line.contains(',') {
        return Cow::Borrowed(line);
    }
    let prefix = match line.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => return Cow::Owned(format!("IP-CIDR,{ip}/32")),
        Ok(IpAddr::V6(ip)) => return Cow::Owned(format!("IP-CIDR6,{ip}/128")),
        _ if crate::util::parse_cidr_v4(line).is_some() => "IP-CIDR",
        _ if crate::util::parse_cidr_v6(line).is_some() => "IP-CIDR6",
        _ if crate::util::is_domain(line.trim_start_matches("+.").trim_start_matches('.')) => {
            "DOMAIN-SUFFIX"
        }
        _ => return Cow::Borrowed(line),
    };
    Cow::Owned(format!("{prefix},{line}"))
}
