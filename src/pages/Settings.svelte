<script>
  import { onMount } from "svelte";
  import {
    settings,
    passwordless,
    configs,
    refreshSettings,
    refreshPasswordless,
    refreshConfigs,
  } from "../lib/store.js";
  import { api } from "../lib/api.js";
  import { updateInfo } from "../lib/store.js";

  let tab = "basic"; // basic | system | about
  let busy = false;
  let lastError = "";
  let checking = false;
  let checkMsg = "";

  async function save(patch) {
    busy = true;
    lastError = "";
    try {
      const cur = $settings || {};
      const next = {
        auto_reconnect: cur.auto_reconnect ?? true,
        kill_switch: cur.kill_switch ?? false,
        auto_connect_on_start: cur.auto_connect_on_start ?? null,
        launch_at_login: cur.launch_at_login ?? false,
        update_manifest_url: cur.update_manifest_url ?? "",
        auto_check_update: cur.auto_check_update ?? true,
        ...patch,
      };
      const r = await api.setSettings(next);
      settings.set(r);
    } catch (e) {
      lastError = String(e);
      await refreshSettings();
    } finally {
      busy = false;
    }
  }

  async function checkUpdate() {
    checking = true;
    checkMsg = "";
    try {
      const r = await api.checkUpdate();
      if (r.has_update) {
        updateInfo.set(r);
        checkMsg = `发现新版本 v${r.latest.version}（顶部 banner 已显示）`;
      } else {
        checkMsg = `当前已是最新版本 v${r.current}`;
      }
    } catch (e) {
      checkMsg = "检查失败：" + String(e);
    } finally {
      checking = false;
    }
  }

  async function togglePasswordless(on) {
    busy = true;
    lastError = "";
    try {
      if (on) await api.enablePasswordless();
      else await api.disablePasswordless();
      await refreshPasswordless();
    } catch (e) {
      lastError = String(e);
    } finally {
      busy = false;
    }
  }

  onMount(async () => {
    await Promise.all([refreshSettings(), refreshPasswordless(), refreshConfigs()]);
  });
</script>

