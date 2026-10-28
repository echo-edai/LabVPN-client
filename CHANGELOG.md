# Changelog

本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [0.1.17] — 预发布（当前）

### Fixed
- **重启 App 后卡在「核心已在运行，请先停止」无法连接**：上一次 App 实例退出/被替换时，
  特权 helper 中的 mihomo 核心可能仍在运行；新实例直接 StartCore 会被 helper 拒绝，
  导致 UI 显示未连接、点击连接/重连都报错。现在 TUN 模式启动核心前会先无条件 StopCore
  （清理残留核心），再启动新核心；`engine.stop()` 在未 attached 时也会尝试停止 helper 核心。

## [0.1.16] — 已发布（2026-10-28）

### Added
- **连接前关闭其他代理程序**（设置 → 引擎，默认开启）：连接 LabVPN 前自动识别并退出
  Clash Verge / ClashX / Mihomo Party / Clash Nyanpasu / FlClash / V2rayU / Hiddify /
  sing-box / Shadowrocket / Surge / Quantumult X / Stash / Loon / ShadowsocksX-NG 等已知代理，
  避免它们占用系统代理或 7890/7891/7897 等端口，导致「LabVPN 已连接但流量没被接管」。
  只匹配白名单进程、显式排除 LabVPN 自身，日志页出现 `[proxy-guard] 已关闭其他代理：...`，
  连接结果警告里也会列出被关闭的程序；可在设置中关闭该行为。

### Fixed
- **窗口隐藏后点击 Dock 图标无法恢复**：之前只处理了托盘图标点击，没有处理 macOS
  的 `RunEvent::Reopen`；现在点击 Dock 图标会在无可见窗口时重新 `show + unminimize + focus`。

## [0.1.15] — 已发布（2026-10-28）

### Added
- **核心自动恢复**：每 15 秒探测 mihomo 控制端口；当 App 仍显示「已连接」但核心实际已退出
  （helper 被重装/重启、mihomo 崩溃、系统唤醒后丢失）时，自动执行完整重连恢复 TUN 和路由。
  10 分钟内最多恢复 3 次，超限记录 error；日志页出现 `[核心自动恢复]` 事件。旧 SSH 会话会
  随 TUN 重建而断开（客户端 `Can't assign requested address`），重连后新连接恢复。

### Fixed
- **连接成功率低（界面/日志大量 `error: interface not found`）**：mihomo 的出口网卡自动探测
  在 macOS 换网、热点切换、helper 重启后会失效，导致所有裸 IP 连接（10.x / DIRECT）
  瞬间失败。现在连接时显式把当前物理默认网卡写入 mihomo 配置 `interface-name`，不再依赖
  运行时自动探测；日志会记录 `出口网卡：en0（显式绑定）`。

## [0.1.14] — 已发布（2026-10-27）

### Added
- **隧道中断自动重启**：KeepAlive 连续失败 3 次（约 3 分钟，说明 RVPN 数据面已被服务端/NAT
  丢弃）时，App 自动重启 fdu-connect（等效重新授权），无需手动断开重连。带频率保护：
  30 分钟内最多自动重启 3 次，超过后记录 error 提示手动处理。事件会写入校内 VPN 日志
  （`[VPN事件] 自动重启 ...`）。
- 自动重启只在托管模式（`managed`）下生效；复用外部 fdu-connect 时不会杀进程。

## [0.1.13] — 已发布（2026-10-27）

### Added
- **运行日志页新增来源切换**：「全部 / 校内 VPN / 核心 / 引擎 / 其他」。校内 VPN 页集中展示
  fdu-connect 的建连、重试、KeepAlive 日志，便于单独排查 RVPN 侧问题。
- **日志分级细化**：fdu-connect 输出按内容分级——`[E]`、`panic`、登录/认证失败等记为
  `error`；`failed`、`timeout`、`context deadline exceeded`、`reset by peer` 等重试/超时
  记为 `warn`；在日志页可直接按级别过滤和高亮。
- **日志保留策略**：`app.log` 按天轮转（跨天归档为 `app-YYYY-MM-DD.log`），单文件 8 MB
  上限、归档保留 7 天，启动时自动清理过期文件；内存保留最近 2000 条。
