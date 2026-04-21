mod config;
mod diag;
mod error;
mod rules;
mod settings;
mod updater;
mod wg;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_log::{Target, TargetKind};

use crate::config::ConfigInfo;
use crate::diag::DiagResult;
use crate::error::AppResult;
use crate::rules::{RuleMode, RuleTemplate};
use crate::settings::Settings;
use crate::wg::{PasswordlessInfo, TunnelStatus, WgPaths};

struct AppState {
    active: Mutex<Option<String>>,
}

fn data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Other(format!("无法获取 app 数据目录: {e}")))?;
    std::fs::create_dir_all(&dir).ok();
    Ok(dir)
}

fn log_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_log_dir()
        .map_err(|e| crate::error::AppError::Other(format!("无法获取 log 目录: {e}")))?;
    std::fs::create_dir_all(&dir).ok();
    Ok(dir)
}

fn wg_paths(app: &AppHandle) -> AppResult<WgPaths> {
    let res = app
        .path()
        .resource_dir()
        .map_err(|e| crate::error::AppError::Other(format!("无法获取 resource 目录: {e}")))?;
    Ok(WgPaths::resolve(&res))
}

/// 自愈：扫 configs 目录，发现 root 拥有的 .conf 就调 helper 改回当前用户。
/// 应对历史问题——0.2.3 之前的 switch-rules 会把 conf chown 成 root，
/// 之后 App 普通进程读取就 Permission denied，整个规则页和编辑都瘫痪。
fn self_heal_config_owner(dir: &std::path::Path, paths: &WgPaths, log_dir: &std::path::Path) {
    use std::os::unix::fs::MetadataExt;
    let configs = dir.join("configs");
    if !configs.is_dir() {
        return;
    }
    let entries = match std::fs::read_dir(&configs) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut needs_fix = false;
    for ent in entries.flatten() {
        if let Ok(meta) = ent.metadata() {
            // 我们只关心被 root（uid=0）拿走的情况，普通用户写入是正常的
            if meta.uid() == 0 {
                needs_fix = true;
                log::warn!("检测到 root 拥有的 conf：{:?}", ent.path());
            }
        }
    }
    if !needs_fix {
        return;
    }
    let user = std::env::var("USER").unwrap_or_else(|_| "root".into());
    let configs_str = match configs.to_str() {
        Some(s) => s,
        None => return,
    };
    log::info!("尝试自愈 conf 所有者 → {}", user);
    if let Err(e) = wg::run_helper_oneshot(paths, log_dir, "fix-config-owner", &[configs_str, &user]) {
        log::warn!("自愈失败（用户可能取消了授权）: {}", e);
    } else {
        log::info!("conf 所有者已修复");
    }
}

#[tauri::command]
fn list_configs(app: AppHandle) -> AppResult<Vec<ConfigInfo>> {
    let dir = data_dir(&app)?;
    config::list_configs(&dir)
}

#[tauri::command]
fn save_config(app: AppHandle, name: String, content: String) -> AppResult<ConfigInfo> {
    let dir = data_dir(&app)?;
    log::info!("save_config name={} ({} bytes)", name, content.len());
    config::save_config(&dir, &name, &content)
}

#[tauri::command]
fn read_config(app: AppHandle, name: String) -> AppResult<String> {
    let dir = data_dir(&app)?;
    config::read_config(&dir, &name)
}

#[tauri::command]
fn delete_config(app: AppHandle, name: String) -> AppResult<()> {
    let dir = data_dir(&app)?;
    log::info!("delete_config name={}", name);
    config::delete_config(&dir, &name)
}

#[tauri::command]
fn connect(app: AppHandle, state: State<'_, AppState>, name: String) -> AppResult<TunnelStatus> {
    config::validate_name(&name)?;
    let dir = data_dir(&app)?;
    let path = config::config_path(&dir, &name);
    if !path.exists() {
        return Err(crate::error::AppError::ConfigNotFound(name));
    }
    let logs = log_dir(&app)?;
    let paths = wg_paths(&app)?;

    let prev_active: Option<String> = state.active.lock().ok().and_then(|g| g.clone());
    if let Some(prev) = &prev_active {
        if prev != &name {
            let prev_path = config::config_path(&dir, prev);
            if prev_path.exists() {
                log::info!("connect 切换：先 down 旧的 {}", prev);
                let _ = wg::down(&prev_path, &paths, &logs);
            }
        }
    }

    if let Ok(st) = wg::status(&name, &paths) {
        if st.connected {
            if let Ok(mut active) = state.active.lock() {
                *active = Some(name.clone());
            }
            sync_active_to_global(&state);
            return Ok(st);
        }
    }

    log::info!("connect name={}", name);
    wg::up(&path, &paths, &logs)?;
    if let Ok(mut active) = state.active.lock() {
        *active = Some(name.clone());
    }
    sync_active_to_global(&state);
    let st = wg::status(&name, &paths)?;
    let _ = app.emit("tunnel-changed", &st);
    Ok(st)
}

