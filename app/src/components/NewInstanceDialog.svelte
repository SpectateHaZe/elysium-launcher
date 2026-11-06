<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText, LOADER_NAMES, type Loader, type LoaderVersion, type PackImport, type VersionType } from "../lib/api";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { importPackFile } from "../lib/install.svelte";
  import { date } from "../lib/format";
  import { MATERIALS } from "../lib/theme";
  import MaterialPicker from "./MaterialPicker.svelte";

  const filters: { type: VersionType; label: string }[] = [
    { type: "release", label: "Releases" },
    { type: "snapshot", label: "Snapshots" },
    { type: "old_beta", label: "Betas" },
    { type: "old_alpha", label: "Alphas" },
  ];

  let name = $state("");
  let filter = $state<VersionType>("release");
  let query = $state("");
  let version = $state<string | null>(null);
  let material = $state(MATERIALS[Math.floor(Math.random() * MATERIALS.length)]!.id);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let loader = $state<Loader>("vanilla");
  let loaderVersions = $state<LoaderVersion[] | null>(null);
  let loaderVersion = $state<string | null>(null);
  let loaderError = $state<string | null>(null);
  let horizonMc = $state("26.3");
  let performance = $state(true);

  const loaders: Loader[] = ["vanilla", "fabric", "quilt", "forge", "neoforge"];
  const horizon = $derived(loader === "horizon");
  const mcVersion = $derived(horizon ? horizonMc : version);

  $effect(() => {
    if (horizon) void api.horizonMinecraft().then((v) => (horizonMc = v));
  });

  // Loader versions for the chosen Minecraft version.
  $effect(() => {
    const l = loader;
    const v = version;
    loaderVersions = null;
    loaderVersion = null;
    loaderError = null;
    if (l === "vanilla" || l === "horizon" || !v) return;
    let current = true;
    api.loaderVersions(l, v)
      .then((list) => {
        if (!current) return;
        loaderVersions = list;
        loaderVersion = (list.find((x) => x.stable) ?? list[0])?.version ?? null;
      })
      .catch((e) => current && (loaderError = errorText(e)));
    return () => {
      current = false;
    };
  });

  async function importPack() {
    try {
      await importPackFile();
    } catch (e) {
      error = errorText(e);
    }
  }

  const list = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return (app.manifest?.versions ?? []).filter((v) => v.type === filter && (!q || v.id.toLowerCase().includes(q)));
  });

  $effect(() => {
    if (version === null && app.manifest) version = app.manifest.latest.release;
  });

  function close() {
    if (!busy) app.dialog = null;
  }

  async function create(e: SubmitEvent) {
    e.preventDefault();
    const mc = mcVersion;
    if (!mc) return;
    if (loader !== "vanilla" && !horizon && !loaderVersion) return;
    busy = true;
    error = null;
    try {
      const fallback = horizon ? "Horizon" : loader === "vanilla" ? `Minecraft ${mc}` : `${LOADER_NAMES[loader]} ${mc}`;
      await app.createInstance(name.trim() || fallback, mc, material, loader, horizon ? null : loaderVersion);
      if (performance && (loader === "fabric" || loader === "quilt") && app.selectedId) {
        const job = await api.installPerformanceMods(app.selectedId);
        app.watchJob(job, (j) => {
          void app.dismissJob(j.id);
          if (j.state.kind === "done") app.toast(`Added ${(j.state.result as string[]).length} performance mods`);
        });
      }
      app.dialog = null;
    } catch (err) {
      error = errorText(err);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<div class="scrim" onclick={close} role="presentation"></div>
<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="new-title">
<form class="form" onsubmit={create}>
  <h2 class="headline dialog-title" id="new-title">New instance</h2>

  <label class="field">
    <!-- svelte-ignore a11y_autofocus -->
    <input bind:value={name} placeholder=" " maxlength="64" autofocus />
    <span class="float">Name</span>
    <span class="support">{name.trim() ? "" : `Leave empty to call it “${horizon ? "Horizon" : loader === "vanilla" ? `Minecraft ${version ?? ""}` : `${LOADER_NAMES[loader]} ${version ?? ""}`}”.`}</span>
  </label>

  <div class="loaders" role="radiogroup" aria-label="Loader">
    {#each loaders as l (l)}
      <button type="button" class="chip state" role="radio" aria-checked={loader === l} onclick={() => (loader = l)}>{LOADER_NAMES[l]}</button>
    {/each}
    {#if app.horizonSupported}
      <span class="divider" aria-hidden="true"></span>
      <button type="button" class="chip state horizon-chip" role="radio" aria-checked={horizon} onclick={() => (loader = "horizon")}>
        <span class="icon">bolt</span>Horizon
      </button>
    {/if}
  </div>

  {#if horizon}
    <div class="horizon group">
      <p class="title-m">Horizon, Minecraft {horizonMc} rewritten in Rust</p>
      <p class="body-m muted">
        An independent, open-source reimplementation of the game. It runs from your own Minecraft files, so it needs a
        Microsoft account that owns Java Edition. Elysium downloads it, checks its signature, and keeps it up to date.
        Java mods don't work in Horizon.
      </p>
    </div>
  {:else}
  <div class="versions">
    <div class="filters" role="group" aria-label="Version type">
      {#each filters as f (f.type)}
        <button type="button" class="chip state" aria-pressed={filter === f.type} onclick={() => (filter = f.type)}>
          {#if filter === f.type}<span class="icon">check</span>{/if}{f.label}
        </button>
      {/each}
      <label class="search">
        <span class="icon">search</span>
        <span class="sr-only">Find a version</span>
        <input bind:value={query} placeholder="Find a version" />
      </label>
    </div>

    <div class="list" role="listbox" aria-label="Minecraft version">
      {#if !app.manifest}
        <div class="empty">
          {#if app.manifestError}
            <p class="body-m">Couldn't load the version list. {app.manifestError}</p>
            <button type="button" class="btn text state" onclick={() => app.loadVersions()}>Try again</button>
          {:else}
            <p class="body-m muted">Loading versions</p>
          {/if}
        </div>
      {:else if list.length === 0}
        <div class="empty"><p class="body-m muted">No versions match “{query.trim()}”.</p></div>
      {:else}
        {#each list as v (v.id)}
          {@const selected = version === v.id}
          <button
            type="button"
            class="option state"
            role="option"
            aria-selected={selected}
            onclick={() => (version = v.id)}
            ondblclick={() => (version = v.id)}
          >
            <span class="icon radio">{selected ? "radio_button_checked" : "radio_button_unchecked"}</span>
            <span class="id title-m">{v.id}</span>
            {#if v.id === app.manifest.latest.release || v.id === app.manifest.latest.snapshot}
              <span class="badge">Latest</span>
            {/if}
            <span class="when body-m muted">{date(v.releaseTime)}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>

  {#if loader !== "vanilla"}
    <div class="loader-version">
      <span class="title-m">{LOADER_NAMES[loader]} version</span>
      {#if loaderError}
        <span class="body-m error">{loaderError}</span>
      {:else if loaderVersions === null}
        <span class="body-m muted">Loading</span>
      {:else if loaderVersions.length === 0}
        <span class="body-m muted">{LOADER_NAMES[loader]} isn't available for Minecraft {version}.</span>
      {:else}
        <select bind:value={loaderVersion} aria-label="{LOADER_NAMES[loader]} version">
          {#each loaderVersions.slice(0, 200) as lv (lv.version)}
            <option value={lv.version}>{lv.version}{lv.stable ? "" : " (beta)"}</option>
          {/each}
        </select>
      {/if}
    </div>
  {/if}
  {#if loader === "fabric" || loader === "quilt"}
    <label class="perf">
      <span>
        <span class="title-m">Add performance mods</span>
        <span class="body-s muted">Sodium, Lithium, FerriteCore and friends: more FPS, less memory.</span>
      </span>
      <input class="switch" type="checkbox" bind:checked={performance} />
    </label>
  {/if}
  {/if}

  <div>
    <h3 class="title-m">Block</h3>
    <div class="picker"><MaterialPicker value={material} onchange={(m) => (material = m)} /></div>
  </div>

  {#if error}<p class="body-m error" role="alert">{error}</p>{/if}

  <div class="buttons">
    <button type="button" class="btn text state import" onclick={importPack}><span class="icon">upload_file</span>Import pack</button>
    <button type="button" class="btn text state" onclick={close}>Cancel</button>
    <button type="submit" class="btn filled state" disabled={!mcVersion || busy || (loader !== "vanilla" && !horizon && !loaderVersion)}>{busy ? "Creating" : "Create"}</button>
  </div>
</form>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.32); z-index: 20; animation: fade 200ms linear; }
  .dialog {
    position: fixed;
    z-index: 21;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    width: min(640px, calc(100vw - 48px));
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    gap: 20px;
    padding: 28px;
    border-radius: var(--shape-xl);
    background: var(--card);
    box-shadow: var(--shadow-float);
    animation: rise 560ms var(--spring);
  }
  .form { display: contents; }
  .versions { display: flex; flex-direction: column; gap: 12px; min-height: 0; flex: 1; }
  .filters { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .filters .search { flex: 1; min-width: 160px; margin-left: 4px; }
  .list {
    height: 280px;
    overflow-y: auto;
    border-radius: 16px;
    background: var(--bg);
    padding: 6px;
  }
  .option {
    appearance: none;
    border: 0;
    width: 100%;
    height: 48px;
    padding: 0 16px 0 12px;
    border-radius: var(--shape-m);
    display: flex;
    align-items: center;
    gap: 12px;
    background: none;
    color: var(--label);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .option:hover { background: var(--fill); }
  .option[aria-selected="true"] { background: color-mix(in srgb, var(--accent) 16%, transparent); }
  .radio { font-size: 22px; color: var(--label-3); }
  .option[aria-selected="true"] .radio { color: var(--accent); }
  .id { flex: 1; }
  .badge {
    font-size: 12px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: var(--shape-full);
    background: var(--accent);
    color: var(--on-accent);
  }
  .when { font-variant-numeric: tabular-nums; }
  .empty { height: 100%; display: grid; place-content: center; justify-items: center; gap: 8px; text-align: center; padding: 0 24px; }
  .picker { margin-top: 10px; }
  .error { color: #ff3b30; }
  .dialog-title { font-size: 36px; }
  .picker :global(.swatch) { width: 44px; height: 44px; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; }
  .buttons .import { margin-right: auto; }
  .perf { display: flex; align-items: center; justify-content: space-between; gap: 16px; cursor: pointer; }
  .perf > span { display: flex; flex-direction: column; }
  .loaders { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .divider { width: 1px; height: 22px; background: var(--separator); margin: 0 4px; }
  .horizon-chip .icon { color: #ff9f0a; }
  .horizon-chip[aria-checked="true"] .icon { color: inherit; }
  .horizon { padding: 16px 20px; background: var(--bg); display: flex; flex-direction: column; gap: 6px; }
  .loader-version { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .loader-version select {
    appearance: none; height: 36px; padding: 0 34px 0 14px; border: 0; border-radius: 10px; min-width: 200px;
    background: var(--fill) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath d='M3 4.5l3 3 3-3' stroke='%2386868b' stroke-width='1.6' fill='none' stroke-linecap='round'/%3E%3C/svg%3E") no-repeat right 12px center;
    color: var(--label); font: inherit; font-weight: 600; cursor: pointer;
  }
  @keyframes fade { from { opacity: 0; } }
  @keyframes rise { from { opacity: 0; transform: translate(-50%, calc(-50% + 24px)) scale(0.98); } }
</style>
