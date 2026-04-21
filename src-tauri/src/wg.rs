use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// /etc/sudoers.d/ 中我们写入的文件名
const SUDOERS_FILENAME: &str = "wg-vpn";

/// 稳定 helper 路径：sudoers 永久授权这条路径。
/// App 改名（WG VPN → MaiSui → Soar）或升级都不会让免密失效。
/// 启动时如果检测到 .app 内的 helper 比这里的新，会自动用 sudo -n 调用
/// 稳定路径的 install-self 子命令把自己更新到最新版（无密码框）。
const STABLE_HELPER_DIR: &str = "/Library/Application Support/Soar";
const STABLE_HELPER_PATH: &str = "/Library/Application Support/Soar/wg-helper.sh";

fn stable_helper_path() -> PathBuf {
    PathBuf::from(STABLE_HELPER_PATH)
}

/// 计算文件 sha256（调 /usr/bin/shasum，不引入额外依赖）
fn sha256_of_file(p: &Path) -> Option<String> {
    let out = Command::new("/usr/bin/shasum")
        .args(["-a", "256", p.to_str()?])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    s.split_whitespace().next().map(|s| s.to_string())
}

#[derive(Debug, Clone)]
pub struct WgPaths {
    pub wg: PathBuf,
    pub wg_quick: PathBuf,
    pub wireguard_go: PathBuf,
    /// bundled wg-helper.sh，存在则代表可以做免密
    pub wg_helper: Option<PathBuf>,
}

impl WgPaths {
    /// 优先使用 app 资源目录里的 bundled 二进制；
    /// 找不到就回退到系统 PATH（开发或调试场景）
    pub fn resolve(resource_dir: &Path) -> Self {
        let bundled = resource_dir.join("wireguard");
        let wg = bundled.join("wg");
        let wg_quick = bundled.join("wg-quick");
        let wireguard_go = bundled.join("wireguard-go");
        let helper = bundled.join("wg-helper.sh");
        let exists = wg.is_file() && wg_quick.is_file() && wireguard_go.is_file();
        if exists {
            return Self {
                wg,
                wg_quick,
                wireguard_go,
                wg_helper: helper.is_file().then_some(helper),
            };
        }
        Self {
            wg: which("wg"),
            wg_quick: which("wg-quick"),
            wireguard_go: which("wireguard-go"),
            wg_helper: None,
        }
    }

    /// wg-quick 所在目录会自动被加入 PATH，wg 必须和 wg-quick 同目录
    /// 我们这里再补上系统目录，保证 networksetup/route/ifconfig 都找得到
    pub fn shell_path(&self) -> String {
        let wg_dir = self
            .wg_quick
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        format!(
            "{}:/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin",
            wg_dir
        )
    }
}

