#!/usr/bin/env bash
# Soar 一键安装/升级脚本
#
# 同事使用方式（一条命令）：
#
#   curl -fsSL https://raw.githubusercontent.com/mf-cpu/soar-vpn/main/install.sh | bash
#
# 自动完成：检测最新版 -> 下载 DMG -> 退出旧版本 -> 卸载 -> 安装 -> 启动
# 兼容老版本：WG VPN / MaiSui / Soar 三种 .app 都会被替换
set -euo pipefail

# ============ 配置（每次发版不用改） ============
REPO="mf-cpu/soar-vpn"
MANIFEST_URL="http://180.76.134.45:8088/wg-vpn/mac/latest.json"
APP_NAME="Soar"
# ===============================================

GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

say()  { printf "${BLUE}==>${NC} %s\n" "$*"; }
ok()   { printf "${GREEN}✓${NC} %s\n" "$*"; }
warn() { printf "${YELLOW}!${NC} %s\n" "$*"; }
die()  { printf "${RED}✗${NC} %s\n" "$*" >&2; exit 1; }

[[ "$(uname -s)" == "Darwin" ]] || die "Soar 目前只支持 macOS"

say "查询最新版本…"
LATEST_JSON=$(curl -fsSL --max-time 8 "$MANIFEST_URL" 2>/dev/null \
  || curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest")

VERSION=$(echo "$LATEST_JSON" | python3 -c 'import json,sys,re
d=json.load(sys.stdin)
print(d.get("version") or d.get("tag_name","").lstrip("v"))')
DMG_URL=$(echo "$LATEST_JSON" | python3 -c 'import json,sys
d=json.load(sys.stdin)
u=d.get("url")
if not u:
    for a in d.get("assets",[]):
        if a["name"].endswith(".dmg"):
            u=a["browser_download_url"]; break
print(u or "")')
MIRROR_URL=$(echo "$LATEST_JSON" | python3 -c 'import json,sys
d=json.load(sys.stdin)
print(d.get("mirror_url") or "")')

[[ -n "$VERSION" && -n "$DMG_URL" ]] || die "拿不到最新版信息，检查网络"
ok "最新版本：v${VERSION}"

INSTALLED_VER=""
for app in "/Applications/Soar.app" "/Applications/MaiSui.app" "/Applications/WG VPN.app"; do
  [[ -d "$app" ]] && {
    INSTALLED_VER=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$app/Contents/Info.plist" 2>/dev/null || echo "")
    INSTALLED_NAME=$(basename "$app" .app)
    break
  }
done
if [[ -n "$INSTALLED_VER" ]]; then
  say "已装：$INSTALLED_NAME v${INSTALLED_VER} → 升级到 Soar v${VERSION}"
  if [[ "$INSTALLED_VER" == "$VERSION" && "$INSTALLED_NAME" == "$APP_NAME" ]]; then
    warn "已经是最新版，无需升级"
    exit 0
  fi
else
  say "首次安装 Soar v${VERSION}"
fi

DMG="/tmp/Soar_${VERSION}.dmg"
say "下载 DMG（GitHub 主源，约 10 MB）…"
if curl -fL --progress-bar --connect-timeout 8 --max-time 240 -o "$DMG" "$DMG_URL"; then
  ok "下载完成：$(du -h "$DMG" | awk '{print $1}')"
elif [[ -n "$MIRROR_URL" ]]; then
  warn "主源失败，回退备用镜像…"
  curl -fL --progress-bar --connect-timeout 8 --max-time 300 -o "$DMG" "$MIRROR_URL"
  ok "下载完成：$(du -h "$DMG" | awk '{print $1}')"
else
  die "DMG 下载失败：$DMG_URL"
fi

say "退出旧版本（如果在跑）…"
for n in Soar MaiSui "WG VPN"; do
  /usr/bin/osascript -e "tell application \"$n\" to quit" 2>/dev/null || true
done
sleep 1
for n in Soar MaiSui "WG VPN"; do
  /usr/bin/pkill -9 -f "$n" 2>/dev/null || true
done

say "清理旧挂载…"
/usr/bin/find /Volumes -maxdepth 1 \( -name 'Soar*' -o -name 'MaiSui*' -o -name 'WG VPN*' \) \
  -exec /usr/bin/hdiutil detach {} -force -quiet \; 2>/dev/null || true

say "解锁 DMG（绕过 Gatekeeper）…"
/usr/bin/xattr -cr "$DMG" 2>/dev/null || true

say "挂载 DMG…"
MOUNT=$(/usr/bin/hdiutil attach "$DMG" -nobrowse -noautoopen | /usr/bin/awk -F'\t' '/Volumes/{print $NF}' | tail -1)
[[ -d "$MOUNT" ]] || die "挂载失败"

APP=$(/usr/bin/find "$MOUNT" -maxdepth 2 -name '*.app' -print | head -1)
[[ -n "$APP" ]] || { /usr/bin/hdiutil detach "$MOUNT" -quiet || true; die "DMG 中没有 .app"; }
APP_BASENAME=$(basename "$APP")

say "卸载旧 App…"
/bin/rm -rf "/Applications/Soar.app" "/Applications/MaiSui.app" "/Applications/WG VPN.app"

say "安装新版到 /Applications/${APP_BASENAME}…"
/usr/bin/ditto "$APP" "/Applications/$APP_BASENAME"
sync
/usr/bin/hdiutil detach "$MOUNT" -quiet || true
/usr/bin/xattr -dr com.apple.quarantine "/Applications/$APP_BASENAME" 2>/dev/null || true

/bin/rm -f "$DMG" 2>/dev/null || true

ok "Soar v${VERSION} 安装完成"
say "启动 Soar…"
/usr/bin/open "/Applications/$APP_BASENAME"

echo
ok "全部完成 🎉"
echo "    以后从 启动台 / Spotlight 搜「Soar」打开即可。"
echo "    所有配置 / 免密授权 / 高级设置已保留。"
