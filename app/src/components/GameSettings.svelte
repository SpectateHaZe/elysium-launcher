<!-- Global game options: keybinds and vanilla settings new and imported instances start with. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { keyLabel, keyName, mouseName, UNBOUND, VANILLA_KEYS } from "../lib/keys";
  import Row from "./Row.svelte";

  let opts = $state<Record<string, string>>({});
  let loaded = $state(false);
  let capturing = $state<string | null>(null);
  let source = $state("");

  $effect(() => {
    api.gameOptions()
      .then((o) => { opts = o; loaded = true; })
      .catch((e) => app.toast(errorText(e)));
  });

  async function save(next: Record<string, string>) {
    opts = next;
    try {
      await api.saveGameOptions(next);
    } catch (e) {
      app.toast(errorText(e));
    }
  }

  function set(key: string, value: string | null) {
    const next = { ...opts };
    if (value === null) delete next[key];
    else next[key] = value;
    void save(next);
  }

  // Keybinds -----------------------------------------------------------------

  function bind(id: string, name: string) {
    capturing = null;
    set(`key_${id}`, name);
  }

  function onkeydown(e: KeyboardEvent) {
    if (!capturing) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      capturing = null;
      return;
    }
    const name = keyName(e);
    if (name) bind(capturing, name);
  }

  // Mouse buttons are taken window-wide while waiting. Side buttons (4 and 5) may arrive
  // only as a release, and that release would otherwise move the app back or forward.
  let swallowRelease = false;
  // The click that follows a bound left click mustn't start waiting again.
  let swallowClick = false;

  function onpress(e: MouseEvent) {
    if (!capturing) return;
    e.preventDefault();
    e.stopPropagation();
    swallowRelease = true;
    bind(capturing, mouseName(e.button));
  }

  function onclickcapture(e: MouseEvent) {
    if (!swallowClick) return;
    swallowClick = false;
    e.preventDefault();
    e.stopPropagation();
  }

  function onrelease(e: MouseEvent) {
    if (swallowRelease) {
      swallowRelease = false;
      swallowClick = true;
      setTimeout(() => (swallowClick = false));
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    if (capturing && (e.button === 3 || e.button === 4)) {
      e.preventDefault();
      e.stopPropagation();
      bind(capturing, mouseName(e.button));
    }
  }

  // Two bindings on one key are allowed in Minecraft but usually a mistake: point them out.
  const clashes = $derived.by(() => {
    const seen = new Map<string, number>();
    for (const k of VANILLA_KEYS) {
      const v = opts[`key_${k.id}`] ?? k.def;
      if (v !== UNBOUND) seen.set(v, (seen.get(v) ?? 0) + 1);
    }
    return seen;
  });

  // Options ------------------------------------------------------------------

  type Range = { key: string; label: string; min: number; max: number; step: number; def: number; toUi: (v: string) => number; fromUi: (n: number) => string; show: (n: number) => string };
  type Toggle = { key: string; label: string; description?: string; def: boolean };

  const RANGES: Range[] = [
    { key: "fov", label: "Field of view", min: 30, max: 110, step: 1, def: 70, toUi: (v) => Math.round(70 + parseFloat(v) * 40), fromUi: (n) => String((n - 70) / 40), show: (n) => (n === 70 ? "Normal" : n === 110 ? "Quake Pro" : String(n)) },
    { key: "renderDistance", label: "Render distance", min: 2, max: 32, step: 1, def: 12, toUi: (v) => parseInt(v), fromUi: String, show: (n) => `${n} chunks` },
    { key: "simulationDistance", label: "Simulation distance", min: 5, max: 32, step: 1, def: 12, toUi: (v) => parseInt(v), fromUi: String, show: (n) => `${n} chunks` },
    { key: "guiScale", label: "Interface size", min: 0, max: 4, step: 1, def: 0, toUi: (v) => parseInt(v), fromUi: String, show: (n) => (n === 0 ? "Auto" : String(n)) },
    { key: "maxFps", label: "Max frame rate", min: 10, max: 260, step: 10, def: 120, toUi: (v) => parseInt(v), fromUi: String, show: (n) => (n >= 260 ? "Unlimited" : `${n} fps`) },
    { key: "gamma", label: "Brightness", min: 0, max: 100, step: 1, def: 50, toUi: (v) => Math.round(parseFloat(v) * 100), fromUi: (n) => String(n / 100), show: (n) => (n === 0 ? "Moody" : n === 100 ? "Bright" : `${n}%`) },
    { key: "mouseSensitivity", label: "Mouse sensitivity", min: 0, max: 200, step: 1, def: 100, toUi: (v) => Math.round(parseFloat(v) * 200), fromUi: (n) => String(n / 200), show: (n) => `${n}%` },
  ];

  const TOGGLES: Toggle[] = [
    { key: "autoJump", label: "Auto-jump", def: false },
    { key: "toggleCrouch", label: "Toggle sneak", description: "Press once to sneak instead of holding the key", def: false },
    { key: "toggleSprint", label: "Toggle sprint", description: "Press once to sprint instead of holding the key", def: false },
    { key: "bobView", label: "View bobbing", def: true },
  ];

  function rangeValue(r: Range): number {
    const v = opts[r.key];
    const n = v === undefined ? NaN : r.toUi(v);
    return Number.isFinite(n) ? Math.min(r.max, Math.max(r.min, n)) : r.def;
  }

  async function copyFrom(id: string) {
    if (!id) return;
    try {
      const o = await api.gameOptionsFrom(id);
      if (Object.keys(o).length === 0) {
        app.toast("That instance hasn't saved any options yet. Play it once first.");
        return;
      }
      await save(o);
      app.toast(`Copied keybinds and options from ${app.instances.find((i) => i.id === id)?.name ?? "the instance"}`);
    } catch (e) {
      app.toast(errorText(e));
    } finally {
      source = "";
    }
  }
</script>

<svelte:window onkeydowncapture={onkeydown} onmousedowncapture={onpress} onmouseupcapture={onrelease} {onclickcapture} />

<div class="scroll"><div class="page">
  <h1 class="headline big">Game</h1>
  <p class="body-l muted intro">
    Keybinds and settings every new or imported instance starts with. Instances you already have keep their own, and mods' keybinds aren't touched.
  </p>

  <div class="group">
    <Row title="Start from an instance" description="Copy the keybinds and settings you already use in one of your instances.">
      <select bind:value={source} onchange={() => copyFrom(source)} aria-label="Copy from an instance">
        <option value="">Choose an instance</option>
        {#each app.instances.filter((i) => i.loader !== "horizon") as i (i.id)}<option value={i.id}>{i.name}</option>{/each}
      </select>
    </Row>
    {#if Object.keys(opts).length}
      <Row title="Reset everything" description="New instances then start with Minecraft's defaults.">
        <button class="btn text state" onclick={() => save({})}>Reset</button>
      </Row>
    {/if}
  </div>

  {#if loaded}
    <section>
      <h2 class="title-l">Controls</h2>
      <div class="group">
        {#each VANILLA_KEYS as k (k.id)}
          {@const value = opts[`key_${k.id}`] ?? k.def}
          {@const custom = opts[`key_${k.id}`] !== undefined && value !== k.def}
          <Row title={k.label}>
            {#if custom}
              <button class="icon-btn state" title="Use the default, {keyLabel(k.def)}" aria-label="Reset {k.label}" onclick={() => set(`key_${k.id}`, null)}><span class="icon">undo</span></button>
            {/if}
            <button
              class="key state"
              data-keep-mouse
              class:capturing={capturing === k.id}
              class:clash={(clashes.get(value) ?? 0) > 1}
              title={(clashes.get(value) ?? 0) > 1 ? "Also used by another action" : undefined}
              onclick={() => (capturing = capturing === k.id ? capturing : k.id)}
              oncontextmenu={(e) => capturing === k.id && e.preventDefault()}
            >{capturing === k.id ? "Press a key or button" : keyLabel(value)}</button>
          </Row>
        {/each}
      </div>
    </section>

    <section>
      <h2 class="title-l">Video and mouse</h2>
      <div class="group">
        {#each RANGES as r (r.key)}
          {@const n = rangeValue(r)}
          <Row title={r.label}>
            <span class="value muted">{r.show(n)}</span>
            <input type="range" min={r.min} max={r.max} step={r.step} value={n} aria-label={r.label} oninput={(e) => set(r.key, r.fromUi(+e.currentTarget.value))} />
            <button class="icon-btn state" title="Use the game's default" aria-label="Reset {r.label}" disabled={opts[r.key] === undefined} onclick={() => set(r.key, null)}><span class="icon">undo</span></button>
          </Row>
        {/each}
      </div>
    </section>

    <section>
      <h2 class="title-l">Gameplay</h2>
      <div class="group">
        {#each TOGGLES as t (t.key)}
          <Row title={t.label} description={t.description}>
            <input class="switch" type="checkbox" checked={opts[t.key] === undefined ? t.def : opts[t.key] === "true"} aria-label={t.label} onchange={(e) => set(t.key, String(e.currentTarget.checked))} />
          </Row>
        {/each}
      </div>
    </section>
  {/if}
</div></div>

<style>
  .scroll { height: 100%; overflow-y: auto; }
  .page { padding: 56px 40px 72px 8px; max-width: 780px; }
  .big { font-size: 56px; }
  .intro { margin: 10px 0 28px; max-width: 58ch; }
  section { padding-top: 36px; }
  section h2 { margin: 0 0 12px 4px; }
  .group { padding: 0 20px; }
  select { height: 36px; border-radius: 10px; border: 0; padding: 0 10px; background: var(--fill); color: var(--label); font: inherit; max-width: 260px; }
  .key {
    appearance: none;
    border: 0;
    min-width: 140px;
    height: 34px;
    padding: 0 14px;
    border-radius: 10px;
    background: var(--fill);
    color: var(--label);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .key.capturing { background: var(--accent); color: var(--on-accent, #fff); }
  .key.clash { box-shadow: inset 0 0 0 2px #ff9500; }
  .value { min-width: 84px; text-align: right; font-size: 14px; font-variant-numeric: tabular-nums; }
  input[type="range"] { width: 220px; accent-color: var(--accent); }
</style>
