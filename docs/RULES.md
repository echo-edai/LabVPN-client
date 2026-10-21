# LabVPN 分流规则设计（特调规则）

> 规则由 `crates/labvpn-core/src/rules.rs` **唯一生成**，前端只做展示与"自定义片段"编辑。
> 顺序即优先级；改动规则必须同步 `crates/labvpn-core/tests/generator_snapshot.rs` 快照。

## 1. 代理出口（proxies）

| 名称（常量） | 类型 | 来源 | 说明 |
|---|---|---|---|
| `🏫 FDU-RVPN` | `socks5` | fdu-connect `127.0.0.1:11080` | 仅在"托管模式"下由 LabVPN 启动；"复用模式"下指向用户已有实例（默认 1080） |
| `🖥 SSH-<host>` | `ssh` | 用户第 3 页配置 | mihomo 原生 SSH 出站，TCP 转发；支持密码或私钥 |
| `DIRECT` / `REJECT` | 内置 | mihomo | 直连 / 阻断 |

## 2. 代理组（proxy-groups，拓扑固定）

| 组名 | 类型 | 成员 | 用途 |
|---|---|---|---|
| `🚀 代理` | select | `♻️ 自动选择` + 订阅节点 + `DIRECT` | 国外流量总出口 |
| `♻️ 自动选择` | url-test | 订阅节点（`use: [订阅]`） | 延迟自动择优，间隔 300s |
| `🎓 校内` | select | `DIRECT`、`🏫 FDU-RVPN` | **校园网检测结果决定**：校内=`DIRECT`，校外=`FDU-RVPN` |
| `🐧 SSH 转发` | select | `DIRECT`、`🖥 SSH-*`、`🚀 代理` | SSH 绑定目标出口 |
| `🛑 广告拦截` | select | `REJECT`、`DIRECT` | 广告规则命中处 |
| `🐟 漏网之鱼` | select | `🚀 代理`、`DIRECT` | `MATCH` 兜底 |

> 组切换通过 mihomo REST API（`PUT /proxies/:name`）热生效，**不重启核心**。

## 3. 默认规则顺序（智能分流预设）

```
 1  DOMAIN,<vpn-server-host>,DIRECT                 # VPN 控制连接直连，防环（必选）
 2  IP-CIDR,<vpn-server-ip>/32,DIRECT,no-resolve    # 若解析得到，双保险
 3  PROCESS-NAME,<fdu-connect 进程名>,DIRECT         # VPN 进程自身流量直连（mac/win/linux）
 4  PROCESS-NAME,<mihomo 进程名>,DIRECT
 5  IP-CIDR,127.0.0.0/8,DIRECT,no-resolve
 6  RULE-SET,LAN,DIRECT                             # 私有网段/本地保留
 7  IP-CIDR,10.0.0.0/8,🎓 校内,no-resolve            # ← 核心需求：内网 IP
 8  RULE-SET,FDUDomain,🎓 校内                       # 复旦域名
 9  <SSH 绑定规则（若配置）>                          # DOMAIN/IP-CIDR → 🐧 SSH 转发
10  RULE-SET,ADBlock,🛑 广告拦截                      # 若开启
11  RULE-SET,ChinaDomain,DIRECT                     # 国内域名
12  GEOIP,CN,DIRECT,no-resolve                      # 国内 IP
13  MATCH,🐟 漏网之鱼                                # 其余 → 订阅
```

设计要点：

1. **校内优先**：`10/8` 与复旦域名必须先于 CN 直连/订阅规则。
2. **防环三保险**：VPN 服务器域名/ IP 直连 + 进程名直连 + TUN `route-exclude-address`。
3. **no-resolve 节约 DNS**：纯 IP 规则不做反向解析。
4. **SSH 绑定**可插在 CN 之前（用户自定义优先级"最高/普通"两档）。
5. 规则集来源与更新见 §6。

## 4. 预设（Routing Preset）

