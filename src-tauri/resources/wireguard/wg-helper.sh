#!/bin/bash
# Soar 提权助手脚本：被 wg-quick / app 调用，需要 root 权限
# - 免密模式时由 sudo -n 直接调用
# - 否则由 osascript with administrator privileges 调用
# 单脚本承载所有 root 操作，sudoers 只授权它一个路径即可
set -e

SELF_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
export PATH="$SELF_DIR:/usr/sbin:/usr/bin:/sbin:/bin:/opt/homebrew/bin:/usr/local/bin"
export WG_QUICK_USERSPACE_IMPLEMENTATION="$SELF_DIR/wireguard-go"

STATE_DIR="/var/run/wg-vpn"
SERVICES_FILE="$STATE_DIR/services"
KS_ANCHOR="wg-vpn"
KS_FLAG="$STATE_DIR/killswitch.on"
KS_PF_CONF="$STATE_DIR/killswitch.pf.conf"

mkdir -p "$STATE_DIR" 2>/dev/null || true
chmod 755 "$STATE_DIR" 2>/dev/null || true

ACTION="${1:-}"
ARG2="${2:-}"

# 列出所有非禁用的网络服务（排除头部说明行和带 * 的禁用项）
list_services() {
  /usr/sbin/networksetup -listallnetworkservices 2>/dev/null \
    | /usr/bin/tail -n +2 \
    | /usr/bin/grep -v '^\*'
}

# 从 conf 中提取 DNS（取第一行 DNS=）
extract_dns_from_conf() {
  local conf="$1"
  /usr/bin/grep -iE '^[[:space:]]*DNS[[:space:]]*=' "$conf" 2>/dev/null \
    | /usr/bin/head -1 \
    | /usr/bin/awk -F'=' '{print $2}' \
    | /usr/bin/tr ',' ' ' \
    | /usr/bin/tr -d '\r'
}

# 从 conf 中提取 Endpoint 主机和端口
extract_endpoint() {
  local conf="$1"
  /usr/bin/grep -iE '^[[:space:]]*Endpoint[[:space:]]*=' "$conf" 2>/dev/null \
    | /usr/bin/head -1 \
    | /usr/bin/awk -F'=' '{print $2}' \
    | /usr/bin/tr -d ' \r'
}

# 把 host:port 里的 host 解析成 IP（如果已经是 IP 就直接返回）
resolve_host() {
  local host="$1"
  if echo "$host" | /usr/bin/grep -qE '^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "$host"
    return
  fi
  /usr/bin/dscacheutil -q host -a name "$host" 2>/dev/null \
    | /usr/bin/awk '/^ip_address:/ {print $2; exit}'
}

apply_protections() {
  local conf="$1"
  local dns_list
  dns_list=$(extract_dns_from_conf "$conf")
  : > "$SERVICES_FILE"
  while IFS= read -r svc; do
    [ -z "$svc" ] && continue
    echo "$svc" >> "$SERVICES_FILE"
    /usr/sbin/networksetup -setv6off "$svc" 2>/dev/null || true
    if [ -n "$dns_list" ]; then
      /usr/sbin/networksetup -setdnsservers "$svc" $dns_list 2>/dev/null || true
    fi
  done < <(list_services)
  chmod 644 "$SERVICES_FILE" 2>/dev/null || true
}

restore_protections() {
  local svcs
  if [ -f "$SERVICES_FILE" ]; then
    svcs=$(/bin/cat "$SERVICES_FILE")
  else
    svcs=$(list_services)
  fi
  while IFS= read -r svc; do
    [ -z "$svc" ] && continue
    /usr/sbin/networksetup -setv6automatic "$svc" 2>/dev/null || true
    /usr/sbin/networksetup -setdnsservers "$svc" Empty 2>/dev/null || true
  done <<EOF
$svcs
EOF
  /bin/rm -f "$SERVICES_FILE" 2>/dev/null || true
}

