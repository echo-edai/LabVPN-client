# LabVPN v0.1.22

## 分流规则导入

- 「融合与规则 → 分流规则档案」新增本地文件导入，支持 Shadowrocket `.conf`、纯文本 `.list/.txt/.rules` 和 Clash YAML `rules` / `payload`。
- 可直接预览 Johnshall 的公开 `lazy_group`，兼容的外链规则转为本地快照。导入前显示策略映射、兜底和跳过项；不载入节点、凭据、脚本、URL 重写或 MITM。
- 导入档案不覆盖手写规则，保留 LabVPN 的校内和 SSH 优先级。用「加入草稿」确认后再保存；运行中的会话不会因预览而重连。

## TUN 与恢复

- 应用确认 mihomo 的 TUN 与自动路由已生效后才显示连接成功。恢复监控不把可达的控制端口当成 TUN 正常。
- macOS helper 不跨会话补路由，也不覆盖现有路由；正常停止时先让核心自行释放路由。
- 使用 TUN 的用户，请在重要连接结束后断开 LabVPN，并到「设置 → 特权服务（TUN）」更新 helper。macOS 如使用 ad-hoc 签名，不代表 Apple 公证。

## 验证

- 2026-11-22 的公开档案成功转换 34 个规则集、47,406 条规则；138 条不兼容记录在预览中报告。生成配置通过 mihomo v1.19.32 的 `-t` 校验。
- 在不启用校内 VPN的测试配置中，`www.youtube.com` 命中订阅组，`www.bilibili.com` 命中导入直连规则，`www.fudan.edu.cn` 仍优先命中 LabVPN 校内规则。
- Rust 全仓测试、Clippy、TypeScript 类型检查及 Web 构建均通过。[深色截图](screenshots/rule-import-dark.png)与[浅色截图](screenshots/rule-import-light.png)使用浏览器模拟数据。

详见 `docs/RULES.md` 与 `docs/RESUME-RECOVERY.md`。导入档案只改变分流，不改变远端服务的地区或账号资格。
