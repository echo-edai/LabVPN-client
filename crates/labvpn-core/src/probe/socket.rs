//! 跨平台 socket 工具：可绑定物理网卡的 TCP/UDP 探测。
//!
//! 为什么需要绑卡：用户机器上可能同时运行第三方 TUN（如 Shadowrocket / Clash TUN），
//! 它会抢占默认路由，导致普通探测无法判断"是否在校内"。绑定物理网卡后可绕过 TUN。

use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

use crate::config::ProbeProtocol;
use crate::probe::ProbeAttempt;

/// 解析主机/端口为 socket 地址（字面 IP 不触发 DNS）。
pub fn resolve(host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
    if let Ok(ip) = host.trim().parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    use std::net::ToSocketAddrs;
    let addrs: Vec<SocketAddr> = (host.trim(), port).to_socket_addrs()?.collect();
    if addrs.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "域名解析失败",
        ));
    }
    Ok(addrs)
}

/// 一次 probe 的目标描述。
#[derive(Debug, Clone, Copy)]
pub struct IfaceRef<'a> {
    /// 网卡名
    pub name: &'a str,
    /// 系统网卡索引（用于绑定）
    pub index: Option<u32>,
}

/// 按协议执行一次探测。
pub fn probe(target: &crate::config::ProbeTarget, iface: Option<IfaceRef<'_>>) -> ProbeAttempt {
    let timeout = Duration::from_millis(target.timeout_ms.max(100));
    match target.protocol {
        ProbeProtocol::Tcp => tcp_connect(
            &target.host,
            target.port,
            timeout,
            iface,
        ),
        ProbeProtocol::UdpDns => dns_query(&target.host, target.port, timeout, iface),
    }
}

/// TCP 连接探测（可绑定网卡）。
pub fn tcp_connect(
    host: &str,
    port: u16,
    timeout: Duration,
    iface: Option<IfaceRef<'_>>,
) -> ProbeAttempt {
    let addrs = match resolve(host, port) {
        Ok(addrs) => addrs,
        Err(err) => return ProbeAttempt::failed(format!("解析失败：{err}")),
    };
    let addr = addrs[0];
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = match Socket::new(domain, Type::STREAM, Some(Protocol::TCP)) {
        Ok(socket) => socket,
        Err(err) => return ProbeAttempt::failed(format!("创建 socket 失败：{err}")),
    };

    let mut bound = false;
    if let Some(iface) = iface {
        bound = bind_to_interface(&socket, domain, iface);
    }

    let start = Instant::now();
    let result = socket.connect_timeout(&SockAddr::from(addr), timeout);
    let elapsed = start.elapsed().as_millis() as u64;
    match result {
        Ok(()) => ProbeAttempt {
            ok: true,
            elapsed_ms: elapsed,
            bound,
            detail: if bound {
                format!("{}ms（已绑定 {}）", elapsed, iface.map(|i| i.name).unwrap_or("-"))
            } else {
                format!("{elapsed}ms")
            },
        },
        Err(err) => ProbeAttempt {
            ok: false,
            elapsed_ms: elapsed,
            bound,
            detail: format!("{}ms（{err}）", elapsed),
        },
    }
}

/// UDP DNS 探测：向 DNS 服务器查询一个公网域名的 A 记录。
pub fn dns_query(
    host: &str,
    port: u16,
    timeout: Duration,
    iface: Option<IfaceRef<'_>>,
) -> ProbeAttempt {
    let addrs = match resolve(host, port) {
        Ok(addrs) => addrs,
        Err(err) => return ProbeAttempt::failed(format!("解析失败：{err}")),
    };
    let addr = addrs[0];
    let domain = if addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = match Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)) {
        Ok(socket) => socket,
        Err(err) => return ProbeAttempt::failed(format!("创建 socket 失败：{err}")),
    };
    let mut bound = false;
    if let Some(iface) = iface {
        bound = bind_to_interface(&socket, domain, iface);
    }
    let bind_addr = if addr.is_ipv4() {
        SocketAddr::from(([0, 0, 0, 0], 0))
    } else {
        SocketAddr::from(([0u16; 8], 0))
    };
    if let Err(err) = socket.bind(&SockAddr::from(bind_addr)) {
        return ProbeAttempt::failed(format!("绑定失败：{err}"));
    }
    if let Err(err) = socket.connect(&SockAddr::from(addr)) {
        return ProbeAttempt::failed(format!("连接失败：{err}"));
    }
    if let Err(err) = socket.set_read_timeout(Some(timeout)) {
        return ProbeAttempt::failed(format!("设置超时失败：{err}"));
    }
    // 转为 std UdpSocket 使用稳定 API
    let udp_socket = std::net::UdpSocket::from(socket);

    let query = build_dns_query("www.fudan.edu.cn", rand_id());
    let start = Instant::now();
    if let Err(err) = udp_socket.send(&query) {
        return ProbeAttempt::failed(format!("发送失败：{err}"));
    }
    let mut buf = [0u8; 512];
    match udp_socket.recv(&mut buf) {
        Ok(size) => {
            let elapsed = start.elapsed().as_millis() as u64;
            match parse_dns_response(&buf[..size]) {
                Some(response) if response.answer_count > 0 => ProbeAttempt {
                    ok: true,
                    elapsed_ms: elapsed,
                    bound,
                    detail: format!("{}ms（DNS 应答）", elapsed),
                },
                Some(_) => ProbeAttempt {
                    ok: false,
                    elapsed_ms: elapsed,
                    bound,
                    detail: "DNS 无应答记录".to_string(),
                },
                None => ProbeAttempt {
                    ok: false,
                    elapsed_ms: elapsed,
                    bound,
                    detail: "DNS 响应非法".to_string(),
                },
            }
        }
        Err(err) => {
            let elapsed = start.elapsed().as_millis() as u64;
            ProbeAttempt {
                ok: false,
                elapsed_ms: elapsed,
                bound,
                detail: format!("{}ms（{err}）", elapsed),
            }
        }
    }
}

