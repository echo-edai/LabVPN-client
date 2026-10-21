//! mihomo REST API 客户端（控制端口，回环 + Bearer secret）。

use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::mpsc;

use crate::dto::{ProxyGroupInfo, TrafficPoint};
use crate::error::{CmdResult, CommandError};

/// mihomo 控制 API。
#[derive(Clone)]
pub struct MihomoApi {
    port: u16,
    secret: String,
    client: reqwest::Client,
}

impl MihomoApi {
    /// 创建客户端（自动绕过系统代理，避免自环）。
    pub fn new(port: u16, secret: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .no_proxy()
            .build()
            .unwrap_or_default();
        Self {
            port,
            secret,
            client,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut builder = self.client.request(method, self.url(path));
        if !self.secret.is_empty() {
            builder = builder.bearer_auth(&self.secret);
        }
        builder
    }

    /// 获取核心版本。
    pub async fn version(&self) -> CmdResult<String> {
        let response = self
            .request(reqwest::Method::GET, "/version")
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(CommandError::new(
                "mihomo",
                format!("获取版本失败：HTTP {}", response.status()),
            ));
        }
        let value: serde_json::Value = response.json().await?;
        Ok(value
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_string())
    }

    /// 热更新配置（PUT /configs?force=true）。
    #[allow(dead_code)]
    pub async fn put_config(&self, yaml: &str) -> CmdResult<()> {
        let response = self
            .request(reqwest::Method::PUT, "/configs?force=true")
            .header(reqwest::header::CONTENT_TYPE, "application/yaml")
            .body(yaml.to_string())
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(CommandError::new(
                "mihomo",
                format!("热更新配置失败：HTTP {}", response.status()),
            )
            .with_detail(response.text().await.unwrap_or_default()))
        }
    }

    /// 切换代理组选中项。
    pub async fn select_proxy(&self, group: &str, name: &str) -> CmdResult<()> {
        let path = format!("/proxies/{}", encode_segment(group));
        let response = self
            .request(reqwest::Method::PUT, &path)
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(CommandError::new(
                "mihomo",
                format!("切换代理组「{group}」失败：HTTP {}", response.status()),
            ))
        }
    }

    /// 测试某个代理/组的延迟（毫秒）。
    pub async fn delay(&self, name: &str, url: &str, timeout_ms: u32) -> CmdResult<u32> {
        let path = format!(
            "/proxies/{}/delay?url={}&timeout={}",
            encode_segment(name),
            encode_segment(url),
            timeout_ms
        );
        let response = self.request(reqwest::Method::GET, &path).send().await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(
                CommandError::new("mihomo", format!("延迟测试失败：HTTP {status}"))
                    .with_detail(body),
            );
        }
        let value: serde_json::Value = serde_json::from_str(&body)?;
        value
            .get("delay")
            .and_then(serde_json::Value::as_u64)
            .map(|delay| delay as u32)
            .ok_or_else(|| CommandError::new("mihomo", "延迟响应缺少 delay 字段"))
    }

    /// 读取全部代理组。
    pub async fn proxy_groups(&self) -> CmdResult<Vec<ProxyGroupInfo>> {
        let response = self
            .request(reqwest::Method::GET, "/proxies")
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(CommandError::new(
                "mihomo",
                format!("读取代理组失败：HTTP {}", response.status()),
            ));
        }
        let value: serde_json::Value = response.json().await?;
        let Some(proxies) = value.get("proxies").and_then(serde_json::Value::as_object) else {
            return Ok(Vec::new());
        };
        let mut groups = Vec::new();
        for (name, proxy) in proxies {
            let kind = proxy
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            if !matches!(kind, "Selector" | "URLTest" | "Fallback" | "LoadBalance") {
                continue;
            }
            let all = proxy
                .get("all")
                .and_then(serde_json::Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let now = proxy
                .get("now")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            let delay = proxy
                .get("history")
                .and_then(serde_json::Value::as_array)
                .and_then(|history| history.last())
                .and_then(|entry| entry.get("delay"))
                .and_then(serde_json::Value::as_u64)
                .map(|delay| delay as u32);
            groups.push(ProxyGroupInfo {
                name: name.clone(),
                kind: kind.to_string(),
                now,
                all,
                delay,
            });
        }
        groups.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(groups)
    }

    /// 订阅流量流（mihomo `/traffic`，每秒一条）。任务结束条件由调用方 abort。
    pub async fn stream_traffic(&self, sender: mpsc::Sender<(u64, u64)>) {
        let response = match self.request(reqwest::Method::GET, "/traffic").send().await {
            Ok(response) => response,
            Err(err) => {
                log::warn!("订阅流量失败: {err}");
                return;
            }
        };
        if !response.status().is_success() {
            log::warn!("订阅流量失败：HTTP {}", response.status());
            return;
        }
        let mut stream = response.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else { break };
            buffer.extend_from_slice(&chunk);
            while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = buffer.drain(..=index).collect();
                let line = String::from_utf8_lossy(&line);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                    let up = value
                        .get("up")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    let down = value
                        .get("down")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0);
                    if sender.send((up, down)).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}

/// 对 URL 路径片段做百分号编码（代理组名含 emoji/中文/空格）。
pub fn encode_segment(input: &str) -> String {
    let mut output = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

/// 构造流量点（累计值由调用方维护）。
pub fn traffic_point(up: u64, down: u64, up_total: u64, down_total: u64) -> TrafficPoint {
    TrafficPoint {
        up,
        down,
        up_total,
        down_total,
        ts: chrono::Local::now().format("%H:%M:%S").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_uri_segment() {
        assert_eq!(
            encode_segment("🚀 代理"),
            "%F0%9F%9A%80%20%E4%BB%A3%E7%90%86"
        );
        assert_eq!(encode_segment("simple-name_1.0~"), "simple-name_1.0~");
    }
}
