<script>
  import { createEventDispatcher } from "svelte";
  import { api } from "../lib/api.js";
  import { refreshConfigs, refreshPasswordless, passwordless } from "../lib/store.js";
  import ConfigEditor from "./ConfigEditor.svelte";

  const dispatch = createEventDispatcher();
  let step = 1;
  let busy = false;
  let lastError = "";

  async function saveConfig(e) {
    busy = true;
    lastError = "";
    try {
      await api.saveConfig(e.detail.name, e.detail.content);
      await refreshConfigs();
      step = 2;
    } catch (e) {
      lastError = String(e);
    } finally {
      busy = false;
    }
  }

  async function enableNoPwd() {
    busy = true;
    lastError = "";
    try {
      await api.enablePasswordless();
      await refreshPasswordless();
      step = 3;
    } catch (e) {
      lastError = String(e);
    } finally {
      busy = false;
    }
  }

  function done() {
    localStorage.setItem("onboard_done", "1");
    dispatch("done");
  }
</script>

<div class="overlay">
  <div class="modal fade-in">
    <div class="hdr">
      <div class="logo"></div>
      <div>
        <div class="t">欢迎使用 Soar</div>
        <div class="muted small">3 步完成初始化</div>
      </div>
      <button class="ghost" on:click={done}>跳过</button>
    </div>

    <div class="steps">
      <div class="step" class:on={step >= 1} class:done={step > 1}>1 · 导入配置</div>
      <div class="step" class:on={step >= 2} class:done={step > 2}>2 · 启用免密</div>
      <div class="step" class:on={step >= 3}>3 · 完成</div>
    </div>

    {#if lastError}
      <div class="err">{lastError}</div>
    {/if}

    <div class="body">
      {#if step === 1}
        <p class="muted">把同事/服务商给的 .conf 文件粘贴进来：</p>
        <ConfigEditor on:save={saveConfig} on:cancel={done} />
      {:else if step === 2}
        <p>
          一次性写入 sudoers，之后连接 / 切规则不再弹密码框。<br />
          点击下方按钮，会出现 <strong>系统授权框</strong>（输入一次开机密码）。
        </p>
        {#if $passwordless?.enabled}
          <p class="ok">免密已启用 ✓</p>
        {/if}
        <div class="actions">
          <button on:click={() => (step = 3)}>跳过</button>
          <button class="primary" on:click={enableNoPwd} disabled={busy}>
            {busy ? "授权中…" : "启用免密"}
          </button>
        </div>
      {:else}
        <p class="ok">完成！可以去「概览」点连接了。</p>
        <div class="muted small">
          - 关闭主窗口 = 隐藏到菜单栏，App 继续运行<br />
          - 任何时候点击菜单栏图标可以重新打开<br />
          - 在「设置」可以开启 Kill Switch、自动重连、开机自启等
        </div>
        <div class="actions">
          <button class="primary" on:click={done}>开始使用</button>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    backdrop-filter: blur(6px);
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .modal {
    width: 540px;
    max-width: 92vw;
    max-height: 90vh;
    overflow: auto;
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 22px 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .hdr {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .logo {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 4px rgba(91, 140, 255, 0.18);
  }
  .t {
    font-size: 16px;
    font-weight: 600;
  }
  .ghost {
    margin-left: auto;
    background: transparent;
    border: none;
    color: var(--text-2);
  }
  .steps {
    display: flex;
    gap: 6px;
  }
  .step {
    flex: 1;
    text-align: center;
    background: var(--bg-3);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 6px;
    font-size: 11px;
    color: var(--text-3);
  }
  .step.on {
    color: var(--text);
    border-color: var(--accent);
  }
  .step.done {
    background: rgba(46, 194, 126, 0.15);
    border-color: rgba(46, 194, 126, 0.4);
    color: var(--green);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .body p {
    margin: 0;
  }
  .ok {
    color: var(--green);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .err {
    background: rgba(237, 94, 110, 0.1);
    border: 1px solid rgba(237, 94, 110, 0.3);
    color: var(--red);
    padding: 8px 10px;
    border-radius: 6px;
    font-size: 12px;
  }
  .small {
    font-size: 11px;
  }
</style>
