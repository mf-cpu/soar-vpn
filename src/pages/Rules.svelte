<script>
  import { onMount } from "svelte";
  import { configs, activeName, refreshConfigs, refreshActive } from "../lib/store.js";
  import { api } from "../lib/api.js";

  let templates = [];
  let pickName = "";
  let state = null; // { mode, allowed_ips }
  let busy = false;
  let lastError = "";
  let lastApplied = "";
  let customText = "";
  let showCustom = false;

  $: pickName = $activeName || $configs[0]?.name || "";

  async function loadState() {
    state = null;
    if (!pickName) return;
    try {
      state = await api.getRuleState(pickName);
      if (state.mode === "custom") {
        customText = state.allowed_ips;
        showCustom = true;
      }
    } catch (e) {
      lastError = String(e);
    }
  }

  async function apply(mode, custom) {
    busy = true;
    lastError = "";
    lastApplied = "";
    try {
      const r = await api.applyRuleMode(pickName, mode, custom ?? null);
      state = r;
      lastApplied = mode;
      setTimeout(() => (lastApplied = ""), 2500);
    } catch (e) {
      lastError = String(e);
    } finally {
      busy = false;
    }
  }

  function applyCustom() {
    if (!customText.trim()) return;
    apply("custom", customText.trim());
  }

  onMount(async () => {
    await Promise.all([refreshConfigs(), refreshActive()]);
    templates = await api.listRuleTemplates();
    await loadState();
  });

  $: if (pickName && !state) loadState();
</script>

<div class="page">
  <h1>规则模式</h1>
  <div class="muted small">
    控制哪些 IP 段走 VPN。已连接时切换会立即生效（热应用，不会断开），断开状态切换则下次连接时生效。
  </div>

  {#if $configs.length === 0}
    <div class="empty">先去「配置」页创建至少一个 WireGuard 配置</div>
  {:else}
    <div class="row">
      <span class="muted">作用于：</span>
      <select
        bind:value={pickName}
        on:change={loadState}
        style="max-width: 220px;"
      >
        {#each $configs as c}
          <option value={c.name}>{c.name}</option>
        {/each}
      </select>
      {#if state && state.mode !== "custom"}
        <span class="tag">当前：{state.mode}</span>
      {:else if state}
        <span class="tag">当前：自定义</span>
      {/if}
    </div>

    {#if lastError}
      <div class="err">{lastError}</div>
    {/if}

    <div class="grid">
      {#each templates as t}
        <div
          class="rule-card"
          class:active={state?.mode === t.mode}
          class:applied={lastApplied === t.mode}
        >
          <div class="row-between">
            <div class="rt">
              {t.title}
              {#if t.recommended}
                <span class="tag ok">推荐</span>
              {/if}
            </div>
            {#if state?.mode === t.mode}
              <span class="tag ok">已应用</span>
            {/if}
          </div>
          <div class="muted small">{t.desc}</div>
          <div class="cidr">
            {t.allowed_ips.split(",").length} 条 CIDR
          </div>
          <button
            class="primary"
            on:click={() => apply(t.mode, null)}
            disabled={busy || state?.mode === t.mode}
          >
            {state?.mode === t.mode ? "已应用" : "应用"}
          </button>
        </div>
      {/each}

      <div class="rule-card" class:active={state?.mode === "custom"}>
        <div class="row-between">
          <div class="rt">自定义</div>
          {#if state?.mode === "custom"}
            <span class="tag ok">已应用</span>
          {/if}
        </div>
        <div class="muted small">手动写 AllowedIPs，逗号分隔，例如 10.0.0.0/8, 192.168.1.0/24</div>
        <textarea rows="3" bind:value={customText} placeholder="0.0.0.0/0"
        ></textarea>
        <button class="primary" on:click={applyCustom} disabled={busy}>应用</button>
      </div>
    </div>

    {#if state}
      <details class="raw-cidrs">
        <summary>查看当前 AllowedIPs（{state.allowed_ips.split(",").length} 条）</summary>
        <pre>{state.allowed_ips}</pre>
      </details>
    {/if}
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 920px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 12px;
  }
  .rule-card {
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition:
      border-color 0.15s,
      box-shadow 0.4s;
  }
  .rule-card.active {
    border-color: var(--accent);
  }
  .rule-card.applied {
    box-shadow: 0 0 0 3px rgba(46, 194, 126, 0.25);
  }
  .rt {
    font-weight: 600;
  }
  .small {
    font-size: 11px;
  }
  .cidr {
    color: var(--text-3);
    font-size: 11px;
  }
  .raw-cidrs {
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 10px 14px;
  }
  .raw-cidrs summary {
    cursor: pointer;
    color: var(--text-2);
    font-size: 12px;
  }
  .raw-cidrs pre {
    margin: 8px 0 0;
    font-size: 11px;
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--text-2);
  }
  .err {
    background: rgba(237, 94, 110, 0.1);
    border: 1px solid rgba(237, 94, 110, 0.3);
    color: var(--red);
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 12px;
  }
</style>
