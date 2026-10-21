# Changelog

本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.1] — 预发布（当前）

### Added
- **内置 fdu-connect**：App 安装包直接包含复旦 RVPN 连接器（AGPL-3.0，独立进程），开箱即用
- macOS 签名/公证流水线（Developer ID + notarytool）与 Windows 代码签名流水线（证书配置后自动启用）
- `scripts/sign-macos.sh`、`scripts/build-local-macos.sh` 一键本机构建/签名/启动
- Release 自动附带 fdu-connect 与 mihomo 的许可证与对应源码归档（GPL/AGPL 合规）

### Security
- 含密码的临时配置文件（fdu-connect config.toml / mihomo YAML、SSH 私钥资产）在进程读取后**覆盖销毁**
- 凭据继续使用 AES-256-GCM + 机器绑定密钥加密存储（0600），无云端上传
- 配置预览对密钥字段脱敏；日志/诊断包自动打码

### Fixed
- 修复跨平台 sidecar 资源命名（`mihomo.exe` / `labvpn-helper.exe`）导致 Windows 包内二进制缺失
- 修复 Windows zip 内二进制与 wintun.dll 路径/架构选择

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
