# 本机启动治理记录

日期：2026-11-15。此文件只记录已识别软件及服务，不保存凭据或原始系统清单。

已通过 launchctl 确认下列标签为 disabled：

| 服务 | 标签 | 域 |
|---|---|---|
| 独立校园连接器 | com.fdu-connect | 当前用户 GUI |
| EasyConnect 代理 | com.sangfor.ECAgentProxy | 当前用户 GUI |
| EasyConnect 监控 | com.sangfor.EasyMonitor | system |
| MotionPro HTTP 服务 | com.arraynetworks.mhttpd | 当前用户 GUI |
| MotionPro VPN 服务 | com.arraynetworks.vpnd | system |
| 雷神后台服务 | com.leigod.helper | system |

Clash Verge 配置中 enable_auto_launch 为 false，enable_tun_mode 为 false。此次不修改其订阅或配置。
LabVPN 登录启动项保留；其 helper 可以空闲运行，但新版应用在手动连接之前不启动转发核心。

本次停用只改变后续自动启动行为，没有卸载软件或清空其配置。当前已经运行的第三方进程可能继续存活至正常退出或重启；这与启动项是否允许下次加载是两个状态。

需手动恢复某个服务时，可在系统设置重新启用其后台项，或对对应域/标签运行 launchctl enable；系统域需要管理员权限。原始检查文件仅保存在被 Git 忽略的 work/audit/ 中。

没有实际重新启动电脑，不能把静态启动项验证表述为“已通过重启验收”。新版发布后还需验证原生应用的启动、连接、断开和退出。

已将 Wi-Fi 的 WPAD 自动发现从 On 改为 Off。复查 scutil 有效状态：HTTP/HTTPS/SOCKS 为关闭，ProxyAutoDiscoveryEnable 为 0，原先的动态 PAC 项不再出现。Shadowrocket 的 OnDemandEnabled 为 FALSE，当前未连接。

原生 v0.1.20 验收：安装包签名校验通过，helper 升级至 0.1.20；打开应用保持未连接，系统路由 HTTPS 返回 200；退出后仅空闲 helper 留存，无 mihomo 或 fdu-connect，手动代理与自动发现均关闭。验收发现启动项开关未同步系统，已在 v0.1.21 源码修正，待最终安装验证。
