# 第三方代理软件卸载验收

日期：2026-11-15（Asia/Shanghai）。用户明确要求完全卸载 EasyConnect、LeiGod、MotionPro。本记录描述本机卸载，不修改 LabVPN 软件版本。

## 卸载前

- EasyConnect 主程序、EasyMonitor、ECAgent、ECAgentProxy 仍存在。
- LeiGod 主程序已不在应用目录，特权 helper 仍运行。
- MotionPro 主程序已不在应用目录，`/usr/local/motionpro` 与 vpnd 仍存在。
- 三个产品共五个 launchd 启动文件、用户配置和日志仍有残留。此前停用自启动没有结束这些当前会话中的进程。

## 已执行

- 用户完成 macOS 本机管理员验证，按已核对的确切程序路径停止进程并移除文件。
- 移除 EasyConnect 主程序、MotionPro 程序目录与命令链接、LeiGod 特权 helper。
- 卸载并删除 `com.sangfor.EasyMonitor`、`com.sangfor.ECAgentProxy`、`com.arraynetworks.vpnd`、`com.arraynetworks.mhttpd`、`com.leigod.helper` 五个 launchd 服务。
- 移除归属于三个产品的应用支持目录、偏好设置、日志与 EasyConnect 临时锁文件；实际删除 24 个已存在的文件或目录。
- 清除 `com.sangfor.EasyConnect` 与 `com.arraynetworks.motionpro` 两条安装包记录；未发现 LeiGod 安装包记录。
- 所有执行脚本与本机原始验收输出放在忽略的 `work/`，临时提权脚本在完成后清理。

## 验收结果

| 项目 | 结果 |
| --- | --- |
| 三个产品的运行进程 | 未发现 |
| 五个目标 launchd 服务 | 均已卸载，查询返回服务不存在 |
| 安装包记录 | 未发现三个产品的匹配记录 |
| 应用、系统与当前用户常见配置和缓存目录 | 未发现匹配残留 |
| root 用户常见配置、缓存与日志目录 | 未发现匹配残留 |
| 系统网络扩展 | 0 个 |
| 已加载的相关内核组件 | 未发现 |
| 三个产品的应用防火墙记录 | 未发现 |
| 有效 HTTP、HTTPS、SOCKS、WPAD 代理 | 均关闭 |
| 卸载后 HTTPS 访问 | HTTP 200，约 77 ms |
| LabVPN 当前运行 | 核心与 helper 正常，未重连或停止现有连接 |
| LabVPN 内一键检查网络 | 系统路由 HTTPS 200，67 ms；代理出口 HTTPS 204，106 ms |
| LabVPN 代理检测面板 | “本次未识别到；不代表系统内不存在其他代理” |

本次没有重启电脑。以上结果覆盖运行组件、启动服务、安装包记录与已检查的常见系统及当前用户目录；不等于删除下载目录中的安装包归档、备份或 macOS 保留的历史记录。LabVPN、Shadowrocket 及其他未指定的软件保留。
