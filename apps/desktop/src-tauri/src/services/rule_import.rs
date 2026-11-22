//! 显式导入时取回公开文本；不复用订阅凭据，也不让 core 加载任意 URL。

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::{stream, StreamExt};
use labvpn_core::rules::import::{self, ImportError, Outlet, Preview, MAX_SOURCE_BYTES};
use reqwest::{Client, Url};

use crate::error::{CmdResult, CommandError};

const LAZY_GROUP: &str =
    "https://johnshall.github.io/Shadowrocket-ADBlock-Rules-Forever/lazy_group.conf";

/// `None` 表示公开预设；本地文件由 WebView 选取并传入文本。
pub async fn preview_file(
    local: Option<(String, String)>,
    default_outlet: Outlet,
    fetch_public: bool,
) -> CmdResult<Preview> {
    let remote = local.is_none();
    let client = if remote || fetch_public {
        Some(
            Client::builder()
                .timeout(Duration::from_secs(8))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent("LabVPN-rule-import")
                .build()
                .map_err(|_| load_error())?,
        )
    } else {
        None
    };
    let (name, text) = if let Some(local) = local {
        local
    } else {
        let client = client.as_ref().ok_or_else(load_error)?;
        let text = fetch(client, LAZY_GROUP).await?;
        ("lazy_group (Johnshall)".to_owned(), text)
    };
    let mut sets = BTreeMap::new();
    if let Some(client) = client.as_ref() {
        let refs = import::referenced_sets(&text).map_err(import_error)?;
        let mut fetched = stream::iter(refs.into_iter().filter(|url| public_set(url)).map(
            |url| async move {
                let result = fetch(client, &url).await;
                (url, result)
            },
        ))
        .buffer_unordered(8);
        let mut total = 0;
        while let Some((url, result)) = fetched.next().await {
            // 公共预设下载失败时不提供一个看似完整的档案。
            let content = match result {
                Ok(content) => content,
                Err(error) if remote => return Err(error),
                Err(_) => continue,
            };
            total += content.len();
            if total > MAX_SOURCE_BYTES {
                return Err(import_error(ImportError::TooLarge));
            }
            sets.insert(url, content);
        }
    }
    tokio::task::spawn_blocking(move || import::preview(&name, &text, default_outlet, &sets))
        .await
        .map_err(|_| import_error(ImportError::InvalidFormat))?
        .map_err(import_error)
}

fn public_set(input: &str) -> bool {
    let Ok(url) = Url::parse(input) else {
        return false;
    };
    if url.scheme() != "https"
        || url.host_str() != Some("raw.githubusercontent.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.as_str() != input
    {
        return false;
    }
    if url.path() == "/iab0x00/ProxyRules/main/Rule/AI.txt" {
        return true;
    }
    let Some(path) = url
        .path()
        .strip_prefix("/blackmatrix7/ios_rule_script/master/rule/")
    else {
        return false;
    };
    let parts: Vec<_> = path.split('/').collect();
    matches!(parts.as_slice(), ["Shadowrocket" | "QuantumultX", name, file]
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && file.strip_suffix(".list").is_some_and(|base| base == *name || base.strip_suffix("_Domain") == Some(*name)))
}

async fn fetch(client: &Client, url: &str) -> CmdResult<String> {
    let mut response = client.get(url).send().await.map_err(|_| load_error())?;
    if !response.status().is_success() {
        return Err(load_error());
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_SOURCE_BYTES as u64)
    {
        return Err(import_error(ImportError::TooLarge));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| load_error())? {
        if bytes.len() + chunk.len() > MAX_SOURCE_BYTES {
            return Err(import_error(ImportError::TooLarge));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| import_error(ImportError::InvalidFormat))
}

fn load_error() -> CommandError {
    CommandError::new(
        "rule_import_download",
        "无法取得完整的公开规则，请重试 / Could not download the public rule files",
    )
}

fn import_error(error: ImportError) -> CommandError {
    match error {
        ImportError::TooLarge => CommandError::new(
            "rule_import_too_large",
            "文件或档案超过导入上限 / Rule file or profile exceeds the import limit",
        ),
        ImportError::InvalidFormat => CommandError::new(
            "rule_import_format",
            "未找到有效的 UTF-8 分流文件 / No valid UTF-8 routing file found",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_accepts_the_documented_public_sources() {
        let base = "https://raw.githubusercontent.com/blackmatrix7/ios_rule_script/master/rule/Shadowrocket/Apple/Apple.list";
        assert!(public_set(base));
        assert!(public_set(
            "https://raw.githubusercontent.com/iab0x00/ProxyRules/main/Rule/AI.txt"
        ));
        assert!(!public_set(&format!("{base}?token=private")));
        assert!(!public_set(&base.replace("https://", "http://")));
        assert!(!public_set(
            &base.replace("Apple.list", "../Google/Google.list")
        ));
        assert!(!public_set(&base.replace(
            "githubusercontent.com",
            "githubusercontent.com.example.org"
        )));
        assert!(!public_set("https://127.0.0.1/rules.txt"));
        assert!(!public_set("file:///tmp/rules.txt"));
    }

    #[tokio::test]
    #[ignore = "需要公开规则服务；设置 LABVPN_RULE_IMPORT_SMOKE 指向输出目录"]
    async fn public_preset_smoke() {
        use labvpn_core::{config::AppConfig, generator, rules::matcher};

        let directory = std::path::PathBuf::from(
            std::env::var_os("LABVPN_RULE_IMPORT_SMOKE").expect("output directory"),
        );
        let result = preview_file(None, Outlet::Proxy, false)
            .await
            .expect("download public preset");
        assert!(result.set_count >= 25 && result.rule_count > 20_000);
        assert!(!result
            .skipped
            .iter()
            .any(|row| row.reason == import::Issue::RemoteSkipped));
        let mut config = AppConfig::default();
        config.routing.imported_rules = result.profile.map(std::sync::Arc::new);
        let generated = generator::generate(&config, &generator::RuntimeContext::default())
            .expect("produce YAML");
        let mut payloads = labvpn_core::rules::providers::builtin_payloads().unwrap();
        payloads.extend(
            generated
                .plan
                .providers
                .iter()
                .filter_map(|provider| provider.inline_payload()),
        );
        for (host, expected) in [
            ("www.youtube.com", labvpn_core::rules::names::GROUP_PROXY),
            ("www.bilibili.com", "DIRECT"),
            ("www.fudan.edu.cn", "DIRECT"),
        ] {
            let result = matcher::match_target(host, &generated.plan, &payloads);
            println!("{host} => {}: {}", result.outbound, result.rule);
            assert_eq!(result.outbound, expected);
        }
        std::fs::create_dir_all(directory.join("rules")).unwrap();
        for (name, body) in labvpn_core::rules::providers::builtin_rule_files() {
            std::fs::write(directory.join("rules").join(name), body).unwrap();
        }
        std::fs::write(directory.join("config.yaml"), generated.yaml).unwrap();
        println!(
            "{}/{} rules; skipped {:?}",
            result.rule_count, result.set_count, result.skipped
        );
    }
}
