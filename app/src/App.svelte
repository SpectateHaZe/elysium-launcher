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
  import GameSettings from "./components/GameSettings.svelte";
  import InstanceSheet from "./components/InstanceSheet.svelte";
  import NewInstanceDialog from "./components/NewInstanceDialog.svelte";
  import ConfirmDialog from "./components/ConfirmDialog.svelte";
  import Snackbar from "./components/Snackbar.svelte";
  import Discover from "./components/Discover.svelte";
  import Content from "./components/Content.svelte";
  import JobsTray from "./components/JobsTray.svelte";
  import ManualDownloads from "./components/ManualDownloads.svelte";
  import MsaDialog from "./components/MsaDialog.svelte";
  import UpdateBanner from "./components/UpdateBanner.svelte";
  import Onboarding from "./components/Onboarding.svelte";
  import ImportDialog from "./components/ImportDialog.svelte";
  import { updater } from "./lib/updater.svelte";
  import ContextMenu from "./components/ContextMenu.svelte";
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";

  const routeKey = $derived("id" in app.route ? `${app.route.name}:${app.route.id}` : app.route.name);
  import type { MenuItem } from "./lib/store.svelte";

  type Editable = HTMLInputElement | HTMLTextAreaElement;

  function editable(t: EventTarget | null): Editable | null {
    if (t instanceof HTMLTextAreaElement) return t;
    if (t instanceof HTMLInputElement && ["text", "search", "url", "email", "number", "password", ""].includes(t.type)) return t;
    return null;
  }

  // Replaces the browser's menu everywhere. Components with their own menu handle the
  // event first; here, text fields get editing commands and selected text gets Copy.
  // Mouse back and forward buttons (and Alt+Left/Right) move through pages. The default
  // would navigate the web view itself, so it's cancelled.
  function onmouseup(e: MouseEvent) {
    // Keybind buttons take side buttons as bindings.
    if ((e.button === 3 || e.button === 4) && !(e.target as Element | null)?.closest?.("[data-keep-mouse]")) {
      e.preventDefault();
      if (e.button === 3) app.back();
      else app.forward();
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (!e.altKey || (e.key !== "ArrowLeft" && e.key !== "ArrowRight")) return;
    e.preventDefault();
    if (e.key === "ArrowLeft") app.back();
    else app.forward();
  }

  function oncontextmenu(e: MouseEvent) {
    if (e.defaultPrevented) return;
    e.preventDefault();
    const field = editable(e.target);
    const items: MenuItem[] = [];
    if (field) {
      const [start, end] = [field.selectionStart ?? 0, field.selectionEnd ?? 0];
      const selected = field.value.slice(start, end);
      const writable = !field.readOnly && !field.disabled;
      const put = (text: string) => {
        field.focus();
        field.setRangeText(text, field.selectionStart ?? start, field.selectionEnd ?? end, "end");
        field.dispatchEvent(new Event("input", { bubbles: true }));
      };
      items.push(
        { label: "Cut", icon: "content_cut", disabled: !selected || !writable || field.type === "password", run: () => void navigator.clipboard.writeText(selected).then(() => put("")) },
        { label: "Copy", icon: "content_copy", disabled: !selected || field.type === "password", run: () => void navigator.clipboard.writeText(selected) },
        {
          label: "Paste",
          icon: "content_paste",
          disabled: !writable,
          run: () => void navigator.clipboard.readText().then(put).catch(() => app.toast("Couldn't read the clipboard. Press Ctrl+V instead.")),
        },
        "separator",
        { label: "Select all", icon: "select_all", run: () => { field.focus(); field.select(); } },
      );
    } else {
      const text = window.getSelection()?.toString() ?? "";
      if (text) items.push({ label: "Copy", icon: "content_copy", run: () => void navigator.clipboard.writeText(text) });
    }
    if (items.length) app.menu = { x: e.clientX, y: e.clientY, items };
  }

  void app.init();
  // Look for a new release shortly after start, without slowing the first paint.
  setTimeout(() => void updater.check(), 4000);

  if (navigator.userAgent.includes("Mac")) document.documentElement.dataset.platform = "macos";

  $effect(() => {
    if (!app.settings) return;
    applyAppearance(app.settings.appearance, theme.dark);
    const root = document.documentElement;
    if (!root.classList.contains("theme-ready")) requestAnimationFrame(() => root.classList.add("theme-ready"));
  });

  // Discord: what you're doing in Elysium while no game runs.
  const presence = $derived.by((): [string | null, string | null] => {
    if (!app.settings?.discord_presence) return [null, null];
    const r = app.route;
    const name = (id: string) => app.instances.find((i) => i.id === id)?.name ?? null;
    switch (r.name) {
      case "home": return ["Browsing the library", `${app.instances.length} instance${app.instances.length === 1 ? "" : "s"}`];
      case "content": return ["Managing mods", name(r.id)];
      case "logs": return ["Reading a game log", name(r.id)];
      case "discover": return ["Discovering mods and modpacks", null];
      case "accounts": return ["Checking accounts", null];
      case "game": return ["Setting up keybinds", null];
      case "settings": return ["Changing settings", null];
    }
  });
  $effect(() => {
    const [details, line] = presence;
    void api.setPresence(details, line).catch(() => {});
  });

  let dontAsk = $state(false);
  const deleting = $derived(
    app.dialog && typeof app.dialog === "object" ? app.instances.find((i) => i.id === (app.dialog as { delete: string }).delete) : undefined,
  );
</script>

<svelte:window {oncontextmenu} {onmouseup} {onkeydown} />

{#if app.fatal}
  <main class="fatal">
    <h1 class="headline">Elysium couldn't start</h1>
    <p class="body-l muted">{app.fatal}</p>
  </main>
{:else if app.ready}
  <div class="shell">
    <Rail />
    <main>
      {#key routeKey}
      <div class="page" in:fly={{ y: 10, duration: 360, easing: cubicOut }}>
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
      {:else if app.route.name === "game"}
        <GameSettings />
      {:else if app.route.name === "settings"}
        <Settings />
      {/if}
      </div>
      {/key}
    </main>
  </div>

  {#if app.sheetOpen && app.selected}
    <InstanceSheet inst={app.selected} />
  {/if}
  {#if app.settings && !app.settings.onboarded && app.accounts.accounts.length === 0}
    <Onboarding />
  {/if}
  {#if app.dialog === "new-instance"}
    <NewInstanceDialog />
  {:else if app.dialog === "import"}
    <ImportDialog />
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
      body={deleting.linked_launcher
        ? `This also deletes it from ${deleting.linked_launcher}, including its worlds, mods and settings. You can't undo this.`
        : deleting.linked_dir
          ? "This removes the instance from Elysium. Its worlds, mods and settings stay in the other launcher's folder."
          : "This deletes the instance folder, including its worlds, screenshots and settings. You can't undo this."}
      confirm={deleting.linked_launcher ? "Delete everywhere" : "Delete"}
      danger
      checkbox="Don't ask again"
      bind:checked={dontAsk}
      onconfirm={() => {
        // `deleting` comes from the dialog state, so read the id before closing it.
        const id = deleting.id;
        app.dialog = null;
        if (dontAsk && app.settings) void app.saveSettings({ ...app.settings, confirm_delete: false });
        dontAsk = false;
        void app.deleteInstance(id);
      }}
      oncancel={() => (app.dialog = null)}
    />
  {/if}
  {#if app.manual}
    <ManualDownloads />
  {/if}
  <JobsTray />
  <UpdateBanner />
  <Snackbar />
  <ContextMenu />
{/if}

<style>
  .shell { display: flex; height: 100%; }
  main { flex: 1; min-width: 0; height: 100%; }
  .page { height: 100%; }
  .fatal { padding: 64px; max-width: 640px; }
</style>
