//! 校内 VPN（fdu-connect）日志解析与健康分析（纯逻辑）。
//!
//! 目标：把 `app.log` 中 `[vpn]` 来源的原始日志（fdu-connect stdout + LabVPN 结构化事件）
//! 解析成可统计的事件，回答「隧道是不是断了、断多久、是否需要重新登录」这类问题。
//!
//! 背景：fdu-connect 只在启动时完成一次 TWFID 授权，**没有**会话中途自动重新登录逻辑；
//! 当 RVPN 数据面被服务端/NAT 丢弃时，只能靠 KeepAlive 失败与 SOCKS5 建连超时体现出来。
//! 因此分析结果重点输出「KeepAlive 失败窗口」与「窗口内校内拨号失败次数」。

use std::collections::BTreeMap;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// 应用日志时间格式。
const TS_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
/// LabVPN 结构化事件前缀（由 `services/vpn.rs` 写入）。
pub const EVENT_PREFIX: &str = "[VPN事件]";

/// VPN 日志事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VpnEventKind {
    /// 会话开始（启动 fdu-connect）
    SessionStart,
    /// 登录/授权（TWFID）
    Login,
    /// 会话结束（停止 VPN / 进程退出）
    SessionEnd,
    /// 会话建立（拿到 Server session ID）
    SessionEstablished,
    /// KeepAlive 成功
    KeepAliveOk,
    /// KeepAlive 失败（隧道内 DNS 不通）
    KeepAliveFail,
    /// RVPN 数据连接收发异常（fdu-connect 正在重新握手）
    TunnelReset,
    /// 校内 TCP 建连失败（context deadline / reset 等）
    DialError,
    /// LabVPN 结构化事件（`[VPN事件] ...`）
    Structured,
}

/// 单条解析后的事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnLogEvent {
    /// 时间戳（`YYYY-MM-DD HH:MM:SS`）
    pub ts: String,
    /// 事件类型
    pub kind: VpnEventKind,
    /// 原始信息（已去掉外层日志前缀）
    pub detail: String,
}

/// 一次「隧道中断」窗口（连续 KeepAlive 失败）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnFailureWindow {
    /// 开始时间
    pub start: String,
    /// 结束时间（恢复或会话结束）
    pub end: String,
    /// 持续秒数
    pub duration_secs: u64,
    /// 窗口内 KeepAlive 失败次数
    pub keepalive_fail: u32,
    /// 窗口内校内 TCP 拨号失败次数
    pub dial_errors: u32,
}

/// VPN 日志分析结果。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VpnLogAnalysis {
    /// 解析到的 `[vpn]` 日志行数
    pub total_lines: usize,
    /// VPN 会话数（Start FDU Connect）
    pub sessions: u32,
    /// KeepAlive 成功次数
    pub keepalive_ok: u32,
    /// KeepAlive 失败次数
    pub keepalive_fail: u32,
    /// KeepAlive 成功率（0–1）
    pub success_rate: f32,
    /// TWFID 授权/更新次数（每次登录一次，会话中途不会出现）
    pub reauth_events: u32,
    /// 校内 TCP 重试次数
    pub socks_retries: u32,
    /// 校内 TCP 拨号失败次数
    pub dial_errors: u32,
    /// 连接被重置次数
    pub connection_resets: u32,
    /// RVPN 数据连接异常（收发重连）次数
    pub tunnel_resets: u32,
    /// 隧道中断窗口
    pub failure_windows: Vec<VpnFailureWindow>,
    /// 主要错误类型（类型 -> 次数，按次数降序）
    pub top_errors: Vec<(String, u32)>,
    /// 结论/建议
    pub findings: Vec<String>,
}

struct Envelope {
    ts: String,
    source: String,
    message: String,
}

/// 解析一行应用日志：`[ts] [level] [source] message`。
fn parse_envelope(line: &str) -> Option<Envelope> {
    let rest = line.trim_start().strip_prefix('[')?;
    let (ts, rest) = rest.split_once("] [")?;
    let (_level, rest) = rest.split_once("] [")?;
    let (source, message) = rest.split_once("] ")?;
    Some(Envelope {
        ts: ts.to_string(),
        source: source.to_string(),
        message: message.to_string(),
    })
}

