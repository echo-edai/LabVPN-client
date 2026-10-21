//! 网络分析采样服务。
//!
//! 后台每 5 秒读取 mihomo 连接列表，按分流类别（校内 / 海外 / 直连 / SSH / 其他）
//! 统计增量流量，并周期采样出口 IP，写入 `stats/` 下的 JSONL 文件（按天滚动、可配置保留天数）。
//! 数据仅保存在本机，不上传任何服务器。

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{Local, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use labvpn_core::rules::names;

use crate::dto::CategoryBytes;
use crate::paths::AppPaths;
use crate::state::AppState;

const TICK: Duration = Duration::from_secs(5);
/// 即使没有流量，也按该间隔落一条零值样本，保证校园网时间线连续。
const IDLE_SAMPLE_SECS: i64 = 300;

/// 一条流量样本（JSONL）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TrafficLine {
    /// 时间（Unix 秒）
    pub ts: i64,
    /// 当时是否在校园网
    pub campus: bool,
    /// 校内流量
    pub school: CategoryBytes,
    /// 海外流量
    pub overseas: CategoryBytes,
    /// 直连流量
    pub direct: CategoryBytes,
    /// SSH 流量
    pub ssh: CategoryBytes,
    /// 其他（拒绝/未知）
    pub other: CategoryBytes,
    /// AI 平台流量
    pub ai: CategoryBytes,
    /// AI 平台请求数
    pub ai_requests: u64,
}

impl TrafficLine {
    fn is_empty(&self) -> bool {
        self.school.up == 0
            && self.school.down == 0
            && self.overseas.up == 0
            && self.overseas.down == 0
            && self.direct.up == 0
            && self.direct.down == 0
            && self.ssh.up == 0
            && self.ssh.down == 0
            && self.other.up == 0
            && self.other.down == 0
    }
}

/// 出口 IP 样本（JSONL）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EgressLine {
    /// 时间
    pub ts: i64,
    /// 路由：direct / proxy
    pub route: String,
    /// 出口 IP
    pub ip: String,
    /// 代理节点名（proxy 路由）
    #[serde(default)]
    pub node: Option<String>,
}

/// 分流类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    School,
    Overseas,
    Direct,
    Ssh,
    Other,
}

impl Category {
    fn add(self, line: &mut TrafficLine, up: u64, down: u64) {
        let slot = match self {
            Self::School => &mut line.school,
            Self::Overseas => &mut line.overseas,
            Self::Direct => &mut line.direct,
            Self::Ssh => &mut line.ssh,
            Self::Other => &mut line.other,
        };
        slot.up += up;
        slot.down += down;
    }
}

fn classify(chains: &[String]) -> Category {
    let has = |name: &str| chains.iter().any(|chain| chain == name);
    if has(names::GROUP_CAMPUS) {
        Category::School
    } else if has(names::GROUP_SSH) {
        Category::Ssh
    } else if has(names::GROUP_PROXY) || has(names::GROUP_AUTO) {
        Category::Overseas
    } else if has(names::DIRECT) {
        Category::Direct
    } else {
        Category::Other
    }
}

fn is_ai_host(host: &str, domains: &[String]) -> bool {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    domains.iter().any(|domain| {
        let domain = domain
            .trim()
            .trim_start_matches('.')
            .trim_start_matches("*.")
            .to_ascii_lowercase();
        !domain.is_empty() && (host == domain || host.ends_with(&format!(".{domain}")))
    })
}

fn day_file(dir: &Path, prefix: &str) -> PathBuf {
    dir.join(format!("{prefix}-{}.jsonl", Local::now().format("%Y%m%d")))
}

fn append_line<T: Serialize>(path: &Path, line: &T) {
    let Ok(json) = serde_json::to_string(line) else {
        return;
    };
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| writeln!(file, "{json}"));
}

