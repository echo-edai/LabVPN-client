//! 平台服务安装/卸载（macOS launchd、Linux systemd、Windows 提权进程模式）。

use std::path::{Path, PathBuf};

use labvpn_core::ipc::SERVICE_LABEL;

use crate::logging;
use crate::Options;

/// 安装目录（macOS/Linux）。
#[cfg(any(target_os = "macos", target_os = "linux"))]
const INSTALL_DIR: &str = "/usr/local/libexec/labvpn";

/// 安装为系统服务（需要管理员权限）。
pub fn install(options: &Options) -> Result<(), String> {
    if !is_elevated() {
        return Err("需要管理员权限：请使用 sudo 或在管理员终端中执行 install".to_string());
    }
    platform_install(options)
}

/// 卸载系统服务。
pub fn uninstall(options: &Options) -> Result<(), String> {
    if !is_elevated() {
        return Err("需要管理员权限：请使用 sudo 或在管理员终端中执行 uninstall".to_string());
    }
    platform_uninstall(options)
}

fn is_elevated() -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(windows)]
    {
        // Windows 不安装服务，直接返回 true 以走提示分支
        true
    }
}

#[cfg(target_os = "macos")]
const PLIST_PATH: &str = "/Library/LaunchDaemons/app.labvpn.helper.plist";