- **连接日志分类**：运行日志新增「连接日志」分页，按出口分为 **外网连接 / 校园网连接 /
  直连 / SSH 连接** 四类（SSH 优先：22 端口、`ssh` 进程或 mihomo `🖥 SSH-*` 出站）。
  TUN 模式下 App 会跟随 helper 的 `core.log` 实时转发新增连接日志；helper 的 `core.log`
  超过 20 MB 时自动归档，避免长期运行无限增长。
- **结构化 VPN 连接日志**：新增 `[VPN事件]` 事件行，覆盖会话开始/结束、TWFID 授权、登录成功、
  Server session ID、KeepAlive 成功/失败、隧道中断与恢复、RVPN 数据连接重连、fdu-connect
  进程退出，以及每 60 秒一次的健康巡检快照。
- **「分析 VPN 日志」**：运行日志 → 校内 VPN →「分析 VPN 日志」，自动解析**最近一次连接**，
  输出 KeepAlive 成功率、隧道中断窗口（起止/时长/期间拨号失败次数）、主要错误类型与结论建议
  （例如「隧道被服务端/NAT 丢弃但 fdu-connect 不会自动重新登录」）。纯解析逻辑位于
  `labvpn-core::vpn_log` 并带单测。

### Docs
- 补充「为什么 `ping 10.x` 不通、但 SSH 可用」：ICMP 不会进入校内 VPN 的 SOCKS5 隧道，
  mihomo TUN 的真实 IP ICMP 固定走本机 DIRECT，不能作为校内连通性判断。

## [0.1.12] — 预发布

### Fixed
- **TUN 模式裸 IP（SSH / 数据库 / 内网）偶发不通**：mihomo 的 `auto-route` 在 macOS 上
  偶发丢失下半区自动路由（`1.0.0.0/8 … 64.0.0.0/2` 这组前缀），`10.0.0.0/8` 落回默认网关，
  表现为 `ssh 10.x`、数据库等裸 IP 连接超时（网页与订阅仍正常）。特权 helper 新增 TUN 自动路由自愈：
  从生成配置推导应有的路由集合（两个默认半区扣除 `route-exclude-address`、跳过保留网段
  `0.0.0.0/8`），每 3 秒校验并在缺失时自动 `route add` 补回，打开 TUN 后直接 `ssh <校内主机>`
  即可，无需手动配置 `ProxyCommand` 或重启核心。实现：`crates/labvpn-helper/src/tun_guard.rs`（仅 macOS）。
  注意自愈集合不包含 `0.0.0.0/1`：实测在 macOS 手动添加该路由会让 mihomo 出口全部
  `network is unreachable`，等价于断网。

## [0.1.11] — 预发布

### Added
- **应用内检测更新**：启动后自动检查 GitHub Release（可在设置中关闭），发现新版本时
  侧边栏显示版本徽标，弹窗展示版本号、发布时间与更新说明，由用户选择
  「前往下载 / 稍后提醒 / 忽略此版本」；设置 → 关于页可手动「检查更新」并恢复被忽略的版本。
  仅比对版本号，不上传任何本地信息；网络失败时静默降级（手动检查会提示原因）。
- 配置新增 `update.autoCheck` / `update.skippedVersion`；纯逻辑（版本解析/比较/Release 挑选）
  位于 `labvpn-core::update` 并带单测。

### Fixed
- **侧边栏左下角版本号显示错误**：之前硬编码为 `0.1.9`，现在读取应用真实版本
  （`system.appVersion`），升级后立即显示正确版本。

## [0.1.10] — 预发布

### Added
- **校园 VPN 默认路由（`proxyAll`）**：生成的 `fdu-connect` 配置新增 `proxy_all = true`，
  交给校内 VPN 的流量**全部由 RVPN 隧道承载**，不再由 RVPN 的域名/网段规则判断为“直连”。
  「校内 VPN → 高级」新增开关，默认开启。
  背景：旦挞（DanXi / Danta）的 `static.fduhole.com` 解析到公网 CDN 后被 RVPN 判为直连，
  又被 TUN 抓回并按校内 DNS 解析（离校不可达），导致其请求反复失败；开启默认路由后恢复正常。
- 复旦域名规则补充 `fudan.sh.cn`（研究生选课/课表备用域名，旦挞会回退访问
  `yjsxktest.fudan.sh.cn`），同步写入 `scripts/sync-rules.mjs` 的本地补充列表。

