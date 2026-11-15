//! 系统代理租约：先落盘，再接管；只恢复仍由本次会话持有的设置。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::platform;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySetting {
    pub scope: String,
    pub key: String,
    pub before: String,
    pub applied: String,
}

#[derive(Serialize, Deserialize)]
struct Lease {
    version: u8,
    settings: Vec<ProxySetting>,
}

fn path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("proxy-lease.json")
}

fn save(data_dir: &Path, lease: &Lease) -> Result<(), String> {
    use std::io::Write;
    let target = path(data_dir);
    let tmp = target.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // 上次写入中断时仅移除固定的临时文件，不跟随符号链接。
    if tmp.exists() || tmp.is_symlink() {
        std::fs::remove_file(&tmp).map_err(|err| err.to_string())?;
    }
    let mut file = options.open(&tmp).map_err(|err| err.to_string())?;
    file.write_all(&serde_json::to_vec(lease).map_err(|err| err.to_string())?)
        .and_then(|()| file.sync_all())
        .map_err(|err| err.to_string())?;
    std::fs::rename(tmp, target).map_err(|err| err.to_string())
}

/// 接管前必须完整记录原值；失败时按同一租约回滚。
pub fn acquire(data_dir: &Path, port: u16) -> Result<(), String> {
    restore(data_dir)?;
    let lease = Lease {
        version: 1,
        settings: platform::capture_proxy_settings(port)?,
    };
    if lease.settings.is_empty() {
        return Err("当前平台没有可安全接管的系统代理设置".to_string());
    }
    save(data_dir, &lease)?;
    for setting in lease
        .settings
        .iter()
        .filter(|s| !matches!(s.key.as_str(), "mode" | "ProxyEnable"))
        .chain(
            lease
                .settings
                .iter()
                .filter(|s| matches!(s.key.as_str(), "mode" | "ProxyEnable")),
        )
    {
        if let Err(err) = platform::write_proxy_setting(setting, &setting.applied) {
            let rollback = restore(data_dir).err().unwrap_or_default();
            return Err(format!("接管失败：{err}；{rollback}"));
        }
    }
    Ok(())
}

/// 断开、退出与异常退出后的下次启动共用；外部修改过的字段保持原样。
pub fn restore(data_dir: &Path) -> Result<(), String> {
    restore_with(
        data_dir,
        platform::read_proxy_setting,
        platform::write_proxy_setting,
    )
}

fn restore_with(
    data_dir: &Path,
    read: impl Fn(&ProxySetting) -> Result<String, String>,
    write: impl Fn(&ProxySetting, &str) -> Result<(), String>,
) -> Result<(), String> {
    let target = path(data_dir);
    if !target.exists() {
        return Ok(());
    }
    let bytes = std::fs::read(&target).map_err(|err| err.to_string())?;
    if bytes.len() > 512 * 1024 {
        return Err("代理恢复记录过大，已保留原文件".to_string());
    }
    let lease: Lease = serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
    if lease.version != 1 {
        return Err("代理恢复记录版本不兼容".to_string());
    }
    let mut pending = Vec::new();
    let mut errors = Vec::new();
    // 先恢复原代理端点，最后恢复 PAC / 代理模式。
    for setting in lease.settings.into_iter().rev() {
        match read(&setting) {
            Ok(current) if current == setting.applied => {
                if let Err(err) = write(&setting, &setting.before) {
                    errors.push(format!("{} / {}：{err}", setting.scope, setting.key));
                    pending.push(setting);
                }
            }
            Ok(current) => {
                if current != setting.before {
                    log::warn!(
                        "{} / {} 已被外部修改，保留当前值",
                        setting.scope,
                        setting.key
                    );
                }
            }
            Err(err) => {
                errors.push(format!("{} / {}：{err}", setting.scope, setting.key));
                pending.push(setting);
            }
        }
    }
    if pending.is_empty() {
        std::fs::remove_file(target).map_err(|err| err.to_string())?;
        Ok(())
    } else {
        pending.reverse();
        save(
            data_dir,
            &Lease {
                version: 1,
                settings: pending,
            },
        )?;
        Err(format!("部分设置尚未恢复，可重试：{}", errors.join("；")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "显式授权后验证真实系统代理接管与恢复，会短暂改变网络服务代理"]
    fn macos_proxy_roundtrip_on_explicit_request() {
        let dir = std::env::temp_dir().join(format!("labvpn-os-lease-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).expect("创建目录");
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = restore(&self.0);
            }
        }
        let cleanup = Cleanup(dir.clone());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("保留测试端口");
        let port = listener.local_addr().expect("本地地址").port();
        let before = platform::capture_proxy_settings(port).expect("捕获原值");
        acquire(&dir, port).expect("接管代理");
        for item in &before {
            assert_eq!(
                platform::read_proxy_setting(item).expect("读取接管值"),
                item.applied
            );
        }
        restore(&dir).expect("恢复代理");
        for item in &before {
            let after = platform::read_proxy_setting(item).expect("读取恢复值");
            let original: Result<serde_json::Value, _> = serde_json::from_str(&item.before);
            // networksetup 无法清空已禁用的空端点，只要求其继续保持关闭。
            if original
                .as_ref()
                .ok()
                .is_some_and(|v| v["enabled"] == false && v["port"] == 0)
            {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&after).expect("端点")["enabled"],
                    false
                );
            } else {
                assert_eq!(after, item.before);
            }
        }
        assert!(!path(&dir).exists());
        drop(cleanup);
        std::fs::remove_dir(dir).expect("清理目录");
    }

    #[test]
    fn restores_owned_values_preserves_external_edits_and_retries_errors() {
        let dir = std::env::temp_dir().join(format!("labvpn-lease-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).expect("创建临时目录");
        let settings = ["owned", "external", "failed"].map(|key| ProxySetting {
            scope: "test".into(),
            key: key.into(),
            before: "off".into(),
            applied: "on".into(),
        });
        save(
            &dir,
            &Lease {
                version: 1,
                settings: settings.to_vec(),
            },
        )
        .expect("保存租约");
        let written = Mutex::new(Vec::new());
        assert!(restore_with(
            &dir,
            |item| Ok(if item.key == "external" {
                "other"
            } else {
                "on"
            }
            .into()),
            |item, value| {
                if item.key == "failed" {
                    return Err("拒绝写入".into());
                }
                written
                    .lock()
                    .expect("锁")
                    .push((item.key.clone(), value.to_string()));
                Ok(())
            }
        )
        .is_err());
        assert_eq!(
            *written.lock().expect("锁"),
            vec![("owned".into(), "off".into())]
        );
        let pending: Lease =
            serde_json::from_slice(&std::fs::read(path(&dir)).expect("读取")).expect("解析");
        assert_eq!(pending.settings.len(), 1);
        assert_eq!(pending.settings[0].key, "failed");
        restore_with(&dir, |_| Ok("on".into()), |_, _| Ok(())).expect("重试成功");
        assert!(!path(&dir).exists());
        std::fs::remove_dir(dir).expect("清理");
    }
}
