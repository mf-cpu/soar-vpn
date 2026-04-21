use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigInfo {
    pub name: String,
    pub path: String,
    pub endpoint: Option<String>,
    pub address: Option<String>,
}

/// 配置文件存放目录: <app_data>/configs/
pub fn configs_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("configs")
}

pub fn ensure_configs_dir(app_data_dir: &Path) -> AppResult<PathBuf> {
    let dir = configs_dir(app_data_dir);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 仅允许字母数字、下划线和短横线，避免 wg-quick 把奇怪字符当参数
pub fn validate_name(name: &str) -> AppResult<()> {
    if name.is_empty() || name.len() > 15 {
        return Err(AppError::InvalidConfig(
            "名称长度必须在 1-15 个字符之间".into(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::InvalidConfig(
            "名称只能包含字母、数字、下划线和短横线".into(),
        ));
    }
    Ok(())
}

pub fn config_path(app_data_dir: &Path, name: &str) -> PathBuf {
    configs_dir(app_data_dir).join(format!("{}.conf", name))
}

pub fn list_configs(app_data_dir: &Path) -> AppResult<Vec<ConfigInfo>> {
    let dir = ensure_configs_dir(app_data_dir)?;
    let mut out = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("conf") {
            continue;
        }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let content = fs::read_to_string(&path).unwrap_or_default();
        let (endpoint, address) = parse_meta(&content);
        out.push(ConfigInfo {
            name,
            path: path.to_string_lossy().into_owned(),
            endpoint,
            address,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn save_config(app_data_dir: &Path, name: &str, content: &str) -> AppResult<ConfigInfo> {
    validate_name(name)?;
    let cleaned = ensure_keepalive(&sanitize_conf(content));
    if !cleaned.contains("[Interface]") || !cleaned.contains("[Peer]") {
        return Err(AppError::InvalidConfig(
            "配置必须包含 [Interface] 和 [Peer] 段".into(),
        ));
    }
    let dir = ensure_configs_dir(app_data_dir)?;
    let path = dir.join(format!("{}.conf", name));
    fs::write(&path, &cleaned)?;
    // 设置为 600，避免私钥泄漏
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }
    let (endpoint, address) = parse_meta(&cleaned);
    Ok(ConfigInfo {
        name: name.to_string(),
        path: path.to_string_lossy().into_owned(),
        endpoint,
        address,
    })
}

/// 容错处理用户粘贴：
/// 1. 统一换行 (\r\n / \r -> \n)
/// 2. 去掉 BOM
/// 3. 如果整段内容首尾都被 " 或 ' 包住，把首尾这对引号脱掉
/// 4. 去掉首尾空白
fn sanitize_conf(input: &str) -> String {
    let mut s = input.replace("\r\n", "\n").replace('\r', "\n");
    if s.starts_with('\u{feff}') {
        s = s.trim_start_matches('\u{feff}').to_string();
    }
    let trimmed = s.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return trimmed[1..trimmed.len() - 1].trim().to_string() + "\n";
        }
    }
    trimmed.to_string() + "\n"
}

/// 兜底：[Peer] 段如果没有 PersistentKeepalive，自动加 25。
/// 客户端在 NAT 后面（家里路由器/运营商）时，没有 keepalive 会导致 NAT 表项
/// 30~120s 后过期，握手频繁失效。25s 是 wireguard 官方推荐值。
fn ensure_keepalive(input: &str) -> String {
    if input.lines().any(|l| {
        let lower = l.trim().to_ascii_lowercase();
        lower.starts_with("persistentkeepalive")
    }) {
        return input.to_string();
    }
    if !input.contains("[Peer]") {
        return input.to_string();
    }
    let lines: Vec<&str> = input.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 1);
    let mut in_peer = false;
    let mut inserted = false;
    for line in &lines {
        let trimmed = line.trim();
        let is_section = trimmed.starts_with('[') && trimmed.ends_with(']');
        if in_peer && !inserted && is_section {
            out.push("PersistentKeepalive = 25".to_string());
            inserted = true;
            in_peer = false;
        }
        out.push((*line).to_string());
        if is_section {
            in_peer = trimmed.eq_ignore_ascii_case("[Peer]");
        }
    }
    if in_peer && !inserted {
        out.push("PersistentKeepalive = 25".to_string());
    }
    let mut joined = out.join("\n");
    if !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

pub fn delete_config(app_data_dir: &Path, name: &str) -> AppResult<()> {
    validate_name(name)?;
    let path = config_path(app_data_dir, name);
    if !path.exists() {
        return Err(AppError::ConfigNotFound(name.to_string()));
    }
    fs::remove_file(&path)?;
    Ok(())
}

pub fn read_config(app_data_dir: &Path, name: &str) -> AppResult<String> {
    let path = config_path(app_data_dir, name);
    if !path.exists() {
        return Err(AppError::ConfigNotFound(name.to_string()));
    }
    Ok(fs::read_to_string(&path)?)
}

fn parse_meta(content: &str) -> (Option<String>, Option<String>) {
    let mut endpoint = None;
    let mut address = None;
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(v) = strip_kv(line, "Endpoint") {
            endpoint = Some(v.to_string());
        } else if let Some(v) = strip_kv(line, "Address") {
            address = Some(v.to_string());
        }
    }
    (endpoint, address)
}

fn strip_kv<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let lower = line.to_ascii_lowercase();
    let key_lower = key.to_ascii_lowercase();
    if lower.starts_with(&key_lower) {
        let rest = line[key.len()..].trim_start();
        if let Some(rest) = rest.strip_prefix('=') {
            return Some(rest.trim());
        }
    }
    None
}