### Fixed
- 校内服务在「RVPN 视为直连」的目标上失败的问题（表现为校内网/旦挞部分功能打不开）。
- **校园 VPN 偶发 `context deadline exceeded`（部分内网主机偶发断连）**：
  原因有两层 —— (1) 目标/路径短时丢包时，RVPN 用户态栈的 TCP 握手会退避重传，
  `gonet.DialTCP` 没有超时也不能取消（黑洞目标实测 >120s 不返回）；
  (2) mihomo 对 SOCKS5 拨号只等 5s，把「慢但能成功」的连接直接判死，
  并在 fdu-connect 侧留下 CLOSE_WAIT 半开连接与泄漏 goroutine。
  解决：**本地补丁版 fdu-connect**（[`vendor/fdu-connect/PATCH.md`](vendor/fdu-connect/PATCH.md)）
  - 握手改用可取消的 `gonet.DialContextTCP`，客户端断开即中止、不再泄漏；
  - SOCKS5 宽限应答：2s 内握手完成则完全保持严格 SOCKS5 语义，超过则先应答成功，
    后续 `Read/Write` 阻塞等待；单次握手 4s，超时后用**新连接**（新源端口/新 NAT 表项）
    重试，直到 20s 总预算（`socks_connect_timeout` / `socks_early_reply_ms`）。
  - gVisor `TCPMinRTO` 从 1s 下调到 500ms，让短暂丢包窗口内的 SYN 重试次数足够多。
  修改后完整源码随仓库提供（AGPL-3.0），安装包内二进制由 `scripts/fetch-sidecars.mjs` 编译。

## [0.1.9] — 预发布

### Fixed（GitHub issues #4 / #5 / #6）
- **Windows 控制台黑框闪烁/抢焦点（#4）**：所有子进程统一通过 `hidden_command` 创建，
  Windows 下设置 `CREATE_NO_WINDOW`；覆盖平台查询、mihomo 核心、fdu-connect、helper 提权、
  注册表读取、ping 探针等全部 spawn 路径。
- **`file://` 三斜杠写法（#5）**：重写 file URL 解析，兼容 `file:///C:/...`、`file://C:/...`、
  `file://localhost/...`、Unix 路径与百分号编码（空格/中文）；错误提示区分格式错误与文件不存在。
- **订阅拉取不走代理导致 IP 鉴权机场 403（#6）**：
  - 新增「订阅拉取方式」：自动 / 仅直连 / 系统代理 / 自定义代理（支持 `http://host:port` 或纯端口）；
  - 自动模式依次尝试 直连 → 本机 mihomo → 系统代理 → 自定义代理 → 环境变量代理；
  - 拉取失败时显示服务端响应体（如 `AUTH_FAIL / IP鉴权失败`）与排查建议；
  - 通过代理拉取时自动切换为本地缓存 provider，避免 mihomo 直连刷新再次 403。

### Changed
- 订阅请求不再复用共享 HTTP 客户端，每次按通道构建（避免代理配置互相影响）。

## [0.1.8] — 预发布

### Fixed
- **修复 Apple Silicon（含 M4）从 Release 下载 dmg 后提示「已损坏，无法打开」**：
  macOS 构建在未配置 Apple 证书时默认执行 **ad-hoc 签名**（`APPLE_SIGNING_IDENTITY=-`），
  并对内置 sidecar 统一签名；CI 增加构建后签名校验，杜绝再次发布无效签名的包。
  注意：ad-hoc 签名不是 Apple 公证，首次打开仍需右键「打开」或在
  「系统设置 → 隐私与安全性 → 仍要打开」中允许。

## [0.1.7] — 预发布

### Changed
- **兜底组「🐟 漏网之鱼」默认改为 DIRECT**（仍可手动切换到代理），未命中规则的流量默认直连
- **新增更细的国内直连 / 国外代理规则**，减少流量落到兜底组：
  - `DOMAIN-SUFFIX,cn,DIRECT`、内置 `ChinaDomainFallback` 规则集直连
  - `GEOSITE,cn` + `GEOIP,CN` 直连（原有）
  - `GEOSITE,geolocation-!cn → 🚀 代理`：国外域名显式走订阅代理，而不是依赖兜底组
- 隐藏 mihomo 内置 `GLOBAL` 组（规则模式下不参与分流，避免误解）

