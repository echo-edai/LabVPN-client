# Changelog

本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### Added
- 三页配置向导：订阅 / 校内 VPN / SSH 转发
- 融合与特调规则页：5 种预设、自定义规则、配置预览、分流诊断
- 总览：一键连接、实时流量、代理组切换、校园网信号、日志
- 日志页、设置页（外观 / 引擎 / TUN / 特权服务 / 路径 / 诊断 / 重置）
- TUN 特权 helper（回环 IPC + 令牌；launchd / systemd / Windows 提权）
- 校园网多信号检测（绑定物理网卡绕过第三方 TUN），校内自动跳过 VPN
- 订阅自动更新、流量/到期展示；mihomo 配置热切换校内分组
- CI（macOS / Windows / Linux）与三平台 Release 工作流
- 文档：BUILD / USER_GUIDE / TROUBLESHOOTING / 架构 / 规则 / 开源选型

### Fixed
- 生成配置通过 `mihomo -t` 校验；预览配置对密钥脱敏
- 与 Clash / Shadowrocket TUN 共存时的内网探测与防环处理

## [0.1.0] — 规划中

首个可发布版本，当前处于实现阶段（见 README 开发状态）。
