# LabVPN 用户指南

> 面向复旦大学师生：把「Clash 订阅 + 校内 RVPN + 公网 SSH」合成一个自动分流客户端。

## 1. 安装

从 [Releases](https://github.com/echo-edai/LabVPN-client/releases) 下载对应平台安装包：

| 平台 | 文件 | 说明 |
|---|---|---|
| macOS 12+ | `.dmg` | 拖入 Applications；首次打开如被拦截，右键 → 打开 |
| Windows 10/11 | `.msi` / `.exe` | 安装后从开始菜单启动 |
| Ubuntu 22.04+ | `.deb` / `.AppImage` | `sudo apt install ./LabVPN_*.deb` |
| AlmaLinux 9+ | `.rpm` | `sudo dnf install ./LabVPN-*.rpm` |

前置：本机需要 **fdu-connect**（校内 VPN 连接器）。LabVPN 会自动发现已有安装，
也可以在向导第 2 步手动指定路径（见 [BUILD.md](BUILD.md#22-fdu-connect校内-vpn)）。

## 2. 首次配置：三页向导

首次启动会进入配置向导（左侧「配置向导」可随时返回）。**三步 + 融合页**：

### 第 1 步：订阅

1. 在机场后台复制 **Clash / mihomo 订阅地址**（不是 SS/V2Ray 单链接）。
2. 粘贴到「订阅地址」，可修改名称与自动更新周期（默认 12 小时）。
3. 点击「导入订阅」：成功后显示节点数、代理组数、流量与到期时间。
4. 建议保持「由内核直接拉取订阅」开启，mihomo 会按周期自动更新节点。

> 也支持本地文件：`file:///Users/you/clash.yaml`。

### 第 2 步：校内 VPN

1. 填写**复旦上网账号（学号）**与**上网密码**（密码仅存入本机加密密钥库）。
2. 服务器默认 `stuvpn.fudan.edu.cn:443`；校园网下建议填写服务器 IP
   `202.120.224.58`，可避免被其他 TUN 的 fake-ip 干扰。
3. 「托管模式」：由 LabVPN 启动/停止 fdu-connect（推荐）；
   若你已用 launchd/systemd 常驻 fdu-connect，可切为「复用模式」。
4. 点击「自动检测」查找本机 fdu-connect；点击「测试登录」验证账号密码。
5. 保持「校园网自动检测」与「校内自动跳过 VPN」开启 —— **在校内时不连 VPN，直接访问**。

### 第 3 步：SSH 转发（可跳过）

1. 打开「启用 SSH 出站代理」，填写公网 IP、SSH 端口、用户名。
2. 选择认证方式：
   - 密码：只保存在本机加密密钥库；
   - 私钥：填写本机私钥路径（如 `~/.ssh/id_ed25519`），可加口令。
3. 在「走 SSH 的目标」中添加域名或 IP/CIDR（例如 `git.example.com`、`10.8.0.0/24`）。
4. 「测试连通性」检查 TCP 端口可达；凭据由 mihomo 在实际连接时校验。

### 第 4 步：融合与特调规则

- 选择**分流预设**：智能分流（默认）/ 极简 / 严格 / 全局 / 直连。
- 可开启广告拦截、国内 GEOSITE/GEOIP 直连、增强规则集。
- **自定义规则**：每行一条 mihomo 规则，可点「校验规则」检查语法与出口。
- 右侧显示规则条数、代理组数、生成警告，可展开查看最终 YAML。
- 底部「分流诊断」：输入域名/IP，查看预计命中的规则与出口。
- 点击「保存并连接」完成向导，进入总览。

## 3. 总览

- **连接开关**：中间圆形按钮一键连接/断开；「重连」用于刷新配置。
- **校园网检测**：实时显示是否在校内、置信度与各探测信号（可手动强制校内/校外）。
- **实时流量**：上下行速率曲线（最近 60 秒）。
- **代理组**：手动切换节点/出口（订阅、校内、SSH、广告、兜底）。
- **快速诊断**：输入目标域名看命中规则。

## 4. 分流规则（默认「智能分流」）

| 目标 | 出口 |
|---|---|
| `10.0.0.0/8`、`*.fudan.edu.cn`、复旦相关域名 | 🎓 校内（校内网络时自动直连） |
| 中国大陆域名 / IP | 直连 DIRECT |
| 广告 / 追踪域名 | 🛑 拦截 |
| 第 3 步绑定的 SSH 目标 | 🐧 SSH 转发 |
| 其他（国外） | 🚀 代理（订阅节点） |

优先级固定顺序，且**防环三保险**：VPN 服务器域名/IP 直连、fdu-connect 进程流量直连、
TUN 路由排除 VPN 服务器。

## 5. TUN 模式 vs 系统代理模式

| | TUN 模式（推荐） | 系统代理模式 |
|---|---|---|
| 接管范围 | 全部流量（含裸 IP/SSH/UDP） | 仅 HTTP/SOCKS 代理流量 |
| 权限 | 需要安装一次特权 helper（管理员授权） | 无需特权 |
| 何时使用 | 默认 | helper 未安装/拒绝授权时自动回退 |

「设置 → 特权服务」可安装/卸载 helper。macOS 安装为 LaunchDaemon，
Linux 安装为 systemd 服务，Windows 在每次启用 TUN 时请求一次 UAC。

## 6. 校内网络自动直连

- 检测信号：本机 IP 段、内网 TCP/DNS 探针（**绑定物理网卡**，可绕过 Shadowrocket/Clash 的 TUN 劫持）、Wi-Fi SSID。
- 检测到校内：**不启动 VPN**，`10.0.0.0/8` 与校内域名走 DIRECT；离开校园网后自动恢复 VPN。
- 可在总览/第 2 步手动强制「校内 / 校外」，或在设置中关闭自动检测。

## 6.5 从现有配置迁移（Clash Verge / 现有 fdu-connect 服务）

很多同学已经有一套「Clash Verge + launchd/systemd 的 fdu-connect」配置，迁移步骤：

1. **订阅**：把 Clash Verge 里正在用的机场订阅地址粘贴到向导第 1 步即可（节点规则由 LabVPN 重新生成，
   不需要导入旧配置的 proxies/规则片段）。
2. **已有 fdu-connect 服务**：
   - 你的 launchd/systemd 服务如果监听 `127.0.0.1:1080`，可把第 2 步切为「复用模式」，SOCKS 端口填 `1080`；
   - 建议迁移到「托管模式」：先 `launchctl unload ~/Library/LaunchAgents/com.fdu-connect.plist`
     （或 `systemctl --user disable --now fdu-connect`），再让 LabVPN 管理 fdu-connect 生命周期，
     这样校园网检测与自动跳过 VPN 才能生效。
3. **不要再设置 `dial_direct_proxy`**：LabVPN 由 mihomo 统一分流；旧配置把它指向 Clash 端口
   会形成环路。托管模式下 LabVPN 生成的 config.toml 已置空。
4. **关闭其他 TUN**：如果 Shadowrocket / Clash Verge 的 TUN 正在运行，先关闭，避免两个 TUN 抢路由。
   若必须共存，让 LabVPN 使用系统代理模式（功能受限：裸 IP 流量不会分流）。
5. 原有 Clash Verge 可以保留但不要同时开启，避免端口（7897/9097/1080/1082）与控制权冲突。

## 7. 隐私与安全

- 密码、TOTP、私钥口令保存在**本机加密密钥库**（AES-256-GCM + 机器绑定，文件权限 0600）。
- 订阅地址中的 token 在日志与诊断包中自动脱敏。
- 除订阅地址、规则集更新与你的 VPN/SSH 目标外，LabVPN 不发起其他网络请求。
- 生成的 mihomo 配置位于应用数据目录（0600），其中 SSH 密码为 mihomo 运行所必需。

## 8. 常见问题

见 [TROUBLESHOOTING.md](TROUBLESHOOTING.md)。快速入口：

- 连不上：先「设置 → 导出诊断包」并查看「日志」页。
- 内网打不开：确认第 2 步登录测试成功、且当前模式为 TUN（系统代理模式无法接管裸 IP）。
- 校内仍连 VPN：在总览把网络位置强制为「自动」并点「重新检测」。
- 与其他代理软件冲突：先退出 Clash/Shadowrocket 的 TUN 模式，或改用本应用统一分流。
