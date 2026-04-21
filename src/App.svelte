<script>
  import Router, { link, location } from "svelte-spa-router";
  import { onMount } from "svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import UpdateBanner from "./components/UpdateBanner.svelte";
  import Home from "./pages/Home.svelte";
  import Profiles from "./pages/Profiles.svelte";
  import Rules from "./pages/Rules.svelte";
  import Logs from "./pages/Logs.svelte";
  import SettingsPage from "./pages/Settings.svelte";
  import { configs, refreshAll, startListeners } from "./lib/store.js";

  const routes = {
    "/": Home,
    "/profiles": Profiles,
    "/rules": Rules,
    "/logs": Logs,
    "/settings": SettingsPage,
  };

  let showOnboard = false;
  onMount(async () => {
    await refreshAll();
    await startListeners();
    const done = localStorage.getItem("onboard_done") === "1";
    if (!done && $configs.length === 0) showOnboard = true;
  });
</script>

<div class="layout">
  <Sidebar />
  <div class="main">
    <UpdateBanner />
    <main class="content">
      <Router {routes} />
    </main>
    <StatusBar />
  </div>
</div>

{#if showOnboard}
  <Onboarding on:done={() => (showOnboard = false)} />
{/if}

<style>
  .layout {
    display: flex;
    height: 100vh;
    width: 100vw;
  }
  .main {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px;
  }
</style>
