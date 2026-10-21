## 0. 你的角色

你是 LabVPN 项目的实现者与维护者。LabVPN 是面向复旦大学师生的跨平台 VPN 分流客户端：

- 一个桌面应用（Tauri 2）：负责配置、UI、生命周期与系统集成。
- 一个代理核心（mihomo，独立 sidecar）：负责真正的高速分流。
- 一个校内 VPN 连接器（fdu-connect，独立 sidecar）：负责访问复旦内网。
- 一个特权 helper（Rust）：仅当启用 TUN 模式时用于提权托管核心。

你的默认行为准则：

1. **先读后写**：改动前阅读相关模块、`docs/ARCHITECTURE.md` 与现有测试。
2. **小步提交**：一个改动只解决一件事，附带测试或验证命令。
3. **不破坏分层**：见第 3 节硬性规则；不确定时保持边界，不要"顺手重构"。
4. **用户可见文案中英双语**（`apps/desktop/src/locales/`）。
5. **不引入大依赖**：新依赖必须在 PR/变更说明中写明理由、体积与替代方案。
6. **安全优先**：任何涉及密码/密钥/提权/子进程的改动，先读第 7 节红线。

---

## 1. 文档地图（阅读顺序）

| 文档 | 内容 |
|---|---|
| `demand.md` | 精简需求、验收标准、默认参数 |
| `docs/ARCHITECTURE.md` | 分层、进程模型、数据流、平台差异、helper 协议 |
| `docs/RULES.md` | 分流规则模型、优先级、预设、自定义语法 |
| `docs/RESEARCH.md` | 选型依据与上游项目（含许可证） |
| `docs/BUILD.md` | 三平台构建、打包、签名、sidecar 获取 |
| `docs/USER_GUIDE.md` | 用户三页配置流程与 FAQ |
| `docs/TROUBLESHOOTING.md` | 常见故障排查 |

---

## 2. 目录结构与职责（强制）

```
LabVPN-client/
├── demand.md / AGENT.md / README.md / LICENSE     # 根目录只放这四类文档
├── docs/                     # 设计、构建、用户、排障文档（Markdown）
├── apps/
│   └── desktop/              # 桌面应用（唯一可执行 UI）
│       ├── src/              # React 前端（只做 UI/交互，不含业务规则）
│       │   ├── components/   # ui/（无状态原子组件）、layout/、features/
│       │   ├── pages/        # 路由页面，一页一文件
│       │   ├── stores/       # Zustand 状态（按领域拆分）
│       │   ├── lib/          # ipc 封装、格式化、常量、平台判断
│       │   ├── locales/      # zh-CN.ts / en-US.ts
│       │   └── styles/       # 设计系统（tokens、Tailwind 入口）
│       └── src-tauri/        # Rust 应用层（命令、状态、系统集成）
│           ├── src/commands/ # #[tauri::command]，只做参数校验+调用 service
│           ├── src/services/ # 业务编排（engine/vpn/subscription/campus/...）
│           ├── src/platform/ # 平台差异实现（service、system proxy、autostart）
│           ├── capabilities/ # Tauri 权限声明（最小化）
│           └── binaries/     # sidecar 占位（二进制不入库，脚本下载）
├── crates/
│   ├── labvpn-core/          # 纯逻辑库：模型、规则、配置生成、订阅解析、探测
│   └── labvpn-helper/        # 特权 helper 二进制（只被安装/提权后运行）
├── resources/
│   ├── rules/                # 内置规则数据（FDU、CN、预设），数据驱动
│   └── icons/                # 应用图标源文件（generate-icons.py 生成）
├── scripts/                  # 开发/构建/发布脚本（Node 或 Shell，单一职责）
├── .github/workflows/        # CI 与三平台发布
└── （测试）                   # 单元测试就近放模块内；跨模块快照放 crates/*/tests/
```

**归属判定口诀**：

- 能在没有 Tauri、没有网络、没有文件系统的环境里单测 → `crates/labvpn-core`。
- 需要读写文件、起进程、调 API、查系统状态 → `apps/desktop/src-tauri/src/services`。
- 只是一层 `#[tauri::command]` → `commands/`，**不超过 30 行**，禁止写业务逻辑。
- 平台分支（macOS/Windows/Linux）→ `platform/`，用 trait + `#[cfg]` 收敛，**禁止散落 `cfg!(target_os)` 业务判断**。
- 前端不生成/拼接 mihomo 配置；一切配置由 Rust 侧生成，前端只预览与编辑"规则片段"。

---

## 3. 分层与依赖方向（硬性规则）

```
前端 React ──(Tauri IPC)──> commands ──> services ──> labvpn-core
                                   │            └─> platform (trait)
                                   └───────────────> crates/labvpn-helper（仅 IPC 协议共享类型）
```