# Kill Switch：除了 lo0、utun*、wg endpoint UDP、DHCP、本地内网，其它出站全 block
enable_killswitch() {
  local conf="$1"
  local endpoint host port ip
  endpoint=$(extract_endpoint "$conf")
  host="${endpoint%:*}"
  port="${endpoint##*:}"
  ip=$(resolve_host "$host")
  if [ -z "$ip" ] || [ -z "$port" ]; then
    echo "killswitch: 解析 endpoint 失败 ($endpoint)" >&2
    return 1
  fi

  cat > "$KS_PF_CONF" <<EOF
# Soar Kill Switch (anchor: $KS_ANCHOR)
set skip on lo0
block drop out quick inet6 all
block drop out all
pass out quick on utun0
pass out quick on utun1
pass out quick on utun2
pass out quick on utun3
pass out quick on utun4
pass out quick on utun5
pass out quick on utun6
pass out quick on utun7
pass out quick on utun8
pass out quick on utun9
pass out quick proto udp to $ip port $port
pass out quick proto udp from any to any port 67
pass out quick proto udp from any to any port 68
pass out quick inet from any to 10.0.0.0/8
pass out quick inet from any to 172.16.0.0/12
pass out quick inet from any to 192.168.0.0/16
pass out quick inet from any to 169.254.0.0/16
pass out quick inet from any to 224.0.0.0/4
EOF
  chmod 644 "$KS_PF_CONF" 2>/dev/null || true

  /sbin/pfctl -E 2>/dev/null || true
  /sbin/pfctl -a "$KS_ANCHOR" -f "$KS_PF_CONF"
  : > "$KS_FLAG"
  chmod 644 "$KS_FLAG" 2>/dev/null || true
  echo "killswitch: enabled (endpoint=$ip:$port)"
}

disable_killswitch() {
  /sbin/pfctl -a "$KS_ANCHOR" -F all 2>/dev/null || true
  /bin/rm -f "$KS_FLAG" "$KS_PF_CONF" 2>/dev/null || true
  echo "killswitch: disabled"
}

# 热切换规则：参数 $2=conf-path, $3=新的 AllowedIPs（逗号分隔）
# 1) 把 conf 中的 AllowedIPs 替换成新值（保留 \n 顺序）
# 2) 如果隧道在线：wg set <iface> peer <pub> allowed-ips <list>
# 3) 删掉所有指向该 iface 的旧路由，按新 list 重建
switch_rules() {
  local conf="$1"
  local new_allowed="$2"
  [ -f "$conf" ] || { echo "switch-rules: conf 不存在: $conf" >&2; exit 2; }
  [ -n "$new_allowed" ] || { echo "switch-rules: 缺少 allowed-ips" >&2; exit 2; }

  local tmp
  tmp=$(/usr/bin/mktemp)
  /usr/bin/awk -v repl="AllowedIPs = $new_allowed" '
    BEGIN { done=0 }
    {
      lower=tolower($0); sub(/^[ \t]+/,"",lower)
      if (!done && lower ~ /^allowedips[ \t]*=/) { print repl; done=1 }
      else { print }
    }
    END { if (!done) print repl }
  ' "$conf" > "$tmp"
  # 用 cat > 而不是 mv：保留原 conf 的 owner/perms（属于普通用户），
  # 不然 root mv 上来后文件就归 root，App 之后再读会 Permission denied。
  /bin/cat "$tmp" > "$conf"
  /bin/rm -f "$tmp"

  local conf_name
  conf_name=$(/usr/bin/basename "$conf" .conf)
  local name_file="/var/run/wireguard/${conf_name}.name"
  [ -f "$name_file" ] || { echo "switch-rules: 隧道未在线，仅更新配置"; return 0; }
  local iface
  iface=$(/usr/bin/tr -d '\r\n' < "$name_file")
  [ -n "$iface" ] || { echo "switch-rules: name 文件为空"; return 0; }

  # 提取 [Peer] 段的 PublicKey
  local pub
  pub=$(/usr/bin/awk '
    /^[ \t]*\[Peer\][ \t]*$/ { in_peer=1; next }
    /^[ \t]*\[/ { in_peer=0 }
    in_peer && tolower($0) ~ /^[ \t]*publickey[ \t]*=/ {
      # 注意：base64 的 PublicKey 末尾可能含 = padding，不能用贪婪 .*=
      # 必须只剥掉行首到第一个 = 的部分
      sub(/^[^=]*=[ \t]*/,""); sub(/[ \t\r]+$/,""); print; exit
    }
  ' "$conf")
  [ -n "$pub" ] || { echo "switch-rules: 找不到 PublicKey" >&2; exit 2; }

  # 标准化 allowed-ips（去空格）
  local norm
  norm=$(echo "$new_allowed" | /usr/bin/tr -d ' ')

  # 1) 拿"切换前"内核里实际生效的 allowed-ips（CIDR 格式精确，避免 netstat 简写删不掉）
  #    输出格式：<pubkey>\t1.2.3.0/24 5.6.0.0/16 …
  local old_allowed
  old_allowed=$("$SELF_DIR/wg" show "$iface" allowed-ips 2>/dev/null \
    | /usr/bin/awk '{for(i=2;i<=NF;i++) print $i}')

  # 2) 写内核
  "$SELF_DIR/wg" set "$iface" peer "$pub" allowed-ips "$norm"

  # 3) 删旧路由（用精确 CIDR，跳过 IPv6）
  #    特殊：0.0.0.0/0 不会出现在路由表里（被拆成 0/1 + 128/1），需要单独删
  echo "$old_allowed" | while IFS= read -r cidr; do
    [ -z "$cidr" ] && continue
    case "$cidr" in
      *:*) continue ;;
      "0.0.0.0/0")
        /sbin/route -q delete -inet 0.0.0.0/1 -interface "$iface" >/dev/null 2>&1 || true
        /sbin/route -q delete -inet 128.0.0.0/1 -interface "$iface" >/dev/null 2>&1 || true
        ;;
      *)
        /sbin/route -q delete -inet "$cidr" >/dev/null 2>&1 || true
        ;;
    esac
  done

  # 4) 加新路由（IPv4 only；0.0.0.0/0 拆成两条覆盖全 v4 空间，保留物理 default）
  echo "$norm" | /usr/bin/tr ',' '\n' | while IFS= read -r cidr; do
    [ -z "$cidr" ] && continue
    case "$cidr" in
      *:*) continue ;;
      "0.0.0.0/0")
        /sbin/route -q -n add -inet 0.0.0.0/1 -interface "$iface" >/dev/null 2>&1 || true
        /sbin/route -q -n add -inet 128.0.0.0/1 -interface "$iface" >/dev/null 2>&1 || true
        ;;
      *)
        /sbin/route -q -n add -inet "$cidr" -interface "$iface" >/dev/null 2>&1 || true
        ;;
    esac
  done

  echo "switch-rules: applied iface=$iface count=$(echo "$norm" | /usr/bin/tr ',' '\n' | /usr/bin/wc -l | /usr/bin/tr -d ' ')"
}

