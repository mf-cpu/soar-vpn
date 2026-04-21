<script>
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api.js";

  let files = [];
  let pickFile = "";
  let raw = "";
  let filter = "";
  let auto = true;
  let timer = null;
  let pre;

  async function loadFiles() {
    files = await api.listLogFiles();
    if (!pickFile && files.length) {
      pickFile =
        files.find((f) => f.includes("Soar")) ||
        files.find((f) => f.includes("MaiSui")) ||
        files.find((f) => f.includes("WG VPN")) ||
        files[0];
    }
  }

  async function tail() {
    if (!pickFile) return;
    try {
      raw = await api.readLogTail(pickFile, 128 * 1024);
      if (auto && pre) {
        await Promise.resolve();
        pre.scrollTop = pre.scrollHeight;
      }
    } catch (e) {
      raw = String(e);
    }
  }

  $: filtered = filter
    ? raw
        .split("\n")
        .filter((l) => l.toLowerCase().includes(filter.toLowerCase()))
        .join("\n")
    : raw;

  onMount(async () => {
    await loadFiles();
    await tail();
    timer = setInterval(tail, 2000);
  });

  onDestroy(() => {
    if (timer) clearInterval(timer);
  });

  $: if (pickFile) tail();
</script>

<div class="page">
  <div class="row-between">
    <h1>日志</h1>
    <button on:click={api.openLogDir}>在 Finder 打开</button>
  </div>

  <div class="bar">
    <select bind:value={pickFile} style="max-width: 220px;">
      {#each files as f}
        <option value={f}>{f}</option>
      {/each}
    </select>
    <input
      placeholder="过滤关键字（实时）"
      bind:value={filter}
      style="flex: 1;"
    />
    <label class="auto"
      ><input type="checkbox" bind:checked={auto} /> 跟随末尾</label
    >
    <button on:click={tail}>刷新</button>
  </div>

  <pre bind:this={pre} class="log">{filtered || "(空)"}</pre>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
  }
  h1 {
    margin: 0;
    font-size: 22px;
    font-weight: 600;
  }
  .bar {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .auto {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-2);
    font-size: 12px;
    white-space: nowrap;
  }
  .auto input {
    width: auto;
  }
  .log {
    flex: 1;
    margin: 0;
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 12px 14px;
    font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace;
    font-size: 11px;
    color: var(--text-2);
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
    min-height: 200px;
  }
</style>
