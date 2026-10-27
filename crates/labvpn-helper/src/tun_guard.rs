//! TUN 自动路由自愈（macOS）。
//!
//! 背景（2026-10-27 实测）：mihomo 的 `auto-route` 在 macOS 上偶发丢失下半区
//! 自动路由（`1.0.0.0/8 … 64.0.0.0/2` 这组前缀；核心进程并未重启）。丢失后
//! `10.0.0.0/8` 等地址落回默认网关，最典型症状就是 `ssh 10.x` 超时。
//!
//! mihomo 的自动路由集合 = 两个默认半区（`0.0.0.0/1`、`128.0.0.0/1`）扣除
//! `tun.route-exclude-address`（防范 VPN 服务器环路，见 `docs/RULES.md` §8），
//! 并跳过保留网段 `0.0.0.0/8`。本模块由 root helper 周期检查这套路由是否仍在
//! TUN 接口上，缺失即补回，用户无需手动添加 `ProxyCommand` 或重启核心。
//!
//! **禁止补 `0.0.0.0/1`**：macOS 上手动添加这条路由会让 mihomo 所有出口连接
//! （包括代理节点）报 `network is unreachable`，等于整机断网——这是 2026-10-27
//! 实测踩到的坑，务必保留 `0.0.0.0/8` 给系统。
//!
//! 仅 macOS 启用：Windows / Linux 的路由安装方式与失败模式不同，不在此假设。

use std::fmt;
use std::sync::Arc;

#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
use crate::logging;
use crate::supervisor::Supervisor;

/// 巡检间隔。
#[cfg(target_os = "macos")]
const CHECK_INTERVAL: Duration = Duration::from_secs(3);
/// 核心启动后的静默期：等 mihomo 自己把路由装完，避免抢跑。
#[cfg(target_os = "macos")]
const STARTUP_GRACE_SECS: u64 = 5;

/// 一个 IPv4 CIDR（网络地址已按前缀长度对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ipv4Cidr {
    network: u32,
    prefix: u8,
}

impl Ipv4Cidr {
    /// 构造（自动对齐网络地址）。
    pub fn new(network: u32, prefix: u8) -> Self {
        let prefix = prefix.min(32);
        Self {
            network: network & mask(prefix),
            prefix,
        }
    }

    /// 解析 `a.b.c.d/n` 或裸 IP（视为 /32）；支持路由表里的缩写形式（如 `128.0/2`）。
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (address, prefix.parse::<u8>().ok()?),
            None => (text, 32),
        };
        if prefix > 32 {
            return None;
        }
        Some(Self::new(parse_ipv4(address)?, prefix))
    }

    /// 是否包含某个地址。
    pub fn contains(&self, address: u32) -> bool {
        address & mask(self.prefix) == self.network
    }

    /// 两个前缀是否重叠（一个包含另一个）。
    pub fn overlaps(&self, other: &Self) -> bool {
        let shorter = self.prefix.min(other.prefix);
        self.network & mask(shorter) == other.network & mask(shorter)
    }

    /// 拆成两个子网。
    fn split(&self) -> Option<(Self, Self)> {
        if self.prefix >= 32 {
            return None;
        }
        let low = Self::new(self.network, self.prefix + 1);
        let high = Self::new(self.network | (1u32 << (31 - self.prefix)), self.prefix + 1);
        Some((low, high))
    }
}

impl fmt::Display for Ipv4Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}/{}",
            (self.network >> 24) & 0xff,
            (self.network >> 16) & 0xff,
            (self.network >> 8) & 0xff,
            self.network & 0xff,
            self.prefix
        )
    }
}

/// 某个 TUN 会话应有的自动路由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunRoutePlan {
    /// 期望安装在 TUN 接口上的路由（`0.0.0.0/0` 去掉排除地址后的补集）。
    pub expected: Vec<Ipv4Cidr>,
    /// 配置里的排除地址（VPN 服务器等，必须直连、不能进 TUN）。
    pub excluded: Vec<Ipv4Cidr>,
}

/// 从 mihomo 配置 YAML 解析 TUN 自动路由计划；未启用 TUN / auto-route 时返回 `None`。
pub fn plan_from_config(yaml: &str) -> Option<TunRoutePlan> {
    let root: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).ok()?;
    let tun = root.get("tun")?;
    if tun.get("enable").and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    if tun.get("auto-route").and_then(|value| value.as_bool()) != Some(true) {
        return None;
    }
    let mut excluded: Vec<Ipv4Cidr> = tun
        .get("route-exclude-address")
        .and_then(|value| value.as_sequence())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().and_then(Ipv4Cidr::parse))
                .collect()
        })
        .unwrap_or_default();
    excluded.sort();
    excluded.dedup();
    Some(TunRoutePlan {
        expected: complement(&excluded),
        excluded,
    })
}

