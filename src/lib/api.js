import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const api = {
  listConfigs: () => invoke("list_configs"),
  saveConfig: (name, content) => invoke("save_config", { name, content }),
  readConfig: (name) => invoke("read_config", { name }),
  deleteConfig: (name) => invoke("delete_config", { name }),
  connect: (name) => invoke("connect", { name }),
  disconnect: (name) => invoke("disconnect", { name }),
  status: (name) => invoke("status", { name }),
  externalIp: () => invoke("external_ip"),
  activeTunnel: () => invoke("active_tunnel"),
  openLogDir: () => invoke("open_log_dir"),

  passwordlessStatus: () => invoke("passwordless_status"),
  enablePasswordless: () => invoke("enable_passwordless"),
  disablePasswordless: () => invoke("disable_passwordless"),

  getSettings: () => invoke("get_settings"),
  setSettings: (s) => invoke("set_settings", { new: s }),

  listRuleTemplates: () => invoke("list_rule_templates"),
  getRuleState: (name) => invoke("get_rule_state", { name }),
  applyRuleMode: (name, mode, custom) => invoke("apply_rule_mode", { name, mode, custom }),

  runDiagnostics: (name) => invoke("run_diagnostics", { name }),

  readLogTail: (file, maxBytes = 64 * 1024) =>
    invoke("read_log_tail", { file, maxBytes }),
  listLogFiles: () => invoke("list_log_files"),

  showMainWindow: () => invoke("show_main_window"),
  log: (level, message) => invoke("frontend_log", { level, message }).catch(() => {}),

  checkUpdate: () => invoke("check_update"),
  installUpdate: (manifest) =>
    invoke("download_and_install_update", { manifest }),
};

export const onTraffic = (cb) => listen("traffic", (e) => cb(e.payload));
export const onTunnelChanged = (cb) => listen("tunnel-changed", (e) => cb(e.payload));
export const onUpdateAvailable = (cb) =>
  listen("update-available", (e) => cb(e.payload));
export const onUpdateProgress = (cb) =>
  listen("update-progress", (e) => cb(e.payload));

export function fmtBytes(n) {
  if (!n && n !== 0) return "—";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = Number(n);
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 ? 0 : v >= 10 ? 1 : 2)} ${u[i]}`;
}

export function fmtBps(n) {
  if (!n && n !== 0) return "—";
  return fmtBytes(n) + "/s";
}

export function fmtAge(secs) {
  if (secs === null || secs === undefined) return "—";
  if (secs >= 1e9) return "从未";
  if (secs < 60) return `${secs}s 前`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m 前`;
  return `${Math.floor(secs / 3600)}h 前`;
}
