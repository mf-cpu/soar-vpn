//! 规则模式：在 conf 的 [Peer] 段里替换 AllowedIPs，并通过 `wg set` 热应用，不断连。
//!
//! 4 个预设：
//! - global  : 0.0.0.0/0
//! - rules   : 全球 - 中国大陆 IP（需要外部数据源，第二期实现自动更新；目前使用内置精简版）
//! - ai_only : AI 服务白名单（Cloudflare/Anthropic/OpenAI 等）
//! - lan_only: 公司内网（10/8 + 172.16/12 + 192.168/16）

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::AppResult;
use crate::wg::{self, WgPaths};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuleMode {
    Global,
    Rules,
    AiOnly,
    LanOnly,
    Custom,
}


#[derive(Debug, Clone, Serialize)]
pub struct RuleTemplate {
    pub mode: RuleMode,
    pub title: String,
    pub desc: String,
    pub allowed_ips: String,
    pub recommended: bool,
}

pub fn list_templates() -> Vec<RuleTemplate> {
    vec![
        RuleTemplate {
            mode: RuleMode::Global,
            title: "全局模式".into(),
            desc: "所有流量都走 VPN（默认）。最简单稳定，国内访问会变慢".into(),
            allowed_ips: "0.0.0.0/0".into(),
            recommended: false,
        },
        RuleTemplate {
            mode: RuleMode::Rules,
            title: "规则模式（推荐）".into(),
            desc: "海外流量走 VPN，中国大陆 IP 直连。访问国外网站不影响国内速度".into(),
            allowed_ips: rules_allowed_ips(),
            recommended: true,
        },
        RuleTemplate {
            mode: RuleMode::AiOnly,
            title: "仅 AI 服务".into(),
            desc: "只让 OpenAI / Claude / Gemini / Cloudflare 走 VPN，其它直连".into(),
            allowed_ips: ai_only_allowed_ips(),
            recommended: false,
        },
        RuleTemplate {
            mode: RuleMode::LanOnly,
            title: "仅公司内网".into(),
            desc: "只让 10/8、172.16/12、192.168/16 走 VPN，公网流量直连".into(),
            allowed_ips: "10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16".into(),
            recommended: false,
        },
    ]
}

/// 读取 conf，识别当前 AllowedIPs 命中哪个模板
pub fn detect_mode(conf_text: &str) -> RuleMode {
    let current = extract_allowed_ips(conf_text)
        .unwrap_or_default()
        .replace(' ', "");
    for t in list_templates() {
        if normalize(&t.allowed_ips) == current {
            return t.mode;
        }
    }
    RuleMode::Custom
}

fn normalize(s: &str) -> String {
    s.replace(' ', "")
}

/// 从 conf 中提取第一个 AllowedIPs 行的值（去掉空格）
pub fn extract_allowed_ips(conf_text: &str) -> Option<String> {
    for line in conf_text.lines() {
        let l = line.trim();
        if l.starts_with('#') {
            continue;
        }
        let lower = l.to_ascii_lowercase();
        if lower.starts_with("allowedips") {
            if let Some(eq) = l.find('=') {
                return Some(l[eq + 1..].trim().to_string());
            }
        }
    }
    None
}

/// 把 conf 文本里的 AllowedIPs 替换成新值（保留其它行）。目前只在测试中使用，
/// 生产路径走 wg-helper.sh 的 awk 实现以便整步原子化执行（需要 root）。
#[allow(dead_code)]
pub fn replace_allowed_ips(conf_text: &str, new_value: &str) -> String {
    let mut out = Vec::new();
    let mut replaced = false;
    for line in conf_text.lines() {
        let l = line.trim_start();
        let lower = l.to_ascii_lowercase();
        if !replaced && lower.starts_with("allowedips") {
            // 保留缩进
            let indent = &line[..line.len() - l.len()];
            out.push(format!("{}AllowedIPs = {}", indent, new_value));
            replaced = true;
        } else {
            out.push(line.to_string());
        }
    }
    if !replaced {
        // 没找到 AllowedIPs 行，加一行到末尾
        out.push(format!("AllowedIPs = {}", new_value));
    }
    out.join("\n") + "\n"
}

