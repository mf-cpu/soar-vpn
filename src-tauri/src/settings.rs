use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppResult;

const FILE: &str = "settings.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// VPN 异常断开时自动重连（默认开）
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    /// Kill Switch：连接时启用 pf 防火墙，VPN 不在线时阻断所有出站
    #[serde(default)]
    pub kill_switch: bool,
    /// 启动 App 时自动连接的 conf 名（None 表示不自动连）
    #[serde(default)]
    pub auto_connect_on_start: Option<String>,
    /// 注册 LaunchAgent 实现开机自启 App
    #[serde(default)]
    pub launch_at_login: bool,
    /// 升级 manifest URL（GET 后返回 {version,url,sha256,notes}）
    /// 空字符串 = 用编译时默认值
    #[serde(default)]
    pub update_manifest_url: String,
    /// 启动时自动检查更新
    #[serde(default = "default_true")]
    pub auto_check_update: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            kill_switch: false,
            auto_connect_on_start: None,
            launch_at_login: false,
            update_manifest_url: String::new(),
            auto_check_update: true,
        }
    }
}

fn default_true() -> bool {
    true
}

fn path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(FILE)
}

pub fn load(app_data_dir: &Path) -> Settings {
    let p = path(app_data_dir);
    match std::fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(app_data_dir: &Path, s: &Settings) -> AppResult<()> {
    let p = path(app_data_dir);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let body = serde_json::to_string_pretty(s)
        .map_err(|e| crate::error::AppError::Other(format!("序列化 settings 失败: {e}")))?;
    std::fs::write(&p, body)?;
    Ok(())
}