/// 清理超过保留天数的统计数据。
fn cleanup(paths: &AppPaths, retention_days: u32) {
    let Ok(entries) = std::fs::read_dir(&paths.stats_dir) else {
        return;
    };
    let cutoff = Local::now() - chrono::Duration::days(i64::from(retention_days.max(1)));
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let date = name
            .trim_end_matches(".jsonl")
            .rsplit('-')
            .next()
            .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y%m%d").ok());
        if let Some(date) = date {
            let as_local = date
                .and_hms_opt(0, 0, 0)
                .and_then(|value| value.and_local_timezone(Local).single());
            if as_local.is_some_and(|value| value < cutoff) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// 通过公网 IP 回显服务查询出口 IP（direct 或 proxy 路由）。
async fn query_egress_ip(proxy_port: Option<u16>) -> Option<String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("LabVPN/egress-check");
    match proxy_port {
        Some(port) => {
            let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{port}")).ok()?;
            builder = builder.proxy(proxy);
        }
        None => builder = builder.no_proxy(),
    }
    let client = builder.build().ok()?;
    for url in [
        "https://api.ipify.org?format=json",
        "https://ifconfig.me/ip",
    ] {
        let Ok(response) = client.get(url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(text) = response.text().await else {
            continue;
        };
        let text = text.trim().to_string();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(ip) = value.get("ip").and_then(serde_json::Value::as_str) {
                return Some(ip.to_string());
            }
        }
        let candidate = text.split_whitespace().next().unwrap_or("").to_string();
        if candidate.parse::<std::net::IpAddr>().is_ok() {
            return Some(candidate);
        }
    }
    None
}

/// 启动网络分析后台采样。
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        cleanup(
            &state.paths,
            state.config_snapshot().analytics.retention_days,
        );
        let mut connection_totals: HashMap<String, (u64, u64)> = HashMap::new();
        let mut seen_connections: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut pending = TrafficLine::default();
        let mut last_egress: Option<Instant> = None;
        let mut last_write = Instant::now();
        let mut last_campus: Option<bool> = None;

        loop {
            tokio::time::sleep(TICK).await;
            let config = state.config_snapshot();
            let analytics = config.analytics.clone();
            if !analytics.enabled {
                continue;
            }
            let on_campus = state
                .last_campus()
                .await
                .map(|status| status.on_campus)
                .unwrap_or(false);

            if state.engine.is_running().await {
                if let Some(api) = state.engine.api().await {
                    if let Ok(connections) = api.connections().await {
                        let now = Utc::now().timestamp();
                        let mut alive: std::collections::HashSet<String> =
                            std::collections::HashSet::new();
                        for connection in &connections {
                            alive.insert(connection.id.clone());
                            let (last_up, last_down) = connection_totals
                                .get(&connection.id)
                                .copied()
                                .unwrap_or((0, 0));
                            let up = connection.upload.saturating_sub(last_up);
                            let down = connection.download.saturating_sub(last_down);
                            connection_totals.insert(
                                connection.id.clone(),
                                (connection.upload, connection.download),
                            );
                            if up == 0 && down == 0 {
                                continue;
                            }
                            let category = classify(&connection.chains);
                            category.add(&mut pending, up, down);
                            if is_ai_host(&connection.host, &analytics.ai_domains) {
                                pending.ai.up += up;
                                pending.ai.down += down;
                                if seen_connections.insert(connection.id.clone()) {
                                    pending.ai_requests += 1;
                                }
                            }
                        }
                        connection_totals.retain(|id, _| alive.contains(id));
                        seen_connections.retain(|id| alive.contains(id));
                        pending.ts = now;
                        pending.campus = on_campus;
                    }

                    // 出口 IP 周期采样
                    let egress_due = last_egress
                        .map(|instant| {
                            instant.elapsed().as_secs() >= analytics.egress_interval_seconds
                        })
                        .unwrap_or(true);
                    if egress_due {
                        last_egress = Some(Instant::now());
                        let node = state.engine.proxy_groups().await.ok().and_then(|groups| {
                            let proxy = groups
                                .iter()
                                .find(|group| group.name == names::GROUP_PROXY)?;
                            let selected = proxy.now.clone()?;
                            if selected == names::GROUP_AUTO {
                                groups
                                    .iter()
                                    .find(|group| group.name == names::GROUP_AUTO)
                                    .and_then(|group| group.now.clone())
                            } else {
                                Some(selected)
                            }
                        });
                        if let Some(ip) = query_egress_ip(None).await {
                            append_line(
                                &day_file(&state.paths.stats_dir, "egress"),
                                &EgressLine {
                                    ts: Utc::now().timestamp(),
                                    route: "direct".to_string(),
                                    ip,
                                    node: None,
                                },
                            );
                        }
                        if let Some(ip) = query_egress_ip(Some(config.engine.mixed_port)).await {
                            append_line(
                                &day_file(&state.paths.stats_dir, "egress"),
                                &EgressLine {
                                    ts: Utc::now().timestamp(),
                                    route: "proxy".to_string(),
                                    ip,
                                    node,
                                },
                            );
                        }
                    }
                }
            }

            // 落盘：有流量或到空闲采样间隔 / 校园网状态变化
            let campus_changed = last_campus != Some(on_campus);
            let due = last_write.elapsed().as_secs() as i64 >= IDLE_SAMPLE_SECS;
            if !pending.is_empty() || due || campus_changed {
                pending.ts = Utc::now().timestamp();
                pending.campus = on_campus;
                append_line(&day_file(&state.paths.stats_dir, "traffic"), &pending);
                pending = TrafficLine::default();
                last_write = Instant::now();
                last_campus = Some(on_campus);
            }
        }
    });
}
