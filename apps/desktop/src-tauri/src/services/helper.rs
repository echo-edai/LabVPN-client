//! 特权 helper 客户端与安装/提权逻辑。

use std::path::{Path, PathBuf};
use std::time::Duration;

use labvpn_core::ipc::{Command, Request, Response, DEFAULT_PORT};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

use crate::dto::OperationResult;
use crate::error::{CmdResult, CommandError};

/// 单次 IPC 超时（启动核心可能较慢，单独放宽）。
const IPC_TIMEOUT: Duration = Duration::from_secs(90);
/// ping 超时
const PING_TIMEOUT: Duration = Duration::from_secs(2);

/// 读取令牌文件。
pub fn read_token(token_file: &Path) -> CmdResult<String> {
    let token = std::fs::read_to_string(token_file)
        .map_err(|err| CommandError::new("helper", format!("读取 helper 令牌失败: {err}")))?
        .trim()
        .to_string();
    if token.len() < 16 {
        return Err(CommandError::new("helper", "helper 令牌无效"));
    }
    Ok(token)
}

/// 确保令牌文件存在（0600），返回令牌。
pub fn ensure_token(token_file: &Path) -> CmdResult<String> {
    if token_file.exists() {
        if let Ok(token) = read_token(token_file) {
            return Ok(token);
        }
    }
    if let Some(parent) = token_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let token = crate::services::secrets::random_hex(32);
    std::fs::write(token_file, &token)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(token_file, std::fs::Permissions::from_mode(0o600));
    }
    Ok(token)
}

/// 发送一条 IPC 请求。
pub async fn request(port: u16, token: &str, command: Command) -> CmdResult<Response> {
    request_with_timeout(port, token, command, IPC_TIMEOUT).await
}

/// 带超时的 IPC 请求。
pub async fn request_with_timeout(
    port: u16,
    token: &str,
    command: Command,
    timeout: Duration,
) -> CmdResult<Response> {
    let connect = tokio::net::TcpStream::connect(("127.0.0.1", port));
    let stream = tokio::time::timeout(timeout, connect)
        .await
        .map_err(|_| CommandError::new("helper", "连接 helper 超时"))?
        .map_err(|err| {
            CommandError::new(
                "helper",
                format!("无法连接 helper（127.0.0.1:{port}）: {err}"),
            )
        })?;
    let (read_half, write_half) = stream.into_split();
    let envelope = Request {
        token: token.to_string(),
        command,
    };
    let mut writer = BufWriter::new(write_half);
    let payload = serde_json::to_string(&envelope)?;
    writer.write_all(payload.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    tokio::time::timeout(timeout, reader.read_line(&mut line))
        .await
        .map_err(|_| CommandError::new("helper", "等待 helper 响应超时"))??;
    if line.trim().is_empty() {
        return Err(CommandError::new("helper", "helper 返回空响应"));
    }
    let response: Response = serde_json::from_str(line.trim())?;
    if !response.ok {
        return Err(CommandError::new(
            "helper",
            response
                .error
                .clone()
                .unwrap_or_else(|| "helper 返回失败".to_string()),
        ));
    }
    Ok(response)
}

/// 探测 helper 是否运行；返回响应（含版本/状态）。
pub async fn ping(port: u16, token: &str) -> Option<Response> {
    request_with_timeout(port, token, Command::Ping, PING_TIMEOUT)
        .await
        .ok()
}

/// 停止 helper。
pub async fn shutdown(port: u16, token: &str) -> bool {
    request_with_timeout(port, token, Command::Shutdown, Duration::from_secs(10))
        .await
        .is_ok()
}

/// macOS/Linux 服务定义文件路径。
#[allow(dead_code)] // Windows 不安装系统服务，该函数仅非 Windows 使用
pub fn service_definition_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        PathBuf::from("/Library/LaunchDaemons/app.labvpn.helper.plist")
    }
    #[cfg(target_os = "linux")]
    {
        PathBuf::from("/etc/systemd/system/labvpn-helper.service")
    }
    #[cfg(windows)]
    {
        // Windows 不安装服务，用占位路径表示"无需安装"
        PathBuf::new()
    }
}

/// helper 是否已安装为系统服务。
pub fn is_installed() -> bool {
    #[cfg(windows)]
    {
        false
    }
    #[cfg(not(windows))]
    {
        service_definition_path().exists()
    }
}

