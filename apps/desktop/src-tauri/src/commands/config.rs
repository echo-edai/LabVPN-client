//! 配置相关命令。

use tauri::State;

use labvpn_core::config::{AppConfig, ValidationIssue};
use labvpn_core::rules::custom::{self, parse_rule};
use labvpn_core::rules::matcher;
use labvpn_core::rules::names;
use labvpn_core::rules::presets::PresetInfo;

use crate::dto::{ConfigEnvelope, DiagnosticsResult, PreviewResult, RuleValidation, SecretUpdates};
use crate::error::{CmdResult, CommandError};
use crate::services::{config as config_service, orchestrator};
use crate::state::AppState;

/// 读取配置信封。
#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> ConfigEnvelope {
    state.config_envelope()
}

/// 保存配置（同时更新密钥）。
#[tauri::command]
pub fn save_config(
    state: State<'_, AppState>,
    mut config: AppConfig,
    secrets: SecretUpdates,
) -> CmdResult<ConfigEnvelope> {
    config_service::apply_secret_updates(&state.secrets, secrets)?;
    config.version = labvpn_core::config::CONFIG_VERSION;
    state.save_config(config)?;
    Ok(state.config_envelope())
}

/// 重置为默认配置（不清除密钥库，便于重新填写）。
#[tauri::command]
pub fn reset_config(state: State<'_, AppState>) -> CmdResult<ConfigEnvelope> {
    state.save_config(AppConfig::default())?;
    state.logs.warn("config", "配置已重置为默认值");
    Ok(state.config_envelope())
}

/// 导出配置 JSON（不含密钥明文）。
#[tauri::command]
pub fn export_config(state: State<'_, AppState>) -> CmdResult<String> {
    config_service::export_json(&state.config_snapshot())
}

/// 导入配置 JSON。
#[tauri::command]
pub fn import_config(
    state: State<'_, AppState>,
    json: String,
    secrets: SecretUpdates,
) -> CmdResult<ConfigEnvelope> {
    let config = config_service::parse_import(&json)?;
    config_service::apply_secret_updates(&state.secrets, secrets)?;
    state.save_config(config)?;
    state.logs.info("config", "配置已导入");
    Ok(state.config_envelope())
}

/// 校验给定配置。
#[tauri::command]
pub fn validate_config(config: AppConfig) -> Vec<ValidationIssue> {
    config.validate()
}

/// 生成配置预览。
#[tauri::command]
pub async fn preview_config(state: State<'_, AppState>) -> CmdResult<PreviewResult> {
    orchestrator::preview(state.inner()).await
}

/// 获取分流预设。
#[tauri::command]
pub fn get_presets() -> Vec<PresetInfo> {
    labvpn_core::rules::presets::all()
}

/// 校验一组自定义规则。
#[tauri::command]
pub fn validate_rules(rules: Vec<String>) -> RuleValidation {
    let known: Vec<String> = names::all_groups()
        .into_iter()
        .chain([
            names::DIRECT.to_string(),
            names::REJECT.to_string(),
            names::PROXY_FDU.to_string(),
        ])
        .collect();
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    let mut warnings = Vec::new();
    for line in rules {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match parse_rule(trimmed) {
            Ok(parsed) => {
                if parsed.kind == "MATCH" {
                    invalid.push(trimmed.to_string());
                    warnings.push("自定义规则不允许 MATCH（系统自动补全）".to_string());
                    continue;
                }
                if known.iter().any(|target| target == &parsed.target)
                    || parsed.target == names::DIRECT
                    || parsed.target == names::REJECT
                {
                    valid.push(parsed.to_rule_string());
                } else {
                    invalid.push(trimmed.to_string());
                    warnings.push(format!("出口「{}」不存在", parsed.target));
                }
            }
            Err(err) => {
                invalid.push(trimmed.to_string());
                warnings.push(err);
            }
        }
    }
    let _ = custom::normalize_rules(&valid, &known);
    RuleValidation {
        valid,
        invalid,
        warnings,
    }
}

/// 分流诊断：输入域名/IP，返回预计命中的规则与出口。
#[tauri::command]
pub async fn diagnose_target(
    state: State<'_, AppState>,
    input: String,
) -> CmdResult<DiagnosticsResult> {
    let target = input.trim();
    if target.is_empty() {
        return Err(CommandError::new("diagnose", "请输入域名或 IP"));
    }
    let plan = orchestrator::rule_plan(state.inner()).await?;
    let payloads = labvpn_core::rules::providers::builtin_payloads()?;
    let outcome = matcher::match_target(target, &plan, &payloads);
    Ok(DiagnosticsResult {
        input: outcome.input,
        matched: outcome.matched,
        rule: outcome.rule,
        outbound: outcome.outbound,
        notes: outcome.notes,
    })
}
