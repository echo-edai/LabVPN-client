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

### 2.2 fdu-connect（校内 VPN）

fdu-connect 使用 **AGPL-3.0**，本项目**不打包**它，有两种方式获得：

1. **使用已有安装**（推荐）：LabVPN 会自动发现
   - `~/LocalDocuments/fdu-connect/fdu-connect`
   - `~/.local/bin/fdu-connect`、`/usr/local/bin/fdu-connect`、`/opt/fdu-connect/fdu-connect`
   - `PATH` 中的 `fdu-connect`
   - 应用数据目录 `…/bin/fdu-connect`
2. **自行编译**（跨平台）：

```bash
git clone https://github.com/AkiraSalvare/fdu-connect.git
cd fdu-connect
go build -o fdu-connect .            # 需要 Go 1.24+
# 或交叉编译：
GOOS=windows GOARCH=amd64 go build -o fdu-connect.exe .
GOOS=linux   GOARCH=amd64 go build -o fdu-connect .
GOOS=linux   GOARCH=arm64 go build -o fdu-connect .
```

把产物放到任意被发现的路径，或在「配置向导 ② 校内 VPN」中显式填写路径。
Windows 需要 `wintun.dll` 时（TUN 模式在 fdu-connect 内不使用，可不放）。

> macOS 首次运行可能被 Gatekeeper 拦截：`xattr -dr com.apple.quarantine fdu-connect`。

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

### 4.1 签名与公证（发布必需）

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
