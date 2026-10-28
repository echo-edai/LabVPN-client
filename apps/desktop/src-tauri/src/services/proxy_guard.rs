//! 连接前关闭本机其他代理程序（设置项：`engine.close_other_proxies`）。
//!
//! 目的：macOS 上经常同时装 Clash Verge / ClashX / V2rayU / Shadowrocket 等代理；
//! 它们只要在运行，就可能继续占用系统代理、7890/7891/7897 等端口，甚至自己的 TUN
//! 抢走流量，导致「LabVPN 显示已连接，但流量并没有被 LabVPN 接管」。
//!
//! 本模块只在用户开启设置后执行：识别**已知代理程序**（白名单，绝不误杀其它进程），
//! 先请 GUI 应用正常退出（osascript quit），再对残留进程发 TERM，最后 KILL。
//! LabVPN 自己的进程（App / helper / mihomo / fdu-connect）会被显式排除。

use std::sync::Arc;

use crate::services::logs::LogStore;

/// 当前平台是否支持自动关闭其他代理。
pub fn supported() -> bool {
    cfg!(unix)
}

#[cfg(unix)]
pub async fn close_other_proxies(logs: &Arc<LogStore>) -> Vec<String> {
    let logs = Arc::clone(logs);
    match tokio::task::spawn_blocking(move || unix_impl::close_blocking(&logs)).await {
        Ok(closed) => closed,
        Err(err) => {
            log::warn!("关闭其他代理任务失败: {err}");
            Vec::new()
        }
    }
}

#[cfg(not(unix))]
pub async fn close_other_proxies(_logs: &Arc<LogStore>) -> Vec<String> {
    Vec::new()
}

#[cfg(unix)]
mod unix_impl {
    use std::process::Command;
    use std::time::Duration;

    use crate::services::logs::LogStore;

    pub(super) struct KnownProxy {
        pub(super) label: &'static str,
        pub(super) app_name: Option<&'static str>,
        pub(super) patterns: &'static [&'static str],
    }

    pub(super) const KNOWN_PROXIES: &[KnownProxy] = &[
        KnownProxy {
            label: "Clash Verge",
            app_name: Some("Clash Verge"),
            patterns: &[
                "/Applications/Clash Verge.app",
                "clash-verge",
                "verge-mihomo",
            ],
        },
        KnownProxy {
            label: "ClashX",
            app_name: Some("ClashX"),
            patterns: &[
                "/Applications/ClashX.app",
                "/Applications/ClashX Pro.app",
                "ClashX Pro",
            ],
        },
        KnownProxy {
            label: "Mihomo Party",
            app_name: Some("Mihomo Party"),
            patterns: &["/Applications/Mihomo Party.app", "mihomo-party"],
        },
        KnownProxy {
            label: "Clash Nyanpasu",
            app_name: Some("Clash Nyanpasu"),
            patterns: &["/Applications/Clash Nyanpasu.app", "nyanpasu"],
        },
        KnownProxy {
            label: "FlClash",
            app_name: Some("FlClash"),
            patterns: &["/Applications/FlClash.app", "flclash"],
        },
        KnownProxy {
            label: "V2rayU",
            app_name: Some("V2rayU"),
            patterns: &["/Applications/V2rayU.app", "v2rayU", "v2ray-core"],
        },
        KnownProxy {
            label: "Hiddify",
            app_name: Some("Hiddify"),
            patterns: &["/Applications/Hiddify.app", "hiddify-core", "hiddify"],
        },
        KnownProxy {
            label: "sing-box",
            app_name: None,
            patterns: &["sing-box"],
        },
        KnownProxy {
            label: "Shadowrocket",
            app_name: Some("Shadowrocket"),
            patterns: &["/Applications/Shadowrocket.app", "Shadowrocket"],
        },
        KnownProxy {
            label: "Surge",
            app_name: Some("Surge"),
            patterns: &["/Applications/Surge.app", "/Applications/Surge 5.app"],
        },
        KnownProxy {
            label: "Quantumult X",
            app_name: Some("Quantumult X"),
            patterns: &["/Applications/Quantumult X.app"],
        },
        KnownProxy {
            label: "Stash",
            app_name: Some("Stash"),
            patterns: &["/Applications/Stash.app"],
        },
        KnownProxy {
            label: "Loon",
            app_name: Some("Loon"),
            patterns: &["/Applications/Loon.app"],
        },
        KnownProxy {
            label: "ShadowsocksX-NG",
            app_name: Some("ShadowsocksX-NG"),
            patterns: &["/Applications/ShadowsocksX-NG.app", "ss-local"],
        },
    ];

    /// 关闭已知代理，返回成功关闭的名称列表。
    pub(super) fn close_blocking(logs: &LogStore) -> Vec<String> {
        let mut closed = Vec::new();
        for proxy in KNOWN_PROXIES {
            let mut pids = find_pids(proxy.patterns);
            if pids.is_empty() {
                continue;
            }
            logs.info(
                "proxy-guard",
                format!(
                    "检测到其他代理 {}（pid: {}），准备关闭",
                    proxy.label,
                    pids.iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            if let Some(app_name) = proxy.app_name {
                let script = format!("tell application \"{app_name}\" to quit");
                let _ = Command::new("osascript").args(["-e", &script]).output();
            }
            for pid in &pids {
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
            }
            std::thread::sleep(Duration::from_millis(1200));
            pids = find_pids(proxy.patterns);
            for pid in &pids {
                let _ = Command::new("kill")
                    .args(["-KILL", &pid.to_string()])
                    .status();
            }
            if pids.is_empty() {
                closed.push(proxy.label.to_string());
                logs.info("proxy-guard", format!("已关闭其他代理：{}", proxy.label));
            } else {
                logs.warn(
                    "proxy-guard",
                    format!(
                        "其他代理仍在运行：{}（pid: {}），请手动退出",
                        proxy.label,
                        pids.iter()
                            .map(u32::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
            }
        }
        closed
    }

    /// 查找匹配的进程（排除 LabVPN 自身进程）。
    fn find_pids(patterns: &[&str]) -> Vec<u32> {
        let mut pids = Vec::new();
        for pattern in patterns {
            let Ok(output) = Command::new("pgrep").args(["-f", pattern]).output() else {
                continue;
            };
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let Ok(pid) = line.trim().parse::<u32>() else {
                    continue;
                };
                if is_labvpn_process(pid) {
                    continue;
                }
                pids.push(pid);
            }
        }
        pids.sort_unstable();
        pids.dedup();
        pids
    }

    /// 判断是否 LabVPN 自己的进程（绝不关闭自己）。
    fn is_labvpn_process(pid: u32) -> bool {
        let Ok(output) = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
        else {
            // 拿不到命令行时保守跳过，避免误杀
            return true;
        };
        let command = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
        command.contains("labvpn") || command.contains("/usr/local/libexec/labvpn")
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::unix_impl::KNOWN_PROXIES;

    #[test]
    fn never_targets_labvpn_itself() {
        for proxy in KNOWN_PROXIES {
            for pattern in proxy.patterns {
                assert!(
                    !pattern.to_ascii_lowercase().contains("labvpn"),
                    "代理匹配规则不能命中 LabVPN 自己：{pattern}"
                );
            }
        }
    }

    #[test]
    fn app_names_match_quit_script_safe() {
        for proxy in KNOWN_PROXIES {
            if let Some(name) = proxy.app_name {
                assert!(
                    !name.contains('"'),
                    "AppleScript 应用名不能包含引号：{name}"
                );
            }
        }
    }
}
