# 操作存档 / 交接（2026-10-22）

> 本文件用于记录当前项目状态与明日待办，方便下次会话直接继续。

## 当前状态

- **公开仓库**：<https://github.com/echo-edai/LabVPN-client>（全新对象存储的干净历史）
- **最新版本**：`v0.1.13`（预发布；TUN 自动路由自愈 + 校内 VPN 结构化日志与日志分析）
  - 修复 Apple Silicon（M 系列）从 Release 下载后提示「已损坏」的问题：macOS 无证书时自动 ad-hoc 签名
  - 资产：macOS ARM/Intel dmg、Windows exe/msi、Ubuntu deb/AppImage、AlmaLinux rpm、fdu-connect 源码归档
- 本机 App：v0.1.13 已构建、ad-hoc 签名并运行中
- 文档与截图：README / USER_GUIDE / RULES / ARCHITECTURE / CHANGELOG 已更新到最新功能；`docs/screenshots/` 已用当前 UI 重新截图（含新增 `analytics.jpg`）
- 测试：`cargo test -p labvpn-core` 66 通过；`cargo clippy --workspace --all-targets -- -D warnings` 通过

## 本次已完成的操作（存档）

1. **git 历史脱敏与仓库重建**
   - 全历史扫描发现的敏感内容：个人校园网内网 IP、RVPN 客户端内网 IP、真实订阅节点名与代理出口 IP、误提交的本地 helper.token 文件。
   - 已用 `git filter-branch` 重写全部历史并强制推送；随后发现 GitHub 仍缓存旧提交对象（可按旧 SHA 访问），
     因此：**旧仓库设为私有并改名为 `echo-edai/LabVPN-client-archive`，另建同名公开仓库并推送干净历史**。
   - 现状：新仓库所有 refs（main + v0.1.0~v0.1.7）均为脱敏后历史；旧 SHA 在新仓库返回 422；旧仓库匿名不可见（404）。
2. **v0.1.7 构建与发布**：GitHub Actions（公开仓库免费额度）四平台构建成功，已发布为 pre-release。
3. **文档更新**：README 版本/下载/功能/截图、USER_GUIDE 新增「网络看板与 AI 平台 IP 暴露」、ARCHITECTURE 增加分析模块、RULES 更新兜底直连与国内/国外显式规则。
4. **小红书**：已用 `xiaohongshu-mcp` v2.5.5 重新发布新版介绍帖（5 张最新 UI 截图 + 新功能文案）。
   旧帖（旧版截图）待手动删除；发布后登录态已清理。
5. **网络受限时的推送脚本**：新增 `scripts/push-via-api.py`（GitHub HTTPS 被校园网直连阻断时，用 GitHub API 推送提交/标签）。

## 明日待办（按优先级）

### 1. 小红书图片更新（已完成 2026-10-22 ✅，剩旧帖删除）
- 已完成：用新版截图重新发布（5 张：dashboard-dark / analytics / merge-rules / setup-subscription / setup-campus-vpn），
  登录态已清理。
- 剩余：在手机小红书 App 手动删除旧帖（同标题，位于新版下面）。
- 已用配图（5 张，`docs/screenshots/`）：`dashboard-dark.jpg`、`analytics.jpg`、`merge-rules.jpg`、
  `setup-subscription.jpg`、`setup-campus-vpn.jpg`
- 下次发布步骤（参考）：
  1. 下载 `xpzouying/xiaohongshu-mcp` v2.5.5 darwin-arm64 两个二进制（mcp + login）；
  2. 启动 MCP 服务（首次会下载内置 Chromium，约 150MB）：
     ```bash
     HOME="$PWD/.xhs-home" ./xiaohongshu-mcp-darwin-arm64 -headless -port :18060
     ```
     > 必须覆盖 HOME，否则默认写 `~/Library/Caches` 可能被沙箱拒绝；`.xhs-home/` 已在 .gitignore。
  3. 用 MCP 客户端调用 `initialize` → `get_login_qrcode`，把返回的 base64 存成 PNG 并**扫码登录**；
  4. 调用 `publish_content` 发布（标题/正文草稿见 `docs/xiaohongshu-post.md`；图片用本地绝对路径）；
  5. 发布成功后删除 `.xhs-home`（含登录态），并在手机上删除旧帖。