/// 把 socket 绑定到指定界面；返回是否成功。
#[allow(unused_variables)]
fn bind_to_interface(socket: &Socket, domain: Domain, iface: IfaceRef<'_>) -> bool {
    let Some(index) = iface.index.and_then(NonZeroU32::new) else {
        return false;
    };
    #[cfg(unix)]
    {
        match domain {
            Domain::IPV4 => socket.bind_device_by_index_v4(Some(index)).is_ok(),
            Domain::IPV6 => socket.bind_device_by_index_v6(Some(index)).is_ok(),
            _ => false,
        }
    }
    #[cfg(windows)]
    {
        // socket2 暂不支持 Windows 的 IP_UNICAST_IF，回退为不绑定（上层仍可依赖 IP 段/SSID 信号）
        let _ = (socket, domain, index);
        false
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

fn rand_id() -> u16 {
    // 用时间戳低 16 位做事务 ID 足够（探测场景不要求密码学随机）
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.subsec_nanos() & 0xffff) as u16)
        .unwrap_or(0x1a2b)
}

/// 构造一个 A 记录 DNS 查询包。
pub fn build_dns_query(name: &str, id: u16) -> Vec<u8> {
    let mut packet = Vec::with_capacity(64);
    packet.extend_from_slice(&id.to_be_bytes());
    packet.extend_from_slice(&0x0100u16.to_be_bytes()); // RD=1
    packet.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    packet.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
    for label in name.trim_end_matches('.').split('.') {
        let len = label.len().min(63);
        packet.push(len as u8);
        packet.extend_from_slice(&label.as_bytes()[..len]);
    }
    packet.push(0);
    packet.extend_from_slice(&1u16.to_be_bytes()); // QTYPE=A
    packet.extend_from_slice(&1u16.to_be_bytes()); // QCLASS=IN
    packet
}

/// DNS 响应摘要。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsResponse {
    /// 事务 ID
    pub id: u16,
    /// 返回码（0=NOERROR）
    pub rcode: u8,
    /// 应答记录数
    pub answer_count: u16,
}

/// 解析 DNS 响应头（只取诊断需要的字段）。
pub fn parse_dns_response(packet: &[u8]) -> Option<DnsResponse> {
    if packet.len() < 12 {
        return None;
    }
    let id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    let rcode = (flags & 0x000f) as u8;
    let answer_count = u16::from_be_bytes([packet[6], packet[7]]);
    Some(DnsResponse {
        id,
        rcode,
        answer_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_dns_query() {
        let query = build_dns_query("www.fudan.edu.cn", 0x1234);
        assert_eq!(query[0..2], [0x12, 0x34]);
        assert_eq!(u16::from_be_bytes([query[4], query[5]]), 1);
        // 以 0 结尾 + QTYPE/QCLASS
        assert!(query.ends_with(&[0, 0, 1, 0, 1]));
    }

    #[test]
    fn parses_dns_response() {
        let mut packet = vec![0u8; 12];
        packet[0] = 0x12;
        packet[1] = 0x34;
        packet[3] = 0x80; // QR=1
        packet[6] = 0;
        packet[7] = 3; // 3 answers
        let parsed = parse_dns_response(&packet).expect("parse");
        assert_eq!(parsed.id, 0x1234);
        assert_eq!(parsed.answer_count, 3);
        assert_eq!(parsed.rcode, 0);
        assert!(parse_dns_response(&[0u8; 4]).is_none());
    }
}
