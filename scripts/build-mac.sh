#!/usr/bin/env bash
# 打包脚本：构建 universal Mac app + 注入 bundled wireguard 二进制 + 重签名 + 出 DMG
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"
TARGET=universal-apple-darwin
APP_NAME="Soar"
APP_VERSION="0.3.1"
DIST_NAME="Soar_${APP_VERSION}.dmg"
BUNDLE_ROOT="src-tauri/target/$TARGET/release/bundle"
APP_PATH="$BUNDLE_ROOT/macos/$APP_NAME.app"
DMG_PATH="$BUNDLE_ROOT/dmg/$DIST_NAME"
DIST_DIR="release"
WG_RES_SRC="src-tauri/resources/wireguard"

echo "==> 0. vite build (frontend)"
pnpm build

echo "==> 1. tauri build (universal)"
pnpm tauri build --target $TARGET

echo "==> 2. 注入 wireguard 二进制到 $APP_PATH/Contents/Resources/wireguard/"
DEST="$APP_PATH/Contents/Resources/wireguard"
rm -rf "$DEST"
mkdir -p "$DEST"
# 用 cat 复制（避免遗留某些 xattr）
for f in wg wg-quick wireguard-go wg-helper.sh bash; do
  cat "$WG_RES_SRC/$f" > "$DEST/$f"
  chmod 755 "$DEST/$f"
done
ls -lh "$DEST"

echo "==> 3. 重新 ad-hoc 签名整个 .app"
codesign --force --deep --sign - "$APP_PATH"
codesign --verify --deep --strict --verbose=2 "$APP_PATH" || true

echo "==> 4. 重建 DMG（删旧的，用 hdiutil 重打）"
rm -f "$DMG_PATH"
mkdir -p "$BUNDLE_ROOT/dmg"
TMP_STAGE=$(mktemp -d)
cp -R "$APP_PATH" "$TMP_STAGE/"
ln -s /Applications "$TMP_STAGE/Applications"
hdiutil create -volname "$APP_NAME" -srcfolder "$TMP_STAGE" -fs HFS+ -format UDZO -imagekey zlib-level=9 -ov "$DMG_PATH"
rm -rf "$TMP_STAGE"

echo "==> 5. 同步到 $DIST_DIR/"
mkdir -p "$DIST_DIR"
cp -f "$DMG_PATH" "$DIST_DIR/$DIST_NAME"

echo "==> 6. 完成"
ls -lh "$APP_PATH" "$DMG_PATH" "$DIST_DIR/$DIST_NAME"
echo
echo "DMG 分发位置: $ROOT/$DIST_DIR/$DIST_NAME"
