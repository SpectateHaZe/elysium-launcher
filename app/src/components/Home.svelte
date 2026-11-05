<script lang="ts">
  import { app, isActive } from "../lib/store.svelte";
  import { glowColor, materialFor, tileColor } from "../lib/theme";
  import { theme } from "../lib/dark.svelte";
  import { duration, relative, date } from "../lib/format";
  import { api } from "../lib/api";
  import Block from "./Block.svelte";
  import WavyProgress from "./WavyProgress.svelte";

  let query = $state("");
  let now = $state(Date.now());

  const inst = $derived(app.selected);
  const material = $derived(materialFor(inst?.material, inst?.name));
  const session = $derived(inst ? app.sessions[inst.id] : undefined);
  const phase = $derived(session?.phase.kind);
  const fraction = $derived(session && session.total > 0 ? session.done / session.total : null);

  const visible = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return app.instances;
    return app.instances.filter((i) => i.name.toLowerCase().includes(q) || i.game_version.includes(q));
  });

  // Live "playing for" counter while the game runs.
  $effect(() => {
    if (phase !== "running") return;
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });

  const status = $derived.by(() => {
    if (!inst || !session) return null;
    const p = session.phase;
    switch (p.kind) {
      case "running":
        return `Playing now, ${duration(Math.max(0, Math.floor((now - Date.parse(p.since)) / 1000)))} so far.`;
      case "crashed":
        return "The game crashed last time. The log shows what went wrong.";
      case "failed":
        return p.message;
      default:
        return null;
    }
  });

  function summary(i: NonNullable<typeof inst>): string {
    if (!i.last_played) return `Not played yet. Created ${date(i.created)}.`;
    return `Played ${duration(i.playtime_secs)} in total, most recently ${relative(i.last_played)}.`;
  }
</script>

