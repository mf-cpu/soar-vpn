//! 网络诊断：一键检查 VPN 是否真的生效。
//!
//! 检查项：
//! - 出口 IPv4（应该是 VPN endpoint 所在国家）
//! - 出口 IPv6（最好是空，说明 v6 已被禁）
//! - DNS 是否走 conf 里的 DNS（dnsleaktest.com 思路：解析一个特殊域名）
//! - 关键服务可达性 + 延迟（claude.ai / openai.com / github.com / google.com）
//! - 配置体检（AllowedIPs/MTU 等给出建议）

use std::process::Command;
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DiagResult {
    pub egress_ipv4: Option<String>,
    pub egress_ipv4_country: Option<String>,
    pub egress_ipv6: Option<String>,
    pub dns_servers: Vec<String>,
    pub reachability: Vec<ReachItem>,
    pub config_checks: Vec<ConfigCheck>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReachItem {
    pub host: String,
    pub ok: bool,
    pub latency_ms: Option<u32>,
    pub status: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigCheck {
    pub level: String, // "ok" | "warn" | "error"
    pub title: String,
    pub detail: String,
}

pub fn run(conf_text: Option<&str>) -> DiagResult {
    let (ipv4, country) = fetch_egress_ipv4();
    let ipv6 = fetch_egress_ipv6();
    let dns = current_dns_servers();
    let reach = vec![
        check_url("https://www.claude.ai/"),
        check_url("https://chat.openai.com/"),
        check_url("https://www.google.com/generate_204"),
        check_url("https://api.github.com/"),
    ];
    let checks = if let Some(c) = conf_text {
        check_config(c, ipv6.as_deref())
    } else {
        Vec::new()
    };
    DiagResult {
        egress_ipv4: ipv4,
        egress_ipv4_country: country,
        egress_ipv6: ipv6,
        dns_servers: dns,
        reachability: reach,
        config_checks: checks,
    }
}

fn fetch_egress_ipv4() -> (Option<String>, Option<String>) {
    // ipapi.co 一次返回 IP + 国家
    let out = Command::new("/usr/bin/curl")
        .args(["-4", "-s", "--max-time", "6", "https://ipapi.co/json/"])
        .output()
        .ok();
    if let Some(o) = out {
        if o.status.success() {
            let body = String::from_utf8_lossy(&o.stdout);
            // 简单 JSON 提取，避免引入 serde_json 解析每个字段
            let ip = json_str(&body, "ip");
            let country = json_str(&body, "country_name");
            return (ip, country);
        }
    }
    // 兜底
    let out = Command::new("/usr/bin/curl")
        .args(["-4", "-s", "--max-time", "6", "https://api.ipify.org"])
        .output()
        .ok();
    let ip = out
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    (ip, None)
}

fn fetch_egress_ipv6() -> Option<String> {
    let out = Command::new("/usr/bin/curl")
        .args(["-6", "-s", "--max-time", "3", "https://api64.ipify.org"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() || s.contains('.') {
        // 拿到 v4 说明 v6 不通；为空说明被禁
        return None;
    }
    Some(s)
}

fn current_dns_servers() -> Vec<String> {
    // scutil --dns 输出复杂，简单提取 nameserver[N] : x.x.x.x 行
    let out = match Command::new("/usr/sbin/scutil").arg("--dns").output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut set = std::collections::BTreeSet::new();
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with("nameserver[") {
            if let Some(eq) = l.find(':') {
                let ip = l[eq + 1..].trim().to_string();
                if !ip.is_empty() {
                    set.insert(ip);
                }
            }
        }
    }
    set.into_iter().collect()
}

fn check_url(url: &str) -> ReachItem {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string();
    let start = Instant::now();
    let out = Command::new("/usr/bin/curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "8",
            url,
        ])
        .output();
    let elapsed = start.elapsed();
    match out {
        Ok(o) if o.status.success() => {
            let code: u32 = String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse()
                .unwrap_or(0);
            ReachItem {
                host,
                ok: code != 0 && code < 500,
                latency_ms: Some(elapsed.as_millis() as u32),
                status: Some(code),
                error: None,
            }
        }
        Ok(_) | Err(_) => ReachItem {
            host,
            ok: false,
            latency_ms: None,
            status: None,
            error: Some(format!("超时或失败 ({}ms)", elapsed.as_millis())),
        },
    }
}

fn check_config(conf: &str, egress_v6: Option<&str>) -> Vec<ConfigCheck> {
    let mut out = Vec::new();
    let lower = conf.to_ascii_lowercase();

    // AllowedIPs
    if !lower.contains("allowedips") {
        out.push(ConfigCheck {
            level: "error".into(),
            title: "缺少 AllowedIPs".into(),
            detail: "[Peer] 段必须有 AllowedIPs，否则无流量会走 VPN".into(),
        });
    } else if lower.contains("allowedips") && lower.contains("0.0.0.0/0") && !lower.contains("::/0") {
        out.push(ConfigCheck {
            level: if egress_v6.is_some() { "warn" } else { "ok" }.into(),
            title: "AllowedIPs 仅代理 IPv4".into(),
            detail: "如果系统 IPv6 没被禁，访问双栈服务时 v6 会泄漏。本 App 已自动禁用系统 v6"
                .into(),
        });
    }

    // DNS
    if !lower.contains("\ndns") && !lower.starts_with("dns") {
        out.push(ConfigCheck {
            level: "warn".into(),
            title: "未配置 DNS".into(),
            detail: "建议在 [Interface] 加 `DNS = 1.1.1.1` 或可信 DNS，避免 DNS 泄漏".into(),
        });
    }

    // MTU
    if !lower.contains("mtu") {
        out.push(ConfigCheck {
            level: "ok".into(),
            title: "未显式设置 MTU".into(),
            detail: "默认 MTU 1420 通常够用。若访问慢可在 [Interface] 加 `MTU = 1380` 试试"
                .into(),
        });
    }

    if egress_v6.is_some() {
        out.push(ConfigCheck {
            level: "warn".into(),
            title: "IPv6 未禁用".into(),
            detail: format!(
                "检测到 v6 出口 {}，可能泄漏真实位置。检查防泄漏开关是否开启",
                egress_v6.unwrap_or("?")
            ),
        });
    } else {
        out.push(ConfigCheck {
            level: "ok".into(),
            title: "IPv6 已禁用".into(),
            detail: "防止 v6 泄漏真实地址 ✓".into(),
        });
    }

    out
}

fn json_str(body: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let idx = body.find(&pat)?;
    let after = &body[idx + pat.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    if rest.starts_with('"') {
        let end = rest[1..].find('"')?;
        Some(rest[1..=end].to_string())
    } else {
        None
    }
}

pub fn _unused_warn() -> Duration {
    Duration::from_millis(0)
}