/// sing-tun/mihomo 的 auto-route 会跳过 `0.0.0.0/8`（不可路由的保留网段），
/// 因此期望集合从 `1.0.0.0/8` 开始——这与历史日志
/// `add route: 1.0.0.0/8: file exists` 以及实测路由表一致。
const AUTO_ROUTE_SKIP: &str = "0.0.0.0/8";

/// 计算 auto-route 期望安装的路由：两个默认半区各自减去排除地址。
///
/// mihomo/sing-tun 的默认路由地址就是 `0.0.0.0/1` + `128.0.0.0/1`，
/// 排除地址会从这两个半区里扣除（而不是对 `0.0.0.0/0` 做一次整体取补），
/// 因此这里也按半区分别计算，保持与核心安装结果一致。
///
/// 注意：**不要**把补出来的 `0.0.0.0/1` 装回去（见 [`AUTO_ROUTE_SKIP`]）。
/// 实测在 macOS 上手动 `route add 0.0.0.0/1` 会让 mihomo 的出口连接全部
/// 变成 `network is unreachable`（连代理节点都连不上），所以保留网段必须留给系统。
pub fn complement(excluded: &[Ipv4Cidr]) -> Vec<Ipv4Cidr> {
    let mut blocked: Vec<Ipv4Cidr> = excluded.to_vec();
    if let Some(reserved) = Ipv4Cidr::parse(AUTO_ROUTE_SKIP) {
        blocked.push(reserved);
    }
    let mut routes = vec![Ipv4Cidr::new(0x0000_0000, 1), Ipv4Cidr::new(0x8000_0000, 1)];
    for prefix in &blocked {
        let mut next = Vec::new();
        for route in routes {
            subtract(route, *prefix, &mut next);
        }
        routes = next;
    }
    routes.sort();
    routes.dedup();
    routes
}

/// `prefix \ blocked`，结果写入 `out`。
fn subtract(prefix: Ipv4Cidr, blocked: Ipv4Cidr, out: &mut Vec<Ipv4Cidr>) {
    if !prefix.overlaps(&blocked) {
        out.push(prefix);
        return;
    }
    if prefix.prefix >= blocked.prefix {
        // prefix 完全落在排除地址内
        return;
    }
    // 沿二叉树向下拆分，直到走到被排除的那一段，兄弟节点都是保留区
    let mut current = prefix;
    while current.prefix < blocked.prefix {
        let Some((low, high)) = current.split() else {
            return;
        };
        if low.contains(blocked.network) {
            out.push(high);
            current = low;
        } else {
            out.push(low);
            current = high;
        }
    }
}

/// 解析路由表里的目的地址。`netstat` 会省略尾部为 0 的字节：
/// `0/1`、`128.0/2` 带前缀；`202.120.225`（/24）、`10`（/8）需要按字节数推断。
#[cfg(any(target_os = "macos", test))]
fn parse_route_destination(field: &str) -> Option<Ipv4Cidr> {
    if field.contains('/') {
        return Ipv4Cidr::parse(field);
    }
    let octets = field.split('.').count();
    let prefix = match octets {
        4 => 32,
        3 => 24,
        2 => 16,
        1 => 8,
        _ => return None,
    };
    Some(Ipv4Cidr::new(parse_ipv4(field)?, prefix))
}

fn mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32))
    }
}

/// 解析 IPv4 地址，允许省略尾部字节（路由表缩写：`128.0`、`202.120.225`）。
fn parse_ipv4(text: &str) -> Option<u32> {
    let mut octets = [0u8; 4];
    let mut count = 0;
    for part in text.split('.') {
        if count >= 4 {
            return None;
        }
        octets[count] = part.parse::<u8>().ok()?;
        count += 1;
    }
    if count == 0 {
        return None;
    }
    Some(u32::from_be_bytes(octets))
}

/// 一条已安装的路由。
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct InstalledRoute {
    destination: Ipv4Cidr,
    interface: String,
}

/// 解析 `netstat -rn -f inet` 输出。
#[cfg(any(target_os = "macos", test))]
fn parse_route_table(text: &str) -> Vec<InstalledRoute> {
    let mut routes = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        let Some(destination) = parse_route_destination(fields[0]) else {
            continue;
        };
        let interface = fields[3];
        if !is_interface_name(interface) {
            continue;
        }
        routes.push(InstalledRoute {
            destination,
            interface: interface.to_string(),
        });
    }
    routes
}