fn parse_ts(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, TS_FORMAT).ok()
}

fn elapsed_secs(start: &str, end: &str) -> u64 {
    let Some(start) = parse_ts(start) else {
        return 0;
    };
    let Some(end) = parse_ts(end) else {
        return 0;
    };
    (end - start).num_seconds().max(0) as u64
}

/// 去掉 fdu-connect 行内自带的时间前缀（`2026/10/27 10:16:43 `）。
fn strip_inner_ts(message: &str) -> &str {
    let mut parts = message.splitn(3, ' ');
    let (Some(first), Some(second)) = (parts.next(), parts.next()) else {
        return message;
    };
    let date_like = first.len() >= 8 && first.contains('/');
    let time_like = second.matches(':').count() == 2;
    if date_like && time_like {
        return parts.next().unwrap_or("");
    }
    message
}

/// 是否属于「KeepAlive 失败」行。
fn is_keepalive_fail(message: &str) -> bool {
    message.starts_with("KeepAlive:") && !message.starts_with("KeepAlive: OK")
}

/// 是否为校内 TCP 拨号失败。
fn is_dial_error(message: &str) -> bool {
    message.contains("attempt") && message.contains("failed")
        || message.contains("[E]:")
        || message.contains("context deadline exceeded")
        || message.contains("connection reset by peer")
        || message.contains("broken pipe")
}

/// 归一化错误类型（用于 top_errors 统计）。
fn normalize_error(message: &str) -> String {
    for marker in [
        "context deadline exceeded",
        "connection reset by peer",
        "i/o timeout",
        "broken pipe",
        "network is unreachable",
        "connection refused",
        "no route to host",
    ] {
        if message.contains(marker) {
            return marker.to_string();
        }
    }
    let detail = message
        .split_once("error:")
        .map(|(_, tail)| tail.trim())
        .unwrap_or(message)
        .trim();
    detail.chars().take(48).collect()
}

/// 把一条 `[vpn]` 消息解析为事件类型。
fn classify(message: &str) -> Option<VpnEventKind> {
    if message.starts_with("Start FDU Connect") {
        return Some(VpnEventKind::SessionStart);
    }
    if message.contains("Server session ID:") {
        return Some(VpnEventKind::SessionEstablished);
    }
    if message.contains("校内 VPN 已停止")
        || message.contains("会话结束")
        || message.contains("fdu-connect 进程退出")
        || message.contains("fdu-connect 退出")
    {
        return Some(VpnEventKind::SessionEnd);
    }
    if message.contains("Update TWFID") || message.contains("TWFID has been authorized") {
        return Some(VpnEventKind::Login);
    }
    if message.starts_with("KeepAlive: OK") {
        return Some(VpnEventKind::KeepAliveOk);
    }
    if is_keepalive_fail(message) {
        return Some(VpnEventKind::KeepAliveFail);
    }
    if message.starts_with("Error occurred while receiving")
        || message.starts_with("Error occurred while sending")
    {
        return Some(VpnEventKind::TunnelReset);
    }
    if message.starts_with(EVENT_PREFIX) {
        return Some(VpnEventKind::Structured);
    }
    if is_dial_error(message) {
        return Some(VpnEventKind::DialError);
    }
    None
}

/// 解析日志文本中的全部 VPN 事件。
pub fn parse_events(text: &str) -> Vec<VpnLogEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let Some(envelope) = parse_envelope(line) else {
            continue;
        };
        if envelope.source != "vpn" {
            continue;
        }
        let detail = strip_inner_ts(&envelope.message);
        let Some(kind) = classify(detail) else {
            continue;
        };
        events.push(VpnLogEvent {
            ts: envelope.ts,
            kind,
            detail: detail.to_string(),
        });
    }
    events
}