| 预设 | 说明 | 与智能分流的差异 |
|---|---|---|
| `smart`（默认） | 智能分流 | 上文完整顺序 |
| `minimal` | 极简三出口 | 仅：校内 → `🎓 校内`，SSH → `🐧 SSH`，MATCH → `🚀 代理`（不做 CN 直连） |
| `strict` | 严格分流 | 开启广告拦截；CN 域名/IP 直连，其余全部代理；不含 SSH 分流 |
| `global` | 全局代理 | mihomo `mode: global`，仅保留防环规则（实际走 `🚀 代理`） |
| `direct` | 全部直连 | `mode: direct`，仅用于排查 |

预设可被用户"另存为自定义"。

## 5. 自定义规则（高级）

在融合页/规则页以文本编辑，语法即 mihomo rules 语法，逐行一条：

```yaml
- DOMAIN,git.example.com,🚀 代理
- DOMAIN-SUFFIX,openai.com,🚀 代理
- IP-CIDR,192.168.10.0/24,DIRECT,no-resolve
- IP-CIDR6,2001:da8:8000::/33,🎓 校内,no-resolve
- PROCESS-NAME,ssh,🐧 SSH 转发
- GEOIP,JP,🚀 代理
```

校验规则（应用前执行，失败不落盘）：

1. 类型必须在白名单：`DOMAIN / DOMAIN-SUFFIX / DOMAIN-KEYWORD / IP-CIDR / IP-CIDR6 / GEOIP / GEOSITE / PROCESS-NAME / RULE-SET / MATCH`。
2. 目标必须是已存在的代理、代理组或内置策略。
3. `MATCH` 只能出现一次且必须在末尾（由系统补全）。
4. 自定义规则默认插入到「SSH 规则之后、CN 直连之前」；用户可勾选"最高优先级"插到 `10/8` 之前（谨慎）。
5. 单行 ≤ 512 字符，总条数 ≤ 500。

## 6. 规则数据源（`resources/rules/`）

