//! 网络分析命令：流量/出口 IP/校园网时间线聚合与 AI 平台域名配置。

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Local, TimeZone};
use tauri::State;

use crate::dto::{
    AiDomainSeries, AiExposureData, AiExposureRecord, AiIpSegment, AnalyticsData, CampusPointDto,
    CategoryBytes, CategoryTotals, EgressSampleDto, TrafficBucketDto,
};
use crate::error::CmdResult;
use crate::services::stats::{AiExposureLine, EgressLine, TrafficLine};
use crate::state::AppState;

fn read_jsonl<T: serde::de::DeserializeOwned>(dir: &Path, prefix: &str, since_day: i64) -> Vec<T> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(prefix) || !name.ends_with(".jsonl") {
            continue;
        }
        let Some(day) = name
            .trim_end_matches(".jsonl")
            .rsplit('-')
            .next()
            .and_then(|value| chrono::NaiveDate::parse_from_str(value, "%Y%m%d").ok())
        else {
            continue;
        };
        let start = day
            .and_hms_opt(0, 0, 0)
            .and_then(|value| value.and_local_timezone(Local).single())
            .map(|value| value.timestamp())
            .unwrap_or(0);
        if start + 86_400 < since_day {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<T>(trimmed) {
                result.push(value);
            }
        }
    }
    result
}

/// 网络分析数据。
#[tauri::command]
pub fn get_analytics(
    state: State<'_, AppState>,
    range: Option<String>,
) -> CmdResult<AnalyticsData> {
    let range = range.unwrap_or_else(|| "24h".to_string());
    let now = Local::now().timestamp();
    let (since, granularity) = match range.as_str() {
        "7d" => (now - 7 * 86_400, "day"),
        "30d" => (now - 30 * 86_400, "day"),
        _ => (now - 86_400, "hour"),
    };
    let traffic: Vec<TrafficLine> = read_jsonl(&state.paths.stats_dir, "traffic", since);
    let egress: Vec<EgressLine> = read_jsonl(&state.paths.stats_dir, "egress", since);
    let mut buckets: BTreeMap<i64, TrafficBucketDto> = BTreeMap::new();
    let mut totals = CategoryTotals::default();
    let mut campus: Vec<CampusPointDto> = Vec::new();
    let mut last_campus: Option<bool> = None;

    for line in traffic.iter().filter(|line| line.ts >= since) {
        let bucket_ts = if granularity == "day" {
            let date = Local
                .timestamp_opt(line.ts, 0)
                .single()
                .map(|value| value.date_naive())
                .unwrap_or_else(|| Local::now().date_naive());
            date.and_hms_opt(0, 0, 0)
                .and_then(|value| value.and_local_timezone(Local).single())
                .map(|value| value.timestamp())
                .unwrap_or(line.ts)
        } else {
            line.ts - line.ts.rem_euclid(3600)
        };
        let bucket = buckets
            .entry(bucket_ts)
            .or_insert_with(|| TrafficBucketDto {
                ts: bucket_ts,
                school: CategoryBytes::default(),
                overseas: CategoryBytes::default(),
                direct: CategoryBytes::default(),
                ssh: CategoryBytes::default(),
                other: CategoryBytes::default(),
                ai: CategoryBytes::default(),
            });
        add_bytes(&mut bucket.school, &line.school);
        add_bytes(&mut bucket.overseas, &line.overseas);
        add_bytes(&mut bucket.direct, &line.direct);
        add_bytes(&mut bucket.ssh, &line.ssh);
        add_bytes(&mut bucket.other, &line.other);
        add_bytes(&mut bucket.ai, &line.ai);
        add_bytes(&mut totals.school, &line.school);
        add_bytes(&mut totals.overseas, &line.overseas);
        add_bytes(&mut totals.direct, &line.direct);
        add_bytes(&mut totals.ssh, &line.ssh);
        add_bytes(&mut totals.other, &line.other);
        add_bytes(&mut totals.ai, &line.ai);
        if last_campus != Some(line.campus) {
            campus.push(CampusPointDto {
                ts: line.ts,
                on_campus: line.campus,
            });
            last_campus = Some(line.campus);
        }
    }

    let mut egress_list: Vec<EgressSampleDto> = egress
        .iter()
        .filter(|line| line.ts >= since)
        .map(|line| EgressSampleDto {
            ts: line.ts,
            route: line.route.clone(),
            ip: line.ip.clone(),
            node: line.node.clone(),
        })
        .collect();
    egress_list.sort_by_key(|line| line.ts);
    egress_list.dedup_by(|a, b| a.ip == b.ip && a.route == b.route);

    Ok(AnalyticsData {
        range,
        granularity: granularity.to_string(),
        totals,
        buckets: buckets.into_values().collect(),
        egress: egress_list,
        campus,
        ai_domains: state.config_snapshot().analytics.ai_domains,
        updated_at: Local::now().to_rfc3339(),
    })
}

