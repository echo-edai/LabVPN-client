# LabVPN 架构设计

> 配套阅读：`demand.md`（需求）、`AGENT.md`（开发规范）、`docs/RULES.md`（规则模型）。

## 1. 设计目标

| 目标 | 手段 |
|---|---|
| 一套代码跑三平台 | Tauri 2（Rust + WebView）+ 跨平台 sidecar（mihomo / fdu-connect） |
| 分流能力对齐 Clash | 一切流量交给 mihomo 规则引擎，不自己写转发逻辑 |
| 校内免 VPN | 多信号校园网检测 + 代理组热切换（不重启核心） |
| 可扩展 | 纯逻辑核心库、数据驱动规则、平台 trait、helper 协议化 |
| 安全 | 最小特权；TUN 才提权；密钥系统钥匙串/本机加密 |
| 好看 | 统一设计系统 + 深浅色 + 动效，前端零业务逻辑 |

## 2. 总体架构

```
┌──────────────────────────── LabVPN 桌面应用（Tauri 2）────────────────────────────┐
│                                                                                   │
│  React 渲染层 (Vite)                    Rust 应用层 (src-tauri)                     │
│  ┌───────────────┐   Tauri IPC   ┌───────────────────────────────────────────┐    │
│  │ pages/ 向导 3+1 │ ───────────> │ commands/  (薄命令层)                      │    │
│  │ stores (Zustand)│ <─────────── │ services/  配置编排 / 引擎 / VPN / 订阅 /  │    │
│  │ components/ UI  │   事件/状态   │            检测 / SSH / 系统代理 / 日志    │    │
│  └───────────────┘               └───────┬───────────────┬───────────────────┘    │
│                                          │               │                        │
│                                  ┌───────▼──────┐ ┌──────▼─────────┐              │
│                                  │ labvpn-core  │ │ platform trait │              │
│                                  │ 纯逻辑库      │ │ (mac/win/linux)│              │
│                                  └──────────────┘ └──────┬─────────┘              │
└──────────────────────────────────────────────────────────┼────────────────────────┘
                    │ 子进程(用户态)                       │ helper IPC (仅 TUN 模式)
          ┌─────────▼─────────┐  ┌──────────▼──────┐  ┌──▼──────────────┐
          │ mihomo（核心）     │  │ fdu-connect     │  │ labvpn-helper   │
          │ TUN / 混合代理端口 │  │ 校内 VPN(SOCKS) │  │ (root/admin)    │
          │ REST 控制 API     │  │ 11080           │  │ 托管 mihomo+TUN │
          └─────────┬─────────┘  └──────────┬──────┘  └─────────────────┘
                    │                       │
                    └──────── 10/8 → FDU ───┘
                             其余 → 订阅节点 / 直连 / SSH
```

**职责边界**

- mihomo：唯一流量入口与规则决策者（TUN 或系统代理），提供 REST API（代理组、延迟、流量、连接）。
- fdu-connect：只暴露 SOCKS5（默认 `127.0.0.1:11080`），由 mihomo 作为 `socks5` outbound 使用；自身不做全局路由。
- labvpn-helper：只在 TUN 模式使用，root 身份启动并守护 mihomo；不做任何业务决策。
- Rust 应用层：配置真相源、生成 mihomo 配置、编排生命周期、系统集成。
- React：只渲染与交互，不拼配置。

## 3. 进程模型与两种运行模式

| 模式 | 谁运行 mihomo | 权限 | 能力 | 默认 |
|---|---|---|---|---|
| **系统代理模式**（user） | GUI 子进程 | 用户态 | HTTP/SOCKS 代理生效；非代理应用（裸 TCP/IP）不接管 | 回退 |
| **TUN 模式**（helper） | helper 子进程 | root/admin | 全流量接管：`10.0.0.0/8`、任意 TCP/UDP、SSH | 首选（一次提权） |

- 首次启用 TUN：安装 helper（macOS LaunchDaemon / Linux systemd；Windows 每次启动弹 UAC）。
- helper 不可用或用户拒绝提权 → 自动降级系统代理，并在 UI 明确提示"当前模式下裸 IP 流量不走分流"。
- 模式切换不丢配置：同一份 `mihomo.yaml`，TUN 相关字段由生成器按模式裁剪。

