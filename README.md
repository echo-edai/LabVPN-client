# LabVPN

> 复旦校园网 · 机场订阅 · 公网 SSH —— 一个客户端，自动分流。

[![CI](https://github.com/echo-edai/LabVPN-client/actions/workflows/ci.yml/badge.svg)](https://github.com/echo-edai/LabVPN-client/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-1f6feb)](#平台支持)

LabVPN 把「Clash 订阅 + 复旦 RVPN + 公网 SSH 中转」整合进一个桌面客户端：
**三页配置、一处融合、一套特调分流规则**。基于 Tauri 2 与 mihomo（Clash.Meta），
支持 macOS / Windows / Ubuntu / AlmaLinux。

## 功能

- **自动分流（一套特调规则）**
  - `10.0.0.0/8` 与 `*.fudan.edu.cn` → 校内 VPN（fdu-connect）
  - 中国大陆域名/IP → 直连
  - 国外域名/IP → 订阅节点（自动择优 / 手动选择）
  - 自定义域名/IP → 公网 SSH 出站
  - 广告拦截、局域网/本机直连、防环（VPN 服务器直连）
- **校内网络检测**：多信号（内网探针 + 本机 IP 段 + Wi-Fi SSID），探针绑定物理网卡以
  绕过第三方 TUN 劫持；**在校内自动跳过 VPN，直接连接**，并支持手动覆盖。
- **三页配置向导**
  1. **订阅**：粘贴 Clash/mihomo 订阅地址，自动拉取、显示节点数/流量/到期；
  2. **校内 VPN**：复旦上网账号 + 密码（存钥匙串），自动发现已安装的 fdu-connect；
  3. **SSH 转发**：公网 IP + 端口 + 认证方式，绑定需要走 SSH 的域名/CIDR。
- **融合页**：实时预览 mihomo 配置、规则总览、自定义规则、分流诊断（输入域名/IP 看走向）。
- **TUN 全流量 + 系统代理双模式**：TUN 一次提权启用（helper），不可用时自动降级。
- **跨平台**：macOS（ARM/Intel）、Windows 10/11、Ubuntu 22.04+、AlmaLinux 9+。
- **可扩展**：纯逻辑核心库（`labvpn-core`）、数据驱动的规则/预设、平台 trait。

## 架构

```
┌─ LabVPN 桌面应用（Tauri 2）─────────────────────────────┐
│  React UI ⇄ Tauri IPC ⇄ Rust services                    │
│                    ├─ labvpn-core  纯逻辑：配置/规则/生成  │
│                    └─ platform trait：系统代理/提权/自启   │
└──────────┬──────────────────────┬───────────────────────┘
           │ 子进程                │ helper IPC（TUN 模式）
     ┌─────▼─────┐          ┌─────▼──────────┐
     │  mihomo   │          │ labvpn-helper  │
     │ 分流核心  │          │ root/admin     │
     └─────┬─────┘          └────────────────┘
           │ socks5
     ┌─────▼─────────┐
     │  fdu-connect  │  校内 RVPN（独立进程，AGPL-3.0）
     └───────────────┘
```

详见 [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)、[`docs/RULES.md`](docs/RULES.md)。

## 平台支持

| 平台 | 安装包 | TUN | 备注 |
|---|---|---|---|
| macOS 12+ | `.dmg` | LaunchDaemon helper（一次授权） | Apple Silicon / Intel |
| Windows 10/11 | `.msi` / `.exe` | 每次启动 UAC 提权 | x64 |
| Ubuntu 22.04+ | `.deb` / `.AppImage` | systemd helper | 需 `webkit2gtk-4.1` |
| AlmaLinux 9+ | `.rpm` | systemd helper | 需 `webkit2gtk-4.1`（AppStream） |

## 快速开始（开发）

前置：Rust stable、Node.js 20+、pnpm/npm，以及平台 WebView 依赖。

```bash
# 1. 安装前端依赖
cd apps/desktop && npm install

# 2. 拉取 sidecar（mihomo；fdu-connect 默认识别本机已有安装）
npm run sidecars

# 3. 开发模式（Vite + Tauri）
npm run dev

# 4. 纯逻辑单测
cargo test -p labvpn-core
```

构建与打包细节见 [`docs/BUILD.md`](docs/BUILD.md)；用户使用说明见
[`docs/USER_GUIDE.md`](docs/USER_GUIDE.md)。

## 仓库结构

```
├── demand.md / AGENT.md        # 需求（精简）与 Agent 开发指南
├── docs/                       # 架构、规则、构建、用户、排障
├── apps/desktop/               # Tauri 应用（React UI + Rust 应用层）
├── crates/labvpn-core/         # 纯逻辑核心（可单测、无 Tauri 依赖）
├── crates/labvpn-helper/       # 特权 helper（TUN 模式）
├── resources/rules/            # 内置分流规则数据（复旦/局域网/广告/国内兜底）
├── scripts/                    # sidecar 获取、规则同步、构建、图标生成
└── .github/workflows/          # CI 与三平台发布
```

## 路线图

- [x] 需求与架构设计、开源选型（mihomo + fdu-connect + Tauri）
- [x] 核心库：配置模型、规则引擎、mihomo 生成器、订阅解析、校园网探测
- [ ] 三页配置向导与融合页 UI
- [ ] TUN 特权 helper 与三平台安装/降级
- [ ] 三平台 CI 打包与签名
- [ ] 订阅多配置、规则命中实时诊断、托盘快捷操作
- [ ] 分享链接/Base64 订阅转换

## 合规与免责

- 本项目仅供个人学习与校园网便利使用；请遵守复旦大学网络使用规定与当地法律法规。
- fdu-connect（AGPL-3.0）与 mihomo（GPL-3.0）为**独立进程**，不随仓库分发；
  见 [`NOTICE`](NOTICE)。
- 非官方项目，与复旦大学、深信服科技无隶属关系。

## License

[MIT](LICENSE)