/// 热切换：把所有"需要 root"的步骤（写 conf、wg set、route 重建）一次性交给
/// wg-helper.sh 的 switch-rules 子命令完成，避免普通进程改路由表失败。
/// 如果隧道不在线，helper 也会更新 conf 文件，下次 connect 自动生效。
pub fn apply_mode(
    paths: &WgPaths,
    log_dir: &Path,
    conf_path: &Path,
    new_allowed_ips: &str,
) -> AppResult<()> {
    let normalized = new_allowed_ips
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    log::info!(
        "apply_rule_mode conf={} count={}",
        conf_path.display(),
        normalized.matches(',').count() + 1,
    );
    wg::switch_rules(conf_path, &normalized, paths, log_dir)
}

// ===================== 内置规则模板内容 =====================

/// "全球减大陆" 精简版（手动维护的常见海外 IP 段，覆盖大部分场景）。
/// TODO Phase 2: 改为后台从 https://raw.githubusercontent.com/17mon/china_ip_list/master/china_ip_list.txt
/// 拉取最新版，本地计算 全球 - 大陆 的差集。这里用一个静态精简版兜底。
fn rules_allowed_ips() -> String {
    // 这是一个简化的"非中国大陆主要 IP 段"集合。涵盖：
    // - Google / Cloudflare / Akamai / Fastly / 主要海外云
    // - 大部分美西/美东/欧/日/新加坡常用网段
    // 不是精确"全球减大陆"——精确版本需要 ~600 行，第二期自动生成。
    [
        // Cloudflare
        "104.16.0.0/12", "104.18.0.0/16", "162.158.0.0/15", "162.159.0.0/16",
        "172.64.0.0/13", "173.245.48.0/20", "188.114.96.0/20", "190.93.240.0/20",
        "197.234.240.0/22", "198.41.128.0/17", "131.0.72.0/22",
        // Google / Anthropic / OpenAI / 主要 AI 云
        "8.8.4.0/24", "8.8.8.0/24", "34.0.0.0/8", "35.0.0.0/8", "64.233.160.0/19",
        "66.102.0.0/20", "66.249.64.0/19", "72.14.192.0/18", "74.125.0.0/16",
        "108.177.8.0/21", "108.177.96.0/19", "142.250.0.0/15", "172.217.0.0/16",
        "172.253.0.0/16", "173.194.0.0/16", "209.85.128.0/17", "216.58.192.0/19",
        "216.239.32.0/19",
        // GitHub / AWS（部分）
        "140.82.112.0/20", "143.55.64.0/20", "192.30.252.0/22", "185.199.108.0/22",
        "13.32.0.0/15", "13.224.0.0/14", "18.160.0.0/12", "52.84.0.0/15", "54.230.0.0/16",
        "99.86.0.0/16", "204.246.164.0/22",
        // Meta / Microsoft / Apple
        "31.13.24.0/21", "31.13.64.0/18", "157.240.0.0/16", "179.60.192.0/22",
        "20.0.0.0/8", "40.64.0.0/10", "52.96.0.0/12", "168.61.0.0/16",
        "17.0.0.0/8",
        // 美国 ARIN 大段（兜底）
        "23.0.0.0/8", "65.0.0.0/8", "67.0.0.0/8", "68.0.0.0/8", "69.0.0.0/8",
        "97.0.0.0/8", "98.0.0.0/8", "100.0.0.0/8", "107.0.0.0/8", "162.0.0.0/8",
        "184.0.0.0/8", "207.0.0.0/8", "208.0.0.0/8", "209.0.0.0/8", "216.0.0.0/8",
    ]
    .join(", ")
}

/// AI 白名单（Cloudflare 占大头，因为 OpenAI/Claude/Anthropic 都在 Cloudflare 后面）
fn ai_only_allowed_ips() -> String {
    [
        // Cloudflare
        "104.16.0.0/12", "104.18.0.0/16", "162.158.0.0/15", "162.159.0.0/16",
        "172.64.0.0/13", "173.245.48.0/20", "188.114.96.0/20", "190.93.240.0/20",
        "197.234.240.0/22", "198.41.128.0/17", "131.0.72.0/22",
        // OpenAI 直接段
        "20.0.0.0/8", "40.64.0.0/10",
        // Google AI
        "142.250.0.0/15", "172.217.0.0/16", "172.253.0.0/16",
        "216.58.192.0/19", "216.239.32.0/19",
    ]
    .join(", ")
}
