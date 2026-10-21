//! 配置持久化与信封组装。

use std::collections::BTreeMap;

use labvpn_core::config::AppConfig;

use crate::dto::{ConfigEnvelope, SecretUpdates};
use crate::error::{CmdResult, CommandError};
use crate::paths::AppPaths;
use crate::services::secrets::SecretStore;

/// 读取配置；损坏时备份并回退默认配置。
pub fn load(paths: &AppPaths) -> (AppConfig, Vec<String>) {
    let mut warnings = Vec::new();
    if !paths.config_file.exists() {
        return (AppConfig::default(), warnings);
    }
    let raw = match std::fs::read_to_string(&paths.config_file) {
        Ok(raw) => raw,
        Err(err) => {
            warnings.push(format!("读取配置失败：{err}"));
            return (AppConfig::default(), warnings);
        }
    };
    if raw.trim().is_empty() {
        return (AppConfig::default(), warnings);
    }
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(value) => match AppConfig::migrate_json(value) {
            Ok(config) => (config, warnings),
            Err(err) => {
                warnings.push(format!("配置迁移失败：{err}"));
                backup_corrupt(paths);
                (AppConfig::default(), warnings)
            }
        },
        Err(err) => {
            warnings.push(format!("配置 JSON 解析失败：{err}"));
            backup_corrupt(paths);
            (AppConfig::default(), warnings)
        }
    }
}

fn backup_corrupt(paths: &AppPaths) {
    let backup = paths.config_file.with_extension("json.bak");
    let _ = std::fs::rename(&paths.config_file, &backup);
    log::warn!("已将损坏配置备份到 {}", backup.display());
}

/// 原子写入配置（0600）。
pub fn save(paths: &AppPaths, config: &AppConfig) -> CmdResult<()> {
    let serialized = serde_json::to_string_pretty(config)?;
    let tmp = paths.config_file.with_extension("json.tmp");
    std::fs::write(&tmp, serialized)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &paths.config_file)?;
    Ok(())
}

/// 解析导入的配置 JSON。
pub fn parse_import(json: &str) -> CmdResult<AppConfig> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|err| CommandError::new("config", format!("配置 JSON 解析失败: {err}")))?;
    Ok(AppConfig::migrate_json(value)?)
}

/// 组装配置信封（含密钥存在状态与校验结果）。
pub fn envelope(paths: &AppPaths, config: &AppConfig, secrets: &SecretStore) -> ConfigEnvelope {
    let mut secrets_present = BTreeMap::new();
    for key in referenced_secret_keys(config) {
        secrets_present.insert(key.clone(), secrets.contains(&key));
    }
    ConfigEnvelope {
        config: config.clone(),
        secrets_present,
        validation: config.validate(),
        paths: paths.info(),
    }
}

/// 配置中引用到的全部密钥键。
pub fn referenced_secret_keys(config: &AppConfig) -> Vec<String> {
    let mut keys = vec![config.engine.controller_secret_ref.clone()];
    if let Some(key) = &config.campus_vpn.password_ref {
        keys.push(key.clone());
    }
    if let Some(key) = &config.campus_vpn.totp_secret_ref {
        keys.push(key.clone());
    }
    if let Some(key) = &config.ssh_proxy.password_ref {
        keys.push(key.clone());
    }
    if let Some(key) = &config.ssh_proxy.private_key_passphrase_ref {
        keys.push(key.clone());
    }
    keys.sort();
    keys.dedup();
    keys
}

/// 应用密钥更新。
pub fn apply_secret_updates(secrets: &SecretStore, updates: SecretUpdates) -> CmdResult<()> {
    if updates.is_empty() {
        return Ok(());
    }
    secrets.set_many(updates)
}

/// 确保控制端口 secret 存在（不存在则生成并保存）。
pub fn ensure_controller_secret(secrets: &SecretStore, config: &AppConfig) -> CmdResult<String> {
    let key = config.engine.controller_secret_ref.trim();
    if key.is_empty() {
        return Err(CommandError::new("config", "控制端口密钥引用为空"));
    }
    secrets.get_or_create(key, 24)
}

/// 供测试/诊断使用：把配置序列化为 JSON。
pub fn export_json(config: &AppConfig) -> CmdResult<String> {
    Ok(serde_json::to_string_pretty(config)?)
}

