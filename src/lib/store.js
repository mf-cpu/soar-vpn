import { writable, derived } from "svelte/store";
import {
  api,
  onTraffic,
  onTunnelChanged,
  onUpdateAvailable,
  onUpdateProgress,
} from "./api.js";

export const configs = writable([]);
export const activeName = writable(null);
export const tunnel = writable(null); // 完整 status
export const traffic = writable({
  connected: false,
  rx_bps: 0,
  tx_bps: 0,
  total_rx: 0,
  total_tx: 0,
  handshake_age: null,
});

// 滚动窗口流量（最多 60 个点，用于图表）
export const trafficSeries = writable({
  t: [],
  rx: [],
  tx: [],
});

const MAX_POINTS = 60;

export const settings = writable(null);
export const passwordless = writable(null);

// 升级相关
// updateInfo: { current, latest:{version,url,sha256,notes,size}, has_update } | null
export const updateInfo = writable(null);
// 进度: { phase: 'downloading'|'verifying'|'downloaded', percent, bytes, total } | null
export const updateProgress = writable(null);

export async function refreshConfigs() {
  configs.set(await api.listConfigs());
}

export async function refreshActive() {
  activeName.set(await api.activeTunnel());
}

export async function refreshSettings() {
  settings.set(await api.getSettings());
}

export async function refreshPasswordless() {
  passwordless.set(await api.passwordlessStatus());
}

export async function refreshAll() {
  await Promise.all([
    refreshConfigs(),
    refreshActive(),
    refreshSettings(),
    refreshPasswordless(),
  ]);
}

let started = false;
export async function startListeners() {
  if (started) return;
  started = true;
  await onTraffic((p) => {
    traffic.set({
      connected: !!p.connected,
      rx_bps: p.rx_bps || 0,
      tx_bps: p.tx_bps || 0,
      total_rx: p.total_rx || 0,
      total_tx: p.total_tx || 0,
      handshake_age: p.handshake_age ?? null,
    });
    trafficSeries.update((s) => {
      const t = Date.now() / 1000;
      s.t.push(t);
      s.rx.push(p.connected ? p.rx_bps || 0 : 0);
      s.tx.push(p.connected ? p.tx_bps || 0 : 0);
      while (s.t.length > MAX_POINTS) {
        s.t.shift();
        s.rx.shift();
        s.tx.shift();
      }
      return { ...s };
    });
  });
  await onTunnelChanged(async () => {
    await refreshActive();
    trafficSeries.set({ t: [], rx: [], tx: [] });
  });
  await onUpdateAvailable((p) => {
    updateInfo.set(p);
  });
  await onUpdateProgress((p) => {
    updateProgress.set(p);
  });
}

export const isConnected = derived(traffic, ($t) => !!$t.connected);
