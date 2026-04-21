<script>
  import { traffic, activeName, isConnected } from "../lib/store.js";
  import { fmtAge } from "../lib/api.js";
</script>

<footer>
  <div class="dot" class:on={$isConnected}></div>
  <span class="state">
    {#if $isConnected}
      已连接 · {$activeName ?? ""}
    {:else}
      未连接
    {/if}
  </span>
  {#if $isConnected}
    <span class="sep">·</span>
    <span>握手 {fmtAge($traffic.handshake_age)}</span>
  {/if}
  <span class="grow"></span>
  <span class="muted">v0.2.4</span>
</footer>

<style>
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 16px;
    border-top: 1px solid var(--line);
    background: var(--bg-2);
    font-size: 11px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-3);
  }
  .dot.on {
    background: var(--green);
    box-shadow: 0 0 0 2px rgba(46, 194, 126, 0.2);
  }
  .state {
    color: var(--text);
  }
  .sep {
    color: var(--text-3);
  }
  .grow {
    flex: 1;
  }
  .muted {
    color: var(--text-3);
  }
</style>
