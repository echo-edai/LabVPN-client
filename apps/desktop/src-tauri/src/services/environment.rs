//! 用户可读的环境快照与有限时的数据面检查。

use crate::{
    error::{CmdResult, CommandError},
    platform,
    state::AppState,
};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    pub label: String,
    pub domain: String,
    pub disabled: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkCheck {
    pub name: String,
    pub ok: Option<bool>,
    pub elapsed_ms: u64,
    pub detail: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentReport {
    pub checked_at: String,
    pub startup_ready: bool,
    pub connection_requested: bool,
    pub core_running: bool,
    pub core_alive: bool,
    pub core_pid: Option<u32>,
    pub vpn_state: String,
    pub vpn_pid: Option<u32>,
    pub proxy_lease_pending: bool,
    pub autostart: bool,
    pub can_disable_startup: bool,
    pub proxies: Vec<String>,
    pub proxy_settings: Vec<String>,
    pub startup_items: Vec<StartupItem>,
    pub environment_keys: Vec<String>,
    pub warnings: Vec<String>,
    pub checks: Vec<NetworkCheck>,
}

pub async fn inspect(state: &AppState, test_network: bool) -> CmdResult<EnvironmentReport> {
    let runtime = state.engine.status().await;
    let vpn = state.vpn.status(&state.config_snapshot().campus_vpn).await;
    let core_alive = runtime.running && state.engine.core_alive().await;
    let (proxies, proxy_settings, startup_items) = tokio::task::spawn_blocking(|| {
        (
            super::proxy_guard::running_proxies(),
            platform::proxy_summary(),
            platform::startup_items(),
        )
    })
    .await
    .map_err(|err| CommandError::new("environment", err.to_string()))?;
    let mut warnings = platform::proxy_warnings();
    if let Some(error) = state
        .startup_error
        .read()
        .ok()
        .and_then(|error| error.clone())
    {
        warnings.push(error);
    }
    if let Some(error) = runtime.last_error.as_ref() {
        warnings.push(error.clone());
    }
    let proxies = proxies.unwrap_or_else(|err| {
        warnings.push(err);
        Vec::new()
    });
    let proxy_settings = proxy_settings.unwrap_or_else(|err| {
        warnings.push(err);
        Vec::new()
    });
    let checks = if test_network {
        let route = check_https("systemRoute", "https://www.baidu.com/", None);
        let mixed = async {
            if core_alive {
                check_https(
                    "mixedProxy",
                    "https://www.gstatic.com/generate_204",
                    Some(runtime.mixed_port),
                )
                .await
            } else {
                NetworkCheck {
                    name: "mixedProxy".into(),
                    ok: None,
                    elapsed_ms: 0,
                    detail: "核心未就绪，跳过代理出口检查 / Core is not ready".into(),
                }
            }
        };
        let campus = async {
            let started = Instant::now();
            if matches!(vpn.state.as_str(), "running" | "reusing") {
                let result =
                    super::vpn::probe_campus_dns(&state.config_snapshot().campus_vpn).await;
                NetworkCheck {
                    name: "campusDns".into(),
                    ok: Some(result.is_ok()),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    detail: result
                        .map(|ms| format!("校园 DNS 数据往返成功 / Campus DNS response: {ms} ms"))
                        .unwrap_or_else(|err| err),
                }
            } else {
                NetworkCheck {
                    name: "campusDns".into(),
                    ok: None,
                    elapsed_ms: 0,
                    detail: "校园隧道未运行，跳过 / Campus tunnel is not running".into(),
                }
            }
        };
        let (route, mixed, campus) = tokio::join!(route, mixed, campus);
        vec![route, mixed, campus]
    } else {
        Vec::new()
    };
    Ok(EnvironmentReport {
        checked_at: chrono::Utc::now().to_rfc3339(),
        startup_ready: state.startup_ready.load(Ordering::Acquire),
        connection_requested: state.connection_requested.load(Ordering::Acquire),
        core_running: runtime.running,
        core_alive,
        core_pid: runtime.pid,
        vpn_state: vpn.state,
        vpn_pid: vpn.pid,
        proxy_lease_pending: state.paths.data_dir.join("proxy-lease.json").exists(),
        autostart: platform::is_autostart_enabled(&state.app),
        can_disable_startup: platform::can_disable_proxy_startup(),
        proxies,
        proxy_settings,
        startup_items,
        environment_keys: [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
        ]
        .into_iter()
        .filter(|key| std::env::var_os(key).is_some())
        .map(str::to_string)
        .collect(),
        warnings,
        checks,
    })
}

async fn check_https(name: &str, url: &str, proxy: Option<u16>) -> NetworkCheck {
    let started = Instant::now();
    let result = async {
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(7))
            .redirect(reqwest::redirect::Policy::limited(3));
        if let Some(port) = proxy {
            builder = builder.proxy(
                reqwest::Proxy::all(format!("http://127.0.0.1:{port}"))
                    .map_err(|err| err.to_string())?,
            );
        }
        let response = builder
            .build()
            .map_err(|err| err.to_string())?
            .head(url)
            .send()
            .await
            .map_err(|err| err.to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }
        Ok(format!("HTTPS {}", response.status()))
    }
    .await;
    NetworkCheck {
        name: name.into(),
        ok: Some(result.is_ok()),
        elapsed_ms: started.elapsed().as_millis() as u64,
        detail: result.unwrap_or_else(|err| err),
    }
}
