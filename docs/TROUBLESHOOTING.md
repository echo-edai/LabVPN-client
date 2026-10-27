# 故障排查（Troubleshooting）

> 出问题的第一件事：打开「日志」页（或 设置 → 导出诊断包），查看最近错误。
> 诊断包包含脱敏配置、系统环境、引擎/VPN/校园网状态与最近日志。

## 1. 无法连接

| 排查 | 操作 |
|---|---|
| 配置不完整 | 总览/向导会给出字段级错误（红色）；补齐后再连接 |
| 端口被占用 | 设置 → 引擎 → 修改混合端口（默认 7897）或控制端口（9097） |
| 核心缺失 | 日志出现「找不到 mihomo 核心」：重新安装或运行 `npm run sidecars` 后重启 |
| helper 未就绪 | TUN 模式需要 helper；在设置页安装，或先使用系统代理模式 |
| 订阅失效 | 订阅页点「刷新」；检查机场是否到期/流量用尽（订阅页显示） |

## 2. 无法访问校内资源（10.x / *.fudan.edu.cn）

1. 第 2 步「测试登录」必须成功（失败通常为账号密码错误、欠费、或需要 TOTP）。
2. 确认当前为 **TUN 模式**：系统代理模式无法接管裸 IP 流量（例如 `ssh 10.x`、数据库客户端）。
3. 检查分流组：总览「代理组」中 `🎓 校内` 应为 `🏫 FDU-RVPN`（校外）或 `DIRECT`（校内）。
4. 若在校园网却仍走 VPN：总览点「重新检测」，或把网络位置设为「自动」。
5. 若内网仍不通，检查是否与其他 TUN（Clash/Shadowrocket）冲突：先退出它们的 TUN 模式。

### 2.1 校内网站「时好时坏」/ 旦挞等 App 部分功能打不开

典型症状（`core.log` 中可复现）：

```
[TCP] ... (Danta) --> static.fduhole.com:443 match RuleSet(FDUDomain) using 🎓 校内[🏫 校内 VPN]
[TCP] dial DIRECT (match ProcessName/fdu-connect) ... --> static.fduhole.com:443
      error: dns resolve failed: all DNS requests failed, first error: server failure: REFUSED
```

原因：这类域名解析到**公网 CDN**，fdu-connect 的 RVPN 规则判定为“直连”，该直连连接又被 TUN 抓回，
mihomo 按 `nameserver-policy` 用校内 DNS（`202.120.224.26`）解析 → 离校不可达 → 请求反复失败。

处理：保持「校内 VPN → 高级 → **校园 VPN 默认路由**」开启（默认已开）。开启后交给校内 VPN 的流量
全部走 RVPN 隧道，不会再出现“VPN 内部二次直连”。若你手动关过它，重新打开并重连即可。

## 3. 校内 VPN 登录失败

| 日志关键字 | 原因 | 处理 |
|---|---|---|
| `SMS code verification failed` | 需要短信验证码 | 该场景需在手机端完成一次验证，或配置 TOTP |
| `password` / `登录失败` | 账号或密码错误 | 重新填写密码；确认没有校园网欠费 |
| `Initial FDU-Connect failed` | 网络不可达/服务器变更 | 检查服务器地址；尝试填写 `202.120.224.58` |
| 超时 45s | 被其他代理劫持/防火墙 | 关闭其他 TUN；确认能直连 `202.120.224.58:443` |
| `fdu-connect 可执行文件不存在` | 未安装/路径错误 | 第 2 步「自动检测」或手动指定路径（见 BUILD.md） |

> 反复失败可能触发学校侧临时封禁；请间隔几分钟再试。

## 4. 校园网检测不准确

- 信号列表会显示每个探测结果与耗时，便于判断。
- 在校园网但显示「校外」：
  - 检查是否被其他 TUN 抢占路由（检测已绑卡，通常不受影响）；
  - 确认本机 IP 命中 `10.188.0.0/16` 或 `202.120.0.0/16`；
  - 第 2 步「高级」中增加可用内网探针（如 `10.176.52.2:443`）。
- 在校外但显示「校内」：把网络位置设为「校外」，或检查是否有同名 SSID 热点。
- 手动强制优先于自动检测，适合演示/固定场景。

## 5. 订阅导入失败

- 只支持 **Clash / mihomo YAML** 订阅；分享链接/Base64 订阅会提示格式错误
  （请在机场后台选择 Clash 订阅）。
