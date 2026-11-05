<!-- Side sheet with everything about one instance. Changes save as you make them. -->
<script lang="ts">
  import { app, isActive } from "../lib/store.svelte";
  import { materialFor } from "../lib/theme";
  import { api, errorText, type Instance, type Overrides } from "../lib/api";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";

  async function exportPack() {
    const path = await saveDialog({ title: "Export as Modrinth pack", defaultPath: `${inst.name}.mrpack`, filters: [{ name: "Modrinth pack", extensions: ["mrpack"] }] });
    if (!path) return;
    try {
      await api.exportPack(inst.id, path);
      app.toast(`Exported ${inst.name}`);
    } catch (e) {
      app.toast(errorText(e));
    }
  }
  import MaterialPicker from "./MaterialPicker.svelte";
  import MemoryFields from "./MemoryFields.svelte";
  import JavaField from "./JavaField.svelte";
  import Row from "./Row.svelte";

  let { inst }: { inst: Instance } = $props();

  const running = $derived(isActive(app.sessions[inst.id]));
  const g = $derived(app.settings!);
  const o = $derived(inst.overrides);

  function set(patch: Partial<Overrides>) {
    void app.updateInstance(inst, { overrides: { ...o, ...patch } });
  }

  function close() {
    app.sheetOpen = false;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<div class="scrim" onclick={close} role="presentation"></div>
<div class="sheet" aria-labelledby="sheet-title" role="dialog" aria-modal="true">
  <header>
    <h2 class="title-l" id="sheet-title">{inst.name}</h2>
    <button class="icon-btn state" aria-label="Close" onclick={close}><span class="icon">close</span></button>
  </header>

  <div class="content">
    <label class="field">
      <input
        value={inst.name}
        placeholder=" "
        maxlength="64"
        onchange={(e) => {
          const name = e.currentTarget.value.trim();
          if (name) void app.updateInstance(inst, { name });
          else e.currentTarget.value = inst.name;
        }}
      />
      <span class="float">Name</span>
    </label>

    <section>
      <h3 class="title-m">Block</h3>
      <p class="body-m muted">The block shown for this instance.</p>
      <MaterialPicker
        value={materialFor(inst.material, inst.name).id}
        onchange={(material) => void app.updateInstance(inst, { material })}
      />
    </section>

    <section>
      <h3 class="title-m">Overrides</h3>
      <p class="body-m muted">Anything left off uses your global settings.</p>
      <div class="group">

      <Row title="Memory" description="RAM for this instance only">
        <input
          class="switch"
          type="checkbox"
          aria-label="Custom memory"
          checked={o.max_memory_mb != null}
          onchange={(e) =>
            e.currentTarget.checked
              ? set({ max_memory_mb: g.max_memory_mb, min_memory_mb: g.min_memory_mb })
              : set({ max_memory_mb: null, min_memory_mb: null })}
        />
        {#snippet below()}
          {#if o.max_memory_mb != null}
            <MemoryFields
              min={o.min_memory_mb ?? g.min_memory_mb}
              max={o.max_memory_mb}
              onchange={(min, max) => set({ min_memory_mb: min, max_memory_mb: max })}
            />
          {/if}
        {/snippet}
      </Row>

      <Row title="Java" description="A specific Java installation">
        <input
          class="switch"
          type="checkbox"
          aria-label="Custom Java"
          checked={o.java_path != null}
          onchange={(e) => set({ java_path: e.currentTarget.checked ? (g.java_path ?? "") : null })}
        />
        {#snippet below()}
          {#if o.java_path != null}
            <JavaField path={o.java_path} onchange={(java_path) => set({ java_path })} />
          {/if}
        {/snippet}
      </Row>

      <Row title="Java arguments" description="Extra flags passed to the JVM">
        <input
          class="switch"
          type="checkbox"
          aria-label="Custom Java arguments"
          checked={o.jvm_args != null}
          onchange={(e) => set({ jvm_args: e.currentTarget.checked ? g.jvm_args : null })}
        />
        {#snippet below()}
          {#if o.jvm_args != null}
            <label class="field">
              <input value={o.jvm_args} placeholder=" " spellcheck="false" onchange={(e) => set({ jvm_args: e.currentTarget.value })} />
              <span class="float">Arguments</span>
            </label>
          {/if}
        {/snippet}
      </Row>

      <Row title="Window size" description="Size of the game window at start">
        <input
          class="switch"
          type="checkbox"
          aria-label="Custom window size"
          checked={o.resolution != null}
          onchange={(e) => set({ resolution: e.currentTarget.checked ? (g.resolution ?? { width: 1280, height: 720 }) : null })}
        />
        {#snippet below()}
          {#if o.resolution}
            {@const r = o.resolution}
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
      </div>
    </section>

    <section>
      <h3 class="title-m">Maintenance</h3>
      <div class="group">
      {#if inst.loader !== "horizon"}
        <Row title="Export as a Modrinth pack" description="Share this instance as a .mrpack others can import">
          <button class="btn tonal state" onclick={exportPack}>Export</button>
        </Row>
      {/if}
      <Row title="Repair files" description="Re-checks every game file, downloads anything damaged, then starts the game">
        <button class="btn outlined state" disabled={running} onclick={() => { close(); void app.play(inst.id, true); }}>Repair</button>
      </Row>
      <Row title="Delete instance" description="Removes its worlds, screenshots and settings for good">
        <button class="btn tonal danger state" disabled={running} onclick={() => (app.dialog = { delete: inst.id })}>Delete</button>
      </Row>
      </div>
    </section>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(0 0 0 / 0.28);
    animation: fade 250ms linear;
    z-index: 10;
  }
  .sheet {
    position: fixed;
    top: 8px;
    right: 8px;
    bottom: 8px;
    width: min(440px, calc(100vw - 120px));
    display: flex;
    flex-direction: column;
    border-radius: var(--shape-xl);
    background: var(--bg);
    box-shadow: var(--shadow-float);
    z-index: 11;
    animation: slide 400ms var(--ease-out);
  }
  header { display: flex; align-items: center; justify-content: space-between; padding: 20px 16px 8px 28px; }
  .content { overflow-y: auto; padding: 12px 28px 28px; display: flex; flex-direction: column; gap: 28px; }
  section > p { margin: 2px 0 12px; }
  section > h3 { font-size: 20px; font-weight: 750; letter-spacing: -0.02em; }
  .field input { background: var(--card); }
  .size { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  @keyframes slide { from { transform: translateX(calc(100% + 16px)); } }
  @keyframes fade { from { opacity: 0; } }
</style>