fn add_bytes(target: &mut CategoryBytes, source: &CategoryBytes) {
    target.up += source.up;
    target.down += source.down;
}

/// 设置 AI 平台域名列表（自动规范化/去重）。
#[tauri::command]
pub fn set_ai_domains(state: State<'_, AppState>, domains: Vec<String>) -> CmdResult<Vec<String>> {
    let mut normalized: Vec<String> = domains
        .into_iter()
        .map(|domain| {
            domain
                .trim()
                .trim_start_matches('.')
                .trim_start_matches("*.")
                .to_ascii_lowercase()
        })
        .filter(|domain| !domain.is_empty())
        .collect();
    normalized.sort();
    normalized.dedup();
    let mut config = state.config_snapshot();
    config.analytics.ai_domains = normalized.clone();
    state.save_config(config)?;
    state.logs.info(
        "analytics",
        format!("AI 平台域名已更新：{} 个", normalized.len()),
    );
    Ok(normalized)
}

/// AI 平台 IP 暴露历史（按平台分组，记录节点与出口 IP 变化）。
#[tauri::command]
pub fn get_ai_exposure(
    state: State<'_, AppState>,
    range: Option<String>,
) -> CmdResult<AiExposureData> {
    let range = range.unwrap_or_else(|| "24h".to_string());
    let now = Local::now().timestamp();
    let since = match range.as_str() {
        "7d" => now - 7 * 86_400,
        "30d" => now - 30 * 86_400,
        _ => now - 86_400,
    };
    let lines: Vec<AiExposureLine> = read_jsonl(&state.paths.stats_dir, "ai-exposure", since);
    let config_domains = state.config_snapshot().analytics.ai_domains;

    // 平台 → 记录（按时间正序）
    let mut grouped: std::collections::BTreeMap<String, Vec<AiExposureLine>> =
        std::collections::BTreeMap::new();
    for line in lines.iter().filter(|line| line.ts >= since) {
        grouped
            .entry(line.domain.clone())
            .or_default()
            .push(line.clone());
    }

    let mut domains: Vec<String> = config_domains.clone();
    for domain in grouped.keys() {
        if !domains.contains(domain) {
            domains.push(domain.clone());
        }
    }
    domains.sort();
    domains.dedup();

    let series = domains
        .iter()
        .map(|domain| {
            let mut items = grouped.get(domain).cloned().unwrap_or_default();
            items.sort_by_key(|item| item.ts);
            let mut segments: Vec<AiIpSegment> = Vec::new();
            for item in &items {
                let ip = item.ip.clone().unwrap_or_else(|| "未知".to_string());
                match segments.last_mut() {
                    Some(segment) if segment.ip == ip && segment.node == item.node => {
                        segment.last_ts = item.ts;
                        segment.count += 1;
                    }
                    _ => segments.push(AiIpSegment {
                        ip,
                        node: item.node.clone(),
                        first_ts: item.ts,
                        last_ts: item.ts,
                        count: 1,
                    }),
                }
            }
            let mut records: Vec<AiExposureRecord> = items
                .iter()
                .rev()
                .take(200)
                .map(|item| AiExposureRecord {
                    ts: item.ts,
                    ip: item.ip.clone(),
                    node: item.node.clone(),
                    host: item.host.clone(),
                })
                .collect();
            records.sort_by_key(|record| std::cmp::Reverse(record.ts));
            AiDomainSeries {
                domain: domain.clone(),
                records,
                segments,
            }
        })
        .collect();

    Ok(AiExposureData {
        range,
        domains,
        series,
        updated_at: Local::now().to_rfc3339(),
    })
}
