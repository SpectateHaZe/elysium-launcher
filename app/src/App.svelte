<script lang="ts">
  import { app } from "./lib/store.svelte";
  import { applyAppearance } from "./lib/theme";
  import { theme } from "./lib/dark.svelte";
  import { api } from "./lib/api";
  import Rail from "./components/Rail.svelte";
  import Home from "./components/Home.svelte";
  import Logs from "./components/Logs.svelte";
  import Accounts from "./components/Accounts.svelte";
  import Settings from "./components/Settings.svelte";
  import InstanceSheet from "./components/InstanceSheet.svelte";
  import NewInstanceDialog from "./components/NewInstanceDialog.svelte";
  import ConfirmDialog from "./components/ConfirmDialog.svelte";
  import Snackbar from "./components/Snackbar.svelte";
  import Discover from "./components/Discover.svelte";
  import Content from "./components/Content.svelte";
  import JobsTray from "./components/JobsTray.svelte";
  import ManualDownloads from "./components/ManualDownloads.svelte";
  import MsaDialog from "./components/MsaDialog.svelte";

  void app.init();

  if (navigator.userAgent.includes("Mac")) document.documentElement.dataset.platform = "macos";

  $effect(() => {
    if (!app.settings) return;
    applyAppearance(app.settings.appearance, theme.dark);
    const root = document.documentElement;
    if (!root.classList.contains("theme-ready")) requestAnimationFrame(() => root.classList.add("theme-ready"));
  });

  const deleting = $derived(
    app.dialog && typeof app.dialog === "object" ? app.instances.find((i) => i.id === (app.dialog as { delete: string }).delete) : undefined,
  );
</script>

{#if app.fatal}
  <main class="fatal">
    <h1 class="headline">Elysium couldn't start</h1>
    <p class="body-l muted">{app.fatal}</p>
  </main>
{:else if app.ready}
  <div class="shell">
    <Rail />
    <main>
      {#if app.route.name === "home"}
        <Home />
      {:else if app.route.name === "logs"}
        <Logs id={app.route.id} />
      {:else if app.route.name === "content"}
        <Content id={app.route.id} />
      {:else if app.route.name === "discover"}
        <Discover />
      {:else if app.route.name === "accounts"}
        <Accounts />
      {:else if app.route.name === "settings"}
        <Settings />
      {/if}
    </main>
  </div>

  {#if app.sheetOpen && app.selected}
    <InstanceSheet inst={app.selected} />
  {/if}
  {#if app.dialog === "new-instance"}
    <NewInstanceDialog />
  {:else if app.dialog === "msa"}
    <MsaDialog />
  {:else if app.dialog === "quit"}
    <ConfirmDialog
      title="Quit while Minecraft is running?"
      body="The game keeps running, but this session's playtime won't be counted and any download in progress stops."
      confirm="Quit launcher"
      onconfirm={() => api.quit()}
      oncancel={() => (app.dialog = null)}
    />
  {:else if deleting}
    <ConfirmDialog
      title="Delete {deleting.name}?"
      body="This removes the instance folder, including its worlds, screenshots and settings. You can't undo this."
      confirm="Delete"
      danger
      onconfirm={() => {
        app.dialog = null;
        void app.deleteInstance(deleting.id);
      }}
      oncancel={() => (app.dialog = null)}
    />
  {/if}
  {#if app.manual}
    <ManualDownloads />
  {/if}
  <JobsTray />
  <Snackbar />
{/if}

<style>
  .shell { display: flex; height: 100%; }
  main { flex: 1; min-width: 0; height: 100%; }
  .fatal { padding: 64px; max-width: 640px; }
</style>
