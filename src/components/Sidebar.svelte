<script>
  import { link, location } from "svelte-spa-router";
  import { traffic, isConnected } from "../lib/store.js";
  import { fmtBps } from "../lib/api.js";

  const items = [
    { path: "/", label: "概览", icon: "home" },
    { path: "/profiles", label: "配置", icon: "list" },
    { path: "/rules", label: "规则", icon: "filter" },
    { path: "/logs", label: "日志", icon: "doc" },
    { path: "/settings", label: "设置", icon: "gear" },
  ];

  function isActive(p, loc) {
    if (p === "/") return loc === "/" || loc === "";
    return loc.startsWith(p);
  }
</script>

<aside class="sidebar">
  <div class="brand">
    <div class="logo" class:on={$isConnected}></div>
    <div class="title">Soar</div>
  </div>

  <nav>
    {#each items as it}
      <a
        href={"#" + it.path}
        use:link
        class:active={isActive(it.path, $location)}
      >
        <span class="ico ico-{it.icon}" aria-hidden="true"></span>
        {it.label}
      </a>
    {/each}
  </nav>

  <div class="foot">
    <div class="status-line" class:on={$isConnected}>
      <span class="dot"></span>
      <span class="lbl">{$isConnected ? "已连接" : "未连接"}</span>
    </div>
    <div class="speed">
      <div class="row">
        <span class="arr">↓</span>
        <span class="val"
          >{$isConnected ? fmtBps($traffic.rx_bps || 0) : "—"}</span
        >
      </div>
      <div class="row">
        <span class="arr up">↑</span>
        <span class="val"
          >{$isConnected ? fmtBps($traffic.tx_bps || 0) : "—"}</span
        >
      </div>
    </div>
  </div>
</aside>

<style>
  .sidebar {
    width: 168px;
    background: var(--bg-2);
    border-right: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    padding: 14px 10px 10px;
    flex-shrink: 0;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px 16px;
  }
  .logo {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--text-3);
    box-shadow: 0 0 0 0 rgba(46, 194, 126, 0);
    transition: background 0.2s, box-shadow 0.2s;
  }
  .logo.on {
    background: var(--green);
    box-shadow: 0 0 0 4px rgba(46, 194, 126, 0.18);
  }
  .title {
    font-weight: 600;
    font-size: 14px;
    letter-spacing: 0.02em;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  nav a {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 6px;
    color: var(--text-2);
    text-decoration: none;
    font-size: 13px;
  }
  nav a:hover {
    background: var(--bg-3);
    color: var(--text);
  }
  nav a.active {
    background: var(--bg-3);
    color: var(--accent);
  }
  .ico {
    width: 16px;
    height: 16px;
    display: inline-block;
    background: currentColor;
    -webkit-mask-position: center;
    -webkit-mask-repeat: no-repeat;
    -webkit-mask-size: contain;
  }
  .ico-home {
    -webkit-mask-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><path d='M3 12L12 3l9 9'/><path d='M5 10v10h14V10'/></svg>");
  }
  .ico-list {
    -webkit-mask-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><line x1='8' y1='6' x2='21' y2='6'/><line x1='8' y1='12' x2='21' y2='12'/><line x1='8' y1='18' x2='21' y2='18'/><line x1='3' y1='6' x2='3.01' y2='6'/><line x1='3' y1='12' x2='3.01' y2='12'/><line x1='3' y1='18' x2='3.01' y2='18'/></svg>");
  }
  .ico-filter {
    -webkit-mask-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><polygon points='22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3'/></svg>");
  }
  .ico-doc {
    -webkit-mask-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><path d='M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z'/><polyline points='14 2 14 8 20 8'/><line x1='8' y1='13' x2='16' y2='13'/><line x1='8' y1='17' x2='14' y2='17'/></svg>");
  }
  .ico-gear {
    -webkit-mask-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round' stroke-linejoin='round'><circle cx='12' cy='12' r='3'/><path d='M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z'/></svg>");
  }

  .foot {
    margin-top: auto;
    padding: 10px;
    border-top: 1px solid var(--line);
  }
  .status-line {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-2);
    margin-bottom: 8px;
  }
  .status-line .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .status-line.on .dot {
    background: var(--green);
    box-shadow: 0 0 0 3px rgba(46, 194, 126, 0.18);
  }
  .status-line.on .lbl {
    color: var(--green);
  }
  .speed {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .speed .row {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .arr {
    color: var(--green);
    font-weight: 700;
    width: 10px;
  }
  .arr.up {
    color: var(--accent);
  }
  .val {
    color: var(--text);
    /* 防止 B/s → KB/s → MB/s 切换时整行抖动 */
    min-width: 8ch;
    display: inline-block;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
