//! 用户自定义规则的解析、校验与归一化。

use crate::util;

/// 允许的规则类型。
pub const ALLOWED_TYPES: &[&str] = &[
    "DOMAIN",
    "DOMAIN-SUFFIX",
    "DOMAIN-KEYWORD",
    "IP-CIDR",
    "IP-CIDR6",
    "GEOIP",
    "GEOSITE",
    "PROCESS-NAME",
    "RULE-SET",
    "MATCH",
];

/// 允许的规则选项。
pub const ALLOWED_OPTIONS: &[&str] = &["no-resolve", "src"];

/// 单行自定义规则长度上限。
pub const MAX_RULE_LEN: usize = 512;

/// 解析后的规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRule {
    /// 类型（大写）
    pub kind: String,
    /// 匹配值（MATCH 时为空）
    pub value: String,
    /// 出口策略 / 代理组
    pub target: String,
    /// 选项
    pub options: Vec<String>,
}

impl ParsedRule {
    /// 归一化为 mihomo 规则行。
    pub fn to_rule_string(&self) -> String {
        if self.kind == "MATCH" {
            return format!("MATCH,{}", self.target);
        }
        let mut parts = vec![self.kind.clone(), self.value.clone(), self.target.clone()];
        parts.extend(self.options.iter().cloned());
        parts.join(",")
    }
}

/// 解析一行规则。支持前置 `- `（YAML 列表写法）与行尾注释 `#`。
pub fn parse_rule(line: &str) -> Result<ParsedRule, String> {
    let raw = line.trim();
    if raw.is_empty() {
        return Err("空规则".to_string());
    }
    let body = raw.strip_prefix("- ").unwrap_or(raw).trim();
    // 去掉行内注释（# 之后），但域名里不会出现 #，安全
    let body = body.split('#').next().unwrap_or(body).trim();
    if body.is_empty() {
        return Err("空规则".to_string());
    }
    if body.chars().count() > MAX_RULE_LEN {
        return Err(format!("规则过长（上限 {MAX_RULE_LEN} 字符）"));
    }
    let parts: Vec<&str> = body.split(',').map(str::trim).collect();
    if parts.is_empty() || parts[0].is_empty() {
        return Err("规则缺少类型".to_string());
    }
    let kind = parts[0].to_ascii_uppercase();
    if !ALLOWED_TYPES.contains(&kind.as_str()) {
        return Err(format!("不支持的规则类型：{}", parts[0]));
    }
    if kind == "MATCH" {
        if parts.len() != 2 || parts[1].is_empty() {
            return Err("MATCH 规则格式应为 MATCH,<出口>；且由系统自动补全，请勿手写".to_string());
        }
        return Ok(ParsedRule {
            kind,
            value: String::new(),
            target: parts[1].to_string(),
            options: Vec::new(),
        });
    }
    if parts.len() < 3 {
        return Err("规则格式应为 类型,值,出口[,选项]".to_string());
    }
    let value = parts[1];
    let target = parts[2];
    if value.is_empty() {
        return Err("规则匹配值不能为空".to_string());
    }
    if target.is_empty() {
        return Err("规则出口不能为空".to_string());
    }
    let options: Vec<String> = parts[3..]
        .iter()
        .filter(|option| !option.is_empty())
        .map(|option| option.to_ascii_lowercase())
        .collect();
    for option in &options {
        if !ALLOWED_OPTIONS.contains(&option.as_str()) {
            return Err(format!("不支持的规则选项：{option}"));
        }
    }
    // 值类型校验
    match kind.as_str() {
        "DOMAIN" | "DOMAIN-SUFFIX" => {
            if !util::is_domain(value) {
                return Err(format!("无效的域名：{value}"));
            }
        }
        "DOMAIN-KEYWORD" => {
            if value.len() < 2 {
                return Err("DOMAIN-KEYWORD 至少 2 个字符".to_string());
            }
        }
        "IP-CIDR" => {
            if util::parse_cidr_v4(value).is_none() {
                return Err(format!("无效的 IPv4 CIDR：{value}"));
            }
        }
        "IP-CIDR6" => {
            if util::parse_cidr_v6(value).is_none() {
                return Err(format!("无效的 IPv6 CIDR：{value}"));
            }
        }
        "GEOIP" | "GEOSITE" => {
            if value.trim().is_empty() {
                return Err("GEOIP/GEOSITE 需要国家码或分类".to_string());
            }
        }
        "PROCESS-NAME" => {
            if value.contains(['/', '\\']) {
                return Err("PROCESS-NAME 请填写进程名而不是路径".to_string());
            }
        }
        "RULE-SET" => {}
        _ => {}
    }
    Ok(ParsedRule {
        kind,
        value: value.to_string(),
        target: target.to_string(),
        options,
    })
}