/// 过滤 `netstat` 里的非接口列（例如标题、过期时间标记）。
#[cfg(any(target_os = "macos", test))]
fn is_interface_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '_')
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphabetic())
}

/// 找出本会话的 TUN 接口。
#[cfg(any(target_os = "macos", test))]
fn tunnel_interface(plan: &TunRoutePlan, routes: &[InstalledRoute]) -> Option<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for expected in &plan.expected {
        for route in routes {
            if route.destination != *expected || !is_tunnel_interface(&route.interface) {
                continue;
            }
            match counts.iter_mut().find(|(name, _)| name == &route.interface) {
                Some((_, count)) => *count += 1,
                None => counts.push((route.interface.clone(), 1)),
            }
        }
    }
    counts
        .into_iter()
        .max_by_key(|(name, count)| (*count, name.clone()))
        .map(|(name, _)| name)
}

#[cfg(any(target_os = "macos", test))]
fn is_tunnel_interface(name: &str) -> bool {
    name.starts_with("utun") || name.starts_with("tun")
}

/// 当前路由表里缺失的期望路由（按期望顺序返回）。
#[cfg(any(target_os = "macos", test))]
fn missing_routes(
    plan: &TunRoutePlan,
    routes: &[InstalledRoute],
    interface: &str,
) -> Vec<Ipv4Cidr> {
    plan.expected
        .iter()
        .copied()
        .filter(|expected| {
            !routes
                .iter()
                .any(|route| route.destination == *expected && route.interface == interface)
        })
        .collect()
}

#[cfg(target_os = "macos")]
const ROUTE_BIN: &str = "/sbin/route";
#[cfg(target_os = "macos")]
const NETSTAT_BIN: &str = "/usr/sbin/netstat";