#[tauri::command]
fn disconnect(app: AppHandle, state: State<'_, AppState>, name: String) -> AppResult<()> {
    config::validate_name(&name)?;
    let dir = data_dir(&app)?;
    let path = config::config_path(&dir, &name);
    let logs = log_dir(&app)?;
    let paths = wg_paths(&app)?;
    log::info!("disconnect name={}", name);
    wg::down(&path, &paths, &logs)?;
    if let Ok(mut active) = state.active.lock() {
        if active.as_deref() == Some(name.as_str()) {
            *active = None;
        }
    }
    sync_active_to_global(&state);
    let _ = app.emit("tunnel-changed", ());
    Ok(())
}

#[tauri::command]
fn status(app: AppHandle, name: String) -> AppResult<TunnelStatus> {
    let paths = wg_paths(&app)?;
    wg::status(&name, &paths)
}

#[tauri::command]
fn external_ip() -> AppResult<String> {
    wg::external_ip()
}

#[tauri::command]
fn active_tunnel(state: State<'_, AppState>) -> AppResult<Option<String>> {
    // 优先读 ACTIVE_NAME（traffic_loop / tray 共用的全局），避免与 state.active
    // 不同步导致 UI 显示"已连接"但 activeName 为空。fallback 到 state.active。
    if let Some(n) = ACTIVE_NAME.lock().ok().and_then(|g| g.clone()) {
        return Ok(Some(n));
    }
    Ok(state.active.lock().ok().and_then(|g| g.clone()))
}

#[tauri::command]
fn open_log_dir(app: AppHandle) -> AppResult<()> {
    let dir = log_dir(&app)?;
    let _ = std::process::Command::new("open").arg(&dir).spawn()?;
    Ok(())
}

#[tauri::command]
fn passwordless_status(app: AppHandle) -> AppResult<PasswordlessInfo> {
    let paths = wg_paths(&app)?;
    Ok(wg::passwordless_info(&paths))
}

#[tauri::command]
fn enable_passwordless(app: AppHandle) -> AppResult<PasswordlessInfo> {
    let paths = wg_paths(&app)?;
    let logs = log_dir(&app)?;
    wg::enable_passwordless(&paths, &logs)
}

#[tauri::command]
fn disable_passwordless(app: AppHandle) -> AppResult<PasswordlessInfo> {
    let paths = wg_paths(&app)?;
    let logs = log_dir(&app)?;
    wg::disable_passwordless(&paths, &logs)
}

#[tauri::command]
fn frontend_log(level: String, message: String) {
    match level.as_str() {
        "error" => log::error!("[js] {}", message),
        "warn" => log::warn!("[js] {}", message),
        "debug" => log::debug!("[js] {}", message),
        _ => log::info!("[js] {}", message),
    }
}

#[derive(serde::Serialize)]
struct FullSettings {
    #[serde(flatten)]
    base: Settings,
    kill_switch_active: bool,
}

#[tauri::command]
fn get_settings(app: AppHandle) -> AppResult<FullSettings> {
    let dir = data_dir(&app)?;
    let paths = wg_paths(&app)?;
    Ok(FullSettings {
        base: settings::load(&dir),
        kill_switch_active: wg::killswitch_status(&paths),
    })
}

