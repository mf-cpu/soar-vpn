//! 应用内升级
//!
//! 流程：
//! 1. App 启动 / 用户点检查更新 → GET <manifest_url> 拿一个 JSON：
//!    { "version": "0.3.1", "url": "https://.../Soar_0.3.1.dmg",
//!      "sha256": "abc...", "notes": "本次更新内容" }
//! 2. 比版本号；若 latest > current → 把元信息推给前端
//! 3. 用户确认 → 后端用 curl 下载 DMG 到 ~/Library/Caches/com.mengfan.wgvpn/updates
//!    （边下载边发进度事件）
//! 4. 校验 sha256
//! 5. 调 wg-helper.sh install-app <dmg>（root，复用现有提权链路）
//!    helper 内部完成：mount → cp → detach → xattr → 退出旧 App → 启动新 App
//!
//! 不引入 reqwest 等依赖，全部走 curl + helper.sh。

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::wg::WgPaths;

/// 默认 manifest URL（用户可在 settings → 关于 中覆盖）。
/// 这个地址挂在公司的百度云 BCC 上，只 serve 一个几百字节的 latest.json。
/// DMG 本体由 manifest 中的 url 字段决定（推荐放 GitHub Release 走 CDN）。
///
/// 多端规划：服务器目录布局是 /wg-vpn/{mac,win,ios,android}/latest.json，
/// 各平台 App 写死自己的子路径，互不影响、互不需要兼容。
pub const DEFAULT_MANIFEST_URL: &str = "http://180.76.134.45:8088/wg-vpn/mac/latest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheck {
    pub current: String,
    pub latest: Option<Manifest>,
    pub has_update: bool,
}

/// 拉 manifest，比版本
pub fn check(manifest_url: &str, current: &str) -> AppResult<UpdateCheck> {
    if manifest_url.trim().is_empty() {
        return Err(AppError::Other(
            "尚未配置升级地址，请在 设置 → 关于 中填入 manifest URL".into(),
        ));
    }
    let body = http_get_text(manifest_url, 8)?;
    let m: Manifest = serde_json::from_str(&body)
        .map_err(|e| AppError::Other(format!("manifest 不是合法 JSON: {}", e)))?;

    let has_update = is_newer(&m.version, current);
    Ok(UpdateCheck {
        current: current.to_string(),
        latest: Some(m),
        has_update,
    })
}

/// 比较语义化版本（仅看 major.minor.patch，多余字段忽略）
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut it = s.trim().trim_start_matches('v').split('.');
        let a = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let b = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let c = it
            .next()
            .map(|x| x.split(|c: char| !c.is_ascii_digit()).next().unwrap_or("0"))
            .and_then(|x| x.parse().ok())
            .unwrap_or(0);
        (a, b, c)
    };
    parse(latest) > parse(current)
}

/// 下载 DMG 到缓存目录，返回本地路径。会按字节数 emit "update-progress" 事件。
pub fn download(app: &AppHandle, manifest: &Manifest) -> AppResult<PathBuf> {
    let cache_dir = updates_dir(app)?;
    std::fs::create_dir_all(&cache_dir).ok();
    let filename = manifest
        .url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("update.dmg");
    let dest = cache_dir.join(filename);
    let _ = std::fs::remove_file(&dest);

    log::info!("开始下载更新 {} → {}", manifest.url, dest.display());
    let _ = app.emit(
        "update-progress",
        serde_json::json!({ "phase": "downloading", "percent": 0 }),
    );

    // curl --progress-bar 不好解析；这里直接用 -sS + 周期性查文件大小
    // 简单稳定起见：启动 curl，主线程轮询输出文件大小
    let dest_clone = dest.clone();
    let url = manifest.url.clone();
    let total = manifest.size.unwrap_or(0);

    // 后台线程跑 curl
    let handle = std::thread::spawn(move || {
        Command::new("/usr/bin/curl")
            .args([
                "-sSL",
                "--fail",
                "--max-time",
                "1800",
                "-o",
                dest_clone.to_string_lossy().as_ref(),
                &url,
            ])
            .output()
    });

    // 进度轮询
    while !handle.is_finished() {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Ok(meta) = std::fs::metadata(&dest) {
            let bytes = meta.len();
            let pct = if total > 0 {
                ((bytes as f64 / total as f64) * 100.0).clamp(0.0, 99.0) as u32
            } else {
                0
            };
            let _ = app.emit(
                "update-progress",
                serde_json::json!({
                    "phase": "downloading",
                    "bytes": bytes,
                    "total": total,
                    "percent": pct,
                }),
            );
        }
    }

    let output = handle
        .join()
        .map_err(|_| AppError::Other("下载线程 panic".into()))?
        .map_err(|e| AppError::Other(format!("启动 curl 失败: {}", e)))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&dest);
        return Err(AppError::Command(format!(
            "下载失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    // 校验 sha256（如果 manifest 提供了）
    if !manifest.sha256.trim().is_empty() {
        let _ = app.emit(
            "update-progress",
            serde_json::json!({ "phase": "verifying", "percent": 99 }),
        );
        let actual = sha256_file(&dest)?;
        if !actual.eq_ignore_ascii_case(manifest.sha256.trim()) {
            let _ = std::fs::remove_file(&dest);
            return Err(AppError::Other(format!(
                "sha256 校验失败：expected={}, actual={}",
                manifest.sha256, actual
            )));
        }
    }

    let _ = app.emit(
        "update-progress",
        serde_json::json!({ "phase": "downloaded", "percent": 100 }),
    );
    Ok(dest)
}

/// 调 helper install-app，由 root 完成 mount/cp/restart。Tauri 进程会被 helper kill 后重启。
pub fn install(dmg: &Path, paths: &WgPaths, log_dir: &Path) -> AppResult<()> {
    let dmg_str = dmg
        .to_str()
        .ok_or_else(|| crate::error::AppError::Other("DMG 路径含非法字符".into()))?;
    crate::wg::run_helper_oneshot(paths, log_dir, "install-app", &[dmg_str])
}

fn updates_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::Other(format!("拿不到缓存目录: {}", e)))?;
    Ok(cache.join("updates"))
}

fn http_get_text(url: &str, timeout_s: u32) -> AppResult<String> {
    let out = Command::new("/usr/bin/curl")
        .args([
            "-sSL",
            "--fail",
            "--max-time",
            &timeout_s.to_string(),
            "-A",
            "wg-vpn-updater",
            url,
        ])
        .output()
        .map_err(|e| AppError::Other(format!("启动 curl 失败: {}", e)))?;
    if !out.status.success() {
        return Err(AppError::Command(format!(
            "GET {} 失败: {}",
            url,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn sha256_file(path: &Path) -> AppResult<String> {
    let out = Command::new("/usr/bin/shasum")
        .args(["-a", "256", path.to_string_lossy().as_ref()])
        .output()
        .map_err(|e| AppError::Other(format!("shasum 启动失败: {}", e)))?;
    if !out.status.success() {
        return Err(AppError::Command(format!(
            "shasum 失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let s = String::from_utf8_lossy(&out.stdout);
    Ok(s.split_whitespace().next().unwrap_or("").to_string())
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.2.2", "0.2.1"));
        assert!(is_newer("0.3.0", "0.2.99"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        assert!(is_newer("v0.2.2", "0.2.1"));
        assert!(is_newer("0.2.2-beta", "0.2.1"));
    }
}