<div class="page">
  <h1>设置</h1>

  <div class="tabs">
    <button class:on={tab === "basic"} on:click={() => (tab = "basic")}>基础</button>
    <button class:on={tab === "system"} on:click={() => (tab = "system")}>系统</button>
    <button class:on={tab === "about"} on:click={() => (tab = "about")}>关于</button>
  </div>

  {#if lastError}
    <div class="err">{lastError}</div>
  {/if}

  {#if tab === "basic" && $settings}
    <section class="card col">
      <div class="setting">
        <div>
          <div class="t">自动重连</div>
          <div class="muted small">检测到握手 &gt; 3 分钟自动 down/up 重建</div>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            checked={$settings.auto_reconnect}
            on:change={(e) => save({ auto_reconnect: e.target.checked })}
            disabled={busy}
          />
          <span></span>
        </label>
      </div>

      <div class="setting">
        <div>
          <div class="t">Kill Switch</div>
          <div class="muted small">
            VPN 断开/未连接时禁止任何流量出公网，防止裸奔泄漏 IP
            {#if $settings.kill_switch_active}
              <span class="tag ok" style="margin-left:6px;">已生效</span>
            {/if}
          </div>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            checked={$settings.kill_switch}
            on:change={(e) => save({ kill_switch: e.target.checked })}
            disabled={busy}
          />
          <span></span>
        </label>
      </div>

      <div class="setting">
        <div>
          <div class="t">启动时自动连接</div>
          <div class="muted small">App 启动后自动连接指定配置</div>
        </div>
        <select
          value={$settings.auto_connect_on_start ?? ""}
          on:change={(e) =>
            save({ auto_connect_on_start: e.target.value || null })}
          disabled={busy}
          style="max-width: 200px;"
        >
          <option value="">不自动连接</option>
          {#each $configs as c}
            <option value={c.name}>{c.name}</option>
          {/each}
        </select>
      </div>
    </section>
  {:else if tab === "system" && $settings && $passwordless}
    <section class="card col">
      <div class="setting">
        <div>
          <div class="t">开机自启</div>
          <div class="muted small">登录系统后自动启动 Soar（后台运行在菜单栏）</div>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            checked={$settings.launch_at_login}
            on:change={(e) => save({ launch_at_login: e.target.checked })}
            disabled={busy}
          />
          <span></span>
        </label>
      </div>

      <div class="setting">
        <div>
          <div class="t">免密模式</div>
          <div class="muted small">
            一次性写入 sudoers，之后连接/断开/切规则不再弹密码框
            {#if $passwordless.enabled}
              <span class="tag ok">已启用</span>
            {:else if $passwordless.available}
              <span class="tag">未启用</span>
            {:else}
              <span class="tag err">不可用</span>
            {/if}
          </div>
        </div>
        {#if $passwordless.enabled}
          <button class="danger" on:click={() => togglePasswordless(false)} disabled={busy}>
            关闭
          </button>
        {:else if $passwordless.available}
          <button class="primary" on:click={() => togglePasswordless(true)} disabled={busy}>
            启用
          </button>
        {/if}
      </div>

      <div class="setting">
        <div>
          <div class="t">日志</div>
          <div class="muted small">查看应用日志、wg-quick 日志</div>
        </div>
        <button on:click={api.openLogDir}>在 Finder 打开</button>
      </div>
    </section>
  {:else if tab === "about" && $settings}
    <section class="card col" style="gap: 14px;">
      <div>
        <div style="font-size: 16px; font-weight: 600;">Soar</div>
        <div class="muted small">基于 WireGuard + Tauri 2 + Svelte 5</div>
      </div>

      <div class="setting" style="border:none; padding:0;">
        <div>
          <div class="t">检查更新</div>
          <div class="muted small">
            手动比对升级地址中的版本号
            {#if checkMsg}
              <span class="hint">· {checkMsg}</span>
            {/if}
          </div>
        </div>
        <button class="primary" on:click={checkUpdate} disabled={checking}>
          {checking ? "检查中…" : "检查更新"}
        </button>
      </div>

      <div class="setting" style="border:none; padding:0;">
        <div>
          <div class="t">启动时自动检查</div>
          <div class="muted small">App 启动 8s 后静默 GET manifest 比版本</div>
        </div>
        <label class="switch">
          <input
            type="checkbox"
            checked={$settings.auto_check_update}
            on:change={(e) => save({ auto_check_update: e.target.checked })}
            disabled={busy}
          />
          <span></span>
        </label>
      </div>

      <label class="url-input">
        <span class="t">升级 manifest URL</span>
        <span class="muted small">
          指向一个返回 JSON 的 URL（{`{version,url,sha256,notes}`}）。
          留空则用编译时默认（GitHub Release）。
        </span>
        <input
          type="text"
          placeholder="https://example.com/wg-vpn/latest.json"
          value={$settings.update_manifest_url || ""}
          on:change={(e) => save({ update_manifest_url: e.target.value })}
          disabled={busy}
        />
      </label>

      <div class="muted small" style="line-height:1.7;">
        - 内置 wg / wg-quick / wireguard-go 通用二进制（intel + apple silicon）<br />
        - 自动 IPv6 / DNS 防泄漏<br />
        - 4 套预置规则模板，热切换不断连<br />
        - 菜单栏托盘 + 自动重连 + Kill Switch
      </div>
    </section>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    max-width: 720px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
  }
  .tabs {
    display: flex;
    gap: 2px;
    background: var(--bg-3);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 3px;
    width: fit-content;
  }
  .tabs button {
    border: none;
    background: transparent;
    color: var(--text-2);
    padding: 6px 16px;
  }
  .tabs button.on {
    background: var(--bg);
    color: var(--text);
  }
  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 0;
    border-bottom: 1px solid var(--line);
  }
  .setting:last-child {
    border-bottom: none;
  }
  .t {
    font-weight: 500;
    margin-bottom: 2px;
  }
  .small {
    font-size: 11px;
  }
  .switch {
    position: relative;
    display: inline-block;
    width: 36px;
    height: 20px;
  }
  .switch input {
    display: none;
  }
  .switch span {
    position: absolute;
    inset: 0;
    background: var(--bg-3);
    border: 1px solid var(--line);
    border-radius: 20px;
    cursor: pointer;
    transition: background 0.2s;
  }
  .switch span::before {
    content: "";
    position: absolute;
    width: 14px;
    height: 14px;
    background: var(--text-2);
    border-radius: 50%;
    top: 2px;
    left: 2px;
    transition: 0.2s;
  }
  .switch input:checked + span {
    background: var(--accent);
    border-color: var(--accent);
  }
  .switch input:checked + span::before {
    background: #fff;
    transform: translateX(16px);
  }
  .err {
    background: rgba(237, 94, 110, 0.1);
    border: 1px solid rgba(237, 94, 110, 0.3);
    color: var(--red);
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 12px;
  }
  .url-input {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-top: 6px;
  }
  .url-input input {
    margin-top: 6px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    font-size: 12px;
  }
  .hint {
    color: var(--accent);
  }
</style>
