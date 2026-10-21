#!/usr/bin/env bash
# 对 LabVPN.app 进行代码签名（Developer ID 或 ad-hoc）与可选公证。
#
# 用法：
#   ./scripts/sign-macos.sh <LabVPN.app> [重建的DMG路径]
#
# 环境变量（签名）：
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: Name (TEAMID)"；未设置则 ad-hoc
# 环境变量（公证，二选一）：
#   APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID
#   APPLE_API_KEY / APPLE_API_KEY_PATH / APPLE_API_ISSUER
set -euo pipefail

APP_DIR="${1:?用法: sign-macos.sh <LabVPN.app> [dmg路径]}"
DMG_OUT="${2:-}"
IDENTITY="${APPLE_SIGNING_IDENTITY:--}"

if [[ ! -d "$APP_DIR" ]]; then
  echo "❌ 找不到 App：$APP_DIR" >&2
  exit 1
fi

if [[ "$IDENTITY" == "-" ]]; then
  echo "⚠️  未配置 APPLE_SIGNING_IDENTITY：使用 ad-hoc 签名（仅本机可运行，分发给他人需 Developer ID + 公证）"
else
  echo "🔐 使用签名身份：$IDENTITY"
fi

sign() {
  local target="$1"
  if [[ "$IDENTITY" == "-" ]]; then
    codesign --force --sign - "$target"
  else
    codesign --force --options runtime --timestamp --sign "$IDENTITY" "$target" 2>/dev/null \
      || codesign --force --sign "$IDENTITY" "$target"
  fi
}

echo "→ 签名内置 sidecar..."
BIN_DIR="$APP_DIR/Contents/Resources/bin"
if [[ -d "$BIN_DIR" ]]; then
  for file in "$BIN_DIR"/*; do
    [[ -f "$file" ]] || continue
    base="$(basename "$file")"
    case "$base" in
      *.txt|*.dll|*.json) continue ;;
    esac
    chmod +x "$file" 2>/dev/null || true
    sign "$file"
  done
fi

echo "→ 签名 App bundle..."
if [[ "$IDENTITY" == "-" ]]; then
  codesign --force --deep --sign - "$APP_DIR"
else
  codesign --force --options runtime --timestamp --deep --sign "$IDENTITY" "$APP_DIR"
fi
codesign --verify --deep --strict --verbose=2 "$APP_DIR" 2>&1 | tail -2

if [[ -n "$DMG_OUT" ]]; then
  echo "→ 重新生成 DMG（确保签名后的 App 被打包）..."
  rm -f "$DMG_OUT"
  mkdir -p "$(dirname "$DMG_OUT")"
  hdiutil create -volname "LabVPN" -srcfolder "$APP_DIR" -ov -format UDZO "$DMG_OUT" >/dev/null
  echo "✅ DMG：$DMG_OUT"
fi

notarize() {
  local target="$1"
  [[ "$IDENTITY" == "-" ]] && return 0
  if [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" && -n "${APPLE_TEAM_ID:-}" ]]; then
    echo "→ 提交公证（Apple ID）：$target"
    xcrun notarytool submit "$target" \
      --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  elif [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_KEY_PATH:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
    echo "→ 提交公证（App Store Connect API Key）：$target"
    xcrun notarytool submit "$target" \
      --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" --wait
  else
    echo "ℹ️  未提供公证凭据，跳过 notarize（其他 Mac 下载后首次打开仍需右键 → 打开）"
    return 0
  fi
  xcrun stapler staple "$target" || true
  echo "✅ 公证完成"
}

if [[ -n "$DMG_OUT" ]]; then
  notarize "$DMG_OUT"
  xcrun stapler staple "$APP_DIR" 2>/dev/null || true
fi
echo "✅ 签名流程完成"