# 应用内升级：把传入的 DMG 挂载、复制到 /Applications、清隔离属性、重启 App。
# 由 Rust 端在下载完成后用 sudo -n / osascript 调用。
install_app() {
  local dmg="$1"
  [ -f "$dmg" ] || { echo "install-app: DMG 不存在: $dmg" >&2; exit 2; }
  /usr/bin/xattr -cr "$dmg" 2>/dev/null || true

  # 卸载已挂载的同名卷（避免 attach 报错）
  for v in /Volumes/WG\ VPN /Volumes/WG\ VPN\ *; do
    [ -d "$v" ] && /usr/bin/hdiutil detach "$v" -force -quiet 2>/dev/null || true
  done

  local mount
  mount=$(/usr/bin/hdiutil attach "$dmg" -nobrowse -noautoopen -quiet \
    | /usr/bin/awk -F'\t' '/Volumes/{print $NF}' | /usr/bin/tail -1)
  [ -n "$mount" ] && [ -d "$mount" ] || { echo "install-app: 挂载失败" >&2; exit 3; }

  local app
  app=$(/usr/bin/find "$mount" -maxdepth 2 -name '*.app' -print | /usr/bin/head -1)
  [ -n "$app" ] || { /usr/bin/hdiutil detach "$mount" -quiet 2>/dev/null || true; echo "install-app: DMG 中找不到 .app" >&2; exit 4; }

  # 优雅退出当前 App（如果还在跑）—— root 跑 osascript 也能 quit
  # 历史改名：WG VPN → MaiSui (v0.3.0) → Soar (v0.3.1)，全部兼容
  /usr/bin/osascript -e 'tell application "Soar" to quit' 2>/dev/null || true
  /usr/bin/osascript -e 'tell application "MaiSui" to quit' 2>/dev/null || true
  /usr/bin/osascript -e 'tell application "WG VPN" to quit' 2>/dev/null || true
  /usr/bin/sleep 1
  /usr/bin/pkill -9 -f "Soar" 2>/dev/null || true
  /usr/bin/pkill -9 -f "MaiSui" 2>/dev/null || true
  /usr/bin/pkill -9 -f "WG VPN" 2>/dev/null || true
  /usr/bin/pkill -9 -f wg-vpn 2>/dev/null || true

  # 旧版残留也一起删（升级路径：WG VPN.app / MaiSui.app -> Soar.app）
  /bin/rm -rf "/Applications/Soar.app" "/Applications/MaiSui.app" "/Applications/WG VPN.app"

  # DMG 里到底叫什么名字不管，照着 .app 实际名字 cp
  local app_basename
  app_basename=$(/usr/bin/basename "$app")
  /bin/cp -R "$app" "/Applications/"
  /usr/bin/hdiutil detach "$mount" -quiet 2>/dev/null || true
  /usr/bin/xattr -cr "/Applications/$app_basename" 2>/dev/null || true

  /bin/rm -f "$dmg" 2>/dev/null || true

  # 用原始用户身份重启（sudo 调用时 SUDO_USER 是真实用户；osascript 调用则直接 open）
  if [ -n "${SUDO_USER:-}" ]; then
    /usr/bin/sudo -u "$SUDO_USER" /usr/bin/open "/Applications/$app_basename" 2>/dev/null || \
      /usr/bin/open "/Applications/$app_basename"
  else
    /usr/bin/open "/Applications/$app_basename"
  fi
  echo "install-app: ok"
}

