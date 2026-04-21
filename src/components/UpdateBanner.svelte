<script>
  import { updateInfo, updateProgress } from "../lib/store.js";
  import { api } from "../lib/api.js";

  let busy = false;
  let err = "";
  let dismissed = false;

  async function install() {
    if (!$updateInfo?.latest) return;
    busy = true;
    err = "";
    try {
      // 此调用通常不会"成功返回"——helper 会 kill 自己再启新版
      await api.installUpdate($updateInfo.latest);
    } catch (e) {
      err = String(e);
      busy = false;
    }
  }

  function dismiss() {
    dismissed = true;
  }

  $: pct = $updateProgress?.percent ?? 0;
  $: phase = $updateProgress?.phase;
  $: phaseText =
    phase === "downloading"
      ? `下载中 ${pct}%`
      : phase === "verifying"
        ? "校验中…"
        : phase === "downloaded"
          ? "准备安装…"
          : "";
</script>

{#if $updateInfo?.has_update && !dismissed}
  <div class="banner">
    <div class="left">
      <span class="dot"></span>
      <div class="text">
        <div class="title">
          有新版本 <b>v{$updateInfo.latest.version}</b>
          可用（当前 v{$updateInfo.current}）
        </div>
        {#if $updateInfo.latest.notes}
          <div class="notes">{$updateInfo.latest.notes}</div>
        {/if}
        {#if err}
          <div class="err">升级失败：{err}</div>
        {/if}
        {#if busy && phaseText}
          <div class="prog">
            <div class="bar"><div class="fill" style="width:{pct}%"></div></div>
            <span class="ph">{phaseText}</span>
          </div>
        {/if}
      </div>
    </div>
    <div class="right">
      <button class="ghost" on:click={dismiss} disabled={busy}>稍后</button>
      <button class="primary" on:click={install} disabled={busy}>
        {busy ? "升级中…" : "立即升级"}
      </button>
    </div>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    background: linear-gradient(
      90deg,
      rgba(82, 130, 255, 0.18),
      rgba(82, 130, 255, 0.05)
    );
    border-bottom: 1px solid var(--line);
  }
  .left {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    min-width: 0;
    flex: 1;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    margin-top: 7px;
    box-shadow: 0 0 0 4px rgba(82, 130, 255, 0.18);
    flex-shrink: 0;
  }
  .title {
    font-size: 13px;
    color: var(--text);
  }
  .notes {
    font-size: 12px;
    color: var(--text-2);
    margin-top: 2px;
    white-space: pre-wrap;
    max-height: 60px;
    overflow: hidden;
  }
  .err {
    font-size: 12px;
    color: #ff8a8a;
    margin-top: 4px;
  }
  .prog {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-2);
  }
  .bar {
    flex: 1;
    height: 4px;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 2px;
    overflow: hidden;
    max-width: 240px;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.3s;
  }
  .right {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  button {
    font-size: 12px;
    padding: 5px 12px;
  }
  button.ghost {
    background: transparent;
    border: 1px solid var(--line);
    color: var(--text-2);
  }
</style>