/// 校验一行规则语法（供配置校验使用）。
pub fn validate_syntax(line: &str) -> Result<(), String> {
    parse_rule(line).map(|_| ())
}

/// 把用户输入的多行文本拆成规则行（去空行与注释行）。
pub fn split_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let line = line.strip_prefix("- ").unwrap_or(line).trim();
            line.split('#').next().unwrap_or(line).trim().to_string()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

/// 规范化用户自定义规则：返回 (有效规则, 警告)。
pub fn normalize_rules(lines: &[String], known_targets: &[String]) -> (Vec<String>, Vec<String>) {
    let mut rules = Vec::new();
    let mut warnings = Vec::new();
    for line in lines {
        match parse_rule(line) {
            Ok(parsed) => {
                if parsed.kind == "MATCH" {
                    warnings.push("自定义规则不允许 MATCH（系统会自动补全），已忽略".to_string());
                    continue;
                }
                if !known_targets.iter().any(|target| target == &parsed.target)
                    && parsed.target != "DIRECT"
                    && parsed.target != "REJECT"
                {
                    warnings.push(format!(
                        "自定义规则「{}」的目标「{}」不存在，已忽略",
                        line.trim(),
                        parsed.target
                    ));
                    continue;
                }
                rules.push(parsed.to_rule_string());
            }
            Err(err) => warnings.push(format!("忽略非法规则「{}」：{err}", line.trim())),
        }
    }
    (rules, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_rules() {
        let rule = parse_rule("DOMAIN-SUFFIX,openai.com,🚀 代理").expect("valid");
        assert_eq!(rule.kind, "DOMAIN-SUFFIX");
        assert_eq!(rule.target, "🚀 代理");
        assert_eq!(rule.to_rule_string(), "DOMAIN-SUFFIX,openai.com,🚀 代理");

        let rule = parse_rule("IP-CIDR,192.168.1.0/24,DIRECT,no-resolve").expect("valid");
        assert_eq!(rule.options, vec!["no-resolve"]);
    }

    #[test]
    fn allows_leading_dash_and_comment() {
        let rule = parse_rule("- DOMAIN,example.com,DIRECT # 注释").expect("valid");
        assert_eq!(rule.to_rule_string(), "DOMAIN,example.com,DIRECT");
    }

    #[test]
    fn rejects_invalid_rules() {
        assert!(parse_rule("FOO,bar,baz").is_err());
        assert!(parse_rule("DOMAIN-SUFFIX,not a domain,DIRECT").is_err());
        assert!(parse_rule("IP-CIDR,10.0.0.0/33,DIRECT").is_err());
        assert!(parse_rule("MATCH,🚀 代理").is_ok());
    }

    #[test]
    fn normalizes_against_known_targets() {
        let lines = vec![
            "DOMAIN,git.example.com,🚀 代理".to_string(),
            "DOMAIN,missing.example.com,不存在的组".to_string(),
            "MATCH,DIRECT".to_string(),
        ];
        let known = vec!["🚀 代理".to_string()];
        let (rules, warnings) = normalize_rules(&lines, &known);
        assert_eq!(rules, vec!["DOMAIN,git.example.com,🚀 代理"]);
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn splits_multiline_text() {
        let lines = split_lines("DOMAIN,a.com,DIRECT\n\n# 注释\n- DOMAIN-SUFFIX,b.com,🚀 代理\n");
        assert_eq!(lines.len(), 2);
    }
}