<div class="home">
  {#if inst}
    <section class="hero" aria-label={inst.name} style:--glow={glowColor(material, theme.dark)}>
      <div class="copy">
        <p class="version">Minecraft {inst.game_version}</p>
        <h1 class="display">{inst.name}</h1>

        <div class="status" aria-live="polite">
          {#if phase === "preparing" && session}
            <p class="subhead">{session.stage || "Getting ready"}{fraction !== null ? `, ${Math.round(fraction * 100)}%` : ""}</p>
            <WavyProgress value={fraction} label="Preparing {inst.name}" />
          {:else if status}
            <p class="subhead" class:error={phase === "failed" || phase === "crashed"}>{status}</p>
          {:else}
            <p class="subhead">{summary(inst)}</p>
          {/if}
        </div>

        <div class="actions">
          {#if phase === "preparing"}
            <button class="btn xl tonal state" onclick={() => app.stop(inst.id)}>
              <span class="icon">close</span>Cancel
            </button>
          {:else if phase === "running"}
            <button class="btn xl filled state" onclick={() => app.stop(inst.id)}>
              <span class="icon filled">stop</span>Stop
            </button>
          {:else}
            <button class="btn xl filled state" onclick={() => app.play(inst.id)}>
              <span class="icon filled">play_arrow</span>Play
            </button>
          {/if}
          <button class="icon-btn lg tonal state" title="Instance settings" aria-label="Instance settings" onclick={() => (app.sheetOpen = true)}>
            <span class="icon">tune</span>
          </button>
          <button class="icon-btn lg tonal state" title="Game log" aria-label="Game log" onclick={() => (app.route = { name: "logs", id: inst.id })}>
            <span class="icon">terminal</span>
          </button>
          <button class="icon-btn lg tonal state" title="Open game folder" aria-label="Open game folder" onclick={() => api.openFolder("instance", inst.id).catch((e) => app.toast(String(e)))}>
            <span class="icon">folder_open</span>
          </button>
        </div>
      </div>
      <div class="art" aria-hidden="true">
        <Block {material} key={inst.id} size={340} />
      </div>
    </section>
  {:else}
    <section class="hero empty" style:--glow={glowColor(materialFor("grass"), theme.dark)}>
      <div class="copy">
        <h1 class="display">Pick a version to play</h1>
        <p class="subhead lead">Any Minecraft release, snapshot, beta or alpha. Elysium downloads the game and the right Java for it.</p>
        <div class="actions">
          <button class="btn xl filled state" onclick={() => (app.dialog = "new-instance")}>
            <span class="icon">add</span>New instance
          </button>
        </div>
      </div>
      <div class="art" aria-hidden="true">
        <Block material={materialFor("grass")} key="welcome" size={340} />
      </div>
    </section>
  {/if}

  {#if app.instances.length > 0}
    <section class="library" aria-labelledby="library-title">
      <header>
        <h2 class="headline" id="library-title">Instances</h2>
        {#if app.instances.length > 8}
          <label class="search">
            <span class="icon">search</span>
            <span class="sr-only">Search instances</span>
            <input bind:value={query} placeholder="Search instances" />
          </label>
        {/if}
        <button class="btn tonal state" onclick={() => (app.dialog = "new-instance")}>
          <span class="icon">add</span>New instance
        </button>
      </header>

      {#if visible.length === 0}
        <p class="body-l muted">No instances match “{query.trim()}”.</p>
      {/if}
      <ul class="grid">
        {#each visible as i (i.id)}
          {@const m = materialFor(i.material, i.name)}
          {@const selected = inst?.id === i.id}
          {@const running = isActive(app.sessions[i.id])}
          <li>
            <button
              class="tile"
              class:selected
              aria-pressed={selected}
              onclick={() => app.select(i.id)}
              ondblclick={() => app.play(i.id)}
            >
              <span class="swatch state" style:background={tileColor(m, theme.dark)}>
                <Block material={m} key={i.id} size={84} />
                {#if running}<span class="live" title="Running"></span>{/if}
              </span>
              <span class="name title-m">{i.name}</span>
              <span class="meta body-m muted">Minecraft {i.game_version}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}
</div>

<style>
  .home {
    height: 100%;
    overflow-y: auto;
    padding: 16px 40px 56px 8px;
  }

  /* The stage: a neutral surface lit in the instance's colour, with the block
     rising out of the top edge like a product shot. */
  .hero {
    position: relative;
    margin-top: 92px;
    padding: 64px 440px 56px 64px;
    min-height: 400px;
    border-radius: 32px;
    background:
      radial-gradient(ellipse 44% 72% at 83% 44%, var(--glow), transparent 72%),
      var(--card);
    box-shadow: var(--shadow-card);
  }
  .copy { max-width: 680px; display: flex; flex-direction: column; }
  .version {
    margin: 0 0 14px;
    font-size: 19px;
    line-height: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--accent);
  }
  .display { overflow-wrap: anywhere; }
  .status { margin: 22px 0 36px; min-height: 56px; display: flex; flex-direction: column; gap: 16px; max-width: 600px; color: var(--label-2); }
  .status .error { color: #ff3b30; }
  .lead { margin: 22px 0 36px; max-width: 34ch; color: var(--label-2); }
  .actions { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .art {
    position: absolute;
    right: 44px;
    top: -84px;
    filter: drop-shadow(0 36px 32px rgb(0 0 0 / 0.25));
  }
  @media (max-width: 1100px) {
    .hero { padding-right: 300px; }
    .art { right: 20px; top: -52px; transform: scale(0.72); transform-origin: top right; }
  }

  .library { margin-top: 64px; }
  header { display: flex; align-items: center; gap: 16px; margin-bottom: 24px; }
  header h2 { flex: 1; }
  header .search { width: 240px; }

  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(156px, 184px));
    gap: 32px 24px;
  }
  .tile {
    appearance: none;
    border: 0;
    padding: 0;
    width: 100%;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .swatch {
    position: relative;
    aspect-ratio: 1;
    display: grid;
    place-items: center;
    border-radius: 28px;
    margin-bottom: 14px;
    transition: transform 250ms var(--ease), box-shadow 200ms linear, filter 150ms linear;
  }
  .tile:hover .swatch { transform: scale(1.02); }
  .tile.selected .swatch { box-shadow: 0 0 0 3px var(--bg), 0 0 0 6px var(--label); }
  .tile:focus-visible { outline: none; }
  .tile:focus-visible .swatch { outline: 3px solid var(--accent); outline-offset: 6px; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 700; }
  .live {
    position: absolute;
    top: 16px;
    right: 16px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #34c759;
    box-shadow: 0 0 0 3px color-mix(in srgb, #34c759 30%, transparent);
  }
</style>
