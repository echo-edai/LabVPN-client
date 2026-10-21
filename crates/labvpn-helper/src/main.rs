//! # labvpn-helper
//!
//! LabVPN 特权 helper：以 root（macOS/Linux）或管理员（Windows）身份托管 mihomo 核心，
//! 用于 TUN 模式。仅监听回环地址，所有请求必须携带令牌。
//!
//! 用法：
//! - `labvpn-helper serve --port 38471 --token-file <path> --workdir <dir> [--core <path>]`
//! - `labvpn-helper install --token-file <path> --workdir <dir> [--port 38471] [--core <path>]`（需 sudo）
//! - `labvpn-helper uninstall`（需 sudo）
//! - `labvpn-helper version`

mod logging;
mod server;
mod service;
mod supervisor;

use std::path::PathBuf;
use std::sync::Arc;

use labvpn_core::ipc::DEFAULT_PORT;

/// 运行参数。
pub struct Options {
    /// IPC 端口
    pub port: u16,
    /// 令牌文件路径（App 写入，权限 0600）
    pub token_file: PathBuf,
    /// 工作目录（配置、规则、日志）
    pub workdir: PathBuf,
    /// 核心二进制路径（默认 helper 同目录）
    pub core: Option<PathBuf>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("serve") | None => serve(&args[1..]),
        Some("install") => install(&args[1..]),
        Some("uninstall") => uninstall(&args[1..]),
        Some("version") | Some("--version") | Some("-V") => {
            println!("labvpn-helper {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(format!("未知命令：{other}（使用 help 查看用法）")),
    };
    if let Err(err) = result {
        eprintln!("❌ 错误：{err}");
        std::process::exit(1);
    }
}

fn serve(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    let token = std::fs::read_to_string(&options.token_file)
        .map_err(|err| format!("读取令牌文件失败（{}）：{err}", options.token_file.display()))?
        .trim()
        .to_string();
    if token.len() < 16 {
        return Err("令牌无效（长度不足）".to_string());
    }
    std::fs::create_dir_all(&options.workdir)
        .map_err(|err| format!("创建工作目录失败：{err}"))?;
    logging::init(options.workdir.join("helper.log"));
    let core_path = match &options.core {
        Some(path) => path.clone(),
        None => service::default_core_path()?,
    };
    if !core_path.exists() {
        return Err(format!("核心二进制不存在：{}", core_path.display()));
    }
    logging::info(format!(
        "helper v{} 启动，核心：{}，工作目录：{}",
        env!("CARGO_PKG_VERSION"),
        core_path.display(),
        options.workdir.display()
    ));
    let supervisor = Arc::new(supervisor::Supervisor::new(core_path, options.workdir));
    server::run(options.port, token, supervisor).map_err(|err| format!("监听失败：{err}"))
}

fn install(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    service::install(&options)
}

fn uninstall(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;
    service::uninstall(&options)
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut port = DEFAULT_PORT;
    let mut token_file: Option<PathBuf> = None;
    let mut workdir: Option<PathBuf> = None;
    let mut core: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let next = |index: usize| -> Result<&String, String> {
            args.get(index + 1)
                .ok_or_else(|| format!("参数 {arg} 缺少取值"))
        };
        match arg {
            "--port" => {
                port = next(index)?
                    .parse()
                    .map_err(|_| "端口必须是数字".to_string())?;
                index += 2;
            }
            "--token-file" => {
                token_file = Some(PathBuf::from(next(index)?));
                index += 2;
            }
            "--workdir" => {
                workdir = Some(PathBuf::from(next(index)?));
                index += 2;
            }
            "--core" => {
                core = Some(PathBuf::from(next(index)?));
                index += 2;
            }
            other => return Err(format!("未知参数：{other}")),
        }
    }
    let token_file = token_file.ok_or_else(|| "缺少 --token-file".to_string())?;
    let workdir = workdir.unwrap_or_else(default_workdir);
    Ok(Options {
        port,
        token_file,
        workdir,
        core,
    })
}

fn default_workdir() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"C:\ProgramData\LabVPN")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/var/lib/labvpn")
    }
}

fn print_help() {
    println!(
        r#"labvpn-helper {} — LabVPN 特权 helper

用法：
  labvpn-helper serve [选项]       运行 helper（通常由系统服务启动）
  labvpn-helper install [选项]     安装为系统服务（需要 sudo / 管理员）
  labvpn-helper uninstall          卸载系统服务（需要 sudo / 管理员）
  labvpn-helper version            显示版本
  labvpn-helper help               显示帮助

选项：
  --port <port>          IPC 端口（默认 {DEFAULT_PORT}）
  --token-file <path>    令牌文件路径（必填）
  --workdir <dir>        工作目录（默认 /var/lib/labvpn 或 C:\ProgramData\LabVPN）
  --core <path>          mihomo 核心路径（默认 helper 同目录）

安全说明：
  - 只监听 127.0.0.1，所有请求必须携带令牌；
  - 只能执行固定的 start/stop/restart/status/ping/shutdown 命令；
  - 子进程路径固定为安装时确定的核心路径，请求无法指定任意可执行文件。
"#,
        env!("CARGO_PKG_VERSION")
    );
}