#[tauri::command]
fn set_settings(app: AppHandle, new: Settings) -> AppResult<FullSettings> {
    let dir = data_dir(&app)?;
    let paths = wg_paths(&app)?;
    let logs = log_dir(&app)?;
    let prev = settings::load(&dir);
    settings::save(&dir, &new)?;

    if new.kill_switch != prev.kill_switch {
        if new.kill_switch {
            let active = ACTIVE_NAME.lock().ok().and_then(|g| g.clone());
            let candidate = active
                .clone()
                .or(new.auto_connect_on_start.clone())
                .or_else(|| {
                    config::list_configs(&dir)
                        .ok()
                        .and_then(|v| v.into_iter().next().map(|c| c.name))
                });
            let name = candidate.ok_or_else(|| {
                crate::error::AppError::Other("启用 Kill Switch 需要至少一个有效配置".into())
            })?;
            let conf = config::config_path(&dir, &name);
            wg::killswitch_set(true, Some(&conf), &paths, &logs)?;
        } else {
            wg::killswitch_set(false, None, &paths, &logs)?;
        }
    }

    if new.launch_at_login != prev.launch_at_login {
        if new.launch_at_login {
            launch_agent::install(&app)?;
        } else {
            launch_agent::uninstall()?;
        }
    }

    Ok(FullSettings {
        base: settings::load(&dir),
        kill_switch_active: wg::killswitch_status(&paths),
    })
}

// ===== 规则模式 =====

#[tauri::command]
fn list_rule_templates() -> Vec<RuleTemplate> {
    rules::list_templates()
}

#[derive(serde::Serialize)]
struct RuleStateResp {
    mode: RuleMode,
    allowed_ips: String,
}

#[tauri::command]
fn get_rule_state(app: AppHandle, name: String) -> AppResult<RuleStateResp> {
    let dir = data_dir(&app)?;
    let conf = config::read_config(&dir, &name)?;
    let mode = rules::detect_mode(&conf);
    let allowed_ips = rules::extract_allowed_ips(&conf).unwrap_or_default();
    Ok(RuleStateResp { mode, allowed_ips })
}

#[tauri::command]
fn apply_rule_mode(app: AppHandle, name: String, mode: RuleMode, custom: Option<String>) -> AppResult<RuleStateResp> {
    let dir = data_dir(&app)?;
    let paths = wg_paths(&app)?;
    let conf_path = config::config_path(&dir, &name);
    if !conf_path.exists() {
        return Err(crate::error::AppError::ConfigNotFound(name));
    }
    let allowed = match mode {
        RuleMode::Custom => custom.unwrap_or_else(|| "0.0.0.0/0".into()),
        other => rules::list_templates()
            .into_iter()
            .find(|t| t.mode == other)
            .map(|t| t.allowed_ips)
            .unwrap_or_else(|| "0.0.0.0/0".into()),
    };
    log::info!("apply_rule_mode name={} mode={:?} ({} CIDRs)", name, mode, allowed.matches(',').count() + 1);
    let logs = log_dir(&app).unwrap_or_else(|_| dir.clone());
    rules::apply_mode(&paths, &logs, &conf_path, &allowed)?;
    let conf = config::read_config(&dir, &name)?;
    Ok(RuleStateResp {
        mode: rules::detect_mode(&conf),
        allowed_ips: rules::extract_allowed_ips(&conf).unwrap_or_default(),
    })
}

// ===== 诊断 =====

#[tauri::command]
fn run_diagnostics(app: AppHandle, name: Option<String>) -> AppResult<DiagResult> {
    let conf_text = if let Some(n) = name {
        let dir = data_dir(&app)?;
        config::read_config(&dir, &n).ok()
    } else {
        None
    };
    Ok(diag::run(conf_text.as_deref()))
}

// ===== 日志 tail =====

#[tauri::command]
fn read_log_tail(app: AppHandle, file: String, max_bytes: Option<u64>) -> AppResult<String> {
    let dir = log_dir(&app)?;
    let safe = file.replace('/', "").replace('\\', "");
    let path = dir.join(&safe);
    if !path.exists() {
        return Ok(String::new());
    }
    let max = max_bytes.unwrap_or(64 * 1024);
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(&path)?;
    let len = f.metadata()?.len();
    let start = if len > max { len - max } else { 0 };
    f.seek(SeekFrom::Start(start))?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    Ok(buf)
}

