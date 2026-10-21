# 开源项目调研与选型（2026-10）

> 目标：用**通用、可维护、跨平台**的开源组件搭出 LabVPN，而不是从零造协议栈。

## 1. 结论速览

| 能力 | 选型 | 许可证 | 理由 |
|---|---|---|---|
| 代理核心 / 分流引擎 | [mihomo (Clash.Meta)](https://github.com/MetaCubeX/mihomo) | GPL-3.0 | 规则生态最强、Clash 订阅原样兼容、TUN 全平台、内置 `ssh` 出站、REST API 完善 |
| 桌面框架 | [Tauri 2](https://github.com/tauri-apps/tauri) | Apache-2.0 / MIT | 跨 macOS/Windows/Linux，Rust 后端可管进程/权限，体积小，Clash Verge Rev 已验证 |
| 校内 VPN 连接器 | [fdu-connect](https://github.com/AkiraSalvare/fdu-connect)（[zju-connect](https://github.com/Mythologyli/zju-connect) 的复旦分支） | AGPL-3.0 | 唯一活跃的复旦 EasyConnect 开源实现，Go 单文件、三平台、支持 TOTP/保活/服务化 |
| 复旦分流规则数据 | [fdu-netrules](https://github.com/AkiraSalvare/fdu-netrules) | 未声明（个人规则集） | 官方维护的 FDU 域名/IP 清单 + mihomo override 参考 |
| 规则集生态 | [ACL4SSR](https://github.com/ACL4SSR/ACL4SSR) / [MetaCubeX/geo](https://github.com/MetaCubeX/meta-rules-dat) | MIT / GPL | ChinaDomain / ChinaIP / geosite / geoip 数据源 |
| UI/交互参考 | [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev)；[EZ4Connect](https://github.com/chenx-dust/EZ4Connect) | GPL-3.0 | 前者是 Tauri+Clash 的成熟架构；后者是校园 VPN GUI 的交互范式 |

## 2. 代理核心：mihomo vs sing-box vs 自研

| 维度 | mihomo | sing-box | 自研 |
|---|---|---|---|
| Clash 订阅兼容 | ✅ 原生（用户已有订阅无需转换） | ❌ 需 subconverter | ❌ |
| 规则表达能力 | ✅ DOMAIN/GEOSITE/GEOIP/RULE-SET/PROCESS/PROVIDER | ✅ 但语法不同 | 工作量大 |
| 全平台 TUN | ✅ macOS/Windows/Linux | ✅ | ❌ |
| SSH 出站 | ✅ `type: ssh`（password / private-key / host-key） | ❌ 需自行实现 | ❌ |
| 既有 GUI 生态/文档 | ✅ 极丰富 | 一般 | 无 |
| 热更新 | ✅ `PUT /configs`、组切换、延迟测速 API | 一般 | - |
| 许可证 | GPL-3.0（独立进程使用，不修改分发） | GPL-3.0 | - |

**结论**：mihomo。我们只通过「配置文件 + REST API」与它交互，保持独立进程边界。

## 3. 校内 VPN：为什么复用 fdu-connect 而不是自己实现

- 复旦 VPN 是 **深信服 Sangfor EasyConnect (SSL VPN)**，控制通道走私有加密协议 + 登录流程（`/por/login_auth.csp`、`TWFID`、CSRF、RSA、SMS/TOTP）。
- [EasierConnect](https://github.com/lyc8503/EasierConnect) 完成了协议逆向（已停维护）；[zju-connect](https://github.com/Mythologyli/zju-connect) 在其上长期维护（800+ star），支持 EasyConnect + aTrust、TUN、DNS 劫持、保活、端口转发。
- 复旦分支 `fdu-connect` 固化了复旦默认值（`stuvpn.fudan.edu.cn`、校内 DNS `202.120.224.26`、`disable-fdu-config`/`fdu-dns-server` 等），**当前用户线上就在用它**（v0.9.0，launchd 托管，SOCKS 1080）。
- 因此 LabVPN 只做**进程编排**：设置参数 → 启动 → 监控日志 → 提供 SOCKS 出口；不重复造轮子。
- 许可证：AGPL-3.0。**不打包进安装包**（避免衍生作品争议）：默认发现用户已有安装（`~/LocalDocuments/fdu-connect/fdu-connect`、`/usr/local/bin/fdu-connect`、`PATH`），或由用户显式选择/下载/编译。

## 4. 参考实现与可借鉴点

### Clash Verge Rev（Tauri 2 + mihomo）
- 架构：Tauri 主进程管配置/进程，`clash-verge-service` 提权跑 TUN，前端 React。
- 借鉴：sidecar 管理、配置合并（Merge/Script）、代理组 UI、流量统计、系统代理实现、服务安装流程。
- 差异：我们内置校园 VPN 与 SSH 编排，面向"三类出口"而非单纯机场客户端。

### EZ4Connect（Qt/C++，zju-connect GUI）
- 借鉴：校园 VPN 连接向导、服务化运行、与 Clash 配合的分流说明。
- 差异：我们用 mihomo 直接做全流量分流，无需用户再拼一套 Clash 规则。

### fdu-netrules（Mihomo override）
- 复旦流量默认分流 `10.0.0.0/8`、`fudan.edu.cn`、`fducslg.com`、`fduhole.com` → 学校 VPN。
- 采用 `fallback` 组（DIRECT 优先、学校 VPN 兜底）实现"校内直连、校外走 VPN"的退化方案。
- 我们保留其域名/IP 清单作为内置规则数据，但校园判定改为主动探测（更准确、可解释）。

## 5. 关键发现（实测）

在用户当前 macOS 环境实测（2026-10，macOS (Apple Silicon)）：

1. 用户处于**复旦校内网络**：`en0 = 10.188.1.10/19`、IPv6 `2001:da8:8001::/64`。
2. 第三方 TUN（Shadowrocket，`utun11=198.18.0.1`）抢占默认路由，连 `10.0.0.0/8` 都被 `8.0.0.0/5` 路由捕获 → **内网不可达**，这正是需要本项目的痛点。
3. 绑定物理网卡后可绕过 TUN 直连内网：
   - `10.176.52.2:443`（mirrors.fducslg.com 内网）✅ 8ms
   - `202.120.224.26:53`（校内 DNS）✅ 14ms
   - `10.203.8.198:443`（appservice）超时（服务自身策略，保留为低权重探针）
4. fdu-connect 正在运行：`127.0.0.1:1080`，日志显示 `Client IP: 10.230.1.10`、`KeepAlive: OK`；另有 `第三方客户端:1082`。
5. fdu-connect 的 `-dial-direct-proxy` 用于把非 RVPN 流量交给外部代理；在 LabVPN 架构中**不设置**该参数（由 mihomo 统一分流，避免回环）。

→ 这些结论直接决定：**校园检测必须绑卡**、**VPN 服务器必须直连/排除**、**默认端口要避开 1080/1082**。

## 6. 许可证与合规

| 组件 | 许可证 | 使用方式 | 义务 |
|---|---|---|---|
| mihomo | GPL-3.0 | 独立进程 + 配置文件 | 分发二进制需附源码/许可证；我们提供下载脚本与 NOTICE |
| fdu-connect | AGPL-3.0 | 独立进程（不打包） | 若分发需提供源码；默认让用户自备 |
| Tauri / React 等 | MIT/Apache-2.0 | 依赖 | 保留版权声明 |
| fdu-netrules 数据 | 未声明 | 内置规则数据 | 在 NOTICE 中注明来源与作者；提供同步脚本 |

本仓库自身代码建议 **MIT**（应用层），第三方组件在 `NOTICE` 中列明。

## 7. 备选与放弃项

| 方案 | 放弃原因 |
|---|---|
| sing-box 核心 | Clash 订阅不兼容，用户迁移成本高 |
| Electron | 体积/内存大，Rust 后端能力弱 |
| Wails (Go) | 可直连 fdu-connect 库，但 WebView/生态与特权管理不如 Tauri 成熟 |
| 自研 EasyConnect 协议 | 高风险、高维护成本，已有成熟开源实现 |
| 内置 subconverter | 增加攻击面与进程；P0 只支持 Clash YAML 订阅 |
| 静态链接 fdu-connect | AGPL 传染性 + 跨平台构建复杂 |
