# Soar

极简跨平台 WireGuard 桌面客户端，基于 Tauri 2 + Rust + Svelte 5 构建。

> Soar — 翱翔。单文件 ~13 MB，朴素、安静、可靠。

## 当前能力（macOS MVP）

- 导入 / 删除 `.conf` 配置（保存在 app 数据目录）
- 一键连接 / 断开（通过 `wg-quick` + macOS 原生授权弹窗提权）
- 实时显示连接状态：握手时间、上下行流量、底层 utun 接口、Endpoint
- 一键查询当前出口 IP（通过 `curl https://api.ipify.org`）
- 后台 3 秒轮询状态，连接状态用绿色指示灯展示

## 系统依赖

### macOS

```bash
brew install wireguard-tools
```

需要 `wg`、`wg-quick` 在 `/opt/homebrew/bin` 或 `/usr/local/bin`。

### Windows（待实现）

需要安装 [WireGuard for Windows](https://www.wireguard.com/install/)。

## 开发

```bash
pnpm install
pnpm tauri dev
```

首次连接时会弹出 macOS 系统授权对话框输入密码（`osascript do shell script with administrator privileges`），授权后 `wg-quick` 才能创建 utun 设备和写路由表。

## 打包

```bash
pnpm tauri build
```

产物位于 `src-tauri/target/release/bundle/`。

## 数据存储位置

- macOS: `~/Library/Application Support/com.mengfan.wgvpn/configs/`

每个配置文件以 `<name>.conf` 命名，权限设为 `0600`。

## 架构

```
┌─────────────────────────────────────┐
│  前端 (Vanilla HTML/CSS/JS)         │
│  src/index.html, src/main.js        │
└─────────────────┬───────────────────┘
                  │ window.__TAURI__.core.invoke
┌─────────────────▼───────────────────┐
│  Rust 后端 (Tauri commands)         │
│  src-tauri/src/                     │
│    lib.rs    - command handlers     │
│    config.rs - .conf CRUD           │
│    wg.rs     - wg-quick / wg show   │
│    error.rs  - 错误类型              │
└─────────────────┬───────────────────┘
                  │ std::process::Command
┌─────────────────▼───────────────────┐
│  系统 WireGuard (wg-quick / wg)     │
└─────────────────────────────────────┘
```

## 后续计划

- [ ] 监听系统 wake 事件，握手 > 3 分钟自动重连
- [ ] System tray icon + 后台运行
- [ ] Windows 平台支持（调用 `wireguard.exe /installtunnelservice` + UAC 提权）
- [ ] 自动从粘贴板/拖拽导入 `.conf`
- [ ] 多隧道并发管理
