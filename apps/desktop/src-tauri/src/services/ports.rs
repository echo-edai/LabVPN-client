//! 按核心的实际运行权限选择 TCP 端口。

use std::{io, net::SocketAddr, time::Duration};

use crate::error::{CmdResult, CommandError};

/// helper 与桌面程序的绑定权限不同，不用普通用户的 bind 推断 root 是否可重用端口。
pub(super) async fn choose(
    mixed: u16,
    controller: u16,
    helper: bool,
) -> CmdResult<(u16, u16, Vec<String>)> {
    let mut warnings = Vec::new();
    let mixed = pick(
        mixed,
        "混合代理端口 / Mixed proxy port",
        helper,
        &[],
        &mut warnings,
    )
    .await?;
    let controller = pick(
        controller,
        "控制端口 / Controller port",
        helper,
        &[mixed],
        &mut warnings,
    )
    .await?;
    Ok((mixed, controller, warnings))
}

async fn pick(
    preferred: u16,
    label: &str,
    helper: bool,
    reserved: &[u16],
    warnings: &mut Vec<String>,
) -> CmdResult<u16> {
    let mut reason = String::new();
    for port in preferred..=preferred.saturating_add(50) {
        let result = if reserved.contains(&port) {
            Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "本次连接的另一个端口 / Reserved by this connection",
            ))
        } else {
            available(port, helper).await
        };
        match result {
            Ok(()) => {
                if port != preferred {
                    warnings.push(format!("{label} {preferred} 暂不可用，本次使用 {port}；原因 / Unavailable, using {port}: {reason}。未确认属于其他软件 / Owner not identified"));
                }
                return Ok(port);
            }
            Err(err) if port == preferred => reason = err.to_string(),
            Err(_) => {}
        }
    }
    Err(CommandError::new(
        "port",
        format!("{label} {preferred} 及邻近端口不可用 / No available port: {reason}"),
    ))
}

async fn available(port: u16, helper: bool) -> io::Result<()> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    if helper {
        // 只确认是否有活跃 TCP 监听；TIME_WAIT 和普通用户的权限不等于监听占用。
        return match tokio::time::timeout(
            Duration::from_millis(200),
            tokio::net::TcpStream::connect(address),
        )
        .await
        {
            Ok(Err(err)) if err.kind() == io::ErrorKind::ConnectionRefused => Ok(()),
            Ok(Ok(_)) => Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                "检测到 TCP 监听 / TCP listener detected",
            )),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "监听探测超时 / Listener probe timed out",
            )),
        };
    }
    crate::platform::probe_local_listener(port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn active_listener_is_unavailable_in_both_modes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(available(port, true).await.is_err());
        assert!(available(port, false).await.is_err());
    }

    #[tokio::test]
    async fn closed_listener_is_reused_without_incrementing() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let (mixed, controller, warnings) = choose(port, port, true).await.unwrap();
        assert_eq!(mixed, port);
        assert_ne!(mixed, controller, "同次连接的端口不能重复");
        assert_eq!(warnings.len(), 1);
        assert!(!warnings[0].contains("已被其他程序占用"));
    }

    #[tokio::test]
    async fn closed_tcp_connection_is_not_a_listener() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut client = tokio::net::TcpStream::connect(address).await.unwrap();
        let (mut server, _) = listener.accept().await.unwrap();
        // 服务端主动关闭后可能保留 TIME_WAIT，但已经不再提供监听服务。
        server.shutdown().await.unwrap();
        client.read_to_end(&mut Vec::new()).await.unwrap();
        drop(client);
        drop(server);
        drop(listener);
        assert!(available(address.port(), true).await.is_ok());
    }

    #[tokio::test]
    async fn busy_preferred_port_is_skipped_without_reusing_selected_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (mixed, controller, warnings) = choose(port, port, true).await.unwrap();
        assert_ne!(mixed, port);
        assert_ne!(controller, port);
        assert_ne!(mixed, controller);
        assert_eq!(warnings.len(), 2);
    }

    #[tokio::test]
    async fn exhausted_range_fails_instead_of_starting_on_busy_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let reserved: Vec<_> = (port..=port.saturating_add(50)).collect();
        assert!(pick(port, "test", true, &reserved, &mut Vec::new())
            .await
            .is_err());
    }
}
