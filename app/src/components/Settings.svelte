<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, type Settings } from "../lib/api";
  import MemoryFields from "./MemoryFields.svelte";
  import JavaField from "./JavaField.svelte";
  import Row from "./Row.svelte";
  import AppearanceSettings from "./AppearanceSettings.svelte";

  const s = $derived(app.settings!);

  function set(patch: Partial<Settings>) {
    void app.saveSettings({ ...s, ...patch });
  }
</script>

<div class="scroll"><div class="page">
  <h1 class="headline big">Settings</h1>
  <p class="body-l muted intro">Defaults for every instance. Each instance can override them in its own settings.</p>

  <section>
    <h2 class="title-l">Appearance</h2>
    <div class="group">
      <AppearanceSettings />
    </div>
  </section>

  <section>
    <h2 class="title-l">Java</h2>
    <div class="group">
    <Row title="Manage Java automatically" description="Downloads the exact Java each Minecraft version was built for">
      <input class="switch" type="checkbox" checked={s.java_path === null} onchange={(e) => set({ java_path: e.currentTarget.checked ? null : "" })} />
      {#snippet below()}
        {#if s.java_path !== null}
          <JavaField path={s.java_path} onchange={(java_path) => set({ java_path })} />
        {/if}
      {/snippet}
    </Row>
    <Row title="Java arguments" description="Extra flags for every instance. Quote values that contain spaces.">
      {#snippet below()}
        <label class="field">
          <input value={s.jvm_args} placeholder=" " spellcheck="false" onchange={(e) => set({ jvm_args: e.currentTarget.value })} />
          <span class="float">Arguments</span>
        </label>
      {/snippet}
    </Row>
    </div>
  </section>

  <section>
    <h2 class="title-l">Memory</h2>
    <div class="group">
    <p class="body-m muted lead">4 to 8 GB suits most modpacks. Vanilla runs well on less.</p>
      <MemoryFields min={s.min_memory_mb} max={s.max_memory_mb} onchange={(min_memory_mb, max_memory_mb) => set({ min_memory_mb, max_memory_mb })} />
    </div>
  </section>

  <section>
    <h2 class="title-l">Game</h2>
    <div class="group">
    <Row title="Fixed window size" description="Start the game window at a set size">
      <input class="switch" type="checkbox" checked={s.resolution !== null} onchange={(e) => set({ resolution: e.currentTarget.checked ? { width: 1280, height: 720 } : null })} />
      {#snippet below()}
        {#if s.resolution}
          {@const r = s.resolution}
          <div class="size">
            <label class="field">
              <input type="number" min="320" max="7680" value={r.width} placeholder=" " onchange={(e) => set({ resolution: { ...r, width: +e.currentTarget.value || r.width } })} />
              <span class="float">Width</span>
            </label>
            <label class="field">
              <input type="number" min="240" max="4320" value={r.height} placeholder=" " onchange={(e) => set({ resolution: { ...r, height: +e.currentTarget.value || r.height } })} />
              <span class="float">Height</span>
            </label>
          </div>
        {/if}
      {/snippet}
    </Row>
    <Row title="Minimize while playing" description="Moves the launcher out of the way once the game starts">
      <input class="switch" type="checkbox" checked={s.hide_on_launch} onchange={(e) => set({ hide_on_launch: e.currentTarget.checked })} />
    </Row>
    </div>
  </section>

  <section>
    <h2 class="title-l">Downloads and storage</h2>
    <div class="group">
    <Row title="Parallel downloads" description="Lower this on slow or unreliable connections">
      <span class="title-m count">{s.concurrent_downloads}</span>
      {#snippet below()}
        <input
          class="slider"
          type="range"
          min="1"
          max="64"
          value={s.concurrent_downloads}
          style:--pct="{((s.concurrent_downloads - 1) / 63) * 100}%"
          oninput={(e) => set({ concurrent_downloads: +e.currentTarget.value })}
          aria-label="Parallel downloads"
        />
      {/snippet}
    </Row>
    <Row title="Data folder" description={app.dataDir}>
      <button class="btn outlined state" onclick={() => api.openFolder("data").catch((e) => app.toast(String(e)))}>Open</button>
    </Row>
    </div>
  </section>

  <p class="body-s muted version">Elysium Launcher {app.version}</p>
</div>
</div>

<style>
  .scroll { height: 100%; overflow-y: auto; }
  .page { padding: 56px 40px 72px 8px; max-width: 780px; }
  .big { font-size: 56px; }
  .intro { margin: 10px 0 8px; max-width: 54ch; }
  section { padding-top: 36px; }
  section h2 { margin: 0 0 12px 4px; }
  .lead { padding: 14px 0 4px; }
  .size { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; max-width: 360px; }
  .count { font-variant-numeric: tabular-nums; min-width: 2ch; text-align: right; font-weight: 700; }
  .version { margin: 40px 0 0 4px; }
  .group :global(.memory) { padding: 4px 0 16px; }
</style>