fn which(cmd: &str) -> PathBuf {
    for prefix in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
    ] {
        let p = PathBuf::from(prefix).join(cmd);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(cmd)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PeerStats {
    pub endpoint: Option<String>,
    pub allowed_ips: Option<String>,
    pub latest_handshake_secs: Option<u64>,
    pub transfer_rx: Option<u64>,
    pub transfer_tx: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TunnelStatus {
    pub name: String,
    pub connected: bool,
    pub interface: Option<String>,
    pub peer: Option<PeerStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordlessInfo {
    /// 是否已启用免密
    pub enabled: bool,
    /// 是否具备启用免密的能力（即 wg-helper.sh 存在）
    pub available: bool,
    /// /etc/sudoers.d/wg-vpn 中实际授权的 helper 路径
    pub authorized_helper: Option<String>,
    /// 当前 app 的 helper 路径
    pub current_helper: Option<String>,
}

fn sudoers_path() -> PathBuf {
    PathBuf::from("/etc/sudoers.d").join(SUDOERS_FILENAME)
}

/// 当前用户名（USER env），获取不到就报错
fn current_user() -> AppResult<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .map_err(|_| AppError::Other("无法获取当前用户名（$USER 为空）".into()))
}

/// 读出 sudoers 文件中授权的 helper 路径（解析 NOPASSWD: 之后的部分）
fn read_authorized_helper() -> Option<String> {
    let content = std::fs::read_to_string(sudoers_path()).ok()?;
    for line in content.lines() {
        if let Some(idx) = line.find("NOPASSWD:") {
            let rest = line[idx + "NOPASSWD:".len()..].trim();
            // sudoers 中空格用 \ 转义，把 "\ " 还原为 " "
            return Some(rest.replace("\\ ", " ").trim().to_string());
        }
    }
    None
}

pub fn passwordless_info(paths: &WgPaths) -> PasswordlessInfo {
    let current = paths
        .wg_helper
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    // 免密的判定 = `sudo -n -l <稳定路径>` 成功。
    // 稳定路径不随 .app 名字 / 安装位置 / 版本变化，所以 sudoers 写一次就永久生效。
    // 老用户（sudoers 还指向 /Applications/WG VPN.app/...）在这里会显示 enabled=false，
    // 设置页会引导他重新启用一次（一次密码框），之后就再不会失配。
    let enabled = can_sudo_n(&stable_helper_path());
    PasswordlessInfo {
        enabled,
        available: paths.wg_helper.is_some(),
        authorized_helper: read_authorized_helper(),
        current_helper: current,
    }
}

/// 启动时调用：让 /Library/Application Support/Soar/wg-helper.sh 与 .app 内 helper 保持一致。
/// - 免密未启用 / sudoers 失配：什么都不做（用户在设置里启用免密时会一次性写入）
/// - sha256 一致：什么都不做
/// - sha256 不一致 + 免密可用：sudo -n 调稳定路径自身的 install-self（无密码框）
/// 失败仅 warn，不阻塞启动。
pub fn sync_stable_helper_if_needed(paths: &WgPaths) {
    let Some(app_helper) = paths.wg_helper.as_ref() else {
        return;
    };
    let stable = stable_helper_path();

    let app_sha = sha256_of_file(app_helper).unwrap_or_default();
    let stable_sha = if stable.is_file() {
        sha256_of_file(&stable).unwrap_or_default()
    } else {
        String::new()
    };

    if !app_sha.is_empty() && app_sha == stable_sha {
        return;
    }

    if !can_sudo_n(&stable) {
        log::info!(
            "稳定 helper 与 .app 不一致（stable_sha={}, app_sha={}），但免密未授权稳定路径，跳过同步。\
             用户下次在设置启用免密时会自动写入新版本。",
            if stable_sha.is_empty() { "<missing>" } else { &stable_sha[..8] },
            if app_sha.is_empty() { "<unknown>" } else { &app_sha[..8] }
        );
        return;
    }

    log::info!(
        "稳定 helper 已过期（{} → {}），用 sudo -n 自动同步",
        if stable_sha.is_empty() { "<missing>".into() } else { stable_sha[..8].to_string() },
        if app_sha.is_empty() { "<unknown>".into() } else { app_sha[..8].to_string() }
    );
    let res = Command::new("/usr/bin/sudo")
        .args([
            "-n",
            STABLE_HELPER_PATH,
            "install-self",
            &app_helper.to_string_lossy(),
        ])
        .output();
    match res {
        Ok(o) if o.status.success() => log::info!("稳定 helper 同步完成"),
        Ok(o) => log::warn!(
            "稳定 helper 同步失败 status={}: {}",
            o.status,
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => log::warn!("调用 sudo -n install-self 失败: {}", e),
    }
}

/// 选择本次执行的 helper 路径：
/// - 免密已生效 + 稳定路径文件存在 → 稳定路径（与 sudoers 授权一致，sudo -n 能匹配）
/// - 否则 → .app 内的 helper（走 osascript 弹密码框，每次操作时认证）
///
/// 同时返回是否可以走 sudo -n（免密通道）。
fn select_helper(paths: &WgPaths) -> Option<(PathBuf, bool)> {
    let info = passwordless_info(paths);
    let stable = stable_helper_path();
    if info.enabled && stable.is_file() {
        return Some((stable, true));
    }
    paths.wg_helper.as_ref().map(|p| (p.clone(), false))
}

/// 当前用户是否能免密 sudo 执行该路径。
/// `sudo -n -l <path>` 在能免密时退出码 0；否则非 0（包括需要密码 / 未授权）。
fn can_sudo_n(path: &Path) -> bool {
    let path_str = match path.to_str() {
        Some(s) => s,
        None => return false,
    };
    let out = Command::new("/usr/bin/sudo")
        .args(["-n", "-l", path_str])
        .output();
    match out {
        Ok(o) => {
            let ok = o.status.success();
            log::debug!(
                "sudo -n -l {} -> exit={:?} ok={} stdout={:?} stderr={:?}",
                path_str,
                o.status.code(),
                ok,
                String::from_utf8_lossy(&o.stdout).trim(),
                String::from_utf8_lossy(&o.stderr).trim(),
            );
            ok
        }
        Err(e) => {
            log::warn!("调 /usr/bin/sudo 失败: {}", e);
            false
        }
    }
}

/// sudoers 文件中转义路径里的空格（其它字符 sudoers 也接受，无需转义）
fn escape_sudoers_path(p: &str) -> String {
    p.replace(' ', "\\ ")
}

/// 启用免密：一次性 osascript 完成
///   1. mkdir -p /Library/Application Support/Soar
///   2. cp .app/wg-helper.sh → /Library/Application Support/Soar/wg-helper.sh
///   3. chown root:wheel + chmod 0755（任何普通用户改不了，sudo 才认）
///   4. 写 /etc/sudoers.d/wg-vpn 授权 *稳定路径*（不是 .app 内路径）
/// 之后无论 .app 怎么重命名 / 升级，sudoers 都不会失配。
pub fn enable_passwordless(paths: &WgPaths, log_dir: &Path) -> AppResult<PasswordlessInfo> {
    let helper = paths
        .wg_helper
        .as_ref()
        .ok_or_else(|| AppError::Other("当前 App 缺少 wg-helper.sh，无法启用免密".into()))?;
    let app_helper_str = helper.to_string_lossy().to_string();
    let user = current_user()?;

    // sudoers 授权的是稳定路径（路径里有空格，sudoers 要求 `\ ` 转义）
    let stable_str = STABLE_HELPER_PATH.to_string();
    let line = format!(
        "{} ALL=(root) NOPASSWD: {}\n",
        user,
        escape_sudoers_path(&stable_str)
    );
    let body = format!(
        "# Soar auto-managed. 删除此文件即可恢复每次连接都需要密码。\n\
         # 授权当前用户免密执行稳定路径下的 wg-helper.sh。\n\
         # 该路径不随 App 改名 / 升级而变化，所以一次启用永久生效。\n\
         {}",
        line
    );

    let tmp_sudo = std::env::temp_dir().join(format!("wg-vpn.sudoers.{}", std::process::id()));
    std::fs::write(&tmp_sudo, body)?;
    let tmp_sudo_str = tmp_sudo.to_string_lossy().to_string();

    let target = sudoers_path().to_string_lossy().to_string();

    // 单 osascript 弹一次密码框，把所有事一并做完
    let q = |s: &str| s.replace('\'', "'\\''");
    let script = format!(
        "/bin/mkdir -p '{dir}' && \
         /bin/chmod 0755 '{dir}' && \
         /bin/cp -f '{src}' '{dst}' && \
         /usr/sbin/chown root:wheel '{dst}' && \
         /bin/chmod 0755 '{dst}' && \
         /usr/sbin/visudo -cf '{tmp}' && \
         /usr/sbin/chown root:wheel '{tmp}' && \
         /bin/chmod 0440 '{tmp}' && \
         /bin/mv '{tmp}' '{sudoers}'",
        dir = q(STABLE_HELPER_DIR),
        src = q(&app_helper_str),
        dst = q(STABLE_HELPER_PATH),
        tmp = q(&tmp_sudo_str),
        sudoers = q(&target),
    );

    log::info!(
        "启用免密：把 {} 安装到 {}，sudoers 授权稳定路径 → {}",
        app_helper_str,
        STABLE_HELPER_PATH,
        target
    );
    let res = run_admin_osascript(&script, log_dir);
    let _ = std::fs::remove_file(&tmp_sudo);
    res?;

    let info = passwordless_info(paths);
    if !info.enabled {
        log::error!(
            "sudoers 写入后校验未生效：authorized={:?} stable={}",
            info.authorized_helper,
            STABLE_HELPER_PATH
        );
        return Err(AppError::Other(
            "sudoers 写入成功但校验未生效，请查看日志".into(),
        ));
    }
    Ok(info)
}

/// 关闭免密：删除 sudoers + 稳定路径下的 helper（彻底清理）
pub fn disable_passwordless(paths: &WgPaths, log_dir: &Path) -> AppResult<PasswordlessInfo> {
    let target = sudoers_path().to_string_lossy().to_string();
    let q = |s: &str| s.replace('\'', "'\\''");
    let script = format!(
        "/bin/rm -f '{}' && /bin/rm -f '{}' && /bin/rmdir '{}' 2>/dev/null || true",
        q(&target),
        q(STABLE_HELPER_PATH),
        q(STABLE_HELPER_DIR),
    );
    log::info!("关闭免密：删除 {} 和稳定 helper", target);
    run_admin_osascript(&script, log_dir)?;
    Ok(passwordless_info(paths))
}

/// 通过 osascript 提权执行命令（Mac 原生授权弹窗），并把命令本身的 stdout+stderr
/// 重定向到 log_dir/wg-quick.log，命令结束后我们读出来打到 log
#[cfg(target_os = "macos")]
fn run_admin_osascript(script: &str, log_dir: &Path) -> AppResult<String> {
    let cmd_log = log_dir.join("wg-quick.log");
    if let Some(parent) = cmd_log.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let cmd_log_str = cmd_log.to_string_lossy().to_string();

    let full = format!(
        "{{ {} ; }} >> '{}' 2>&1",
        script,
        cmd_log_str.replace('\'', "'\\''")
    );
    let escaped = full.replace('\\', "\\\\").replace('"', "\\\"");
    let osa = format!(
        "do shell script \"{}\" with administrator privileges",
        escaped
    );

    log::debug!("osascript: {}", script);
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", &osa])
        .output()?;

    let tail = read_tail(&cmd_log, 4096).unwrap_or_default();
    if !tail.trim().is_empty() {
        log::info!("提权命令输出 (tail):\n{}", tail.trim_end());
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.contains("User canceled") || stderr.contains("用户已取消") {
            log::warn!("用户取消了授权");
            return Err(AppError::UserCancelled);
        }
        log::error!(
            "提权命令失败 status={} stderr={} tail={}",
            output.status,
            stderr.trim(),
            tail.trim_end()
        );
        return Err(AppError::Command(format!(
            "命令执行失败: {}\n详见日志: {}",
            stderr.trim(),
            cmd_log_str
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(not(target_os = "macos"))]
fn run_admin_osascript(_script: &str, _log_dir: &Path) -> AppResult<String> {
    Err(AppError::Other(
        "当前平台暂未实现提权，请先在 macOS 下运行".into(),
    ))
}

/// 调用 helper 执行 killswitch-on / killswitch-off。需要 helper 存在，且——
/// killswitch-on 需要传 conf 路径（用于解析 endpoint）；off 不需要。
pub fn killswitch_set(
    enable: bool,
    conf: Option<&Path>,
    paths: &WgPaths,
    log_dir: &Path,
) -> AppResult<()> {
    let helper = paths
        .wg_helper
        .as_ref()
        .ok_or_else(|| AppError::Other("当前 App 缺少 wg-helper.sh".into()))?;
    let action = if enable { "killswitch-on" } else { "killswitch-off" };
    let conf_arg = conf.map(|p| p.to_path_buf());
    run_helper_action(helper, action, conf_arg.as_deref(), paths, log_dir)
}

pub fn killswitch_status(paths: &WgPaths) -> bool {
    // 这个不需要 root：直接看 /var/run/wg-vpn/killswitch.on 是否存在
    Path::new("/var/run/wg-vpn/killswitch.on").exists()
        && paths.wg_helper.is_some()
}

/// 通用 helper 调用：把 action + 任意字符串参数拼起来交给 sudo -n / osascript。
/// 用于 install-app 这类参数自由的子命令。
pub fn run_helper_oneshot(
    paths: &WgPaths,
    log_dir: &Path,
    action: &str,
    args: &[&str],
) -> AppResult<()> {
    let (helper, use_sudo) = select_helper(paths)
        .ok_or_else(|| AppError::Other("当前 App 缺少 wg-helper.sh".into()))?;
    let helper_str = helper
        .to_str()
        .ok_or_else(|| AppError::Other("helper 路径含非法字符".into()))?;
    let arg_strs: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();

    let cmd_log = log_dir.join("wg-quick.log");
    if let Some(parent) = cmd_log.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cmd_log)?;
    use std::io::Write;

    if use_sudo {
        let mut sudo_args = vec!["-n".to_string(), helper_str.into(), action.into()];
        sudo_args.extend(arg_strs.iter().cloned());
        let output = Command::new("/usr/bin/sudo").args(&sudo_args).output()?;
        let _ = writeln!(
            log_file,
            "[sudo -n {} {:?}]\nstdout:\n{}\nstderr:\n{}",
            action,
            arg_strs,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            return Err(AppError::Command(format!(
                "helper {} 失败: {}",
                action,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        return Ok(());
    }

    let mut script = format!("'{}' {}", helper_str.replace('\'', "'\\''"), action);
    for a in &arg_strs {
        script.push_str(&format!(" '{}'", a.replace('\'', "'\\''")));
    }
    let out = run_admin_osascript(&script, log_dir)?;
    let _ = writeln!(log_file, "[osascript {}]\n{}", action, out);
    Ok(())
}

/// 调用 helper switch-rules：原子地写 conf + wg set + 重建路由（需要 root）
pub fn switch_rules(
    conf: &Path,
    allowed_ips: &str,
    paths: &WgPaths,
    log_dir: &Path,
) -> AppResult<()> {
    let (helper, use_sudo) = select_helper(paths)
        .ok_or_else(|| AppError::Other("当前 App 缺少 wg-helper.sh".into()))?;
    let helper_str = helper
        .to_str()
        .ok_or_else(|| AppError::Other("helper 路径含非法字符".into()))?;
    let conf_str = conf
        .to_str()
        .ok_or_else(|| AppError::Other("配置路径含非法字符".into()))?;

    let cmd_log = log_dir.join("wg-quick.log");
    if let Some(parent) = cmd_log.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cmd_log)?;
    use std::io::Write;

    if use_sudo {
        let output = Command::new("/usr/bin/sudo")
            .args(["-n", helper_str, "switch-rules", conf_str, allowed_ips])
            .output()?;
        let cidr_count = allowed_ips.matches(',').count() + 1;
        let _ = writeln!(
            log_file,
            "[switch-rules {} cidrs={}]\nstdout:\n{}\nstderr:\n{}",
            std::path::Path::new(conf_str)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(conf_str),
            cidr_count,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
        if !output.status.success() {
            return Err(AppError::Command(format!(
                "helper switch-rules 失败: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        return Ok(());
    }

    let script = format!(
        "'{}' switch-rules '{}' '{}'",
        helper_str.replace('\'', "'\\''"),
        conf_str.replace('\'', "'\\''"),
        allowed_ips.replace('\'', "'\\''"),
    );
    let out = run_admin_osascript(&script, log_dir)?;
    let _ = writeln!(log_file, "[osascript switch-rules]\n{}", out);
    Ok(())
}

/// 通用 helper 调用：免密走 sudo -n，否则 osascript。
/// 注意：传入的 `helper` 参数仅在非免密模式作为 osascript 的执行路径；免密模式
/// 永远走稳定路径（select_helper 选定）。
fn run_helper_action(
    helper: &Path,
    action: &str,
    conf: Option<&Path>,
    paths: &WgPaths,
    log_dir: &Path,
) -> AppResult<()> {
    let (run_helper, use_sudo) = select_helper(paths).unwrap_or_else(|| (helper.to_path_buf(), false));
    let helper_str = run_helper
        .to_str()
        .ok_or_else(|| AppError::Other("helper 路径含非法字符".into()))?;
    let conf_str = match conf {
        Some(c) => Some(
            c.to_str()
                .ok_or_else(|| AppError::Other("配置路径含非法字符".into()))?
                .to_string(),
        ),
        None => None,
    };

    if use_sudo {
        let mut args: Vec<String> = vec!["-n".into(), helper_str.into(), action.into()];
        if let Some(c) = &conf_str {
            args.push(c.clone());
        }
        let cmd_log = log_dir.join("wg-quick.log");
        if let Some(parent) = cmd_log.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&cmd_log)?;
        let output = Command::new("/usr/bin/sudo").args(&args).output()?;
        use std::io::Write;
        let _ = writeln!(
            log_file,
            "[sudo -n {} {:?}]\nstdout:\n{}\nstderr:\n{}",
            action,
            conf,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(AppError::Command(format!(
                "helper {} 失败: {}",
                action,
                stderr.trim()
            )));
        }
        return Ok(());
    }
    // 普通模式：osascript
    let mut script = format!("'{}' {}", helper_str.replace('\'', "'\\''"), action);
    if let Some(c) = &conf_str {
        script.push_str(&format!(" '{}'", c.replace('\'', "'\\''")));
    }
    run_admin_osascript(&script, log_dir)?;
    Ok(())
}

/// 用 sudo -n 直接执行（免密模式）。命令的 stdout/stderr 同样会被记录到 log_dir/wg-quick.log
fn run_sudo_n(helper: &Path, action: &str, conf: &Path, log_dir: &Path) -> AppResult<()> {
    let cmd_log = log_dir.join("wg-quick.log");
    if let Some(parent) = cmd_log.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let cmd_log_str = cmd_log.to_string_lossy().to_string();
    let mut log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cmd_log)?;

    log::debug!(
        "sudo -n {} {} {}",
        helper.display(),
        action,
        conf.display()
    );

    let output = Command::new("/usr/bin/sudo")
        .args([
            "-n",
            helper.to_str().ok_or_else(|| {
                AppError::Other("wg-helper.sh 路径含非法字符".into())
            })?,
            action,
            conf.to_str().ok_or_else(|| {
                AppError::Other("配置路径含非法字符".into())
            })?,
        ])
        .output()?;

    use std::io::Write;
    let _ = writeln!(
        log_file,
        "[sudo -n {} {}]\nstdout:\n{}\nstderr:\n{}",
        action,
        conf.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        // sudo -n 在没有 NOPASSWD 时返回类似 "a password is required"
        if stderr.contains("a password is required") || stderr.contains("需要密码") {
            log::warn!("sudo -n 失败：免密授权已失效，需要重新启用免密");
            return Err(AppError::Command(
                "免密授权已失效，请在「免密模式」里重新启用".into(),
            ));
        }
        log::error!(
            "wg-helper {} 失败 status={} stderr={}",
            action,
            output.status,
            stderr.trim()
        );
        return Err(AppError::Command(format!(
            "wg-helper {} 失败: {}\n详见日志: {}",
            action,
            stderr.trim(),
            cmd_log_str
        )));
    }
    Ok(())
}

fn read_tail(path: &Path, max_bytes: usize) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let start = if len as usize > max_bytes {
        len - max_bytes as u64
    } else {
        0
    };
    f.seek(SeekFrom::Start(start))?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    Ok(buf)
}

/// 启动隧道：调用 wg-helper.sh up <conf>（含 wg-quick + 放开 socket 权限）
pub fn up(conf_path: &Path, paths: &WgPaths, log_dir: &Path) -> AppResult<()> {
    write_log_separator(log_dir, &format!("wg-quick up {}", conf_path.display()));
    run_helper(paths, "up", conf_path, log_dir)
}

pub fn down(conf_path: &Path, paths: &WgPaths, log_dir: &Path) -> AppResult<()> {
    write_log_separator(log_dir, &format!("wg-quick down {}", conf_path.display()));
    run_helper(paths, "down", conf_path, log_dir)
}

/// 调用 wg-helper.sh：免密模式走 sudo -n（用稳定路径）；否则走 osascript with administrator privileges。
/// 没有 helper 时回退到旧逻辑（直接 osascript 调 wg-quick）。
fn run_helper(paths: &WgPaths, action: &str, conf: &Path, log_dir: &Path) -> AppResult<()> {
    if let Some((helper, use_sudo)) = select_helper(paths) {
        if use_sudo {
            return run_sudo_n(&helper, action, conf, log_dir);
        }
        let helper_str = helper.to_string_lossy();
        let conf_str = conf
            .to_str()
            .ok_or_else(|| AppError::Other("配置路径含非法字符".into()))?;
        let script = format!(
            "'{}' {} '{}'",
            helper_str.replace('\'', "'\\''"),
            action,
            conf_str.replace('\'', "'\\''")
        );
        run_admin_osascript(&script, log_dir)?;
        return Ok(());
    }

    // 无 helper（开发模式，bundled 资源不全）：直接 osascript 调 wg-quick + chmod，
    // 保持和老版本一致的行为
    let conf_str = conf
        .to_str()
        .ok_or_else(|| AppError::Other("配置路径含非法字符".into()))?;
    let wg_quick = paths.wg_quick.to_string_lossy();
    let prelude = format!(
        "export PATH='{}'; export WG_QUICK_USERSPACE_IMPLEMENTATION='{}'; ",
        paths.shell_path().replace('\'', "'\\''"),
        paths.wireguard_go.to_string_lossy().replace('\'', "'\\''")
    );
    let script = match action {
        "up" => format!(
            "{}'{}' up '{}' && chmod -R 755 /var/run/wireguard 2>/dev/null; chmod 644 /var/run/wireguard/*.sock /var/run/wireguard/*.name 2>/dev/null; true",
            prelude,
            wg_quick.replace('\'', "'\\''"),
            conf_str.replace('\'', "'\\''")
        ),
        "down" => format!(
            "{}'{}' down '{}'",
            prelude,
            wg_quick.replace('\'', "'\\''"),
            conf_str.replace('\'', "'\\''")
        ),
        other => return Err(AppError::Other(format!("未知 action: {}", other))),
    };
    run_admin_osascript(&script, log_dir)?;
    Ok(())
}

fn write_log_separator(log_dir: &Path, title: &str) {
    use std::io::Write;
    let path: PathBuf = log_dir.join("wg-quick.log");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "\n========== [{}] {} ==========", now, title);
    }
}

/// 查询单个隧道状态。
/// wg-quick 把 `<name> -> utunN` 的映射写到 /var/run/wireguard/<name>.name，
/// 我们先读这个文件拿到真正的接口名，再 wg show <utunN> dump
pub fn status(name: &str, paths: &WgPaths) -> AppResult<TunnelStatus> {
    let mut st = TunnelStatus {
        name: name.to_string(),
        connected: false,
        interface: None,
        peer: None,
    };

    let iface = match run_wg_name(name) {
        Ok(s) => s.trim().to_string(),
        Err(e) => {
            log::debug!("没找到 {}.name 文件（可能未连接）: {}", name, e);
            return Ok(st);
        }
    };
    if iface.is_empty() {
        return Ok(st);
    }
    st.interface = Some(iface.clone());

    // 校验接口确实存在（防止 .name 文件残留）
    let interfaces_out = match run_wg(paths, &["show", "interfaces"]) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("wg show interfaces 失败: {}", e);
            return Ok(st);
        }
    };
    if !interfaces_out
        .split_ascii_whitespace()
        .any(|i| i == iface.as_str())
    {
        log::debug!(".name 文件存在但 {} 接口不在线，认为未连接", iface);
        st.interface = None;
        return Ok(st);
    }
    st.connected = true;

    match run_wg(paths, &["show", &iface, "dump"]) {
        Ok(dump) => parse_dump(&dump, &mut st),
        Err(e) => log::warn!("wg show {} dump 失败: {}", iface, e),
    }
    Ok(st)
}

fn run_wg(paths: &WgPaths, args: &[&str]) -> AppResult<String> {
    let output = Command::new(&paths.wg).args(args).output()?;
    if !output.status.success() {
        return Err(AppError::Command(format!(
            "wg {:?} 失败: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_wg_name(name: &str) -> AppResult<String> {
    let path = format!("/var/run/wireguard/{}.name", name);
    Ok(std::fs::read_to_string(path)?)
}

fn parse_dump(dump: &str, st: &mut TunnelStatus) {
    let mut peer = PeerStats::default();
    for (i, line) in dump.lines().enumerate() {
        let cols: Vec<&str> = line.split('\t').collect();
        if i == 0 {
            continue;
        }
        if cols.len() < 8 {
            continue;
        }
        peer.endpoint = non_dash(cols[2]);
        peer.allowed_ips = non_dash(cols[3]);
        peer.latest_handshake_secs = cols[4].parse::<u64>().ok().map(|ts| {
            if ts == 0 {
                u64::MAX
            } else {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                now.saturating_sub(ts)
            }
        });
        peer.transfer_rx = cols[5].parse::<u64>().ok();
        peer.transfer_tx = cols[6].parse::<u64>().ok();
        break;
    }
    st.peer = Some(peer);
}

fn non_dash(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t == "(none)" || t == "-" {
        None
    } else {
        Some(t.to_string())
    }
}

pub fn external_ip() -> AppResult<String> {
    let output = Command::new("curl")
        .args(["-s", "--max-time", "5", "https://api.ipify.org"])
        .output()?;
    if !output.status.success() {
        return Err(AppError::Command("查询出口 IP 失败".into()));
    }
    let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if ip.is_empty() {
        return Err(AppError::Command("出口 IP 返回为空".into()));
    }
    Ok(ip)
}