case "$ACTION" in
  up)
    [ -n "$ARG2" ] || { echo "usage: $0 up <conf-path>" >&2; exit 2; }
    # 必须显式用 bundle 的 bash 5 跑 wg-quick；否则 wg-quick 的 shebang
    # `#!/usr/bin/env bash` 会找到 /bin/bash 3.2，触发 "bash 3 detected" 失败。
    # 同时把 SELF_DIR 放到 PATH 最前，wg-quick 内部 fork 的子脚本也走 bash 5。
    PATH="$SELF_DIR:$PATH" "$SELF_DIR/bash" "$SELF_DIR/wg-quick" up "$ARG2"
    chmod 755 /var/run/wireguard 2>/dev/null || true
    if [ -n "${SUDO_USER:-}" ]; then
      chown "$SUDO_USER" /var/run/wireguard/*.sock /var/run/wireguard/*.name 2>/dev/null || true
    fi
    chmod 666 /var/run/wireguard/*.sock 2>/dev/null || true
    chmod 644 /var/run/wireguard/*.name 2>/dev/null || true
    apply_protections "$ARG2"
    if [ -f "$KS_FLAG" ]; then
      enable_killswitch "$ARG2" || true
    fi
    ;;
  down)
    [ -n "$ARG2" ] || { echo "usage: $0 down <conf-path>" >&2; exit 2; }
    PATH="$SELF_DIR:$PATH" "$SELF_DIR/bash" "$SELF_DIR/wg-quick" down "$ARG2" || true
    # 兜底：清理 down 没删干净的 .name/.sock，避免下一次 up 报
    # `'xxx' already exists as 'utunN'`。即使 utun 已不存在，stale 的 .name
    # 文件还在也会触发误判。
    if [ -n "$ARG2" ]; then
      iface_name=$(/usr/bin/basename "$ARG2" .conf 2>/dev/null || true)
      if [ -n "$iface_name" ]; then
        /bin/rm -f "/var/run/wireguard/${iface_name}.name" "/var/run/wireguard/${iface_name}.sock" 2>/dev/null || true
      fi
    fi
    restore_protections
    ;;
  killswitch-on)
    [ -n "$ARG2" ] || { echo "usage: $0 killswitch-on <conf-path>" >&2; exit 2; }
    enable_killswitch "$ARG2"
    ;;
  killswitch-off)
    disable_killswitch
    ;;
  killswitch-status)
    [ -f "$KS_FLAG" ] && echo "on" || echo "off"
    ;;
  switch-rules)
    [ -n "$ARG2" ] || { echo "usage: $0 switch-rules <conf-path> <allowed-ips-csv>" >&2; exit 2; }
    switch_rules "$ARG2" "${3:-}"
    ;;
  install-app)
    [ -n "$ARG2" ] || { echo "usage: $0 install-app <dmg-path>" >&2; exit 2; }
    install_app "$ARG2"
    ;;
  fix-config-owner)
    # 自愈：把 configs 目录里属于 root 的 .conf 文件 chown 回普通用户
    # 用法：fix-config-owner <configs-dir> <user>
    [ -n "$ARG2" ] || { echo "usage: $0 fix-config-owner <configs-dir> <user>" >&2; exit 2; }
    [ -n "${3:-}" ] || { echo "usage: $0 fix-config-owner <configs-dir> <user>" >&2; exit 2; }
    target_dir="$ARG2"
    target_user="$3"
    [ -d "$target_dir" ] || exit 0
    /usr/sbin/chown -R "$target_user:staff" "$target_dir"
    /bin/chmod -R u+rw "$target_dir"
    echo "ok: chown -R $target_user:staff $target_dir"
    ;;
  *)
    echo "wg-helper: unknown action: $ACTION" >&2
    echo "usage: $0 {up|down|killswitch-on|killswitch-off|killswitch-status|switch-rules|install-app} [args]" >&2
    exit 2
    ;;
esac