- HTTP 403/404：地址失效或有 UA 限制；可在设置中调整 User-Agent（高级）。
- 流量为 0 或节点为空：机场返回了错误页，用「测试地址」查看。
- 订阅 token 泄露风险：日志/诊断包已脱敏；截图分享前请留意。

## 6. TUN 模式相关问题

| 现象 | 处理 |
|---|---|
| 设置页提示 helper 未安装 | 点「安装特权服务」，按系统弹窗授权 |
| macOS 安装后仍不可用 | 检查 `/Library/LaunchDaemons/app.labvpn.helper.plist`；`launchctl print system/app.labvpn.helper` |
| Linux 安装失败 | 需要 `pkexec`/`sudo`；AlmaLinux 可 `sudo dnf install -y polkit` |
| Windows 每次启动都弹 UAC | 设计如此（无系统服务）；可选 `schtasks /RL HIGHEST` 自建开机任务 |
| 开启 TUN 后无网络 | 关闭其他 TUN；尝试 TUN 协议栈切换为 `gvisor`；把 MTU 降到 1400 |
| TUN 与公司 VPN 冲突 | 在设置中把公司 VPN 服务器加入「路由排除地址」 |
| 裸 IP 不通（`ssh 10.x` 超时，网页/订阅却正常） | 先看 `helper.log` 是否有「TUN 自动路由缺失，已补回」——那是 helper 在自愈。若仍不通：重连一次；`route -n get 10.134.142.226` 应显示 `interface: utun*`（不是 `en0`）；紧急可 `sudo route -n add -net 10.0.0.0/8 -interface utunX` 后重试 |

### 6.1 裸 IP（SSH / 数据库 / 内网）偶发不通

`ssh -vvv 10.x` 停在 `Connecting …` 后超时、连 `Connection established` 都没有，说明 TCP 没通；
而 `nc -X 5 -x 127.0.0.1:11080 <ip> 22` 能拿到 `SSH-2.0-OpenSSH_*`，说明校内 VPN 和服务器都正常。
此时多半是 mihomo 的自动路由丢了**下半区**（`1.0.0.0/8 … 64.0.0.0/2` 这组前缀）：
`10.0.0.0/8` 属于这个范围，包会从默认网关发到公网。

- helper（macOS）会在 3 秒内自动补回，`helper.log` 记录缺失/补回的具体前缀；
- 若自动补回失败，退出 LabVPN 重连（核心重启会重新安装完整路由）；
- 不要通过给 `~/.ssh/config` 加 `ProxyCommand` 来长期绕过——那会掩盖同类问题（本项修复前的老办法）。
- 注意：**不要**手动 `route add 0.0.0.0/1`——在 macOS 上这条路由会让 mihomo 的出口连接
  全部报 `network is unreachable`（表现为整机断网、聊天/网页全部失败）。保留网段
  `0.0.0.0/8` 必须留给系统。

## 7. 与 Clash / Shadowrocket 冲突

- 同一时间只应保留一个 TUN 接管者。
- 若希望保留 Clash Verge，可让 LabVPN 使用系统代理模式，并由 Clash 指向上游？不推荐链路嵌套；
  建议直接用 LabVPN 统一分流（订阅导入功能等价）。
- fdu-connect 的 `-dial-direct-proxy` 在 LabVPN 托管的配置中为空（由 mihomo 统一分流），
  如果你手工改过旧配置，请勿再指向 Clash 端口，避免环路。

## 8. 系统代理无法设置/恢复

- macOS：`networksetup` 需要当前用户权限；公司 MDM 可能禁止修改网络设置。
- Linux：仅支持 GNOME `gsettings`；KDE/XFCE 请手动设置或使用 TUN 模式。
- Windows：写注册表后通过 WinINet 通知刷新；个别应用需要重启。
- 异常退出后系统代理可能残留：设置页「关闭系统代理」或重新启动应用会自动清理。

## 9. 性能与稳定性

- 首次启动会下载 geosite/geoip 数据（约 20MB），之后复用缓存。
- 订阅节点很多时「自动选择」测速会占用少量带宽，可在代理组面板手动指定。
- 日志级别调为 `debug` 仅用于排障，会产生大量日志。
- 应用崩溃/核心退出会在日志中显示退出码；引擎会自动尝试重启（见状态卡片）。

## 10. 反馈问题

请附上：

1. 操作系统与版本、LabVPN 版本（设置 → 关于）；
2. 「日志」页最近 50 行（注意脱敏）；
3. 「设置 → 导出诊断包」生成的文件路径；
4. 复现步骤与期望行为。

