# Changelog

本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

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

## [0.1.9] — 预发布（当前）

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