## 4. 数据流

### 4.1 配置 → 生效

```
用户编辑(前端表单)
  → invoke("save_config", AppConfig)            # 校验 + 持久化（secrets 另存）
  → services::config::save()
  → services::engine::apply()
       1. rules::build(&config, ctx)             # 规则列表（纯函数）
       2. generator::generate(&config, rules)    # mihomo YAML（纯函数）
       3. validate: 调用 mihomo -t（可选，开发/应用前）
       4. 写入 app_data/core/config.yaml
       5. 若核心在运行 → 重启核心（TUN/DNS 等变更必须重启，避免半热更新状态）
       6. 根据校园网检测切换组 "🎓 校内"           # 不重启
```

### 4.2 流量路径（离校，TUN 模式）

```
应用 → TUN(mihomo) → 规则匹配
  ├─ 10.0.0.0/8 / *.fudan.edu.cn → 🎓 校内 → FDU-RVPN(socks5 11080) → 校园内网
  ├─ SSH 绑定域名/CIDR          → 🐧 SSH   → ssh outbound（公网服务器）
  ├─ CN 域名/IP                 → DIRECT
  ├─ 广告域名                    → REJECT
  └─ 其他                       → 🚀 代理 → proxy-provider(订阅) → 机场
```

### 4.3 流量路径（校内）

校园网检测命中 → 不启动 fdu-connect → `🎓 校内` 组切到 `DIRECT` → 校内流量走本地网络，其余规则不变。

### 4.4 校园网检测

`labvpn-core/src/probe.rs` + `apps/desktop/src-tauri/src/services/campus.rs`

信号（投票 + 置信度）：

| 信号 | 说明 | 权重 |
|---|---|---|
| 内网 TCP 探针 | 绑定物理网卡直连 `10.176.52.2:443`、`202.120.224.26:53(UDP)`、`10.203.8.198:443` | 高 |
| 本地 IP 段 | 物理网卡地址命中 `10.188.0.0/16`、`202.120.0.0/16`、`2001:da8:8001::/48` | 高 |
| Wi-Fi SSID | `fduwireless*`、`FudanUniversity`、`eduroam`（尽力而为） | 中 |
| 默认路由特征 | 网关/接口提示（辅助诊断） | 低 |

关键实现：**探针绑定物理接口**以绕过第三方 TUN（Shadowrocket/Clash TUN）对默认路由的劫持：

- macOS：`setsockopt(IPPROTO_IP, IP_BOUND_IF, ifindex)`（免提权）
- Windows：`setsockopt(IPPROTO_IP, IP_UNICAST_IF, ifindex)`（免提权）
- Linux：优先 `SO_BINDTODEVICE`（需 root/helper），否则绑定源 IP 后普通探测，仍失败则仅用 IP 段/SSID 信号

结果结构：`CampusStatus { on_campus, confidence, signals[], checked_at, manual_override }`。
状态机每 60s 或网络变更（接口 up/down、默认路由变化）时重检；`manual_override` 优先。

## 5. 配置模型（`labvpn-core/src/config.rs`）

```rust
AppConfig {
  version: u32,
  subscription: SubscriptionConfig { url?, name, update_interval_hours, user_agent, use_remote_provider },
  campus_vpn: CampusVpnConfig { enabled, username, password_ref, server, port, totp_secret_ref?,
                                socks_port, binary_path?, auto_detect, skip_when_on_campus,
                                probe_targets[], campus_cidrs[], extra_vpn_args[] },
  ssh_proxy: SshProxyConfig { enabled, host, port, username, auth: Password{ref}|Key{path, passphrase_ref},
                              routes: Vec<SshRoute{ kind: Domain|Cidr, value }> },
  routing: RoutingConfig { mode: Smart|Minimal|Strict|Global|Direct, custom_rules: Vec<String>,
                           adblock: bool, dns: DnsConfig, group_overrides },
  engine: EngineConfig { mixed_port, controller_port, controller_secret_ref, tun: bool, stack, mtu,
                        allow_lan, ipv6, system_proxy: bool, auto_start: bool, log_level },
  ui: UiConfig { theme, language, tray, auto_launch },
}
```