- 文案要点：仅交流、不提供任何 VPN/机场订阅或账号、需自备合法复旦账号与订阅、遵守学校规定、欢迎 fork。

### 2. ✅ 已彻底删除旧归档仓库（2026-10-22）
- `echo-edai/LabVPN-client-archive`（含旧提交对象）已通过网页删除，GitHub API 验证已不存在。
- 当前公开仓库 `echo-edai/LabVPN-client` 为全新对象存储，历史中无敏感信息。

### 3. 已修复：macOS「已损坏」问题（ad-hoc 签名，v0.1.8）
- 原因：无 Apple 证书时 Tauri 构建出的 .app 签名无效，Apple Silicon（M 系列）下载后 Gatekeeper 报「已损坏」。
- 修复：release workflow 在 macOS 未配置证书时自动 `APPLE_SIGNING_IDENTITY=-` 做 ad-hoc 签名，
  sidecar 也统一 ad-hoc 签名，并在构建后执行 `codesign --verify --deep --strict` 校验。
- v0.1.8 仍需用户右键打开/在隐私设置允许（非 Apple 公证）。

### 4. 待实现：macOS Network Extension（系统设置 → VPN 显示 LabVPN）
- 需求：像 Shadowrocket 一样在「系统设置 → VPN」出现并可被系统开关控制。
- 技术结论：必须用 `NEPacketTunnelProvider`；`networksetup`/`scutil --nc` 都无法创建 VPN 服务。
- 阻塞：Apple Developer Program（$99/年）+ Network Extension entitlement + Developer ID 签名/公证。
- 跟踪：GitHub issue #7，milestone v0.3.0；方案（轻量控制面 / 完整 tunnel）见 `docs/ARCHITECTURE.md`。
- 需维护者确认：是否购买/已拥有 Apple Developer Program 账号。

### 5. 可选：正式签名 / 公证
- 购买 Apple Developer Program（Developer ID）后配置 GitHub Secrets：`APPLE_SIGNING_IDENTITY`、`APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_ID`、`APPLE_PASSWORD`、`APPLE_TEAM_ID`、`KEYCHAIN_PASSWORD`；Windows 证书同理。
- 配置后重新打 tag 触发 Release，即可实现「下载即安装」。

### 4. 文档/截图后续
- [x] README 英文版：`README_en.md`（中文 README 顶部已加语言切换）
- 可选：浅色主题全页面截图；`docs/screenshots/` 文件名统一。

## 环境注意事项

- **GitHub HTTPS 从校园网直连会被阻断**：`api.github.com` 与 `ssh.github.com:443` 可用；
  推送提交请用 `scripts/push-via-api.py`（API 方式）或临时 deploy key + SSH 443 强制推送。
- GitHub Actions：公开仓库免费额度可用；私有仓库会触发付款/限额限制。
- 小红书登录态**不保存**在仓库中；`.xhs-home/`、`.xhs-post/` 已加入 .gitignore 并已清理。
- 本机运行时数据（配置/凭据/stats）位于 `~/Library/Application Support/app.labvpn.desktop/`，不属于仓库内容。

## 常用命令

```bash
# 逻辑测试 / 静态检查
cargo test -p labvpn-core
cargo clippy --workspace --all-targets -- -D warnings

# 构建并签名本机 macOS App
cd apps/desktop && npm run build
cd ../.. && bash scripts/sign-macos.sh target/release/bundle/macos/LabVPN.app
open target/release/bundle/macos/LabVPN.app

# 网络受限时推送（示例）
git ls-remote https://github.com/echo-edai/LabVPN-client.git refs/heads/main   # 取远端 sha（如可访问）
OLD_REMOTE=<远端 main sha> BASE_LOCAL=<上次推送的本地提交> NEW_LOCAL=$(git rev-parse HEAD) \
  SSL_CERT_FILE=/etc/ssl/cert.pem python3 scripts/push-via-api.py
```

## 关键链接

- 仓库：<https://github.com/echo-edai/LabVPN-client>
- Release：<https://github.com/echo-edai/LabVPN-client/releases/tag/v0.1.13>
- 旧仓库（私有归档）：`echo-edai/LabVPN-client-archive`
