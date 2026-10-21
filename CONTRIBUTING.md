# 贡献指南

欢迎参与 LabVPN！请先阅读 [`AGENT.md`](AGENT.md)（开发规范、分层与目录约定）与
[`demand.md`](demand.md)（需求与验收标准）。

## 开发流程

1. Fork 仓库并从 `main` 创建分支：`feat/xxx`、`fix/xxx`、`docs/xxx`。
2. 本地验证：
   ```bash
   cargo test -p labvpn-core
   cargo clippy --workspace --all-targets -- -D warnings
   cargo fmt --all
   cd apps/desktop && npm install && npm run typecheck && npm run build:web
   ```
3. 提交信息遵循 `type(scope): summary`（如 `feat(rules): 支持 SSH 绑定`）。
4. 发起 PR，描述中写明：动机、改动点、验证方式（命令+结果）、风险与回滚。

## 代码边界（硬性）

- 纯逻辑（配置/规则/生成器/解析/探测）放 `crates/labvpn-core`，不依赖 Tauri。
- 命令层只做参数校验与 service 调用；平台差异收敛到 `platform/`。
- 前端不生成 mihomo 配置；不直接调用 `@tauri-apps/api`（统一走 `lib/ipc.ts`）。
- 涉及分流规则的改动必须附「规则命中测试」结果，并更新 `docs/RULES.md`。

## 不接受的改动

- 提交密钥、订阅地址、日志、二进制 sidecar；
- 在日志/错误信息中输出密码或 token；
- 引入新的重量级依赖而无说明；
- 破坏 `macOS / Windows / Linux` 任一平台的构建。

## 发布

由维护者打 tag `vX.Y.Z`，Release 工作流自动构建四平台产物（草稿）。详见
[`docs/BUILD.md`](docs/BUILD.md)。
