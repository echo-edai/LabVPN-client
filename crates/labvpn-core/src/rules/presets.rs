//! 分流预设的元数据（前端通过命令获取，禁止在前端硬编码）。

use serde::{Deserialize, Serialize};

use crate::config::RoutingPreset;

/// 预设的展示信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    /// 稳定 ID
    pub id: String,
    /// 中文名称
    pub label: String,
    /// 中文说明
    pub description: String,
    /// 是否推荐
    pub recommended: bool,
    /// 是否包含国内直连规则
    pub china_direct: bool,
    /// 是否包含广告拦截
    pub adblock: bool,
}

/// 全部预设（顺序固定，前端按此顺序展示）。
pub fn all() -> Vec<PresetInfo> {
    const ORDER: [RoutingPreset; 5] = [
        RoutingPreset::Smart,
        RoutingPreset::Minimal,
        RoutingPreset::Strict,
        RoutingPreset::Global,
        RoutingPreset::Direct,
    ];
    ORDER
        .iter()
        .map(|preset| PresetInfo {
            id: preset.id().to_string(),
            label: preset.label_zh().to_string(),
            description: preset.description_zh().to_string(),
            recommended: *preset == RoutingPreset::Smart,
            china_direct: matches!(preset, RoutingPreset::Smart | RoutingPreset::Strict),
            adblock: matches!(preset, RoutingPreset::Strict),
        })
        .collect()
}

/// 按 ID 查找预设。
pub fn get(id: &str) -> Option<RoutingPreset> {
    all()
        .into_iter()
        .zip([
            RoutingPreset::Smart,
            RoutingPreset::Minimal,
            RoutingPreset::Strict,
            RoutingPreset::Global,
            RoutingPreset::Direct,
        ])
        .find(|(info, _)| info.id == id)
        .map(|(_, preset)| preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_all_presets() {
        let all = all();
        assert_eq!(all.len(), 5);
        assert!(all.iter().any(|info| info.recommended));
    }

    #[test]
    fn gets_preset_by_id() {
        assert_eq!(get("smart"), Some(RoutingPreset::Smart));
        assert_eq!(get("unknown"), None);
    }
}
