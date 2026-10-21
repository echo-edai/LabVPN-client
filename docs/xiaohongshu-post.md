# 小红书发布草稿（配图已更新，待重发）

> 旧帖配图为旧版本 UI；`xiaohongshu-mcp` 无编辑/删除接口，需重新发布后在手机端删除旧帖。
> 配图请使用本仓库 `docs/screenshots/` 下的最新截图（本地绝对路径）。

## 标题

复旦校园网分流客户端开源了

## 正文

最近给自己写了一个复旦校园网分流工具 LabVPN，顺手开源出来，欢迎交流使用 🙌

它解决的是我每天都要折腾的事：校园网、自己的订阅、公网 SSH 是三套配置，手动切换很麻烦。现在做成三个并行配置 + 一套自动分流规则：

· 10.0.0.0/8 与复旦域名/IP → 自动走校内直连 / 校内 VPN
· 国内网站 → 显式直连
· 国外网站 → 显式走自己的订阅节点
· 指定目标 → 可走 SSH 转发
· 兜底流量 → 默认直连

几个我比较喜欢的功能：
1｜校内自动检测：ping 通校内网关就判定在校园网内，自动直连并跳过 VPN，不用手动切
2｜节点策略：可手动固定节点，也可按自定义测速 URL + 间隔自动选最低延迟
3｜网络看板：本机记录流量分类、出口 IP、校园网状态
4｜AI 平台 IP 追踪：按域名记录「ChatGPT/Claude/Gemini 等平台实际看到的出口 IP 与节点」，点平台可看 24h/周/月 IP 变化
5｜三配置并行：订阅 / 校内 VPN / SSH 转发可任意组合启用，不是强制三步走
6｜跨平台：macOS / Windows / Ubuntu / AlmaLinux，基于 Tauri 2 + mihomo，内置 fdu-connect

⚠️ 声明：仅做学习交流与技术研究，不提供、不分享任何 VPN / 机场订阅、账号或代理服务。需要自己有合法的复旦校园账号和自己购买/自有的订阅，并遵守学校网络规定与当地法律法规。

项目地址：https://github.com/echo-edai/LabVPN-client
欢迎 fork、提 issue，一起完善对复旦同学的代理支持 🙏

## 话题标签

#复旦 #开源 #校园网 #Tauri #Rust #效率工具 #学生开发者 #网络工具

## 配图（本地绝对路径，按顺序）

1. `docs/screenshots/dashboard-dark.jpg`（封面）
2. `docs/screenshots/analytics.jpg`（网络看板）
3. `docs/screenshots/merge-rules.jpg`（融合与规则）
4. `docs/screenshots/setup-subscription.jpg`（并行配置）
