#!/bin/bash
# Soar 发版工具
#
# 流程：
#   1. 读 package.json 拿 VERSION
#   2. 算 sha256 + 文件大小
#   3. 让你写 release notes
#   4. 生成 latest.json（DMG 下载 URL = GitHub Release）
#   5. 提示 gh 上传 DMG 到 GitHub Release
#   6. scp latest.json 到 BCC nginx 目录
#
# 用法：
#   bash scripts/publish.sh
#
# 环境变量（可选）：
#   GH_REPO     GitHub repo（默认从 git remote 推断）
#   BCC_HOST    服务器 ssh 目标（默认 root@180.76.134.45）
#   BCC_PATH    服务器 manifest 路径（默认 /var/www/wg-vpn/）

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION=$(node -p "require('./package.json').version")
DMG="release/Soar_${VERSION}.dmg"
MANIFEST="release/latest.json"

GH_REPO="${GH_REPO:-}"
BCC_HOST="${BCC_HOST:-root@180.76.134.45}"
# 多端布局：mac 部署到 wg-vpn/mac/，未来 windows 走 wg-vpn/win/ 各自独立
BCC_PATH="${BCC_PATH:-/var/www/wg-vpn/mac/}"
PUBLIC_URL="${PUBLIC_URL:-http://180.76.134.45:8088/wg-vpn/mac/latest.json}"

# 自动从 git remote 推断 repo
if [ -z "$GH_REPO" ] && git remote get-url origin >/dev/null 2>&1; then
    url=$(git remote get-url origin)
    # 支持 git@github.com:owner/repo.git 和 https://github.com/owner/repo.git
    GH_REPO=$(echo "$url" | sed -E 's#(git@github.com:|https://github.com/)([^/]+/[^.]+)(\.git)?#\2#' | head -1)
fi

if [ -z "$GH_REPO" ]; then
    cat <<EOF
❌ 没有指定 GitHub repo。三选一：
   1) 在仓库目录 git init && git remote add origin git@github.com:OWNER/REPO.git
   2) 跑命令时加：GH_REPO=owner/repo bash scripts/publish.sh
   3) 编辑本脚本，把 GH_REPO 默认值改掉
EOF
    exit 1
fi

[ -f "$DMG" ] || { echo "❌ 找不到 $DMG，先跑 bash scripts/build-mac.sh"; exit 1; }

echo "==> 配置"
echo "    VERSION  = $VERSION"
echo "    DMG      = $DMG"
echo "    GH_REPO  = $GH_REPO"
echo "    BCC      = $BCC_HOST:$BCC_PATH"

echo "==> 计算 sha256 / 文件大小"
SHA=$(/usr/bin/shasum -a 256 "$DMG" | awk '{print $1}')
SIZE=$(/usr/bin/stat -f%z "$DMG")
DMG_URL="https://github.com/${GH_REPO}/releases/download/v${VERSION}/Soar_${VERSION}.dmg"
echo "    sha256 = $SHA"
echo "    size   = $SIZE"
echo "    url    = $DMG_URL"

# Release notes
NOTES_FILE="release/notes-${VERSION}.txt"
if [ ! -f "$NOTES_FILE" ]; then
    cat > "$NOTES_FILE" <<EOF
v${VERSION} 更新内容（一两句话即可，会显示在用户的升级 banner 上）：

EOF
    ${EDITOR:-vi} "$NOTES_FILE"
fi
NOTES=$(cat "$NOTES_FILE")
NOTES_JSON=$(printf '%s' "$NOTES" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read().strip()))')

echo "==> 写 $MANIFEST"
mkdir -p release
cat > "$MANIFEST" <<EOF
{
  "version": "${VERSION}",
  "url": "${DMG_URL}",
  "sha256": "${SHA}",
  "size": ${SIZE},
  "notes": ${NOTES_JSON}
}
EOF
cat "$MANIFEST"
echo

# Step 1: 上传 DMG 到 GitHub Release
echo "================================================================"
echo "Step 1: 上传 DMG 到 GitHub Release"
echo "================================================================"
if command -v gh >/dev/null 2>&1; then
    if gh release view "v${VERSION}" --repo "${GH_REPO}" >/dev/null 2>&1; then
        echo "Release v${VERSION} 已存在，覆盖上传 DMG…"
        gh release upload "v${VERSION}" "$DMG" --repo "${GH_REPO}" --clobber
    else
        echo "创建 Release v${VERSION} 并上传…"
        gh release create "v${VERSION}" "$DMG" \
            --repo "${GH_REPO}" \
            --title "v${VERSION}" \
            --notes-file "${NOTES_FILE}"
    fi
    echo "✅ DMG 已上传：$DMG_URL"
else
    cat <<EOF
未检测到 gh 命令。手动上传步骤：
  1) 打开 https://github.com/${GH_REPO}/releases/new
  2) Tag = v${VERSION}，标题 = v${VERSION}
  3) 拖入 $DMG
  4) 发布
建议装 gh：brew install gh && gh auth login
EOF
    read -p "上传完成后按回车继续 →" _
fi

# Step 2: 把 manifest 推送到 BCC
echo
echo "================================================================"
echo "Step 2: 推送 manifest 到 BCC（${BCC_HOST}）"
echo "================================================================"
echo "scp $MANIFEST ${BCC_HOST}:${BCC_PATH}latest.json"
if scp "$MANIFEST" "${BCC_HOST}:${BCC_PATH}latest.json"; then
    echo "✅ manifest 已推送"
else
    echo "❌ scp 失败。常见原因："
    echo "   - 安全组没放行 22 端口"
    echo "   - ssh 用户名错（默认 root，如果你是 ubuntu 改用 BCC_HOST=ubuntu@180.76.134.45）"
    echo "   - 第一次连接时需要 ssh-copy-id ${BCC_HOST}（免密）"
    exit 1
fi

# Step 3: 验证
echo
echo "================================================================"
echo "Step 3: 验证 manifest 在外网可访问"
echo "================================================================"
if curl -sf "$PUBLIC_URL" -o /tmp/wgvpn-check.json; then
    echo "✅ 外网拉到的 manifest:"
    cat /tmp/wgvpn-check.json
    rm -f /tmp/wgvpn-check.json
    REMOTE_VER=$(grep -oE '"version":[^,]+' /tmp/wgvpn-check.json 2>/dev/null | head -1 || echo "")
else
    echo "⚠️  外网拉不到 http://180.76.134.45:8088/wg-vpn/latest.json"
    echo "    检查：1) 服务器 nginx 是否在跑 2) 安全组是否放行 8088"
fi

echo
echo "================================================================"
echo "🎉 v${VERSION} 发布完成"
echo "    所有装了 Soar 的同事，下次启动 App（最多 8 秒）会看到升级 banner"
echo "    点「立即升级」自动下载安装重启，全程零交互"
echo "================================================================"
