<script>
  import { createEventDispatcher } from "svelte";
  const dispatch = createEventDispatcher();

  export let initialName = "";
  export let initialContent = "";
  export let editing = false;

  let name = initialName;
  let content = initialContent;
  let mode = "raw"; // raw | form

  // 解析当前 content 抽取出表单字段
  function parse(text) {
    const out = {
      privateKey: "",
      address: "",
      dns: "",
      mtu: "",
      publicKey: "",
      allowedIps: "0.0.0.0/0",
      endpoint: "",
      persistentKeepalive: "",
    };
    let section = "";
    for (const raw of text.split(/\r?\n/)) {
      const l = raw.trim();
      if (l.startsWith("#") || !l) continue;
      if (l.startsWith("[")) {
        section = l.toLowerCase();
        continue;
      }
      const eq = l.indexOf("=");
      if (eq < 0) continue;
      const k = l.slice(0, eq).trim().toLowerCase();
      const v = l.slice(eq + 1).trim();
      if (section === "[interface]") {
        if (k === "privatekey") out.privateKey = v;
        else if (k === "address") out.address = v;
        else if (k === "dns") out.dns = v;
        else if (k === "mtu") out.mtu = v;
      } else if (section === "[peer]") {
        if (k === "publickey") out.publicKey = v;
        else if (k === "allowedips") out.allowedIps = v;
        else if (k === "endpoint") out.endpoint = v;
        else if (k === "persistentkeepalive") out.persistentKeepalive = v;
      }
    }
    return out;
  }

  function build(form) {
    const lines = ["[Interface]"];
    if (form.privateKey) lines.push(`PrivateKey = ${form.privateKey}`);
    if (form.address) lines.push(`Address = ${form.address}`);
    if (form.dns) lines.push(`DNS = ${form.dns}`);
    if (form.mtu) lines.push(`MTU = ${form.mtu}`);
    lines.push("", "[Peer]");
    if (form.publicKey) lines.push(`PublicKey = ${form.publicKey}`);
    if (form.allowedIps) lines.push(`AllowedIPs = ${form.allowedIps}`);
    if (form.endpoint) lines.push(`Endpoint = ${form.endpoint}`);
    if (form.persistentKeepalive)
      lines.push(`PersistentKeepalive = ${form.persistentKeepalive}`);
    return lines.join("\n") + "\n";
  }

  let form = parse(initialContent);
  let errMsg = "";

  function syncFromForm() {
    content = build(form);
  }

  function syncToForm() {
    form = parse(content);
  }

  function switchMode(m) {
    if (m === mode) return;
    if (mode === "raw") syncToForm();
    else syncFromForm();
    mode = m;
  }

  function submit() {
    errMsg = "";
    let final = mode === "form" ? build(form) : content;
    const cleanName = name.trim();
    if (!cleanName) {
      errMsg = "请填写名称";
      return;
    }
    if (!/^[A-Za-z0-9_-]{1,15}$/.test(cleanName)) {
      errMsg = "名称只能用字母/数字/下划线/短横线，且 ≤15 字符";
      return;
    }
    if (!/\[Interface\]/i.test(final) || !/\[Peer\]/i.test(final)) {
      errMsg = "配置必须同时包含 [Interface] 和 [Peer] 段";
      return;
    }
    if (!/PrivateKey\s*=/i.test(final) || !/PublicKey\s*=/i.test(final)) {
      errMsg = "配置缺少 PrivateKey 或 PublicKey";
      return;
    }
    if (!/Endpoint\s*=/i.test(final)) {
      errMsg = "[Peer] 段缺少 Endpoint";
      return;
    }
    dispatch("save", { name: cleanName, content: final });
  }
</script>

<div class="editor">
  <div class="hdr">
    <input
      placeholder="名称（字母/数字/下划线/短横线，≤15）"
      bind:value={name}
      disabled={editing}
    />
    <div class="seg">
      <button
        class:on={mode === "raw"}
        on:click={() => switchMode("raw")}>原始</button
      >
      <button
        class:on={mode === "form"}
        on:click={() => switchMode("form")}>表单</button
      >
    </div>
  </div>

  {#if mode === "raw"}
    <textarea rows="14" bind:value={content} placeholder="粘贴 .conf 内容…"
    ></textarea>
    <div class="muted small">支持直接粘贴 WireGuard 客户端导出的 .conf</div>
  {:else}
    <div class="form-grid">
      <fieldset>
        <legend>[Interface]</legend>
        <label
          ><span>PrivateKey</span><input
            bind:value={form.privateKey}
            placeholder="客户端私钥"
          /></label
        >
        <label
          ><span>Address</span><input
            bind:value={form.address}
            placeholder="10.0.0.2/32"
          /></label
        >
        <label
          ><span>DNS</span><input
            bind:value={form.dns}
            placeholder="1.1.1.1"
          /></label
        >
        <label
          ><span>MTU</span><input
            bind:value={form.mtu}
            placeholder="1420"
          /></label
        >
      </fieldset>
      <fieldset>
        <legend>[Peer]</legend>
        <label
          ><span>PublicKey</span><input
            bind:value={form.publicKey}
            placeholder="服务端公钥"
          /></label
        >
        <label
          ><span>Endpoint</span><input
            bind:value={form.endpoint}
            placeholder="1.2.3.4:51820"
          /></label
        >
        <label
          ><span>AllowedIPs</span><input
            bind:value={form.allowedIps}
            placeholder="0.0.0.0/0"
          /></label
        >
        <label
          ><span>PersistentKeepalive</span><input
            bind:value={form.persistentKeepalive}
            placeholder="25"
          /></label
        >
      </fieldset>
    </div>
  {/if}

  {#if errMsg}
    <div class="err">{errMsg}</div>
  {/if}
  <div class="actions">
    <button on:click={() => dispatch("cancel")}>取消</button>
    <button class="primary" on:click={submit}>保存</button>
  </div>
</div>

<style>
  .editor {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hdr {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .hdr input {
    flex: 1;
  }
  .seg {
    display: flex;
    background: var(--bg-3);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 2px;
  }
  .seg button {
    border: none;
    background: transparent;
    padding: 4px 12px;
    color: var(--text-2);
  }
  .seg button.on {
    background: var(--bg);
    color: var(--text);
  }
  .form-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  fieldset {
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 10px 12px 12px;
    margin: 0;
  }
  legend {
    color: var(--text-2);
    padding: 0 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  fieldset label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-top: 8px;
  }
  fieldset span {
    font-size: 11px;
    color: var(--text-2);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .err {
    background: rgba(255, 80, 80, 0.12);
    border: 1px solid rgba(255, 80, 80, 0.4);
    color: #ff8a8a;
    border-radius: 6px;
    padding: 8px 10px;
    font-size: 12px;
  }
</style>