/// 安装 helper：把 helper + mihomo 复制到系统目录并注册服务（需要管理员授权）。
pub fn install(
    helper: &Path,
    core: &Path,
    token_file: &Path,
    workdir: &Path,
    port: u16,
) -> CmdResult<OperationResult> {
    let mut args: Vec<String> = vec![
        "install".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--token-file".to_string(),
        token_file.display().to_string(),
        "--workdir".to_string(),
        workdir.display().to_string(),
        "--core".to_string(),
        core.display().to_string(),
    ];
    args.insert(0, helper.display().to_string());
    elevate(&args)?;
    Ok(OperationResult::ok(
        "特权 helper 安装请求已提交（如弹出系统授权请确认）",
    ))
}

/// 卸载 helper。
pub fn uninstall(helper: &Path) -> CmdResult<OperationResult> {
    let args = vec![helper.display().to_string(), "uninstall".to_string()];
    elevate(&args)?;
    Ok(OperationResult::ok("helper 卸载请求已提交"))
}

/// 以管理员身份运行 helper（用于 Windows 等无服务场景）。
pub fn start_elevated(
    helper: &Path,
    token_file: &Path,
    workdir: &Path,
    core: &Path,
    port: u16,
) -> CmdResult<OperationResult> {
    let args: Vec<String> = vec![
        helper.display().to_string(),
        "serve".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--token-file".to_string(),
        token_file.display().to_string(),
        "--workdir".to_string(),
        workdir.display().to_string(),
        "--core".to_string(),
        core.display().to_string(),
    ];
    elevate(&args)?;
    Ok(OperationResult::ok(
        "已请求以管理员身份启动 helper，请在弹出的系统授权窗口中确认",
    ))
}

/// 以管理员权限执行命令（macOS: osascript；Linux: pkexec；Windows: PowerShell UAC）。
pub fn elevate(args: &[String]) -> CmdResult<()> {
    if args.is_empty() {
        return Err(CommandError::new("helper", "空命令"));
    }
    #[cfg(target_os = "macos")]
    {
        let script = shell_join(args);
        let escaped = script.replace('\\', "\\\\").replace('"', "\\\"");
        let apple_script = format!(
            "do shell script \"{escaped}\" with prompt \"LabVPN 需要管理员权限来启用 TUN 模式\" with administrator privileges"
        );
        let output = crate::platform::hidden_command("osascript")
            .args(["-e", &apple_script])
            .output()
            .map_err(|err| CommandError::new("helper", format!("调用 osascript 失败: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.contains("User canceled") || stderr.contains("-128") {
                return Err(CommandError::new("helper", "用户取消了管理员授权"));
            }
            return Err(CommandError::new(
                "helper",
                format!("提权执行失败: {stderr}"),
            ));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let pkexec_available = crate::services::core_bin::command_exists("pkexec");
        let output = if pkexec_available {
            let mut command_args = vec!["--disable-internal-agent".to_string()];
            command_args.extend(args.iter().cloned());
            crate::platform::hidden_command("pkexec")
                .args(&command_args)
                .output()
        } else {
            let mut command_args = args.to_vec();
            command_args.insert(0, "-n".to_string());
            crate::platform::hidden_command("sudo")
                .args(&command_args)
                .output()
        }
        .map_err(|err| CommandError::new("helper", format!("提权执行失败: {err}")))?;
        if !output.status.success() {
            return Err(CommandError::new(
                "helper",
                format!(
                    "提权执行失败（请确保 pkexec 或 sudo 可用）：{}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        // PowerShell Start-Process -Verb RunAs，参数逐项转义
        let mut escaped_args: Vec<String> = Vec::new();
        let program = args[0].clone();
        for arg in &args[1..] {
            escaped_args.push(format!("'{}'", arg.replace('\'', "''")));
        }
        let command_line = if escaped_args.is_empty() {
            String::new()
        } else {
            format!("-ArgumentList {}", escaped_args.join(","))
        };
        let script = format!(
            "Start-Process -FilePath '{}' {} -Verb RunAs",
            program.replace('\'', "''"),
            command_line
        );
        let output = crate::platform::hidden_command("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .map_err(|err| CommandError::new("helper", format!("调用 PowerShell 失败: {err}")))?;
        if !output.status.success() {
            return Err(CommandError::new(
                "helper",
                format!(
                    "提权启动失败：{}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = args;
        Err(CommandError::new("helper", "当前平台不支持提权"))
    }
}

/// 用单引号转义并拼接命令（用于 osascript，仅 macOS 使用）。
#[cfg(target_os = "macos")]
fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_./:=@".contains(c))
            {
                arg.clone()
            } else {
                format!("'{}'", arg.replace('\'', "'\"'\"'"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 默认 IPC 端口。
pub fn default_port() -> u16 {
    DEFAULT_PORT
}
