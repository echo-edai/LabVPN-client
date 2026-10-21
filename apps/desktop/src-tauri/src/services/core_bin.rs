//! sidecar 二进制定位（mihomo 核心 / labvpn-helper）。

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::dto::CoreInfo;
use crate::paths::AppPaths;

/// 解析结果。
pub struct ResolvedBinary {
    /// 路径（None 表示未找到）
    pub path: Option<PathBuf>,
    /// 来源标识
    pub source: String,
}

fn is_usable(path: &Path) -> bool {
    path.is_file()
        && std::fs::metadata(path)
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
}

fn first_usable(candidates: Vec<(PathBuf, &'static str)>) -> ResolvedBinary {
    for (path, source) in candidates {
        if is_usable(&path) {
            return ResolvedBinary {
                path: Some(path),
                source: source.to_string(),
            };
        }
    }
    ResolvedBinary {
        path: None,
        source: "missing".to_string(),
    }
}

/// 资源目录下的文件。
fn resource_file(app: &AppHandle, relative: &str) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?;
    let candidate = dir.join(relative);
    is_usable(&candidate).then_some(candidate)
}

/// 开发模式二进制目录（`apps/desktop/src-tauri/binaries`）。
pub fn dev_binaries_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries")
}

/// 二进制文件名（按平台）。
pub fn core_binary_name() -> &'static str {
    if cfg!(windows) {
        "mihomo.exe"
    } else {
        "mihomo"
    }
}

/// helper 二进制文件名。
pub fn helper_binary_name() -> &'static str {
    if cfg!(windows) {
        "labvpn-helper.exe"
    } else {
        "labvpn-helper"
    }
}

/// 在 PATH 中查找可执行文件（仅部分平台路径使用）。
#[allow(dead_code)]
fn path_search(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if is_usable(&candidate) && is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(unix)]
#[allow(dead_code)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}

/// 解析 mihomo 核心路径。
///
/// 优先级：App 资源 → 数据目录 bin → 开发目录 → PATH。
pub fn resolve_core(app: &AppHandle, paths: &AppPaths) -> ResolvedBinary {
    let name = core_binary_name();
    let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(path) = resource_file(app, &format!("bin/{name}")) {
        candidates.push((path, "resource"));
    }
    candidates.push((paths.bin_dir.join(name), "appData"));
    candidates.push((dev_binaries_dir().join(name), "dev"));
    candidates.push((dev_binaries_dir().join(format!("mihomo-{}-{}", std::env::consts::OS, std::env::consts::ARCH)), "dev"));
    first_usable(candidates)
}

/// 解析 helper 路径。
pub fn resolve_helper(app: &AppHandle, paths: &AppPaths) -> Option<PathBuf> {
    let name = helper_binary_name();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = resource_file(app, &format!("bin/{name}")) {
        candidates.push(path);
    }
    candidates.push(paths.bin_dir.join(name));
    candidates.push(dev_binaries_dir().join(name));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release").join(name));
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug").join(name));
    candidates
        .into_iter()
        .find(|candidate| is_usable(candidate))
        .or_else(|| {
            // 已安装到系统的 helper（macOS/Linux）
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                let installed = PathBuf::from("/usr/local/libexec/labvpn").join(name);
                if is_usable(&installed) {
                    return Some(installed);
                }
            }
            None
        })
}

/// 读取核心版本（执行 `mihomo -v`）。
pub fn core_version(core_path: &Path) -> Option<String> {
    let output = std::process::Command::new(core_path)
        .arg("-v")
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

/// 组装 CoreInfo（不做运行时探测，版本由调用方补充）。
pub fn core_info(resolved: &ResolvedBinary) -> CoreInfo {
    CoreInfo {
        binary_path: resolved
            .path
            .as_ref()
            .map(|path| path.display().to_string()),
        version: None,
        source: resolved.source.clone(),
    }
}

/// PATH 中是否存在某命令。
#[allow(dead_code)]
pub fn command_exists(command: &str) -> bool {
    path_search(command).is_some()
}