1. `labvpn-core` **禁止**依赖 tauri、tokio、reqwest、keyring 等应用层 crate；只允许 serde、thiserror、libc/windows-sys（探测用）。
2. 生成 mihomo 配置的函数必须是**纯函数**：`(&AppConfig, RuleContext) -> GeneratedConfig`，相同输入必定相同输出，可快照测试。
3. 前端通过 `lib/ipc.ts` 统一封装 `invoke`；组件中禁止直接调用 `@tauri-apps/api` 原始 API。
4. 任何新页面必须：路由注册 → store 字段 → ipc 方法 → Rust command → service → 测试。
5. 命令返回统一用 `Result<T, CommandError>`（`serde` 序列化为 `{ code, message, detail? }`），禁止 `unwrap()` 直接抛给前端。
6. 事件（日志、状态、流量）统一走 `services/events.rs` 定义的 channel 名常量，前后端共用一份 TS 常量镜像。

---

## 4. 技术栈与版本

| 层 | 技术 | 约定 |
|---|---|---|
| 桌面壳 | Tauri 2.12+ | 只用官方插件：`autostart`、`single-instance`；打开外部路径/URL 走自研 `platform::open_path` |
| 前端 | React 19 + TypeScript 5.x + Vite 8 | 函数组件 + hooks；禁止 class 组件 |
| 样式 | Tailwind CSS 4（`@tailwindcss/vite`） | 颜色/圆角/阴影走 `styles/tokens.css`，禁止魔法色值 |
| 状态 | Zustand 5 | 按领域分 store，禁止全局万能 store |
| 路由 | React Router 7 | 页面级懒加载 |
| 图标 | lucide-react | 禁止引入第二套图标库 |
| 后端 | Rust 1.85+（stable，2024 edition） | 异步用 tokio；错误用 thiserror/anyhow |
| 核心 | mihomo（sidecar） | 只通过配置文件 + REST API 交互，不 fork 修改 |
| 校内 VPN | fdu-connect（sidecar，Go） | 只通过 config.toml + 日志 + SOCKS 端口交互 |
| YAML | serde_yaml_ng | 生成用 `serde_yaml_ng::Value`，避免手拼字符串 |

---

## 5. 编码规范

### Rust
- 命名：crate/模块 snake_case；类型 CamelCase；常量 SCREAMING_SNAKE。
- 模块文件优先 `foo.rs`，子模块多时 `foo/mod.rs`；单文件 ≤ 400 行为宜。
- 公开 API 必须写 `///` 文档注释；`pub` 范围最小化。
- 禁止 `panic!`/`unwrap`/`expect` 出现在 service 层以上（测试除外）；用 `Result` + 上下文。
- 平台代码：`#[cfg(target_os = "macos")]` 放在 `platform/<os>.rs` 内部实现，trait 签名跨平台一致。
- 新增外部命令调用必须：参数数组化（禁止拼 shell 字符串）、超时、捕获 stdout/stderr、脱敏日志。

### TypeScript / React
- 严格模式；禁止 `any`（确需时 `unknown` + 类型守卫）。
- 组件名 PascalCase，文件名与组件同名；hooks 以 `use` 开头。
- 组件 ≤ 200 行，超出拆 `features/`。
- 所有用户可见文案进 locales；数字/时间用 `lib/format.ts`。
- 状态更新用不可变写法；副作用集中在 `useEffect` + 清理函数。

### 提交与变更说明
- 提交信息：`type(scope): summary`，如 `feat(rules): 支持 SSH 绑定规则`。
- 每个 PR/变更必须包含：动机、改动点、验证方式（命令+结果）、风险与回滚。
- **禁止**提交：二进制 sidecar、`node_modules`、`.toolchain/`、真实订阅地址/密码、日志文件。

---

## 6. 分流规则约定（核心业务，改动需谨慎）

1. 规则**顺序即优先级**，只能由 `crates/labvpn-core/src/rules.rs` 的构建器生成；禁止前端拼规则顺序。
2. 固定前三条必须存在：
   - `DOMAIN,<vpn-server>,DIRECT`（VPN 控制连接直连，防环）
   - `IP-CIDR,<vpn-server-ip>/32,DIRECT,no-resolve`（若解析得到）
   - `PROCESS-NAME,<fdu-connect 进程名>,DIRECT` / 核心进程直连
3. 校内分流统一走代理组 `🎓 校内`（select：`DIRECT` / `🏫 FDU-RVPN`）。程序根据校园网检测结果切换组，**不要**通过重启核心实现切换。
4. 代理组命名与规则引用必须来自 `crates/labvpn-core/src/rules/names.rs` 同源常量（前端 `lib/constants.ts` 镜像）。
5. 新增内置规则集：数据文件放 `resources/rules/`，在 `docs/RULES.md` 登记来源/许可证/更新方式，并加同步脚本。
6. 修改生成器必须更新 `crates/labvpn-core/tests/generator_snapshot.rs` 快照并说明原因。

