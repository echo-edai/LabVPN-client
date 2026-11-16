//! 网络变化或长时间暂停后的保守恢复策略；不把本地 API 存活等同于网络可用。

use std::time::{Duration, SystemTime};

pub(super) struct ResumeRecovery {
    last_poll: SystemTime,
    path: Vec<String>,
    pending: bool,
    stable: u8,
    failures: u8,
}

impl ResumeRecovery {
    pub(super) fn new(now: SystemTime) -> Self {
        Self {
            last_poll: now,
            path: Vec::new(),
            pending: false,
            stable: 0,
            failures: 0,
        }
    }

    pub(super) fn reset(&mut self, now: SystemTime) {
        *self = Self::new(now);
    }

    /// 两轮物理地址稳定后才允许恢复，等待 DHCP 与路由重建。
    pub(super) fn observe(&mut self, now: SystemTime, path: Vec<String>) -> bool {
        let paused =
            now.duration_since(self.last_poll).unwrap_or_default() > Duration::from_secs(45);
        let changed = path != self.path;
        if paused || changed {
            self.pending = true;
            self.stable = 0;
            self.failures = 0;
        }
        self.last_poll = now;
        self.path = path;
        if self.path.is_empty() {
            self.stable = 0;
            return false;
        }
        self.stable = self.stable.saturating_add(1);
        self.stable >= 2
    }

    pub(super) fn needs_check(&self) -> bool {
        self.pending && self.stable >= 2
    }

    /// 两个独立出口都连续两轮失败才重建，单点失败不打断 SSH。
    pub(super) fn checked(&mut self, healthy: bool) -> bool {
        if healthy {
            self.pending = false;
            self.failures = 0;
            return false;
        }
        self.failures = self.failures.saturating_add(1);
        self.failures >= 2
    }
}

/// 指纹只留内存，不把本机地址写入日志或上传。
pub(super) fn physical_path() -> Vec<String> {
    let mut path: Vec<_> = labvpn_core::probe::collect_interfaces()
        .into_iter()
        .filter(|iface| !iface.is_virtual)
        .filter_map(|iface| {
            let ip = iface.ip?;
            if ip.is_loopback() || ip.is_unspecified() {
                return None;
            }
            match ip {
                std::net::IpAddr::V4(ip) if ip.is_link_local() => return None,
                std::net::IpAddr::V6(ip) if ip.is_unicast_link_local() => return None,
                _ => {}
            }
            Some(format!("{}:{:?}:{ip}", iface.name, iface.index))
        })
        .collect();
    path.sort();
    path.dedup();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }
    fn path(name: &str) -> Vec<String> {
        vec![name.into()]
    }

    #[test]
    fn waits_for_stability_and_two_failed_checks() {
        let mut policy = ResumeRecovery::new(at(0));
        assert!(!policy.observe(at(15), path("a")));
        assert!(!policy.needs_check());
        assert!(policy.observe(at(30), path("a")));
        assert!(policy.needs_check());
        assert!(!policy.checked(false));
        assert!(policy.observe(at(45), path("a")));
        assert!(policy.checked(false));
    }

    #[test]
    fn sleep_gap_rechecks_unchanged_interface() {
        let mut policy = ResumeRecovery::new(at(0));
        policy.observe(at(15), path("a"));
        policy.observe(at(30), path("a"));
        policy.checked(true);
        assert!(!policy.needs_check());
        assert!(!policy.observe(at(180), path("a")));
        assert!(policy.observe(at(195), path("a")));
        assert!(policy.needs_check());
        assert!(!policy.checked(true), "网络正常不重启");
        assert!(!policy.needs_check());
    }

    #[test]
    fn offline_and_path_changes_reset_failure_streak() {
        let mut policy = ResumeRecovery::new(at(0));
        policy.observe(at(15), path("a"));
        policy.observe(at(30), path("a"));
        policy.checked(false);
        assert!(!policy.observe(at(45), Vec::new()));
        assert!(!policy.observe(at(60), path("b")));
        assert!(policy.observe(at(75), path("b")));
        assert!(!policy.checked(false));
        policy.reset(at(80));
        assert!(!policy.needs_check(), "断开清除恢复意图");
    }
}