/// 截取最近一次 `Start FDU Connect` 之后的日志片段（用于「分析最近一次连接」）。
pub fn latest_session_slice(text: &str) -> &str {
    let mut offset = 0usize;
    let mut latest = 0usize;
    for line in text.split_inclusive('\n') {
        if line.contains("Start FDU Connect") {
            latest = offset;
        }
        offset += line.len();
    }
    &text[latest..]
}

/// 分析日志文本，得到 KeepAlive 健康度、隧道中断窗口与结论。
pub fn analyze_log(text: &str) -> VpnLogAnalysis {
    let events = parse_events(text);
    let mut analysis = VpnLogAnalysis {
        total_lines: events.len(),
        ..Default::default()
    };
    let mut error_types: BTreeMap<String, u32> = BTreeMap::new();
    let mut open: Option<(String, u32, u32)> = None; // (start, fail, dial)
    let close_window =
        |analysis: &mut VpnLogAnalysis, open: &mut Option<(String, u32, u32)>, end: &str| {
            let Some((start, fails, dials)) = open.take() else {
                return;
            };
            if fails == 0 {
                return;
            }
            analysis.failure_windows.push(VpnFailureWindow {
                duration_secs: elapsed_secs(&start, end),
                start,
                end: end.to_string(),
                keepalive_fail: fails,
                dial_errors: dials,
            });
        };

    for event in &events {
        match event.kind {
            VpnEventKind::SessionStart => {
                close_window(&mut analysis, &mut open, &event.ts);
                analysis.sessions += 1;
            }
            VpnEventKind::SessionEnd => {
                close_window(&mut analysis, &mut open, &event.ts);
            }
            VpnEventKind::Login => {
                if event.detail.contains("Update TWFID") {
                    analysis.reauth_events += 1;
                }
            }
            VpnEventKind::KeepAliveOk => {
                analysis.keepalive_ok += 1;
                close_window(&mut analysis, &mut open, &event.ts);
            }
            VpnEventKind::KeepAliveFail => {
                analysis.keepalive_fail += 1;
                match open.as_mut() {
                    Some((_, fails, _)) => *fails += 1,
                    None => open = Some((event.ts.clone(), 1, 0)),
                }
            }
            VpnEventKind::DialError => {
                analysis.dial_errors += 1;
                if event.detail.contains("connection reset by peer") {
                    analysis.connection_resets += 1;
                }
                let kind = normalize_error(&event.detail);
                *error_types.entry(kind).or_insert(0) += 1;
                if let Some((_, _, dials)) = open.as_mut() {
                    *dials += 1;
                }
            }
            VpnEventKind::TunnelReset => {
                analysis.tunnel_resets += 1;
            }
            VpnEventKind::SessionEstablished | VpnEventKind::Structured => {}
        }
        if event.detail.contains("attempt") && event.detail.contains("retrying") {
            analysis.socks_retries += 1;
        }
    }
    if let Some((start, _, _)) = open.as_ref() {
        let end = events
            .last()
            .map(|event| event.ts.clone())
            .unwrap_or_else(|| start.clone());
        close_window(&mut analysis, &mut open, &end);
    }

    let total_keepalive = analysis.keepalive_ok + analysis.keepalive_fail;
    analysis.success_rate = if total_keepalive == 0 {
        0.0
    } else {
        analysis.keepalive_ok as f32 / total_keepalive as f32
    };
    let mut top: Vec<(String, u32)> = error_types.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top.truncate(6);
    analysis.top_errors = top;
    analysis.findings = build_findings(&analysis);
    analysis
}

