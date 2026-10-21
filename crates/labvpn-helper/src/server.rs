//! 回环 TCP JSON-Lines 服务端。

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use labvpn_core::ipc::{
    is_compatible, verify_token, Command, Request, Response, MAX_REQUEST_BYTES, PROTOCOL_VERSION,
};

use crate::logging;
use crate::supervisor::Supervisor;

/// 启动服务（阻塞）。
pub fn run(port: u16, token: String, supervisor: Arc<Supervisor>) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    logging::info(format!(
        "helper 已监听 127.0.0.1:{port}（协议 v{PROTOCOL_VERSION}）"
    ));
    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let is_loopback = stream
                    .peer_addr()
                    .map(|addr| addr.ip().is_loopback())
                    .unwrap_or(false);
                if !is_loopback {
                    logging::warn("拒绝非回环连接");
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                }
                let token = token.clone();
                let supervisor = Arc::clone(&supervisor);
                std::thread::spawn(move || {
                    if let Err(err) = handle(stream, &token, &supervisor) {
                        logging::warn(format!("请求处理失败: {err}"));
                    }
                });
            }
            Err(err) => logging::error(format!("接受连接失败: {err}")),
        }
    }
    Ok(())
}

fn handle(mut stream: TcpStream, token: &str, supervisor: &Supervisor) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream.take(MAX_REQUEST_BYTES as u64 + 1));
    let mut line = String::new();
    reader.read_line(&mut line)?;
    if line.trim().is_empty() {
        return respond(&mut stream, &Response::failure("空请求"));
    }
    if line.len() > MAX_REQUEST_BYTES {
        return respond(&mut stream, &Response::failure("请求过大"));
    }
    let request: Request = match serde_json::from_str(line.trim_end()) {
        Ok(request) => request,
        Err(err) => {
            return respond(
                &mut stream,
                &Response::failure(format!("请求解析失败: {err}")),
            )
        }
    };
    if !is_compatible(request_protocol_hint(&request)) {
        logging::warn("客户端协议版本可能不兼容");
    }
    if !verify_token(token, &request.token) {
        logging::warn("令牌校验失败，拒绝请求");
        return respond(&mut stream, &Response::failure("认证失败"));
    }

    let (response, shutdown) = dispatch(request.command, supervisor);
    respond(&mut stream, &response)?;
    if shutdown {
        logging::info("收到 shutdown，helper 退出");
        std::process::exit(0);
    }
    Ok(())
}

/// 当前协议版本固定，客户端版本在响应中返回；此函数保留用于未来扩展校验。
fn request_protocol_hint(_request: &Request) -> u32 {
    PROTOCOL_VERSION
}

fn respond(stream: &mut TcpStream, response: &Response) -> io::Result<()> {
    let json = serde_json::to_string(response)
        .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"响应序列化失败\"}".to_string());
    stream.write_all(json.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

fn dispatch(command: Command, supervisor: &Supervisor) -> (Response, bool) {
    match command {
        Command::Ping => (Response::success().with_status(supervisor.status()), false),
        Command::StartCore {
            config_yaml,
            assets,
        } => match supervisor.start(&config_yaml, &assets) {
            Ok(status) => (Response::success().with_status(status), false),
            Err(err) => (
                Response::failure(err).with_status(supervisor.status()),
                false,
            ),
        },
        Command::StopCore => (Response::success().with_status(supervisor.stop()), false),
        Command::RestartCore {
            config_yaml,
            assets,
        } => {
            let _ = supervisor.stop();
            match supervisor.start(&config_yaml, &assets) {
                Ok(status) => (Response::success().with_status(status), false),
                Err(err) => (
                    Response::failure(err).with_status(supervisor.status()),
                    false,
                ),
            }
        }
        Command::Status => (Response::success().with_status(supervisor.status()), false),
        Command::Shutdown => {
            let _ = supervisor.stop();
            (Response::success(), true)
        }
    }
}
