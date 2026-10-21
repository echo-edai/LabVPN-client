//! 应用数据路径管理（三平台统一布局）。

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::error::{CmdResult, CommandError};

/// 应用全部运行时路径。
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// App 数据根目录
    pub data_dir: PathBuf,
    /// 配置 JSON
    pub config_file: PathBuf,
    /// 密钥库（加密）
    pub secrets_file: PathBuf,
    /// helper 令牌
    pub token_file: PathBuf,
    /// 直接模式下的 mihomo 工作目录
    pub core_dir: PathBuf,
    /// 生成的 mihomo 配置
    pub core_config: PathBuf,
    /// helper 模式下的工作目录（root 可写）
    pub helper_workdir: PathBuf,
    /// 规则文件目录（直接模式）
    pub rules_dir: PathBuf,
    /// 订阅缓存目录
    pub profiles_dir: PathBuf,
    /// 订阅缓存文件
    pub profile_file: PathBuf,
    /// fdu-connect 配置目录
    pub vpn_dir: PathBuf,
    /// fdu-connect config.toml
    pub vpn_config_file: PathBuf,
    /// 日志目录
    pub logs_dir: PathBuf,
    /// 应用日志文件
    pub app_log_file: PathBuf,
    /// 应用自带/下载的二进制目录
    pub bin_dir: PathBuf,
}

impl AppPaths {
    /// 基于 Tauri 应用数据目录初始化并创建所需目录。
    pub fn init(app: &AppHandle) -> CmdResult<Self> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|err| CommandError::new("paths", format!("无法定位应用数据目录: {err}")))?;
        let paths = Self {
            config_file: data_dir.join("config.json"),
            secrets_file: data_dir.join("secrets.enc"),
            token_file: data_dir.join("helper.token"),
            core_dir: data_dir.join("core"),
            core_config: data_dir.join("core").join("config.yaml"),
            helper_workdir: data_dir.join("helper"),
            rules_dir: data_dir.join("core").join("rules"),
            profiles_dir: data_dir.join("profiles"),
            profile_file: data_dir.join("profiles").join("profile.yaml"),
            vpn_dir: data_dir.join("vpn"),
            vpn_config_file: data_dir.join("vpn").join("config.toml"),
            logs_dir: data_dir.join("logs"),
            app_log_file: data_dir.join("logs").join("app.log"),
            bin_dir: data_dir.join("bin"),
            data_dir,
        };
        for dir in [
            &paths.data_dir,
            &paths.core_dir,
            &paths.helper_workdir,
            &paths.rules_dir,
            &paths.profiles_dir,
            &paths.vpn_dir,
            &paths.logs_dir,
            &paths.bin_dir,
        ] {
            std::fs::create_dir_all(dir)
                .map_err(|err| CommandError::new("paths", format!("创建目录失败: {err}")))?;
        }
        Ok(paths)
    }

    /// 路径信息（展示在设置页/诊断包）。
    pub fn info(&self) -> crate::dto::PathsInfo {
        crate::dto::PathsInfo {
            data_dir: self.data_dir.display().to_string(),
            config_file: self.config_file.display().to_string(),
            core_dir: self.core_dir.display().to_string(),
            helper_workdir: self.helper_workdir.display().to_string(),
            logs_dir: self.logs_dir.display().to_string(),
            bin_dir: self.bin_dir.display().to_string(),
        }
    }
}
