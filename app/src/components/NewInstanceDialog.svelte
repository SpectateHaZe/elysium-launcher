<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { errorText, type VersionType } from "../lib/api";
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
    if (!version) return;
    busy = true;
    error = null;
    try {
      await app.createInstance(name.trim() || `Minecraft ${version}`, version, material);
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
    <span class="support">{name.trim() ? "" : `Leave empty to call it “Minecraft ${version ?? ""}”.`}</span>
  </label>

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

  <div>
    <h3 class="title-m">Color</h3>
    <div class="picker"><MaterialPicker value={material} onchange={(m) => (material = m)} /></div>
  </div>

  {#if error}<p class="body-m error" role="alert">{error}</p>{/if}

  <div class="buttons">
    <button type="button" class="btn text state" onclick={close}>Cancel</button>
    <button type="submit" class="btn filled state" disabled={!version || busy}>{busy ? "Creating" : "Create"}</button>
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
    animation: rise 350ms var(--ease-out);
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
    color: var(--md-on-surface);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .option:hover { background: var(--fill); }
  .option[aria-selected="true"] { background: color-mix(in srgb, var(--md-primary) 16%, transparent); }
  .radio { font-size: 22px; color: var(--label-3); }
  .option[aria-selected="true"] .radio { color: var(--md-primary); }
  .id { flex: 1; }
  .badge {
    font-size: 12px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: var(--shape-full);
    background: var(--md-primary);
    color: var(--md-on-primary);
  }
  .when { font-variant-numeric: tabular-nums; }
  .empty { height: 100%; display: grid; place-content: center; justify-items: center; gap: 8px; text-align: center; padding: 0 24px; }
  .picker { margin-top: 10px; }
  .error { color: #ff3b30; }
  .dialog-title { font-size: 36px; }
  .picker :global(.swatch) { width: 44px; height: 44px; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes rise { from { opacity: 0; transform: translate(-50%, calc(-50% + 24px)) scale(0.98); } }
</style>
