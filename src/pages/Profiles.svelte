<script>
  import { onMount } from "svelte";
  import {
    configs,
    activeName,
    isConnected,
    refreshConfigs,
    refreshActive,
  } from "../lib/store.js";
  import { api } from "../lib/api.js";
  import ConfigEditor from "../components/ConfigEditor.svelte";

  let editorMode = null; // null | 'new' | 'edit'
  let editingName = "";
  let editingContent = "";
  let busy = false;
  let busyName = "";
  let lastError = "";
  let armedDelete = "";

  async function openNew() {
    editorMode = "new";
    editingName = "";
    editingContent = "";
  }

  async function openEdit(c) {
    try {
      const text = await api.readConfig(c.name);
      editingName = c.name;
      editingContent = text;
      editorMode = "edit";
    } catch (e) {
      lastError = String(e);
    }
  }

  async function onSave(e) {
    const { name, content } = e.detail;
    busy = true;
    lastError = "";
    try {
      await api.saveConfig(name, content);
      await refreshConfigs();
      editorMode = null;
    } catch (e) {
      lastError = String(e);
    } finally {
      busy = false;
    }
  }

  async function onDelete(c) {
    if (armedDelete !== c.name) {
      armedDelete = c.name;
      setTimeout(() => {
        if (armedDelete === c.name) armedDelete = "";
      }, 3000);
      return;
    }
    armedDelete = "";
    try {
      await api.deleteConfig(c.name);
      await refreshConfigs();
      await refreshActive();
    } catch (e) {
      lastError = String(e);
    }
  }

  async function toggleConnect(c) {
    busy = true;
    busyName = c.name;
    lastError = "";
    try {
      if ($isConnected && $activeName === c.name) {
        await api.disconnect(c.name);
      } else {
        await api.connect(c.name);
      }
      await refreshActive();
    } catch (e) {
      lastError = String(e);
    } finally {
      busy = false;
      busyName = "";
    }
  }

  onMount(refreshConfigs);
</script>

<div class="page">
  <div class="row-between">
    <h1>配置</h1>
    <button class="primary" on:click={openNew}>+ 新建配置</button>
  </div>

  {#if lastError}
    <div class="err">{lastError}</div>
  {/if}

  {#if editorMode}
    <section class="card fade-in">
      <div class="card-title">
        {editorMode === "new" ? "新建配置" : `编辑：${editingName}`}
      </div>
      <ConfigEditor
        initialName={editingName}
        initialContent={editingContent}
        editing={editorMode === "edit"}
        on:save={onSave}
        on:cancel={() => (editorMode = null)}
      />
    </section>
  {/if}

  {#if $configs.length === 0 && !editorMode}
    <div class="empty">
      <div>还没有任何配置</div>
      <button class="primary" on:click={openNew}>导入第一个配置</button>
    </div>
  {:else}
    <div class="grid">
      {#each $configs as c (c.name)}
        <div class="cfg-card" class:active={$activeName === c.name && $isConnected}>
          <div class="row-between">
            <div class="name">{c.name}</div>
            {#if $activeName === c.name && $isConnected}
              <span class="tag ok">已连接</span>
            {/if}
          </div>
          <div class="muted small">
            {c.endpoint ?? "(未填 Endpoint)"}
          </div>
          <div class="muted small">
            {c.address ?? "(未填 Address)"}
          </div>
          <div class="actions">
            <button
              class={$isConnected && $activeName === c.name ? "danger" : "primary"}
              on:click={() => toggleConnect(c)}
              disabled={busy}
            >
              {busy && busyName === c.name
                ? "…"
                : $isConnected && $activeName === c.name
                  ? "断开"
                  : "连接"}
            </button>
            <button on:click={() => openEdit(c)}>编辑</button>
            <button
              class="danger"
              class:armed={armedDelete === c.name}
              on:click={() => onDelete(c)}
            >
              {armedDelete === c.name ? "再点一次确认" : "删除"}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
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
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
  }
  .cfg-card {
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    transition: border-color 0.15s;
  }
  .cfg-card.active {
    border-color: var(--green);
  }
  .name {
    font-weight: 600;
    font-size: 14px;
  }
  .small {
    font-size: 11px;
  }
  .actions {
    margin-top: 8px;
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .actions button {
    flex: 1;
    min-width: 60px;
    font-size: 12px;
    padding: 5px 8px;
  }
  button.armed {
    background: var(--red);
    border-color: var(--red);
    color: #fff;
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