## [0.1.6] — 预发布

### Changed
- **AI 平台 IP 暴露重新实现**：不再展示全局代理出口 IP 冒充平台 IP。
  现在按平台记录：每次检测到该平台域名连接，记录时间、命中域名、代理链最终节点、
  该节点当时的出口 IP（`stats/ai-exposure-*.jsonl`）。UI 支持点击平台切换，
  以「IP+节点」分行的时间线图展示 24h / 7d / 30d 暴露变化，并列出最近记录。
- 去掉原来仅全局口径的「AI 平台流量/出口 IP 样本」误导性展示，补充记录原理说明

## [0.1.5] — 预发布

### Added
- **网络看板（新页面）**：本机存储与分析流量/网络状况
  - 分流流量按「校内 / 海外 / 直连 / SSH / 其他」分类统计，支持 24 小时 / 7 天 / 30 天视图
  - 出口 IP 追踪：直连出口 IP 与代理出口 IP（含节点名）定时采样
  - 校园网状态时间线（在校内 / 校外切换记录）
- **AI 平台 IP 追踪**：默认内置 ChatGPT / OpenAI / Claude / Gemini / Perplexity 等域名，
  支持用户自行添加域名前缀；展示 AI 平台流量与代理出口 IP 的长期变化（24h / 周 / 月）
- 数据存储于本机 `stats/` JSONL（按天滚动，默认保留 90 天，可配置采样/探测间隔），不上传云端

### Fixed
- 校园网卡片布局问题：判定依据换行、信号明细两行展示、手动覆盖按钮独立一行，修复文字截断/拥挤

## [0.1.4] — 预发布

### Changed
- **校园网检测 UI**：不再展示置信度百分比，直接显示「当前在校园网内 / 当前在校园网外」，
  并明确展示判定依据（默认 `Ping 10.134.140.1`）；判定依据可在「校内 VPN」页自行修改
- **导航结构**：「融合与规则」从并行配置中移出，归入「设置与工具」（与日志、设置同级）；
  并行配置仅保留 订阅 / 校内 VPN / SSH 转发 三项，删除 ①②③ 序号与上一步/下一步顺序导航
- **校内分流组**：改名 `🏫 校内 VPN`（原 FDU-RVPN），仪表盘标注「自动切换」并显示当前出口，
  明确提示「校内直连 / 校外经校内 VPN」由校园网检测自动决定

## [0.1.3] — 预发布

### Added
- **Ping 校内网关判定**：ping 通 `10.134.140.1` ⇒ 100% 在校内，自动跳过 fdu-connect、
  校内流量全部直连（即使经 NAT 路由器上网）
- **三配置并行**：订阅 / 校内 VPN / SSH 转发改为并行配置页签，可任意组合启用或单独停用，
  不再有 1→2→3 顺序
- **节点选择策略**：国外节点支持「手动固定」或「自动选择最低延迟」，可自定义测速 URL、
  测速间隔与切换容差；手动节点支持热切换；自动模式由 mihomo url-test 定时切换
- 订阅配置增加独立「启用/停用」开关

### Changed
- 校园网检测信号排序调整：Ping 网关 > 校内 IP > 内网探针 > Wi-Fi SSID
- 高级探测目标支持新增/编辑 ICMP Ping 类型（无需端口）；旧配置自动迁移补全 Ping 探针
- 退出应用时确保关闭系统代理并停止核心（避免残留代理设置导致断网）

## [0.1.2] — 预发布

### Fixed
- **核心启动超时**：端口冲突自动避让（Clash Verge/Shadowrocket 等占用 7897 时自动换端口并提示）
- 启动前清理上次异常退出遗留的核心进程（PID 文件校验进程名），避免端口被自己占用
- 核心控制端口就绪校验升级为「控制端口 + 混合端口」双重校验，超时提升至 60s 并输出最近核心日志
- **内置 geodata**（GeoSite.dat / geoip.metadb）：首启不再联网下载 30MB+，从根本上消除首次启动慢
- helper 模式同样内置 geodata 并清理遗留核心进程

### Changed
- **校园网置信度算法**：校内 IP / 复旦 SSID / 校园内网探针任一命中即保底 90% 置信度；
  经 NAT 路由器上网（本地 IP 非校内段）但内网可达时不再被误判为低置信度

## [0.1.1] — 预发布

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