/// 生成中文结论。
fn build_findings(analysis: &VpnLogAnalysis) -> Vec<String> {
    let mut findings = Vec::new();
    findings.push(format!(
        "共 {} 个 VPN 会话；KeepAlive 成功 {} 次、失败 {} 次（成功率 {:.1}%）",
        analysis.sessions,
        analysis.keepalive_ok,
        analysis.keepalive_fail,
        analysis.success_rate * 100.0
    ));
    if let Some(longest) = analysis
        .failure_windows
        .iter()
        .max_by_key(|window| window.duration_secs)
    {
        findings.push(format!(
            "检测到 {} 次隧道中断；最长一次 {} → {}（{} 秒），期间校内 TCP 拨号失败 {} 次",
            analysis.failure_windows.len(),
            longest.start,
            longest.end,
            longest.duration_secs,
            longest.dial_errors
        ));
    } else {
        findings.push("未检测到 KeepAlive 连续失败窗口（隧道数据面健康）".to_string());
    }
    if analysis.sessions > 0 && analysis.reauth_events <= analysis.sessions {
        findings.push(
            "会话中途没有出现 TWFID 重新授权事件：fdu-connect 只在启动时登录一次，\
             隧道被服务端/NAT 丢弃后不会自动重新登录，只能等待恢复或重启校内 VPN"
                .to_string(),
        );
    }
    if analysis.connection_resets > 0 || analysis.tunnel_resets > 0 {
        findings.push(format!(
            "连接被重置 {} 次、RVPN 数据连接重新握手 {} 次：说明隧道数据面出现过丢包/中断，\
             并非单纯认证失败",
            analysis.connection_resets, analysis.tunnel_resets
        ));
    }
    if !analysis.failure_windows.is_empty() {
        findings.push(
            "建议：检测到 KeepAlive 连续失败 3 次以上时自动重启 fdu-connect（等效于重新授权），\
             避免长时间隧道中断后只能手动重连"
                .to_string(),
        );
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> &'static str {
        r#"
[2026-10-27 10:16:43] [info] [vpn] 2026/10/27 10:16:43 Start FDU Connect v0.9.0
[2026-10-27 10:16:47] [info] [vpn] 2026/10/27 10:16:47 VPN server version: M7.6.8R2
[2026-10-27 10:16:47] [info] [vpn] 2026/10/27 10:16:47 TWFID: abcdef0123456789
[2026-10-27 10:16:47] [info] [vpn] 2026/10/27 10:16:47 Update TWFID: abcdef0123456789
[2026-10-27 10:16:47] [info] [vpn] 2026/10/27 10:16:47 TWFID has been authorized
[2026-10-27 10:16:47] [info] [vpn] 2026/10/27 10:16:47 Server session ID: session-1
[2026-10-27 10:16:47] [info] [vpn] 2026/10/27 10:16:47 KeepAlive: OK
[2026-10-27 10:17:47] [info] [vpn] 2026/10/27 10:17:47 KeepAlive: OK
[2026-10-27 10:18:47] [info] [vpn] 2026/10/27 10:18:47 KeepAlive: lookup www.baidu.com on 223.5.5.5:53: read udp: i/o timeout
[2026-10-27 10:19:47] [info] [vpn] 2026/10/27 10:19:47 KeepAlive: lookup www.baidu.com on 223.5.5.5:53: read udp: i/o timeout
[2026-10-27 10:20:47] [info] [vpn] 2026/10/27 10:20:47 KeepAlive: lookup www.baidu.com on 223.5.5.5:53: read udp: i/o timeout
[2026-10-27 10:20:50] [info] [vpn] 2026/10/27 10:20:50 SOCKS5: tcp -> 10.134.142.226:22 attempt 1 failed (context deadline exceeded), retrying
[2026-10-27 10:20:54] [info] [vpn] 2026/10/27 10:20:54 SOCKS5: tcp -> 10.134.142.226:22 attempt 2 failed (context deadline exceeded), retrying
[2026-10-27 10:21:47] [info] [vpn] 2026/10/27 10:21:47 KeepAlive: OK
[2026-10-27 10:21:47] [warn] [vpn] [VPN事件] 隧道恢复 中断时长=180s keepalive_fail=3
"#
    }

    #[test]
    fn parses_and_classifies_events() {
        let events = parse_events(fixture());
        assert!(events
            .iter()
            .any(|event| event.kind == VpnEventKind::SessionStart));
        assert!(events.iter().any(|event| event.kind == VpnEventKind::Login));
        assert!(events
            .iter()
            .any(|event| event.kind == VpnEventKind::SessionEstablished));
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == VpnEventKind::KeepAliveFail)
                .count(),
            3
        );
        assert!(events
            .iter()
            .any(|event| event.kind == VpnEventKind::Structured));
    }

    #[test]
    fn analyzes_failure_window_and_findings() {
        let analysis = analyze_log(fixture());
        assert_eq!(analysis.sessions, 1);
        assert_eq!(analysis.keepalive_ok, 3);
        assert_eq!(analysis.keepalive_fail, 3);
        assert_eq!(analysis.reauth_events, 1);
        assert_eq!(analysis.dial_errors, 2);
        assert_eq!(analysis.failure_windows.len(), 1);
        let window = &analysis.failure_windows[0];
        assert_eq!(window.keepalive_fail, 3);
        assert_eq!(window.dial_errors, 2);
        assert_eq!(window.duration_secs, 180);
        assert!(!analysis.findings.is_empty());
        assert!(analysis
            .findings
            .iter()
            .any(|finding| finding.contains("重新授权") || finding.contains("重新登录")));
    }

    #[test]
    fn handles_empty_and_unrelated_logs() {
        assert_eq!(analyze_log("").total_lines, 0);
        let analysis = analyze_log(
            "[2026-10-27 10:00:00] [info] [core] something\n\
             [2026-10-27 10:00:01] [info] [vpn] 无关内容\n",
        );
        assert_eq!(analysis.total_lines, 0);
        assert!(analysis
            .findings
            .iter()
            .any(|finding| finding.contains("未检测到")));
    }

    #[test]
    fn latest_session_slice_keeps_only_last_session() {
        let text = "\
[2026-10-27 10:00:00] [info] [vpn] 2026/10/27 10:00:00 Start FDU Connect v0.9.0
[2026-10-27 10:01:00] [info] [vpn] 2026/10/27 10:01:00 KeepAlive: OK
[2026-10-27 10:02:00] [info] [vpn] 2026/10/27 10:02:00 Start FDU Connect v0.9.0
[2026-10-27 10:03:00] [info] [vpn] 2026/10/27 10:03:00 KeepAlive: lookup: i/o timeout
[2026-10-27 10:04:00] [info] [vpn] 2026/10/27 10:04:00 校内 VPN 已停止
";
        let slice = latest_session_slice(text);
        let analysis = analyze_log(slice);
        assert_eq!(analysis.sessions, 1);
        assert_eq!(analysis.keepalive_fail, 1);
        assert_eq!(analysis.failure_windows.len(), 1);
        assert_eq!(analysis.failure_windows[0].duration_secs, 60);
    }

    #[test]
    #[ignore = "需要设置 LABVPN_VPN_LOG 指向真实 app.log 才有意义"]
    fn analyze_external_log_when_provided() {
        let Ok(path) = std::env::var("LABVPN_VPN_LOG") else {
            return;
        };
        let text = std::fs::read_to_string(path).expect("read log");
        let slice = latest_session_slice(&text);
        let analysis = analyze_log(slice);
        println!("{}", serde_json::to_string_pretty(&analysis).expect("json"));
        assert!(analysis.total_lines > 0);
    }

    #[test]
    fn counts_resets_and_tunnel_resets() {
        let text = "\
[2026-10-27 10:00:00] [info] [vpn] 2026/10/27 10:00:00 Start FDU Connect v0.9.0
[2026-10-27 10:00:05] [info] [vpn] 2026/10/27 10:00:05 Error occurred while receiving, retrying: EOF
[2026-10-27 10:00:10] [warn] [vpn] [SOCKS5] 2026/10/27 10:00:10 [E]: server: read tcp 10.230.1.1:123: connection reset by peer
";
        let analysis = analyze_log(text);
        assert_eq!(analysis.tunnel_resets, 1);
        assert_eq!(analysis.connection_resets, 1);
        assert_eq!(analysis.dial_errors, 1);
    }
}