---

## 7. 安全红线（违反即拒绝合入）

1. 密码、TOTP、私钥**不得**写进明文配置、日志、崩溃报告或前端持久化。
   - 前端表单只在内存持有；提交后立即清空。
   - 秘密存储只允许 `services/secrets.rs`（系统钥匙串优先，失败回退本机加密文件，0600）。
2. fdu-connect 的密码如果必须落盘，只能写进临时 `config.toml`（权限 0600，退出即删），不得进命令行参数（`ps` 可见）。
3. helper 请求必须带一次性/长期令牌校验；helper 只接受固定子命令与固定核心路径；只监听回环。
4. 不接受来自渲染进程的任意路径执行/任意命令参数（命令白名单，参数结构化）。
5. 订阅 URL 可能携带 token：日志、截图、导出配置中默认打码。
6. 遵守上游许可证：mihomo（GPL-3.0）、fdu-connect（AGPL-3.0）作为**独立进程**使用；不静态链接、不修改后重新分发而不开源。详见 `docs/RESEARCH.md`。

---

## 8. 验证清单（每次改动必须执行）

```bash
# 1) 纯逻辑单测 + 快照
cargo test -p labvpn-core

# 2) Rust 全量编译检查（无 Tauri 运行时依赖）
cargo check --workspace --all-targets

# 3) 前端类型检查与构建
cd apps/desktop && npm run typecheck && npm run build

# 4) 生成配置语法校验（需要 sidecar）
npm run sidecars && ./apps/desktop/src-tauri/binaries/mihomo -t -d ./tmp -f ./tmp/config.yaml

# 5) 手动冒烟（本机）：配置向导 → 连接 → 分流验证
#    校内：curl -m 3 https://10.203.8.198；校外：curl ipinfo.io；国内：curl baidu.com
```

- 涉及 UI 的改动必须附**深浅色**截图（`docs/screenshots/`）。
- 涉及分流的改动必须附「规则命中测试」结果（规则页输入 3 个样本域名的输出）。

---

## 9. 扩展指引（按场景照做）

| 想做什么 | 改动位置 |
|---|---|
| 新增一条内置规则/域名 | `resources/rules/*.yaml` + `docs/RULES.md` 登记 + 单测 |
| 新增规则预设 | `labvpn-core/src/rules/presets.rs` + 前端 presets 列表（同源常量） |
| 支持新订阅格式 | `labvpn-core/src/subscription/` 新增 parser 实现 + 测试 fixtures |
| 新增协议出口（如 WireGuard） | 优先使用 mihomo 原生 outbound；仅在 `generator` 增加映射，不写连接逻辑 |
| 新增平台能力 | `apps/desktop/src-tauri/src/platform/` 加 trait 方法 + 三平台实现 |
| 新增页面 | `pages/` + 路由 + locales 两语言 + 侧边栏入口 |
| 接入新特权操作 | 先扩展 `docs/ARCHITECTURE.md` 的 helper 协议，再实现 helper + 客户端 + 测试 |

---

## 10. 常见坑（血泪经验）

1. **TUN 与第三方代理冲突**：检测现有 utun/TUN 网卡；提示用户关闭其他 TUN，或自动改用系统代理模式。
2. **fdu-connect 的 `dial-direct-proxy`**：我们让 mihomo 做分流，给 fdu-connect 传空值，避免回环。
3. **VPN 服务器必须直连**：否则 TUN 抓取控制连接导致登录失败，规则与 `route-exclude-address` 双保险。
4. **fake-ip 与校内域名**：校内域名规则必须在 DNS 解析前命中（DOMAIN 规则 + 域名透传给 SOCKS），不要用 IP 规则兜底。
5. **Windows 路径与空格**：子进程参数一律数组传递；禁止 `cmd /c` 拼接。
6. **Linux WebKitGTK 依赖**：AlmaLinux 9 需要 `webkit2gtk4.1`（AppStream），Ubuntu 22.04+ 同理；打包文档需列全。
7. **macOS 公证**：Helper 需要独立签名并随主包安装；本地开发可用 ad-hoc 签名。
8. **端口占用**：启动前检测 7897/9097/11080 是否被旧版本占用，避免"已连接但没网"。

---

## 11. Definition of Done

一次改动完成需同时满足：

- [ ] 需求/设计文档同步更新（行为变化必须写进 `docs/`）。
- [ ] 新增/修改逻辑有测试；`cargo test` 与前端 build 通过。
- [ ] 无新增告警（`cargo clippy --workspace` 与 `npm run typecheck` 干净）。
- [ ] 用户可见变化更新 `README` 或 `docs/USER_GUIDE.md`。
- [ ] 安全红线自查通过（密钥、命令注入、权限）。
- [ ] 在至少一个平台完成冒烟验证，并在变更说明写明平台与结果。