- 敏感字段只存 **引用**（`password_ref` 等），真实值在 `services/secrets.rs`。
- 配置落盘：`app_data/config.json`；生成物：`app_data/core/config.yaml`；订阅缓存：`app_data/profiles/`；日志：`app_data/logs/`。
- 版本迁移：`config::migrate(from_version)`，旧字段保留默认值，禁止静默丢数据。

## 6. mihomo 配置生成管线（`labvpn-core/src/generator.rs`）

输入：`AppConfig` + `RuntimeContext { on_campus, vpn_running, ssh_ready, resolved_paths }`
输出：`GeneratedConfig { yaml, warnings, sections }`

生成顺序（固定，便于快照测试）：

1. **基础**：`mixed-port`、`allow-lan`、`mode: rule`、`log-level`、`external-controller`、`secret`、`ipv6`
2. **DNS**：fake-ip、`nameserver`、`proxy-server-nameserver`、`nameserver-policy`（校内域名 → 校内 DNS）、`fake-ip-filter`
3. **TUN**（仅 TUN 模式）：`stack`、`auto-route`、`auto-detect-interface`、`dns-hijack`、`route-exclude-address`（VPN 服务器 IP）
4. **proxies**：`🏫 FDU-RVPN`（socks5）、`🖥 SSH-*`（ssh outbound）、内置 `DIRECT/REJECT` 引用说明
5. **proxy-providers**：订阅（http 或 file），`health-check`，`override` 统一前缀
6. **proxy-groups**：见 `docs/RULES.md` 的拓扑常量
7. **rule-providers**：内置（resource 文件）+ 遥测（远程 CN/geosite，可关）
8. **rules**：由 `rules::build()` 生成的最终有序列表
9. **sniffer / profile / geodata**：域名还原、缓存、geo 文件路径
10. **warnings**：订阅缺失、SSH 未配置、端口冲突等，回传前端展示

> mihomo 的 `-t` 测试模式用于 CI 与"应用前校验"，永远不要生成后不校验就重启核心。

## 7. 生命周期与状态机

```
                ┌──────────────┐
   启动/手动 ──> │   Idle       │
                └──────┬───────┘
   配置合法且用户连接  │ start()
                ┌──────▼───────┐   detect campus → on_campus && skip
                │  Detecting   │───────────────┐
                └──────┬───────┘               │
                       │ off-campus            │ on-campus
                ┌──────▼───────┐        ┌──────▼───────┐
                │ VPNDialing   │        │ DirectOnly   │
                └──────┬───────┘        └──────┬───────┘
                       │ 就绪                   │
                ┌──────▼───────────────────────▼────┐
                │            Running                 │  ← 组热切换、订阅更新、健康检查
                └──────┬───────────────┬─────────────┘
                       │ 用户停止       │ 核心崩溃
                ┌──────▼───────┐ ┌─────▼────────┐
                │  Stopping    │ │ Restarting   │ (指数退避 1s/2s/4s/... max 30s)
                └──────────────┘ └──────────────┘
```

事件（Tauri emit）：`status://changed`、`traffic://updated`、`log://appended`、`campus://changed`、`vpn://state`。
前端 store 只订阅事件 + 初次 `invoke("get_snapshot")`，避免轮询。

## 8. 特权 helper 设计（`crates/labvpn-helper`）

**目标**：以 root/admin 运行 mihomo（TUN 必需），其余一切仍在用户态。

- 二进制：`labvpn-helper`（与主应用一同打包，安装时复制到系统目录）
- 通信：回环 TCP `127.0.0.1:38471` + 令牌（`app_data/helper.token`，0600），JSON Lines 协议
- 命令集（白名单）：
  | 命令 | 参数 | 说明 |
  |---|---|---|
  | `ping` | - | 存活与版本 |
  | `start_core` | `config_yaml: string, assets: map<path,content>` | 写入工作目录（内置规则 + 附加资源）后启动 mihomo |
  | `stop_core` | - | 停止并等待退出 |
  | `restart_core` | `config_yaml: string` | 先停后起 |
  | `status` | - | 核心 PID、运行时长、最近错误 |
  | `shutdown` | - | 卸载/退出 helper |