#[cfg(target_os = "macos")]
fn platform_install(options: &Options) -> Result<(), String> {
    let install_dir = Path::new(INSTALL_DIR);
    let (helper, core) = copy_binaries(install_dir, options)?;
    std::fs::create_dir_all(&options.workdir)
        .map_err(|err| format!("创建工作目录失败: {err}"))?;
    let plist = macos_plist(&helper, &core, options)?;
    std::fs::write(PLIST_PATH, plist).map_err(|err| format!("写入 LaunchDaemon 失败: {err}"))?;
    let label = format!("system/{SERVICE_LABEL}");
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &label])
        .output();
    let output = std::process::Command::new("launchctl")
        .args(["bootstrap", "system", PLIST_PATH])
        .output()
        .map_err(|err| format!("launchctl 执行失败: {err}"))?;
    if !output.status.success() {
        // 兼容老系统
        let fallback = std::process::Command::new("launchctl")
            .args(["load", "-w", PLIST_PATH])
            .output()
            .map_err(|err| format!("launchctl load 失败: {err}"))?;
        if !fallback.status.success() {
            return Err(format!(
                "launchctl 启动失败: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    logging::info(format!(
        "已安装 LaunchDaemon：{PLIST_PATH}，helper={} node={}",
        helper.display(),
        core.display()
    ));
    println!("✅ LabVPN helper 已安装并启动");
    println!("   LaunchDaemon: {PLIST_PATH}");
    println!("   helper: {}", helper.display());
    println!("   核心: {}", core.display());
    Ok(())
}

#[cfg(target_os = "macos")]
fn platform_uninstall(_options: &Options) -> Result<(), String> {
    let label = format!("system/{SERVICE_LABEL}");
    let _ = std::process::Command::new("launchctl")
        .args(["bootout", &label])
        .output();
    let _ = std::fs::remove_file(PLIST_PATH);
    let _ = std::fs::remove_dir_all(INSTALL_DIR);
    println!("✅ LabVPN helper 已卸载（工作目录与日志保留）");
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_plist(helper: &Path, core: &Path, options: &Options) -> Result<String, String> {
    let workdir = options.workdir.display();
    let log = options.workdir.join("launchd.log");
    let err_log = options.workdir.join("launchd.err.log");
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{SERVICE_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{helper}</string>
        <string>serve</string>
        <string>--port</string>
        <string>{port}</string>
        <string>--token-file</string>
        <string>{token}</string>
        <string>--workdir</string>
        <string>{workdir}</string>
        <string>--core</string>
        <string>{core}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{err_log}</string>
</dict>
</plist>
"#,
        helper = helper.display(),
        core = core.display(),
        token = options.token_file.display(),
        port = options.port,
        workdir = workdir,
        log = log.display(),
        err_log = err_log.display(),
    ))
}

#[cfg(target_os = "linux")]
const UNIT_PATH: &str = "/etc/systemd/system/labvpn-helper.service";

#[cfg(target_os = "linux")]
fn platform_install(options: &Options) -> Result<(), String> {
    let install_dir = Path::new(INSTALL_DIR);
    let (helper, core) = copy_binaries(install_dir, options)?;
    std::fs::create_dir_all(&options.workdir)
        .map_err(|err| format!("创建工作目录失败: {err}"))?;
    let unit = format!(
        r#"[Unit]
Description=LabVPN privileged helper (TUN mode)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={helper} serve --port {port} --token-file {token} --workdir {workdir} --core {core}
Restart=on-failure
RestartSec=2
NoNewPrivileges=false

[Install]
WantedBy=multi-user.target
"#,
        helper = helper.display(),
        core = core.display(),
        token = options.token_file.display(),
        port = options.port,
        workdir = options.workdir.display(),
    );
    std::fs::write(UNIT_PATH, unit).map_err(|err| format!("写入 systemd unit 失败: {err}"))?;
    run_command("systemctl", &["daemon-reload"])?;
    run_command("systemctl", &["enable", "labvpn-helper"])?;
    run_command("systemctl", &["restart", "labvpn-helper"])?;
    println!("✅ LabVPN helper 已安装并启动（systemd）");
    println!("   服务: labvpn-helper.service");
    println!("   helper: {}", helper.display());
    println!("   核心: {}", core.display());
    println!("   查看日志: journalctl -u labvpn-helper -f");
    Ok(())
}

#[cfg(target_os = "linux")]
fn platform_uninstall(_options: &Options) -> Result<(), String> {
    let _ = run_command("systemctl", &["disable", "--now", "labvpn-helper"]);
    let _ = std::fs::remove_file(UNIT_PATH);
    let _ = run_command("systemctl", &["daemon-reload"]);
    let _ = std::fs::remove_dir_all(INSTALL_DIR);
    println!("✅ LabVPN helper 已卸载（工作目录与日志保留）");
    Ok(())
}

#[cfg(windows)]
fn platform_install(_options: &Options) -> Result<(), String> {
    println!("Windows 不安装系统服务。");
    println!("LabVPN 会在启用 TUN 模式时以管理员身份（UAC）启动 helper：");
    println!("  labvpn-helper.exe serve --port <port> --token-file <path> --workdir <path> --core <path>");
    println!("此方式下无需安装任何服务，退出 App 时 helper 自动关闭。");
    Ok(())
}

#[cfg(windows)]
fn platform_uninstall(_options: &Options) -> Result<(), String> {
    println!("Windows 无需卸载服务。");
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn copy_binaries(install_dir: &Path, options: &Options) -> Result<(PathBuf, PathBuf), String> {
    std::fs::create_dir_all(install_dir).map_err(|err| format!("创建安装目录失败: {err}"))?;
    let current = std::env::current_exe().map_err(|err| format!("无法定位 helper: {err}"))?;
    let helper_target = install_dir.join("labvpn-helper");
    copy_executable(&current, &helper_target)?;

    let core_source = match &options.core {
        Some(core) => core.clone(),
        None => default_core_path()?,
    };
    if !core_source.exists() {
        return Err(format!("找不到核心二进制: {}", core_source.display()));
    }
    let core_target = install_dir.join("mihomo");
    copy_executable(&core_source, &core_target)?;
    Ok((helper_target, core_target))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn copy_executable(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::copy(source, target).map_err(|err| {
        format!(
            "复制 {} 到 {} 失败: {err}",
            source.display(),
            target.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755))
            .map_err(|err| format!("设置可执行权限失败: {err}"))?;
    }
    Ok(())
}

/// 默认核心路径：helper 同目录下的 `mihomo`。
pub fn default_core_path() -> Result<PathBuf, String> {
    let current = std::env::current_exe().map_err(|err| format!("无法定位 helper: {err}"))?;
    let name = if cfg!(windows) { "mihomo.exe" } else { "mihomo" };
    current
        .parent()
        .map(|parent| parent.join(name))
        .ok_or_else(|| "无法定位核心二进制".to_string())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run_command(program: &str, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("执行 {program} 失败: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{} {} 失败: {}",
            program,
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}
