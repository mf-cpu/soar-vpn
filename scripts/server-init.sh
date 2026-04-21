#!/bin/bash
# Soar manifest 服务器一次性初始化（Ubuntu 24.04）
#
# 用途：在 BCC（百度智能云 / 任何 Ubuntu 服务器）上装一个轻量 nginx，
# 监听 8088 端口，把 /var/www/wg-vpn/ 暴露成静态目录。
# manifest（latest.json）放在这个目录里，几百字节，不抢服务器其它服务的带宽。
#
# 怎么跑：
#   ssh root@180.76.134.45
#   curl -sSL https://raw.githubusercontent.com/<owner>/<repo>/main/scripts/server-init.sh | bash
#   # 或者把这个文件 scp 上去再 bash 跑
#
# 跑完之后：
#   - http://180.76.134.45:8088/wg-vpn/      → 目录列表
#   - http://180.76.134.45:8088/wg-vpn/latest.json  → manifest
#
# 卸载：
#   rm /etc/nginx/sites-enabled/wg-vpn /etc/nginx/sites-available/wg-vpn
#   nginx -s reload && rm -rf /var/www/wg-vpn

set -euo pipefail

PORT="${PORT:-8088}"
ROOT_DIR="/var/www/wg-vpn"

echo "==> 1/4 安装 nginx（如果已经有就跳过）"
if ! command -v nginx >/dev/null 2>&1; then
  apt-get update -qq
  apt-get install -y -qq nginx
else
  echo "    nginx 已安装：$(nginx -v 2>&1)"
fi

echo "==> 2/4 创建静态目录 $ROOT_DIR（多端布局）"
# 多端布局：每个平台独占一个子目录，互不影响。
# - mac/      macOS 客户端的 manifest
# - win/      未来 Windows 客户端
# - ios/      未来 iOS 客户端
# - android/  未来 Android 客户端
for sub in mac win ios android; do
    mkdir -p "$ROOT_DIR/$sub"
done
chown -R www-data:www-data "$ROOT_DIR"
chmod -R 755 "$ROOT_DIR"

# 放一个占位 mac/latest.json，避免 App 在你第一次发版前 GET 报 404
cat > "$ROOT_DIR/mac/latest.json" <<EOF
{
  "version": "0.0.0",
  "url": "",
  "sha256": "",
  "notes": "占位 manifest。等你第一次跑 publish.sh 后会被覆盖。"
}
EOF

echo "==> 3/4 写入 nginx 站点配置（监听 $PORT）"
cat > /etc/nginx/sites-available/wg-vpn <<NGINX
server {
    listen $PORT default_server;
    listen [::]:$PORT default_server;

    server_name _;
    root /var/www;

    # 仅暴露 /wg-vpn/ 目录
    location /wg-vpn/ {
        autoindex on;
        autoindex_exact_size off;
        autoindex_localtime on;
        # 允许跨域，方便浏览器调试
        add_header Access-Control-Allow-Origin *;
        # 防缓存：manifest 必须及时更新
        location ~ \.json$ {
            add_header Cache-Control "no-cache, no-store, must-revalidate";
            add_header Pragma "no-cache";
            expires 0;
            add_header Access-Control-Allow-Origin *;
        }
    }

    # 其它路径返回 404，不暴露 /var/www 下其它内容
    location / {
        return 404;
    }
}
NGINX

ln -sf /etc/nginx/sites-available/wg-vpn /etc/nginx/sites-enabled/wg-vpn

# 如果默认站点也监听 80，无影响（我们用 8088）；这里不动它

echo "==> 4/4 测试配置 + reload"
nginx -t
if systemctl is-active --quiet nginx; then
    systemctl reload nginx
else
    systemctl enable --now nginx
fi

# UFW 如果启用了，也放行
if command -v ufw >/dev/null 2>&1 && ufw status | grep -q "Status: active"; then
    ufw allow $PORT/tcp >/dev/null 2>&1 || true
    echo "    已放行 ufw $PORT/tcp"
fi

echo
echo "==================================================="
echo "✅ 完成。下一步："
echo
echo "1) 在百度云控制台 → 安全组，放行入站 TCP $PORT 端口"
echo
echo "2) 本机自测："
echo "   curl http://127.0.0.1:$PORT/wg-vpn/mac/latest.json"
echo
echo "3) 外网自测（在你 mac 上）："
echo "   curl http://180.76.134.45:$PORT/wg-vpn/mac/latest.json"
echo
echo "4) 之后每次发版只要在 mac 上跑："
echo "   bash scripts/build-mac.sh"
echo "   bash scripts/publish.sh"
echo "==================================================="