#[tauri::command]
fn list_log_files(app: AppHandle) -> AppResult<Vec<String>> {
    let dir = log_dir(&app)?;
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if let Some(n) = e.file_name().to_str() {
                if n.ends_with(".log") {
                    out.push(n.to_string());
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

#[tauri::command]
fn show_main_window(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    Ok(())
}

// ===== 应用内升级 =====

fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn manifest_url(app: &AppHandle) -> String {
    let dir = match data_dir(app) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let s = settings::load(&dir);
    if !s.update_manifest_url.trim().is_empty() {
        return s.update_manifest_url.trim().to_string();
    }
    updater::DEFAULT_MANIFEST_URL.to_string()
}

#[tauri::command]
fn check_update(app: AppHandle) -> AppResult<updater::UpdateCheck> {
    let url = manifest_url(&app);
    updater::check(&url, &current_version())
}

#[tauri::command]
async fn download_and_install_update(
    app: AppHandle,
    manifest: updater::Manifest,
) -> AppResult<()> {
    let app_clone = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> AppResult<()> {
        let dmg = updater::download(&app_clone, &manifest)?;
        let paths = wg_paths(&app_clone)?;
        let logs = log_dir(&app_clone).unwrap_or_else(|_| std::env::temp_dir());
        log::info!("准备安装更新 {} → 调 helper install-app", dmg.display());
        // helper 会 kill 当前 App 并启动新版，本调用通常永远不会"成功返回"——
        // 实际上 sudo 会一直阻塞直到 helper 退出。helper 在 kill 自己之前会 open。
        updater::install(&dmg, &paths, &logs)
    })
    .await
    .map_err(|e| crate::error::AppError::Other(format!("升级线程 panic: {}", e)))??;
    Ok(())
}

static ACTIVE_NAME: std::sync::LazyLock<std::sync::Mutex<Option<String>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(None));

fn sync_active_to_global(state: &State<'_, AppState>) {
    if let Ok(g) = state.active.lock() {
        if let Ok(mut s) = ACTIVE_NAME.lock() {
            *s = g.clone();
        }
    }
}

mod launch_agent {
    use std::path::PathBuf;
    use tauri::AppHandle;

    use crate::error::{AppError, AppResult};

    const LABEL: &str = "com.mengfan.wgvpn.launch";

    fn plist_path() -> AppResult<PathBuf> {
        let home = std::env::var("HOME").map_err(|_| AppError::Other("无法获取 $HOME".into()))?;
        Ok(PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", LABEL)))
    }

    pub fn install(_app: &AppHandle) -> AppResult<()> {
        let exe = std::env::current_exe()
            .map_err(|e| AppError::Other(format!("无法获取 App 路径: {e}")))?;
        let app_path = {
            let mut p = exe.clone();
            for _ in 0..3 {
                p.pop();
            }
            p
        };
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/open</string>
        <string>-a</string>
        <string>{app_path}</string>
    </array>
    <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
            label = LABEL,
            app_path = app_path.to_string_lossy()
        );
        let path = plist_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&path, body)?;
        let _ = std::process::Command::new("/bin/launchctl")
            .args(["load", "-w", path.to_string_lossy().as_ref()])
            .status();
        Ok(())
    }

    pub fn uninstall() -> AppResult<()> {
        let path = plist_path()?;
        if path.exists() {
            let _ = std::process::Command::new("/bin/launchctl")
                .args(["unload", "-w", path.to_string_lossy().as_ref()])
                .status();
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .level(log::LevelFilter::Info)
                .max_file_size(2 * 1024 * 1024)
                // KeepOne：单文件满 2MB 后轮转，最多保留 1 份历史 + 当前 = 4MB 上限。
                // KeepAll 会无限累积，长期使用不安全。
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            active: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            list_configs,
            save_config,
            read_config,
            delete_config,
            connect,
            disconnect,
            status,
            external_ip,
            active_tunnel,
            open_log_dir,
            passwordless_status,
            enable_passwordless,
            disable_passwordless,
            get_settings,
            set_settings,
            list_rule_templates,
            get_rule_state,
            apply_rule_mode,
            run_diagnostics,
            read_log_tail,
            list_log_files,
            show_main_window,
            frontend_log,
            check_update,
            download_and_install_update,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .setup(|app| {
            log::info!("=== Soar 启动 ===");
            // 启动时扫描已存在的隧道，恢复 ACTIVE_NAME（避免 App 重启后 UI 显示
            // "未连接" 但实际网卡还在线的误导）
            if let (Ok(dir), Ok(paths)) = (data_dir(app.handle()), wg_paths(app.handle())) {
                // 自愈：早期版本（≤0.2.3）的 switch-rules 会把 conf 文件 chown 给 root，
                // 之后 App 普通进程读不了。这里检测一次，发现就调 helper 修回来。
                if let Ok(logs) = log_dir(app.handle()) {
                    self_heal_config_owner(&dir, &paths, &logs);
                }
                if let Ok(items) = config::list_configs(&dir) {
                    for it in items {
                        if let Ok(st) = wg::status(&it.name, &paths) {
                            if st.connected {
                                // 同时写两个 active 状态，否则前端拿到 null：
                                //  - ACTIVE_NAME（全局 static）：tray / 后台 loop 读
                                //  - state.active（AppState）：active_tunnel 命令读，前端 store 取
                                if let Ok(mut g) = ACTIVE_NAME.lock() {
                                    *g = Some(it.name.clone());
                                }
                                let state = app.state::<AppState>();
                                if let Ok(mut g) = state.active.lock() {
                                    *g = Some(it.name.clone());
                                }
                                log::info!(
                                    "启动时检测到在线隧道: {} (state.active 已写入)",
                                    it.name
                                );
                                // 延迟 1.5s 主动通知前端 refresh（前端 listener 此时已就绪）
                                let h = app.handle().clone();
                                std::thread::spawn(move || {
                                    std::thread::sleep(Duration::from_millis(1500));
                                    let _ = h.emit("tunnel-changed", ());
                                });
                                break;
                            }
                        }
                    }
                }
            }
            build_tray(app.handle())?;
            let h1 = app.handle().clone();
            std::thread::spawn(move || background_loop(h1));
            let h2 = app.handle().clone();
            std::thread::spawn(move || traffic_loop(h2));
            // 启动 8s 后静默检查更新一次（避免和首屏渲染抢资源）
            let h3 = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(8));
                let dir = match data_dir(&h3) {
                    Ok(d) => d,
                    Err(_) => return,
                };
                let s = settings::load(&dir);
                if !s.auto_check_update {
                    return;
                }
                let url = manifest_url(&h3);
                if url.is_empty() {
                    return;
                }
                match updater::check(&url, &current_version()) {
                    Ok(c) if c.has_update => {
                        log::info!(
                            "检测到新版本: {} → {}",
                            c.current,
                            c.latest.as_ref().map(|m| m.version.as_str()).unwrap_or("?")
                        );
                        let _ = h3.emit("update-available", &c);
                    }
                    Ok(_) => log::info!("当前已是最新版本"),
                    Err(e) => log::warn!("检查更新失败: {}", e),
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    let show = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
    let toggle = MenuItemBuilder::with_id("toggle", "连接 / 断开当前").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "退出").build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show, &toggle])
        .separator()
        .item(&quit)
        .build()?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("Soar")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "toggle" => {
                let app_h = app.clone();
                std::thread::spawn(move || tray_toggle(&app_h));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn tray_toggle(app: &AppHandle) {
    let dir = match data_dir(app) {
        Ok(d) => d,
        Err(_) => return,
    };
    let paths = match wg_paths(app) {
        Ok(p) => p,
        Err(_) => return,
    };
    let logs = log_dir(app).unwrap_or_else(|_| dir.clone());
    let name = ACTIVE_NAME
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .or_else(|| settings::load(&dir).auto_connect_on_start)
        .or_else(|| {
            config::list_configs(&dir)
                .ok()
                .and_then(|v| v.into_iter().next().map(|c| c.name))
        });
    let Some(name) = name else { return };
    let conf = config::config_path(&dir, &name);
    if !conf.exists() {
        return;
    }
    let st = wg::status(&name, &paths).ok();
    let connected = st.as_ref().map(|s| s.connected).unwrap_or(false);
    if connected {
        let _ = wg::down(&conf, &paths, &logs);
        if let Ok(mut g) = ACTIVE_NAME.lock() {
            *g = None;
        }
    } else if wg::up(&conf, &paths, &logs).is_ok() {
        if let Ok(mut g) = ACTIVE_NAME.lock() {
            *g = Some(name);
        }
    }
    let _ = app.emit("tunnel-changed", ());
}

fn background_loop(app: AppHandle) {
    std::thread::sleep(Duration::from_millis(1500));
    let dir = match data_dir(&app) {
        Ok(d) => d,
        Err(_) => return,
    };
    let paths = match wg_paths(&app) {
        Ok(p) => p,
        Err(_) => return,
    };
    let logs = log_dir(&app).unwrap_or_else(|_| dir.clone());

    let s = settings::load(&dir);
    if let Some(name) = &s.auto_connect_on_start {
        let conf = config::config_path(&dir, name);
        if conf.exists() {
            log::info!("启动时自动连接: {}", name);
            if wg::up(&conf, &paths, &logs).is_ok() {
                if let Ok(mut g) = ACTIVE_NAME.lock() {
                    *g = Some(name.clone());
                }
                let _ = app.emit("tunnel-changed", ());
            }
        }
    }

    loop {
        std::thread::sleep(Duration::from_secs(30));
        let s = settings::load(&dir);
        if !s.auto_reconnect {
            continue;
        }
        let name = ACTIVE_NAME.lock().ok().and_then(|g| g.clone());
        let Some(name) = name else { continue };
        let conf = config::config_path(&dir, &name);
        if !conf.exists() {
            continue;
        }
        let st = match wg::status(&name, &paths) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let stale = st
            .peer
            .as_ref()
            .and_then(|p| p.latest_handshake_secs)
            .map(|s| s == u64::MAX || s > 180)
            .unwrap_or(true);
        if !st.connected || stale {
            log::warn!("自动重连：{} → down/up", name);
            let _ = wg::down(&conf, &paths, &logs);
            // 等内核彻底释放 utun 设备 + 清掉 .name/.sock，否则 up 会报
            // `'xxx' already exists as 'utunN'`。3s 是经验值。
            std::thread::sleep(Duration::from_secs(3));
            let _ = wg::up(&conf, &paths, &logs);
            let _ = app.emit("tunnel-changed", ());
        }
    }
}

fn fmt_bps(n: u64) -> String {
    let units = ["B", "K", "M", "G"];
    let mut i = 0;
    let mut v = n as f64;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 {
        format!("{:.0}{}/s", v, units[i])
    } else if v >= 10.0 {
        format!("{:.1}{}/s", v, units[i])
    } else {
        format!("{:.2}{}/s", v, units[i])
    }
}

/// 流量采样：每秒一次，emit `traffic` 事件 + 更新托盘 tooltip
fn traffic_loop(app: AppHandle) {
    let paths = match wg_paths(&app) {
        Ok(p) => p,
        Err(_) => return,
    };
    let mut last_rx: u64 = 0;
    let mut last_tx: u64 = 0;
    let mut last_t = Instant::now();
    let mut last_name: Option<String> = None;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let name = ACTIVE_NAME.lock().ok().and_then(|g| g.clone());
        let Some(name) = name else {
            last_rx = 0;
            last_tx = 0;
            last_name = None;
            let _ = app.emit("traffic", serde_json::json!({"connected": false}));
            if let Some(tray) = app.tray_by_id("main-tray") {
                let _ = tray.set_tooltip(Some("Soar · 未连接"));
            }
            continue;
        };
        let st = match wg::status(&name, &paths) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !st.connected {
            let _ = app.emit("traffic", serde_json::json!({"connected": false}));
            if let Some(tray) = app.tray_by_id("main-tray") {
                let _ = tray.set_tooltip(Some("Soar · 未连接"));
            }
            continue;
        }
        let (rx, tx) = st
            .peer
            .as_ref()
            .map(|p| (p.transfer_rx.unwrap_or(0), p.transfer_tx.unwrap_or(0)))
            .unwrap_or((0, 0));
        let now = Instant::now();
        let dt = now.duration_since(last_t).as_secs_f64().max(0.001);
        let (rx_bps, tx_bps) = if last_name.as_deref() == Some(&name) && rx >= last_rx && tx >= last_tx {
            (
                ((rx - last_rx) as f64 / dt) as u64,
                ((tx - last_tx) as f64 / dt) as u64,
            )
        } else {
            (0, 0)
        };
        last_rx = rx;
        last_tx = tx;
        last_t = now;
        last_name = Some(name.clone());
        let handshake_age = st
            .peer
            .as_ref()
            .and_then(|p| p.latest_handshake_secs);
        let _ = app.emit(
            "traffic",
            serde_json::json!({
                "connected": true,
                "name": name,
                "rx_bps": rx_bps,
                "tx_bps": tx_bps,
                "total_rx": rx,
                "total_tx": tx,
                "handshake_age": handshake_age,
            }),
        );
        if let Some(tray) = app.tray_by_id("main-tray") {
            let tip = format!(
                "Soar · {}\n↓ {}   ↑ {}",
                name,
                fmt_bps(rx_bps),
                fmt_bps(tx_bps),
            );
            let _ = tray.set_tooltip(Some(&tip));
        }
    }
}