- 安全约束：
  - 只接受回环连接；令牌恒定时间比较；
  - 可执行文件固定为 helper 同目录的 `mihomo`（由安装时确定），请求无法指定路径；
  - 配置内容大小上限（如 8MB）、超时、错误不泄露路径以外信息；
  - 运行期不监听除回环外的端口；退出时确保子进程被回收。
- 安装方式：
  | 平台 | 安装 | 降级 |
  |---|---|---|
  | macOS | `/Library/LaunchDaemons/app.labvpn.helper.plist` + `launchctl bootstrap system` | `osascript … with administrator privileges` 临时启动 |
  | Linux | `/etc/systemd/system/labvpn-helper.service` + `systemctl enable --now` | `pkexec`/`sudo` 临时启动 |
  | Windows | 无服务（避免 SCM 复杂度）：每次开启 TUN 以 UAC 启动 helper 进程；提供 `schtasks /RL HIGHEST` 脚本给高级用户 | 系统代理模式 |

## 9. 平台差异矩阵

| 能力 | macOS | Windows | Linux |
|---|---|---|---|
| 系统代理 | `networksetup -setwebproxy/-setsocksfirewallproxy` | WinINet 注册表 + `InternetSetOption` 通知 | GNOME `gsettings`（KDE 暂不支持，提示手动） |
| 探测绑卡 | `IP_BOUND_IF` | `IP_UNICAST_IF` | `SO_BINDTODEVICE`（root）→ 源 IP 回退 |
| 自启 | LaunchAgent（`tauri-plugin-autostart`） | 注册表 Run / 启动项 | `~/.config/autostart` |
| 服务日志 | `log show`/文件 | 文件 | `journalctl -u labvpn-helper` |
| 应用数据 | `~/Library/Application Support/app.labvpn.desktop` | `%APPDATA%\app.labvpn.desktop` | `~/.local/share/app.labvpn.desktop` |
| 进程名规则 | `PROCESS-NAME,fdu-connect` | `PROCESS-NAME,fdu-connect.exe` | `PROCESS-NAME,fdu-connect` |

## 10. 安全模型

1. 秘密：`services/secrets.rs` → `SecretStore` trait
   - `KeychainStore`（macOS Keychain / Windows Credential Manager / Linux Secret Service，尽力而为）
   - `EncryptedFileStore`（AES-256-GCM，密钥 = HKDF/SHA256(machine-id + 随机盐)，文件 0600）
2. 渲染进程零文件系统权限：只在 `capabilities/default.json` 开最小命令集合（自定义命令 + 必要插件）。
3. 子进程：路径白名单（应用自带或用户显式选择的二进制）、参数数组、超时、环境变量最小化。
4. 日志脱敏：订阅 URL token、密码、私钥内容统一 `***`。
5. 更新：sidecar 通过 SHA256 校验（`scripts/fetch-sidecars.mjs` 内置校验和）；helper 由主包签名信任链保护。

## 11. 关键扩展点

| 扩展点 | 位置 | 方式 |
|---|---|---|
| 规则预设 | `crates/labvpn-core/src/rules/` | 新增 `RulePreset` 实现 + 数据文件 |
| 订阅格式 | `crates/labvpn-core/src/subscription/` | `SubscriptionParser` trait |
| 新出口协议 | mihomo 原生支持优先；`generator/proxies.rs` 映射 | 不改流量路径 |
| 新平台 | `platform/` | 实现 `PlatformService` trait |
| 新提权操作 | `docs/ARCHITECTURE.md` §8 协议 | helper + 客户端 + 测试先行 |
| UI 主题 | `apps/desktop/src/styles/tokens.css` | 只改 token |

## 12. 可观测性

- 结构化日志：`tracing`/`log` 分级，滚动文件（≤5MB×3），UI 日志页可过滤/导出。
- 健康检查：核心 `GET /version`、组延迟、fdu-connect 端口探测、订阅刷新结果。
- 诊断包：一键导出「配置（脱敏）+ 日志 + 环境信息（网卡/路由/端口）」，便于排障。
