//! 全局应用状态。

use std::sync::{Arc, RwLock};
use std::time::Instant;

use labvpn_core::config::AppConfig;
use labvpn_core::probe::CampusStatus;
use tauri::AppHandle;

use crate::dto::{ConfigEnvelope, CoreInfo};
use crate::error::{CmdResult, CommandError};
use crate::paths::{AppPaths, };
use crate::services::config as config_service;
use crate::services::core_bin;
use crate::services::engine::EngineManager;
use crate::services::logs::LogStore;
use crate::services::secrets::SecretStore;
use crate::services::vpn::VpnManager;

/// 校园网检测缓存。
pub struct CampusState {
    /// 最近一次检测结果
    pub last: Option<CampusStatus>,
}

/// 应用全局状态（Tauri 管理）。
pub struct AppState {
    /// AppHandle
    pub app: AppHandle,
    /// 路径
    pub paths: AppPaths,
    /// 配置
    pub config: RwLock<AppConfig>,
    /// 密钥库
    pub secrets: SecretStore,
    /// 日志
    pub logs: Arc<LogStore>,
    /// 引擎
    pub engine: EngineManager,
    /// 校内 VPN
    pub vpn: VpnManager,
    /// 校园网检测
    pub campus: tokio::sync::Mutex<CampusState>,
    /// 校园网手动覆盖
    pub campus_override: RwLock<Option<bool>>,
    /// HTTP 客户端（订阅）
    pub http: reqwest::Client,
    /// 核心信息
    pub core: RwLock<CoreInfo>,
    /// 启动时间
    pub started_at: Instant,
}

impl AppState {
    /// 初始化状态（加载配置、密钥、解析核心）。
    pub fn new(app: &AppHandle) -> CmdResult<Self> {
        let paths = AppPaths::init(app)?;
        let logs = Arc::new(LogStore::new(&paths.app_log_file));
        let (config, warnings) = config_service::load(&paths);
        for warning in &warnings {
            logs.warn("config", warning.clone());
        }
        let secrets = SecretStore::load(paths.secrets_file.clone());
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|err| CommandError::new("http", format!("初始化 HTTP 客户端失败: {err}")))?;
        let engine = EngineManager::new(
            app.clone(),
            Arc::clone(&logs),
            config.engine.mixed_port,
            config.engine.controller_port,
        );
        let vpn = VpnManager::new(app.clone(), Arc::clone(&logs));
        let resolved = core_bin::resolve_core(app, &paths);
        let mut core_info = core_bin::core_info(&resolved);
        if let Some(path) = resolved.path.as_deref() {
            core_info.version = core_bin::core_version(path);
        }
        logs.info(
            "app",
            format!(
                "LabVPN 启动：配置版本 {}，核心 {}（{}）",
                config.version,
                core_info.version.clone().unwrap_or_else(|| "未找到".to_string()),
                core_info.source
            ),
        );
        Ok(Self {
            app: app.clone(),
            paths,
            config: RwLock::new(config),
            secrets,
            logs,
            engine,
            vpn,
            campus: tokio::sync::Mutex::new(CampusState { last: None }),
            campus_override: RwLock::new(None),
            http,
            core: RwLock::new(core_info),
            started_at: Instant::now(),
        })
    }

    /// 配置快照（克隆）。
    pub fn config_snapshot(&self) -> AppConfig {
        self.config
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// 保存配置（内存 + 磁盘）。
    pub fn save_config(&self, config: AppConfig) -> CmdResult<()> {
        config_service::save(&self.paths, &config)?;
        if let Ok(mut guard) = self.config.write() {
            *guard = config;
        }
        Ok(())
    }

    /// 配置信封。
    pub fn config_envelope(&self) -> ConfigEnvelope {
        let config = self.config_snapshot();
        config_service::envelope(&self.paths, &config, &self.secrets)
    }

    /// 校园网手动覆盖。
    pub fn campus_override(&self) -> Option<bool> {
        self.campus_override
            .read()
            .map(|guard| *guard)
            .unwrap_or(None)
    }

    /// 设置校园网手动覆盖。
    pub fn set_campus_override(&self, value: Option<bool>) {
        if let Ok(mut guard) = self.campus_override.write() {
            *guard = value;
        }
    }

    /// 当前核心信息。
    pub fn core_info(&self) -> CoreInfo {
        self.core
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// 重新解析核心二进制。
    pub fn refresh_core_info(&self) -> CoreInfo {
        let resolved = core_bin::resolve_core(&self.app, &self.paths);
        let mut info = core_bin::core_info(&resolved);
        if let Some(path) = resolved.path.as_deref() {
            info.version = core_bin::core_version(path);
        }
        if let Ok(mut guard) = self.core.write() {
            *guard = info.clone();
        }
        info
    }

    /// 最近一次校园网检测。
    pub async fn last_campus(&self) -> Option<CampusStatus> {
        self.campus.lock().await.last.clone()
    }

    /// 更新校园网检测缓存（不广播）。
    pub async fn set_last_campus(&self, status: CampusStatus) {
        self.campus.lock().await.last = Some(status);
    }

    /// 运行时长（秒）。
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}
