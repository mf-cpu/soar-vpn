<script>
  import { onMount } from "svelte";
  import { push } from "svelte-spa-router";
  import {
    configs,
    activeName,
    traffic,
    isConnected,
    settings,
    refreshConfigs,
    refreshActive,
    refreshSettings,
  } from "../lib/store.js";
  import { api, fmtBps, fmtBytes, fmtAge } from "../lib/api.js";
  import TrafficChart from "../components/TrafficChart.svelte";

  let busy = false;
  let busyMsg = "";
  let pickName = "";
  let lastError = "";

  let diag = null;
  let diagBusy = false;
  let diagError = "";

  $: pickName = $activeName || $configs[0]?.name || "";

  async function toggleConnect() {
    if (!pickName) {
      push("/profiles");
      return;
    }
    busy = true;
    lastError = "";
    try {
      if ($isConnected && $activeName === pickName) {
        busyMsg = "断开中…";
        await api.disconnect(pickName);
      } else {
        busyMsg = "连接中…";
        await api.connect(pickName);
      }
      await refreshActive();
    } catch (e) {
      lastError = String(e);
    } finally {
      busy = false;
      busyMsg = "";
    }
  }

  async function runDiag() {
    diagBusy = true;
    diagError = "";
    try {
      diag = await api.runDiagnostics($activeName ?? null);
    } catch (e) {
      diagError = String(e);
    } finally {
      diagBusy = false;
    }
  }

  onMount(async () => {
    await Promise.all([refreshConfigs(), refreshActive(), refreshSettings()]);
  });
</script>