/// 启动后台巡检线程（仅 macOS 生效）。
pub fn spawn(supervisor: Arc<Supervisor>) {
    #[cfg(target_os = "macos")]
    {
        let result = std::thread::Builder::new()
            .name("tun-route-guard".to_string())
            .spawn(move || watch(supervisor));
        if let Err(err) = result {
            logging::warn(format!("启动 TUN 路由守护线程失败：{err}"));
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = supervisor;
    }
}

#[cfg(target_os = "macos")]
fn watch(supervisor: Arc<Supervisor>) {
    // 首轮多等一会儿，避免和 mihomo 自己的路由安装抢跑
    std::thread::sleep(Duration::from_secs(2));
    loop {
        let status = supervisor.status();
        if status.running && status.uptime_secs.unwrap_or(0) >= STARTUP_GRACE_SECS {
            if let Some(plan) = supervisor.tun_plan() {
                if !plan.expected.is_empty() {
                    if let Err(err) = reconcile(&plan) {
                        logging::warn(format!("TUN 自动路由巡检失败：{err}"));
                    }
                }
            }
        }
        std::thread::sleep(CHECK_INTERVAL);
    }
}

#[cfg(target_os = "macos")]
fn reconcile(plan: &TunRoutePlan) -> Result<(), String> {
    let routes = read_route_table()?;
    let Some(interface) = tunnel_interface(plan, &routes) else {
        // TUN 尚未就绪（或并非本会话的 TUN）：下轮再看
        return Ok(());
    };

    let mut repaired: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for expected in missing_routes(plan, &routes, &interface) {
        match install_route(expected, &interface) {
            Ok(()) => repaired.push(expected.to_string()),
            Err(err) => failed.push(format!("{expected}: {err}")),
        }
    }

    if !repaired.is_empty() {
        logging::warn(format!(
            "TUN 自动路由缺失 {} 条，已补回（接口 {interface}）：{}",
            repaired.len(),
            repaired.join(", ")
        ));
    }
    if !failed.is_empty() {
        logging::warn(format!(
            "TUN 自动路由补回失败 {} 条：{}",
            failed.len(),
            failed.join("；")
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn read_route_table() -> Result<Vec<InstalledRoute>, String> {
    let output = std::process::Command::new(NETSTAT_BIN)
        .args(["-rn", "-f", "inet"])
        .output()
        .map_err(|err| format!("执行 netstat 失败：{err}"))?;
    if !output.status.success() {
        return Err(format!(
            "netstat 退出异常：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(parse_route_table(&String::from_utf8_lossy(&output.stdout)))
}

#[cfg(target_os = "macos")]
fn install_route(destination: Ipv4Cidr, interface: &str) -> Result<(), String> {
    let destination = destination.to_string();
    let add = std::process::Command::new(ROUTE_BIN)
        .args(["-n", "add", "-net", &destination, "-interface", interface])
        .output()
        .map_err(|err| format!("执行 route add 失败：{err}"))?;
    if add.status.success() {
        return Ok(());
    }
    // 目标前缀可能已存在于别的接口（file exists），尝试改指到 TUN
    let change = std::process::Command::new(ROUTE_BIN)
        .args([
            "-n",
            "change",
            "-net",
            &destination,
            "-interface",
            interface,
        ])
        .output()
        .map_err(|err| format!("执行 route change 失败：{err}"))?;
    if change.status.success() {
        return Ok(());
    }
    Err(format!(
        "{} / {}",
        String::from_utf8_lossy(&add.stderr).trim(),
        String::from_utf8_lossy(&change.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cidr_and_abbreviated_forms() {
        assert_eq!(
            Ipv4Cidr::parse("202.120.224.58/32").map(|prefix| prefix.to_string()),
            Some("202.120.224.58/32".to_string())
        );
        assert_eq!(
            Ipv4Cidr::parse("128.0/2").map(|prefix| prefix.to_string()),
            Some("128.0.0.0/2".to_string())
        );
        assert_eq!(
            Ipv4Cidr::parse("0/1").map(|prefix| prefix.to_string()),
            Some("0.0.0.0/1".to_string())
        );
        assert_eq!(Ipv4Cidr::parse("10.134.142.226/33"), None);
        assert_eq!(Ipv4Cidr::parse("not-an-ip"), None);
    }

    #[test]
    fn complement_matches_mihomo_auto_route() {
        // 实测基线（2026-10-27，mihomo v1.19.32 + route-exclude 202.120.224.58/32）：
        // 跳过保留网段 0.0.0.0/8 后，1.0.0.0/8 … 64.0.0.0/2 共 7 条，
        // 加上 128.0.0.0/1 侧的 31 条，共 38 条。
        let plan = plan_from_config(
            r#"
tun:
  enable: true
  auto-route: true
  route-exclude-address:
  - 202.120.224.58/32
"#,
        )
        .expect("plan");
        let expected: Vec<String> = plan.expected.iter().map(ToString::to_string).collect();
        assert_eq!(
            expected,
            vec![
                "1.0.0.0/8",
                "2.0.0.0/7",
                "4.0.0.0/6",
                "8.0.0.0/5",
                "16.0.0.0/4",
                "32.0.0.0/3",
                "64.0.0.0/2",
                "128.0.0.0/2",
                "192.0.0.0/5",
                "200.0.0.0/7",
                "202.0.0.0/10",
                "202.64.0.0/11",
                "202.96.0.0/12",
                "202.112.0.0/13",
                "202.120.0.0/17",
                "202.120.128.0/18",
                "202.120.192.0/19",
                "202.120.224.0/27",
                "202.120.224.32/28",
                "202.120.224.48/29",
                "202.120.224.56/31",
                "202.120.224.59/32",
                "202.120.224.60/30",
                "202.120.224.64/26",
                "202.120.224.128/25",
                "202.120.225.0/24",
                "202.120.226.0/23",
                "202.120.228.0/22",
                "202.120.232.0/21",
                "202.120.240.0/20",
                "202.121.0.0/16",
                "202.122.0.0/15",
                "202.124.0.0/14",
                "202.128.0.0/9",
                "203.0.0.0/8",
                "204.0.0.0/6",
                "208.0.0.0/4",
                "224.0.0.0/3",
            ]
        );
        // 排除地址不能出现在期望集合里
        assert!(!plan
            .expected
            .iter()
            .any(|prefix| prefix.contains(0xca78_e03a)));
        // 0.0.0.0/8（保留网段）必须留给系统：手动装 0.0.0.0/1 会让 mihomo 出口全部不可达
        assert!(!plan
            .expected
            .iter()
            .any(|prefix| prefix.contains(0x0000_0001)));
    }

    #[test]
    fn plan_is_none_without_auto_route() {
        let yaml = r#"
tun:
  enable: true
  auto-route: false
"#;
        assert!(plan_from_config(yaml).is_none());
        let yaml = "tun:\n  enable: false\n";
        assert!(plan_from_config(yaml).is_none());
    }

    #[test]
    fn plan_without_excludes_skips_reserved_network() {
        let plan = plan_from_config("tun:\n  enable: true\n  auto-route: true\n").expect("plan");
        let expected: Vec<String> = plan.expected.iter().map(ToString::to_string).collect();
        assert_eq!(
            expected,
            vec![
                "1.0.0.0/8",
                "2.0.0.0/7",
                "4.0.0.0/6",
                "8.0.0.0/5",
                "16.0.0.0/4",
                "32.0.0.0/3",
                "64.0.0.0/2",
                "128.0.0.0/1",
            ]
        );
    }

    #[test]
    fn excludes_are_ignored_for_plan() {
        let plan = plan_from_config(
            r#"
tun:
  enable: true
  auto-route: true
  route-exclude-address:
  - 10.0.0.0/8
  - 2001:da8::/32
"#,
        )
        .expect("plan");
        assert_eq!(plan.excluded.len(), 1);
        assert!(plan
            .expected
            .iter()
            .any(|prefix| prefix.to_string() == "16.0.0.0/4"));
    }

    #[test]
    fn parses_netstat_output() {
        let table = r#"
Routing tables

Internet:
Destination        Gateway            Flags               Netif Expire
default            192.168.3.1        UGScg                 en0
0/1                198.18.0.1         UGSc               utun11
128.0/2            198.18.0.1         UGSc               utun11
198.18.0.1         198.18.0.1         UH                 utun11
202.120.224.59/32  198.18.0.1         UGSc               utun11
202.120.225        198.18.0.1         UGSc               utun11
192.168.3          link#12            UCS                   en0      !
"#;
        let routes = parse_route_table(table);
        assert_eq!(routes.len(), 6);
        assert_eq!(routes[0].destination.to_string(), "0.0.0.0/1");
        assert_eq!(routes[0].interface, "utun11");
        assert_eq!(routes[4].destination.to_string(), "202.120.225.0/24");
    }

    #[test]
    fn infers_prefix_for_abbreviated_route_destinations() {
        assert_eq!(
            parse_route_destination("0/1").map(|prefix| prefix.to_string()),
            Some("0.0.0.0/1".to_string())
        );
        assert_eq!(
            parse_route_destination("128.0").map(|prefix| prefix.to_string()),
            Some("128.0.0.0/16".to_string())
        );
        assert_eq!(
            parse_route_destination("202.120.225").map(|prefix| prefix.to_string()),
            Some("202.120.225.0/24".to_string())
        );
        assert_eq!(
            parse_route_destination("10.134.142.226").map(|prefix| prefix.to_string()),
            Some("10.134.142.226/32".to_string())
        );
        assert_eq!(parse_route_destination("default"), None);
    }

    #[test]
    fn detects_tunnel_interface_from_plan_routes() {
        let plan = plan_from_config(
            "tun:\n  enable: true\n  auto-route: true\n  route-exclude-address:\n  - 202.120.224.58/32\n",
        )
        .expect("plan");
        let table = parse_route_table(
            "default 192.168.3.1 UGScg en0\n\
             128.0/2 198.18.0.1 UGSc utun11\n\
             0/1 198.18.0.1 UGSc utun11\n",
        );
        assert_eq!(tunnel_interface(&plan, &table).as_deref(), Some("utun11"));
    }

    #[test]
    fn missing_lower_half_route_is_detected() {
        let plan = plan_from_config(
            "tun:\n  enable: true\n  auto-route: true\n  route-exclude-address:\n  - 202.120.224.58/32\n",
        )
        .expect("plan");
        // 完整路由表：期望集合 + 默认路由
        let mut complete = String::from("default 192.168.3.1 UGScg en0\n");
        for prefix in &plan.expected {
            complete.push_str(&format!("{prefix} 198.18.0.1 UGSc utun11\n"));
        }
        let routes = parse_route_table(&complete);
        assert!(missing_routes(&plan, &routes, "utun11").is_empty());

        // 模拟运行期丢失下半区中的 8.0.0.0/5（覆盖 10.0.0.0/8 的那条）
        let mut partial = String::from("default 192.168.3.1 UGScg en0\n");
        for prefix in &plan.expected {
            if prefix.to_string() == "8.0.0.0/5" {
                continue;
            }
            partial.push_str(&format!("{prefix} 198.18.0.1 UGSc utun11\n"));
        }
        let routes = parse_route_table(&partial);
        assert_eq!(
            missing_routes(&plan, &routes, "utun11"),
            vec![Ipv4Cidr::parse("8.0.0.0/5").expect("prefix")]
        );
    }
}