提交到 <https://github.com/echo-edai/LabVPN-client/issues>。

## 核心启动失败 / 控制端口未就绪

**现象**：提示「核心控制端口未就绪（60s 超时）」或「混合代理端口未监听」。

**原因与处理**：

1. **端口被其他代理软件占用**（最常见）：
   ```bash
   lsof -nP -iTCP -sTCP:LISTEN | grep -E ":(7897|9097|1080|11080)"
   ```
   - macOS：退出 Clash Verge / Shadowrocket / Surge 等；用 `scutil --proxy` 确认系统代理已关闭
   - Windows：退出 Clash for Windows / v2rayN / 其他 mihomo 客户端
   - 新版 LabVPN 会自动避让被占用端口（日志/提示中会说明本次使用的端口），也可以
     在「设置 → 引擎」里手动改混合端口与控制端口。
2. **上次异常退出遗留核心进程**：新版启动时会自动清理（校验 PID 文件 + 进程名）。
   手动处理：`pkill -f "app.labvpn.desktop/core/config.yaml"`。
3. **首次启动 geodata 下载慢**：v0.1.2 起 App 已内置 GeoSite.dat / geoip.metadb，不再联网下载。
4. **权限问题**：直接模式不需要 root；TUN 模式必须先安装特权 helper（设置页）。

排查顺序：日志页看 `core` 源的最后几行 → 确认端口占用 → 关闭其他代理软件 → 重试。

## 为什么 macOS「系统设置 → VPN」里没有 LabVPN？

- 能在系统设置 VPN 列表里出现的应用（如 Shadowrocket、ClashX Pro）使用的是
  **Network Extension（`NEPacketTunnelProvider`）**，由 Apple 授权 entitlement 并签名；
- LabVPN 使用**特权 helper 启动 mihomo TUN（utun）**实现全流量接管：功能上等同于 VPN，
  但 macOS 不会把这种 utun 显示为「VPN」服务（Clash Verge 等同类方案也一样）；
- macOS 没有公开的 CLI/plist 可以直接创建 VPN 服务：`networksetup` 只能创建 PPPoE/VLAN/bond，
  `scutil --nc` 只能管理已存在的 NE VPN 配置，不能创建。

如需在系统设置中出现 LabVPN，必须引入 Network Extension，属于 v0.3.0 计划（见 GitHub issue #7），
前置条件是 Apple Developer Program 与 Network Extension entitlement。

## 订阅导入报 HTTP 403 / AUTH_FAIL（IP 鉴权机场）

现象：导入订阅提示 `HTTP 403`，响应体类似 `{"error":"AUTH_FAIL，IP鉴权失败，请先登陆"}`；
同一链接在 Clash Verge 中正常。

原因：机场对订阅来源 IP 做了白名单，LabVPN 默认直连拉取导致被拒。

处理：在「并行配置 → 订阅 → 订阅拉取方式」选择：
- **自定义代理**：填本机代理端口（如 Clash Verge 默认 `7897`，可只填端口），或
- **系统代理**：若系统代理已开启（Clash Verge / Shadowrocket 等）。

选择「自动」时会依次尝试 直连 → 本机 mihomo → 系统代理 → 自定义代理。

通过代理拉取成功后，会自动改用本地缓存 provider，避免 mihomo 直连刷新再次 403。

## file:// 本地订阅导入失败

订阅地址支持标准 `file:///C:/Users/xxx/sub.yaml`（Windows）、`file://C:/...`、Unix
`file:///home/xxx/sub.yaml` 以及 `file://localhost/...`；空格/中文可写成 `%20` 形式。
若提示「路径不存在」，请确认路径与转义；格式错误会单独提示。

## 校园网置信度偏低

置信度 = 各信号（校内 IP 段 / 校园内网探针 / 复旦 Wi-Fi SSID）加权比例。

- 只要**任一确定性信号**命中（校内 IP、复旦 SSID、任一校园内网探针可达），置信度**至少 90%**。
- 若通过**路由器 NAT 上网**（本机 IP 是 192.168.x 等），IP 段信号不命中，但内网探针可达时
  仍会正确判定为「校内」并给出 ≥90% 置信度。
- 如果置信度低且判定为校外：检查是否有第三方 TUN（Shadowrocket/Clash Verge）劫持流量；
  LabVPN 的探针会绑定物理网卡绕过 TUN，但建议测试时只保留一个 TUN。