<div class="page">
  <h1>概览</h1>

  <section class="hero">
    <div class="left">
      <div class="state-line">
        <span class="dot" class:on={$isConnected}></span>
        <span class="state-text">
          {$isConnected ? "已连接" : "未连接"}
        </span>
        {#if $isConnected}
          <span class="active-name">· {$activeName}</span>
        {/if}
      </div>

      <div class="hero-stats">
        <div>
          <div class="muted small">下载速度</div>
          <div class="big">{fmtBps($traffic.rx_bps)}</div>
        </div>
        <div>
          <div class="muted small">上传速度</div>
          <div class="big">{fmtBps($traffic.tx_bps)}</div>
        </div>
        <div>
          <div class="muted small">总下载</div>
          <div class="big">{fmtBytes($traffic.total_rx)}</div>
        </div>
        <div>
          <div class="muted small">总上传</div>
          <div class="big">{fmtBytes($traffic.total_tx)}</div>
        </div>
      </div>

      {#if lastError}
        <div class="err">{lastError}</div>
      {/if}
    </div>

    <div class="right">
      <select bind:value={pickName} disabled={busy || $configs.length === 0}>
        {#if $configs.length === 0}
          <option value="">尚无配置</option>
        {:else}
          {#each $configs as c}
            <option value={c.name}>{c.name}</option>
          {/each}
        {/if}
      </select>
      <button
        class={$isConnected && $activeName === pickName ? "danger" : "primary"}
        on:click={toggleConnect}
        disabled={busy || !pickName}
      >
        {busy
          ? busyMsg
          : $isConnected && $activeName === pickName
            ? "断开"
            : "连接"}
      </button>
      {#if $configs.length === 0}
        <button on:click={() => push("/profiles")}>导入配置</button>
      {/if}
    </div>
  </section>

  <section class="card">
    <div class="card-title">实时流量（近 60s）</div>
    <TrafficChart />
    <div class="muted small handshake">
      握手：{fmtAge($traffic.handshake_age)}
    </div>
  </section>

  <section class="card">
    <div class="row-between" style="margin-bottom: 12px;">
      <div class="card-title" style="margin: 0;">网络诊断</div>
      <button on:click={runDiag} disabled={diagBusy} class="primary">
        {diagBusy ? "检测中…" : "一键诊断"}
      </button>
    </div>

    {#if diagError}
      <div class="err">{diagError}</div>
    {/if}

    {#if diag}
      <div class="diag-grid">
        <div class="diag-cell">
          <div class="muted small">出口 IPv4</div>
          <div class="big-2">
            {diag.egress_ipv4 ?? "—"}
            {#if diag.egress_ipv4_country}
              <span class="muted small">({diag.egress_ipv4_country})</span>
            {/if}
          </div>
        </div>
        <div class="diag-cell">
          <div class="muted small">出口 IPv6</div>
          <div class="big-2">
            {#if diag.egress_ipv6}
              <span class="warn">{diag.egress_ipv6}</span>
              <span class="tag warn">可能泄漏</span>
            {:else}
              <span class="ok">已禁用</span>
            {/if}
          </div>
        </div>
        <div class="diag-cell" style="grid-column: span 2;">
          <div class="muted small">DNS 服务器</div>
          <div class="dns-list">
            {#each diag.dns_servers as d}
              <span class="kbd">{d}</span>
            {:else}
              <span class="muted">—</span>
            {/each}
          </div>
        </div>
      </div>

      <div class="reach">
        {#each diag.reachability as r}
          <div class="reach-row" class:ok={r.ok} class:err={!r.ok}>
            <span class="reach-host">{r.host}</span>
            <span class="reach-status">
              {#if r.ok}
                <span class="tag ok">{r.status} · {r.latency_ms}ms</span>
              {:else}
                <span class="tag err">{r.error ?? "失败"}</span>
              {/if}
            </span>
          </div>
        {/each}
      </div>

      {#if diag.config_checks.length}
        <div class="checks">
          {#each diag.config_checks as c}
            <div class="check {c.level}">
              <div class="row" style="gap:6px;">
                <span class="check-dot"></span>
                <strong>{c.title}</strong>
              </div>
              <div class="muted small">{c.detail}</div>
            </div>
          {/each}
        </div>
      {/if}
    {:else if !diagBusy}
      <div class="muted small">点击「一键诊断」检查 IP / DNS / 可达性 / 配置</div>
    {/if}
  </section>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 920px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
  }
  .hero {
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 20px 24px;
    display: flex;
    align-items: center;
    gap: 24px;
  }
  .hero .left {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .state-line {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--text-3);
    transition: 0.2s;
  }
  .dot.on {
    background: var(--green);
    box-shadow: 0 0 0 4px rgba(46, 194, 126, 0.18);
  }
  .state-text {
    font-weight: 600;
  }
  .active-name {
    color: var(--text-2);
  }
  .hero-stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
    font-variant-numeric: tabular-nums;
    /* 防止单位切换 (B→KB→MB) 时整块抖动 */
    contain: layout;
  }
  .big {
    font-size: 16px;
    font-weight: 600;
    /* 给数值最小宽度 + 等宽数字，避免每秒刷新时抖动 */
    min-width: 7ch;
    font-variant-numeric: tabular-nums;
    /* 数字内容直接更新文本节点，不触发整块布局重排 */
    will-change: contents;
  }
  .big-2 {
    font-size: 14px;
    font-weight: 500;
  }
  .small {
    font-size: 11px;
  }
  .right {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 160px;
  }
  .err {
    background: rgba(237, 94, 110, 0.1);
    border: 1px solid rgba(237, 94, 110, 0.3);
    color: var(--red);
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 12px;
  }
  .handshake {
    margin-top: 6px;
    text-align: right;
  }
  .diag-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
    margin-bottom: 14px;
  }
  .diag-cell {
    background: var(--bg-3);
    border-radius: 6px;
    padding: 10px 12px;
  }
  .ok {
    color: var(--green);
  }
  .warn {
    color: var(--yellow);
  }
  .dns-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }
  .reach {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 8px;
    margin-bottom: 14px;
  }
  .reach-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: var(--bg-3);
    padding: 8px 12px;
    border-radius: 6px;
  }
  .reach-host {
    font-family: ui-monospace, monospace;
    font-size: 12px;
  }
  .checks {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .check {
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--bg-3);
    border-left: 3px solid var(--text-3);
  }
  .check.ok {
    border-left-color: var(--green);
  }
  .check.warn {
    border-left-color: var(--yellow);
  }
  .check.error {
    border-left-color: var(--red);
  }
  .check-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
  }
</style>
