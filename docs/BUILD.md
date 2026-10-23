# 构建与打包指南

> 目标：在 macOS / Windows / Ubuntu / AlmaLinux 上从源码构建可安装的 LabVPN。
> 构建产物由 **Tauri 应用** + **mihomo sidecar** + **labvpn-helper** 三部分组成。

## 1. 环境准备

| 平台 | 依赖 |
|---|---|
| 通用 | Rust stable（1.85+）、Node.js 20+（推荐 22）、npm、Git |
| macOS | Xcode Command Line Tools（`xcode-select --install`） |
| Windows | Visual Studio Build Tools（C++ 桌面开发）、WebView2 Runtime（Win10/11 自带） |
| Ubuntu 22.04+ | `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf` |
| AlmaLinux 9+ | `webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel patchelf rpm-build` |

Ubuntu：

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
  librsvg2-dev patchelf rpm
```

AlmaLinux 9（需 EPEL/CRB，`webkit2gtk4.1` 在 AppStream）：

```bash
sudo dnf install -y epel-release
sudo dnf install -y webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel \
  patchelf rpm-build gcc gcc-c++ openssl-devel
```

> 包名以发行版仓库为准；若 `webkit2gtk4.1` 不存在，请先确认系统版本（Tauri 2 需要 webkit2gtk-4.1）。

## 2. 获取 sidecar

### 2.1 mihomo 核心

```bash
node scripts/fetch-sidecars.mjs            # 下载当前平台（含 SHA256 记录到 manifest.json）
node scripts/fetch-sidecars.mjs --force    # 强制更新
node scripts/fetch-sidecars.mjs --helper   # 同时编译并复制 labvpn-helper
```

脚本固定 mihomo 版本（见脚本内 `MIHOMO_VERSION`），三平台资产来自官方 Release：

- macOS：`mihomo-darwin-arm64` / `mihomo-darwin-amd64`
- Linux：`mihomo-linux-amd64` / `mihomo-linux-arm64`
- Windows：`mihomo-windows-amd64.zip` + 自动下载 `wintun.dll`

### 2.2 fdu-connect（校内 VPN，**随安装包打包**）

`scripts/fetch-sidecars.mjs` 会自动准备 fdu-connect：

1. 若 `binaries/fdu-connect` 已存在 → 跳过（`--force` 可重建）；
2. **优先**用 Go 1.24.x 编译 [`vendor/fdu-connect/`](../vendor/fdu-connect/PATCH.md)
   （上游基线 `a904877f0eda0c7b1d7933ad6e80d5e8d3ffd584` + `robust-socks.patch`：
   可取消握手 + SOCKS5 宽限应答）；
3. 没有 Go 时回退：`FDU_CONNECT_BIN` → 本机已有安装 → 从上游固定 commit 克隆编译；
4. 最后写入 `binaries/fdu-connect`（Windows 为 `.exe`），随 App 一起打包。

> 必须使用 **Go 1.24.x**：上游依赖 `sagernet/sing` 的 `net.errNoSuchInterface` linkname
> 在 Go 1.25+ 已变化，链接会失败。

```bash
# CI / 干净环境（有 Go）
node scripts/fetch-sidecars.mjs --require-fdu
# 本机（使用已有 fdu-connect）
FDU_CONNECT_BIN=~/LocalDocuments/fdu-connect/fdu-connect node scripts/fetch-sidecars.mjs
```

**AGPL-3.0 合规**：fdu-connect 以独立进程打包，源码经本地补丁修改；修改后的**完整源码**
随仓库提供于 `vendor/fdu-connect/`（`PATCH.md` + `robust-socks.patch`），许可证与 NOTICE
随 App 位于 `Resources/licenses/`。

### 2.3 labvpn-helper（TUN 模式特权服务）

```bash
cargo build -p labvpn-helper --release
cp target/release/labvpn-helper apps/desktop/src-tauri/binaries/labvpn-helper
```

`scripts/fetch-sidecars.mjs --helper` 会自动完成以上两步。

## 3. 开发模式

```bash
cd apps/desktop
npm install
npm run sidecars          # 等价于 node ../../scripts/fetch-sidecars.mjs
npm run dev               # Tauri dev：Vite + Rust
```

只调试前端（浏览器，使用 Mock 后端）：

```bash
cd apps/desktop
npm run dev:web           # http://127.0.0.1:5173
```

单元测试与检查：

```bash
cargo test -p labvpn-core
cargo check --workspace --all-targets
cd apps/desktop && npm run typecheck && npm run build:web
```

## 4. 打包

```bash
cd apps/desktop
npm run build             # tauri build，产物在 src-tauri/target/release/bundle/
```

| 平台 | 产物 |
|---|---|
| macOS | `LabVPN.app`、`.dmg` |
| Windows | `.msi`、`.exe`（NSIS） |
| Ubuntu | `.deb`、`.AppImage` |
| AlmaLinux | `.rpm`（`bundle.targets` 已包含 all；需 `rpm` 工具链） |

### 4.1 签名与公证

#### macOS（Developer ID + 公证）

1. 准备 Apple Developer Program 账号，创建 **Developer ID Application** 证书；
2. 导出 `.p12` 并安装到本机钥匙串，或配置到 GitHub Secrets；
3. 本机一键构建 + 签名 + 公证 + 启动：

```bash
APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)" \
APPLE_ID="you@example.com" APPLE_PASSWORD="app-specific-password" APPLE_TEAM_ID="TEAMID" \
./scripts/build-local-macos.sh
```

GitHub Secrets（Release 自动使用）：
`APPLE_SIGNING_IDENTITY`、`APPLE_CERTIFICATE`（base64 p12）、`APPLE_CERTIFICATE_PASSWORD`、
`APPLE_ID`、`APPLE_PASSWORD`（App 专用密码）、`APPLE_TEAM_ID`、`KEYCHAIN_PASSWORD`。

> 免费 Apple 账号无法公证，下载后仍需右键 → 打开；付费 Developer Program 才能实现
> 「下载即安装」。

#### Windows（代码签名证书）

1. 购买 OV/EV 代码签名证书，导出 `.pfx`；
2. 配置 Secrets：`WINDOWS_CERTIFICATE`（base64 pfx）、`WINDOWS_CERTIFICATE_PASSWORD`、
   `WINDOWS_CERT_THUMBPRINT`（证书指纹）；
3. Release 工作流会自动导入证书并把 `certificateThumbprint` 写入构建配置。

#### 无证书时的行为

- macOS：构建脚本执行 **ad-hoc 签名**（本机可直接运行；他人下载会被 Gatekeeper 拦截）；
- Windows：未签名（SmartScreen 提示，可"仍要运行"）；
- Linux：无需签名。

### 4.2 旧版说明

- **macOS**：配置 `APPLE_CERTIFICATE`、`APPLE_SIGNING_IDENTITY`、`APPLE_ID`、
  `APPLE_PASSWORD`、`APPLE_TEAM_ID` 等环境变量后由 Tauri 完成签名与公证。
  Helper 与核心需要一同签名（两者都在 App bundle 内，Tauri 会随包签名）。
- **Windows**：配置代码签名证书（`TAURI_SIGNING_PRIVATE_KEY` / `CERTIFICATE`）。
- **Linux**：`.deb`/`.rpm` 未签名；生产分发建议启用仓库签名（`reprepro`/`createrepo`）。

### 4.2 版本号

统一在 `Cargo.toml`（workspace.package.version）与 `apps/desktop/package.json`、
`tauri.conf.json` 三处修改，打 tag `vX.Y.Z` 触发 Release 工作流。

## 5. CI / Release

| 工作流 | 触发 | 内容 |
|---|---|---|
| `.github/workflows/ci.yml` | push / PR | 三平台 `cargo test`、clippy、前端 typecheck/build、规则同步检查 |
| `.github/workflows/release.yml` | tag `v*` | 四平台矩阵构建并上传 Release（草稿） |

发布前检查清单：

- [ ] `cargo test -p labvpn-core` 通过
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` 通过
- [ ] `npm run typecheck && npm run build:web` 通过
- [ ] 生成配置通过 `mihomo -t` 校验
- [ ] 三平台人工冒烟（连接 / 分流 / 断开）
- [ ] NOTICE 与第三方版本号更新

