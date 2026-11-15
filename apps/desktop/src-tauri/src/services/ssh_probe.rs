//! SSH 数据面探测：必须收到协议欢迎行，不能只依赖 TCP 握手。

use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;

use crate::dto::SshTestResult;

/// 只读 SSH 欢迎行，不发凭据、不登录。
pub async fn test(host: &str, port: u16) -> SshTestResult {
    let started = Instant::now();
    let result = tokio::time::timeout(Duration::from_secs(6), async {
        let mut stream = tokio::net::TcpStream::connect((host, port))
            .await
            .map_err(|err| format!("TCP 连接失败：{err}"))?;
        let mut data = Vec::new();
        loop {
            let byte = stream
                .read_u8()
                .await
                .map_err(|err| format!("未收到 SSH 欢迎行：{err}"))?;
            data.push(byte);
            if data.len() > 8192 {
                return Err("目标返回内容超过 SSH 探测上限".to_string());
            }
            if byte == b'\n' {
                if let Some(banner) = ssh_banner(&data) {
                    return Ok(banner);
                }
            }
        }
    })
    .await;
    let (ok, message) = match result {
        Ok(Ok(banner)) => (
            true,
            format!("已收到 SSH 欢迎行：{banner}。尚未验证认证和会话稳定性"),
        ),
        Ok(Err(err)) => (false, err),
        Err(_) => (
            false,
            "SSH 数据探测超时（6s）；端口接受连接也不代表远端服务可用".to_string(),
        ),
    };
    SshTestResult {
        ok,
        message,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

fn ssh_banner(data: &[u8]) -> Option<String> {
    data.split_inclusive(|byte| *byte == b'\n')
        .filter(|line| line.ends_with(b"\n") && line.len() <= 255)
        .filter_map(|line| std::str::from_utf8(line).ok())
        .map(str::trim_end)
        .find(|line| line.starts_with("SSH-2.0-") || line.starts_with("SSH-1.99-"))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[test]
    fn requires_complete_protocol_banner() {
        assert!(ssh_banner(b"HTTP/1.1 200 OK\r\n").is_none());
        assert!(ssh_banner(b"SSH-2.0-server").is_none());
        assert_eq!(
            ssh_banner(b"notice\r\nSSH-2.0-server\r\n"),
            Some("SSH-2.0-server".to_string())
        );
    }

    #[tokio::test]
    async fn accepted_tcp_without_ssh_is_failure() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(b"HTTP/1.1 200 OK\r\n").await.unwrap();
        });
        assert!(!test("127.0.0.1", port).await.ok);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn fragmented_banner_is_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            stream.write_all(b"SSH-2.").await.unwrap();
            tokio::task::yield_now().await;
            stream.write_all(b"0-test\r\n").await.unwrap();
        });
        assert!(test("127.0.0.1", port).await.ok);
        server.await.unwrap();
    }
}