| 文件 | 内容 | 来源 | 更新 |
|---|---|---|---|
| `fdu-domain.yaml` | `DOMAIN-SUFFIX,fudan.edu.cn` / `fducslg.com` / `fduhole.com` 等 | [fdu-netrules](https://github.com/AkiraSalvare/fdu-netrules) + 官方域名核对 | `npm run sync:rules` 同步 |
| `fdu-ip.yaml` | `IP-CIDR,10.0.0.0/8` | fdu-netrules | 同上 |
| `lan.yaml` | 私有/保留网段 | 内置 | 手工 |
| `china-domain.yaml` | CN 域名（离线兜底） | ACL4SSR / meta-rules-dat | 同步脚本 |
| `china-ip.yaml` | CN IP（离线兜底） | meta-rules-dat | 同步脚本 |
| `adblock.yaml` | 广告域名 | ACL4SSR BanAD | 同步脚本 |

> mihomo 也支持 `GEOSITE,cn` / `GEOIP,CN`（依赖 geo 数据文件）；生成器优先使用内置离线 rule-provider（可用、可审计），geo 规则作为可选开关。

## 7. DNS 设计（fake-ip）

```yaml
dns:
  enable: true
  ipv6: false
  enhanced-mode: fake-ip
  fake-ip-range: 198.18.0.1/16
  fake-ip-filter:            # 必须真实 IP 的域名（校内域名**不**在此列）
    - '*.lan'
    - '*.local'
    - '+.localhost'
  nameserver: [223.5.5.5, 119.29.29.29]
  proxy-server-nameserver: [223.5.5.5]   # 订阅/SSH 服务器域名直连解析
  nameserver-policy:
    '+.fudan.edu.cn': [202.120.224.26]   # 校内 DNS（仅校内可用，失败回退）
    '+.fducslg.com': [202.120.224.26]
```

- 校内域名规则命中在 DNS 之前（DOMAIN 规则），因此离校时域名会透传给 FDU SOCKS，由其校内 DNS 解析，无需本地可达。
- 校内域名**保持 fake-ip**：在校内时连接回到 mihomo 后由 `nameserver-policy` 用校内 DNS 解析；离校时由 fdu-connect 解析，两条路径都可用。
- `nameserver-policy` 仅在需要本地真实解析（如 DIRECT 直连校内）时生效。
- 若用户手工切换"直连模式"，DNS 同步回退为系统解析。

## 8. TUN 配置（TUN 模式）

```yaml
tun:
  enable: true
  stack: mixed            # macOS/Linux 推荐 mixed；Windows 用 gvisor
  auto-route: true
  auto-detect-interface: true
  dns-hijack: [any:53, tcp://any:53]
  route-exclude-address:
    - 202.120.224.58/32   # VPN 服务器，防环
  mtu: 1500
  strict-route: false     # Windows 下如遇路由冲突开启
```

## 9. 规则命中测试

融合页提供"分流诊断"输入框（域名或 IP），Rust 侧按最终规则列表做**顺序模拟匹配**，返回：

```json
{ "input": "www.fudan.edu.cn", "matched": "RULE-SET,FDUDomain", "outbound": "🎓 校内", "note": "校园网内将直连" }
```

实现：`crates/labvpn-core/src/rules/matcher.rs`（DOMAIN/SUFFIX/KEYWORD/IP-CIDR/GEOIP 简化模拟；GEOIP 用内置 CN 清单近似）。
该功能与真实 mihomo 决策一致性由集成测试保证（同一批样本对比 `GET /connections` 的 chains）。

## 10. 可扩展性约定

- 新增规则集 = 新增 `resources/rules/*.yaml` + 在 `rules::providers()` 注册 + 文档登记。
- 新增预设 = 实现 `preset::Preset` 并注册到 `PRESETS`，前端从同一 API 拉取（不硬编码）。
- 规则生成器保持纯函数：`build(config, ctx) -> RulePlan`，其中 `RulePlan { groups, providers, rules, warnings }` 可单独快照测试。

## 代理节点选择策略

国外流量最终出口为 `🚀 代理` 组，其节点策略由「融合」页的**国外节点选择**配置决定：

| 模式 | mihomo 生成方式 | 行为 |
|---|---|---|
| 自动（默认） | `♻️ 自动选择` 组为 `url-test`，成员为订阅 provider | 按配置的 **测速 URL / 间隔 / 容差** 自动选延迟最低节点并定时切换（mihomo 内置定时器，不需要 App 轮询） |
| 手动 | `🚀 代理` 组为 `select`，成员仅 provider；App 启动后调用 REST API 固定用户指定节点 | 固定使用所选节点；修改节点时热切换（`PUT /proxies/🚀 代理`），无需重启核心 |

- 测速 URL 默认 `http://www.gstatic.com/generate_204`，间隔默认 300s（允许 30–3600s），容差 50ms。
- 手动模式下节点列表来自本地订阅缓存（`profiles/profile.yaml` 的 `proxies`）。
- 策略修改（模式/URL/间隔/容差）会重新生成配置并重启核心；仅切换手动节点为热切换。

## 国内直连与兜底策略

规则顺序（在自定义规则之后）：

1. `DOMAIN-SUFFIX,cn` → DIRECT（所有 .cn 域名）
2. 内置 `ChinaDomainFallback` 规则集 → DIRECT（常见国内站点兜底）
3. `GEOSITE,cn` → DIRECT（国内域名库）
4. `GEOIP,CN` → DIRECT（国内 IP 库，no-resolve）
5. `GEOSITE,geolocation-!cn` → 🚀 代理（国外域名显式走订阅代理）
6. `MATCH,🐟 漏网之鱼`

**🐟 漏网之鱼 默认出口为 DIRECT**（可手动切换到代理）。因此未命中任何规则的流量默认直连，
不会被动走代理；国外流量由第 5 条显式路由到订阅节点，不再依赖兜底组。

> `GLOBAL` 是 mihomo 内置的全局出口组（Global 模式下使用），规则模式下不参与分流，
> UI 已隐藏。

## 校园网判定与直连

1. **Ping 校内网关**（默认 `10.134.140.1`）：ping 通 ⇒ 置信度 **100%**，判定在校内；
   自动跳过 fdu-connect（不建立 RVPN），`🎓 校内` 组切到 `DIRECT`，10/8 与复旦域名/IP 全部直连。
2. 否则按校内 IP 段 / 校园内网探针 / 复旦 Wi-Fi 综合评分（强证据保底 90%）。
3. 判断为校内且开启「校内自动跳过 VPN」时，即使 RVPN 已配置也不会启动/会主动停止。