## 6. AlmaLinux 说明

- AlmaLinux 9 使用 `webkit2gtk4.1`；如果构建机无法联网安装依赖，可在容器中构建：

```bash
docker run --rm -v "$PWD:/src" -w /src almalinux:9 bash -lc '
  dnf install -y epel-release && \
  dnf install -y webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel \
    patchelf rpm-build gcc gcc-c++ nodejs npm && \
  curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && \
  source "$HOME/.cargo/env" && cd apps/desktop && npm ci && npm run build'
```

- 生成的 `.rpm` 可在同版本 AlmaLinux/RHEL/CentOS Stream 上安装。
- fdu-connect 在 AlmaLinux 上可直接使用官方 Linux amd64 二进制或自行 `go build`。

## 7. 常见构建问题

| 现象 | 处理 |
|---|---|
| `webkit2gtk` 找不到 | 确认安装的是 `4.1` 版本；Ubuntu 22.04 需升级 webkit 或使用 24.04 构建机 |
| `error: linker 'cc' not found` | 安装 build-essential / gcc-c++ |
| 打包时提示找不到 `binaries/mihomo` | 先执行 `npm run sidecars`（占位空文件会被视为缺失） |
| 图标缺失 | `python3 scripts/generate-icons.py`（需要 Pillow） |
| Windows 构建慢/失败 | 确认已安装 VS Build Tools 与 WebView2；路径不要含中文/空格 |
| macOS 未公证无法打开 | 开发阶段用 ad-hoc 签名或右键打开；发布使用正式证书公证 |
