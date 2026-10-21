#!/usr/bin/env bash
# 在本机构建、签名并启动 LabVPN（macOS）。
# 用法：./scripts/build-local-macos.sh [--no-open]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# 使用仓库内的本地 Rust 工具链（如果存在）
if [[ -d "$ROOT/.toolchain/cargo/bin" ]]; then
  export RUSTUP_HOME="$ROOT/.toolchain/rustup"
  export CARGO_HOME="$ROOT/.toolchain/cargo"
  export PATH="$ROOT/.toolchain/cargo/bin:$PATH"
fi

echo "== 1/5 准备 sidecar（mihomo + fdu-connect + helper）=="
node scripts/fetch-sidecars.mjs
node scripts/fetch-sidecars.mjs --helper

echo "== 2/5 安装前端依赖 =="
(cd apps/desktop && npm install --no-audit --no-fund --cache "$ROOT/apps/desktop/.npm-cache")

echo "== 3/5 构建 App（Release）=="
(cd apps/desktop && npm run build)

APP_DIR="apps/desktop/src-tauri/target/release/bundle/macos/LabVPN.app"
DMG_DIR="apps/desktop/src-tauri/target/release/bundle/dmg"
DMG_OUT="$(ls -t "$DMG_DIR"/*.dmg 2>/dev/null | head -1 || true)"
if [[ -z "$DMG_OUT" ]]; then
  DMG_OUT="apps/desktop/src-tauri/target/release/bundle/dmg/LabVPN_local.dmg"
fi

echo "== 4/5 签名（Developer ID 或 ad-hoc）=="
bash scripts/sign-macos.sh "$APP_DIR" "$DMG_OUT"

echo "== 5/5 启动 LabVPN =="
if [[ "${1:-}" == "--no-open" ]]; then
  echo "App: $APP_DIR"
else
  open "$APP_DIR"
  echo "🚀 已启动：$APP_DIR"
fi
