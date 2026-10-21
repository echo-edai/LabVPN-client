//! 本机加密密钥库（AES-256-GCM + 机器绑定密钥）。
//!
//! 说明：这是 Keychain/Credential Manager/Secret Service 之外的**兜底方案**，
//! 三者不可用或实现成本高的场景下保证密码不落明文，文件权限 0600。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{CmdResult, CommandError};

const KEY_CONTEXT: &str = "labvpn-secrets-v1";
/// 密钥库文件版本
const FILE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct SecretFile {
    version: u32,
    nonce: String,
    data: String,
}

/// 加密密钥库。
pub struct SecretStore {
    path: PathBuf,
    key: [u8; 32],
    cache: Mutex<BTreeMap<String, String>>,
}

impl SecretStore {
    /// 加载（不存在或损坏时以空库启动；损坏文件会备份为 `.bak`）。
    pub fn load(path: PathBuf) -> Self {
        let key = derive_key();
        let cache = match read_file(&path, &key) {
            Ok(map) => map,
            Err(message) => {
                if path.exists() {
                    log::warn!("密钥库读取失败（{message}），已备份并重新开始");
                    let backup = path.with_extension("enc.bak");
                    let _ = std::fs::rename(&path, backup);
                }
                BTreeMap::new()
            }
        };
        Self {
            path,
            key,
            cache: Mutex::new(cache),
        }
    }

    /// 读取密钥。
    pub fn get(&self, key: &str) -> Option<String> {
        self.cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(key).cloned())
    }

    /// 写入单条密钥并落盘。
    pub fn set(&self, key: &str, value: &str) -> CmdResult<()> {
        let mut updates = BTreeMap::new();
        updates.insert(key.to_string(), value.to_string());
        self.set_many(updates)
    }

    /// 批量写入（空值表示删除）。
    pub fn set_many(&self, updates: BTreeMap<String, String>) -> CmdResult<()> {
        if updates.is_empty() {
            return Ok(());
        }
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| CommandError::new("secrets", "密钥库锁损坏"))?;
        for (key, value) in updates {
            if value.is_empty() {
                cache.remove(&key);
            } else {
                cache.insert(key, value);
            }
        }
        persist(&self.path, &self.key, &cache)
    }

    /// 某密钥是否存在。
    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// 若不存在则生成随机值并保存（用于控制端口 secret）。
    pub fn get_or_create(&self, key: &str, length: usize) -> CmdResult<String> {
        if let Some(value) = self.get(key) {
            if !value.is_empty() {
                return Ok(value);
            }
        }
        let value = random_hex(length.max(16));
        self.set(key, &value)?;
        Ok(value)
    }

}

/// 生成密码学随机十六进制串。
pub fn random_hex(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    if getrandom::fill(&mut buffer).is_err() {
        // 极端情况下兜底（不理想但不会 panic）
        let uuid = uuid::Uuid::new_v4();
        return uuid.simple().to_string();
    }
    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 生成随机 UUID v4 字符串（测试辅助）。
#[cfg(test)]
pub fn random_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn derive_key() -> [u8; 32] {
    let machine = machine_id();
    let mut hasher = Sha256::new();
    hasher.update(KEY_CONTEXT.as_bytes());
    hasher.update(machine.as_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn machine_id() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(index) = text.find("IOPlatformUUID") {
                let rest = &text[index..];
                if let Some(uuid) = rest.split('"').nth(3) {
                    return uuid.trim().to_string();
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
            if let Ok(content) = std::fs::read_to_string(path) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    #[cfg(windows)]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args([
                "query",
                r"HKLM\SOFTWARE\Microsoft\Cryptography",
                "/v",
                "MachineGuid",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some(last) = text.split_whitespace().last() {
                if last.len() > 8 {
                    return last.to_string();
                }
            }
        }
    }
    // 兜底：主机名 + 用户名（仍能保证不同机器密钥不同）
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "labvpn-host".to_string());
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "labvpn-user".to_string());
    format!("{host}-{user}")
}

fn read_file(path: &Path, key: &[u8; 32]) -> Result<BTreeMap<String, String>, String> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(path).map_err(|err| format!("读取失败: {err}"))?;
    if raw.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let file: SecretFile =
        serde_json::from_str(&raw).map_err(|err| format!("格式非法: {err}"))?;
    if file.version != FILE_VERSION {
        return Err(format!("版本不支持: {}", file.version));
    }
    let engine = base64::engine::general_purpose::STANDARD;
    let nonce = engine
        .decode(file.nonce.as_bytes())
        .map_err(|err| format!("nonce 解码失败: {err}"))?;
    if nonce.len() != 12 {
        return Err("nonce 长度非法".to_string());
    }
    let data = engine
        .decode(file.data.as_bytes())
        .map_err(|err| format!("数据解码失败: {err}"))?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|err| format!("密钥非法: {err}"))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), data.as_ref())
        .map_err(|_| "解密失败（密钥不匹配或文件被篡改）".to_string())?;
    serde_json::from_slice(&plaintext).map_err(|err| format!("内容解析失败: {err}"))
}

fn persist(path: &Path, key: &[u8; 32], cache: &BTreeMap<String, String>) -> CmdResult<()> {
    let plaintext = serde_json::to_vec(cache)?;
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|err| CommandError::new("secrets", format!("密钥非法: {err}")))?;
    let mut nonce_bytes = [0u8; 12];
    getrandom::fill(&mut nonce_bytes)
        .map_err(|err| CommandError::new("secrets", format!("随机数生成失败: {err}")))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_ref())
        .map_err(|_| CommandError::new("secrets", "加密失败"))?;
    let engine = base64::engine::general_purpose::STANDARD;
    let file = SecretFile {
        version: FILE_VERSION,
        nonce: engine.encode(nonce_bytes),
        data: engine.encode(ciphertext),
    };
    let serialized = serde_json::to_string_pretty(&file)?;
    let tmp = path.with_extension("enc.tmp");
    std::fs::write(&tmp, serialized)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypted_store() {
        let dir = std::env::temp_dir().join(format!("labvpn-secrets-test-{}", random_uuid()));
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("secrets.enc");
        let store = SecretStore::load(path.clone());
        store.set("ssh.password", "s3cr3t").expect("set");
        assert!(store.contains("ssh.password"));
        drop(store);
        let reloaded = SecretStore::load(path.clone());
        assert_eq!(reloaded.get("ssh.password").as_deref(), Some("s3cr3t"));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn generates_random_hex() {
        let first = random_hex(16);
        let second = random_hex(16);
        assert_eq!(first.len(), 32);
        assert_ne!(first, second);
    }
}
