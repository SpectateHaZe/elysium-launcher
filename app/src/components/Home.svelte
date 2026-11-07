<script lang="ts">
  import { app, isActive } from "../lib/store.svelte";
  import { glowColor, materialFor, tileColor } from "../lib/theme";
  import { theme } from "../lib/dark.svelte";
  import { duration, relative, date, bytes, span } from "../lib/format";
  import { api, fileSrc, iconSrc, LOADER_NAMES, type Fix, type Overview } from "../lib/api";
  import { installProject } from "../lib/install.svelte";
  import Block from "./Block.svelte";
  import InstanceArt from "./InstanceArt.svelte";
  import WavyProgress from "./WavyProgress.svelte";
  import { fade, fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { cubicOut } from "svelte/easing";

  let query = $state("");
  let now = $state(Date.now());

  // Grid or list; remembered on this computer only.
  const VIEW_KEY = "elysium.libraryView";
  let view = $state<"grid" | "list">(readView());
  function readView(): "grid" | "list" {
    try {
      return localStorage.getItem(VIEW_KEY) === "list" ? "list" : "grid";
    } catch {
      return "grid";
    }
  }
  function setView(v: "grid" | "list") {
    view = v;
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      // Storage unavailable: the choice lasts until restart.
    }
  }

  function loaderText(i: (typeof app.instances)[number]): string {
    return i.loader === "horizon" ? "Horizon" : i.loader === "vanilla" ? `Minecraft ${i.game_version}` : `${i.game_version}, ${LOADER_NAMES[i.loader]}`;
  }

  // Shift+F10 and the menu key open the right-click menu from the keyboard.
  function menuKey(e: KeyboardEvent, i: (typeof app.instances)[number]) {
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) app.openMenu(e, app.instanceMenu(i));
  }

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
        return p.diagnosis ? null : "The game crashed last time. The log shows what went wrong.";
      case "handed_off":
        return `Opened in the Minecraft Launcher. Choose “${inst.name}” there and press Play.`;
      case "failed":
        return p.message;
      default:
        return null;
    }
  });

  const diagnosis = $derived(session?.diagnosis ?? null);

  function fixLabel(f: Fix): string | null {
    switch (f.kind) {
      case "more_memory": return "Give it more memory";
      case "install_mod": return `Install ${f.name}`;
      case "check_mod": return `Review ${f.name}`;
      case "automatic_java": return "Use automatic Java";
      case "remove_duplicate": return "Open mods";
      case "disable_mod": return `Turn off ${f.name}`;
      case "undo_update": return "Undo the update";
      default: return null;
    }
  }

  async function applyFix(f: Fix) {
    const i = inst;
    if (!i) return;
    switch (f.kind) {
      case "more_memory":
        app.sheetOpen = true;
        app.toast("Turn on Memory under Overrides and raise the maximum");
        break;
      case "install_mod":
        app.targetId = i.id;
        await installProject({ source: "modrinth", id: f.id, slug: f.id, title: f.name, summary: "", author: "", icon_url: null, downloads: 0, updated: null, kind: "mod", categories: [] });
        break;
      case "undo_update":
        try {
          await api.snapshotRestore(i.id, f.snapshot);
          app.toast("Mods and settings are back to how they were. Press Play to try again.");
        } catch (e) {
          app.toast(String(e));
        }
        break;
      case "disable_mod":
        try {
          await api.contentSetEnabled(i.id, f.file, false);
          app.toast(`Turned off ${f.name}. Press Play to try again.`);
        } catch (e) {
          app.toast(String(e));
        }
        break;
      case "check_mod":
      case "remove_duplicate":
        app.route = { name: "content", id: i.id };
        break;
      case "automatic_java":
        await app.updateInstance(i, { overrides: { ...i.overrides, java_path: null } });
        if (app.settings?.java_path !== null && app.settings) await app.saveSettings({ ...app.settings, java_path: null });
        app.toast("Automatic Java is on. Press Play to try again.");
        break;
    }
  }

  // Stats and screenshots: refetched when the selection changes and when a session ends.
  let overview = $state<Overview | null>(null);
  $effect(() => {
    const id = inst?.id;
    void phase;
    if (!id) return;
    let live = true;
    api.instanceOverview(id).then((o) => live && (overview = o)).catch(() => live && (overview = null));
    return () => { live = false; };
  });

  const stats = $derived.by(() => {
    if (!inst) return [];
    const played = inst.playtime_secs
      ? { value: duration(inst.playtime_secs) }
      : inst.last_played
        ? { value: "Not timed", hint: "Time isn't counted when the game runs through the Minecraft Launcher" }
        : { value: "Not yet" };
    const last = inst.last_played ? relative(inst.last_played) : "Never";
    const out: { label: string; value: string; hint?: string }[] = [
      { label: "Played", ...played },
      { label: "Last played", value: last.charAt(0).toUpperCase() + last.slice(1) },
    ];
    if (overview) {
      if (inst.loader !== "vanilla" && inst.loader !== "horizon") out.push({ label: "Mods", value: String(overview.mods) });
      out.push({ label: "Worlds", value: String(overview.worlds) }, { label: "On disk", value: bytes(overview.size) });
      if (overview.startup_ms) out.push({ label: "Startup", value: span(overview.startup_ms), hint: "Time from pressing Play to the main menu, last time" });
    }
    return out;
  });

  const shots = $derived(overview?.screenshots ?? []);
  let shot = $state(0);
  const current = $derived(shots[shot % Math.max(1, shots.length)]);
  $effect(() => {
    void inst?.id;
    shot = 0;
  });
  function step(d: number) {
    shot = (shot + d + shots.length) % shots.length;
  }

  function shotName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function summary(i: NonNullable<typeof inst>): string {
    return i.last_played ? `Created ${date(i.created)}.` : `Not played yet. Created ${date(i.created)}.`;
  }
</script>

<div class="home">
  {#if inst}
    <section class="hero" class:raised={!inst.icon} aria-label={inst.name} style:--glow={glowColor(material, theme.dark)} oncontextmenu={(e) => app.openMenu(e, app.instanceMenu(inst))}>
      {#key inst.id}
      <div class="copy" in:fly={{ y: 14, duration: 420, easing: cubicOut }}>
        <p class="version">
          {#if inst.loader === "horizon"}Horizon, Minecraft {inst.game_version} in Rust{:else}Minecraft {inst.game_version}{inst.loader !== "vanilla" ? ` with ${LOADER_NAMES[inst.loader]}` : ""}{/if}
        </p>
        <h1 class="display">{inst.name}</h1>

        <div class="status" aria-live="polite">
          {#if phase === "preparing" && session}
            <p class="subhead">{session.stage || "Getting ready"}{fraction !== null ? `, ${Math.round(fraction * 100)}%` : ""}</p>
            <WavyProgress value={fraction} label="Preparing {inst.name}" />
          {:else if diagnosis}
            <div class="diagnosis" role="alert">
              <p class="title-m">{diagnosis.title}</p>
              <p class="body-m">{diagnosis.detail}</p>
              {#if diagnosis.fixes.some((f) => fixLabel(f))}
                <div class="fixes">
                  {#each diagnosis.fixes as f, n (n)}
                    {@const label = fixLabel(f)}
                    {#if label}<button class="btn tonal state" onclick={() => applyFix(f)}>{label}</button>{/if}
                  {/each}
                </div>
              {/if}
            </div>
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
          {#if inst.loader !== "horizon"}
            <button class="btn lg tonal state" onclick={() => (app.route = { name: "content", id: inst.id })}>
              <span class="icon">extension</span>Mods
            </button>
          {/if}
          <button class="btn lg tonal state" onclick={() => (app.sheetOpen = true)}>
            <span class="icon">tune</span>Settings
          </button>
          <button class="icon-btn lg tonal state" title="Game log" aria-label="Game log" onclick={() => (app.route = { name: "logs", id: inst.id })}>
            <span class="icon">terminal</span>
          </button>
          <button class="icon-btn lg tonal state" title="Open game folder" aria-label="Open game folder" onclick={() => api.openFolder("instance", inst.id).catch((e) => app.toast(String(e)))}>
            <span class="icon">folder_open</span>
          </button>
        </div>

        <dl class="stats">
          {#each stats as s (s.label)}
            <div title={s.hint}>
              <dt>{s.label}</dt>
              <dd>{s.value}</dd>
            </div>
          {/each}
        </dl>
      </div>
      {#if current}
        {@const s = current}
        {@const name = shotName(s.path)}
        <figure class="gallery" aria-label="Screenshots" in:fade={{ duration: 300 }}>
          <div class="frame">
            {#key s.path}
              <button class="pic" title="Open {name}" aria-label="Open {name}" in:fade={{ duration: 220 }} onclick={() => api.openScreenshot(inst.id, name).catch((e) => app.toast(String(e)))}>
                <img src={fileSrc(s.path)} alt="" decoding="async" />
              </button>
            {/key}
            {#if shots.length > 1}
              <button class="nav prev icon-btn state" title="Previous screenshot" aria-label="Previous screenshot" onclick={() => step(-1)}><span class="icon">chevron_left</span></button>
              <button class="nav next icon-btn state" title="Next screenshot" aria-label="Next screenshot" onclick={() => step(1)}><span class="icon">chevron_right</span></button>
            {/if}
          </div>
          <figcaption>
            <span>{relative(new Date(s.taken).toISOString()).replace(/^./, (c) => c.toUpperCase())}</span>
            {#if shots.length > 1}<span class="muted">{(shot % shots.length) + 1} of {shots.length}</span>{/if}
          </figcaption>
        </figure>
      {/if}
      {#if inst.icon}
        <!-- The pack's own picture fills the card's right side and fades into it. -->
        <div class="wash" aria-hidden="true" in:fade={{ duration: 700 }}>
          <img src={iconSrc(inst)} alt="" onerror={(e) => ((e.currentTarget as HTMLElement).hidden = true)} />
        </div>
        <div class="backdrop" aria-hidden="true" in:fade={{ duration: 520 }}>
          <img src={iconSrc(inst)} alt="" onerror={(e) => ((e.currentTarget as HTMLElement).hidden = true)} />
        </div>
      {:else}
        <div class="art" aria-hidden="true" in:fly={{ y: 24, duration: 520, easing: cubicOut }}>
          <InstanceArt {inst} size={340} radius={56} />
        </div>
      {/if}
      {/key}
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
        <h2 class="headline" id="library-title">Instances <span class="count">{app.instances.length}</span></h2>
        <div class="segmented" role="group" aria-label="Show instances as">
          <button class="state" aria-pressed={view === "grid"} title="Grid" aria-label="Grid" onclick={() => setView("grid")}><span class="icon">grid_view</span></button>
          <button class="state" aria-pressed={view === "list"} title="List" aria-label="List" onclick={() => setView("list")}><span class="icon">view_list</span></button>
        </div>
        {#if app.instances.length > 8}
          <label class="search">
            <span class="icon">search</span>
            <span class="sr-only">Search instances</span>
            <input bind:value={query} placeholder="Search instances" />
          </label>
        {/if}
        <button class="btn text state" onclick={() => (app.dialog = "import")}>
          <span class="icon">move_to_inbox</span>Import
        </button>
        <button class="btn tonal state" onclick={() => (app.dialog = "new-instance")}>
          <span class="icon">add</span>New instance
        </button>
      </header>

      {#if visible.length === 0}
        <p class="body-l muted">No instances match “{query.trim()}”.</p>
      {/if}
      {#if view === "grid"}
        <ul class="grid" in:fade={{ duration: 200 }}>
          {#each visible as i, n (i.id)}
            {@const m = materialFor(i.material, i.name)}
            {@const selected = inst?.id === i.id}
            {@const running = isActive(app.sessions[i.id])}
            <li class="cell" animate:flip={{ duration: 300, easing: cubicOut }} in:fly={{ y: 16, duration: 380, delay: Math.min(n, 12) * 28, easing: cubicOut }}>
              <button
                class="tile"
                class:selected
                aria-pressed={selected}
                title={i.name}
                onclick={() => app.open(i)}
                oncontextmenu={(e) => app.openMenu(e, app.instanceMenu(i))}
                onkeydown={(e) => menuKey(e, i)}
              >
                <span class="swatch state" style:background={tileColor(m, theme.dark)}>
                  <InstanceArt inst={i} size={84} fill radius={28} />
                  {#if running}<span class="live">Playing</span>{/if}
                  {#if i.pack?.source === "sync"}
                    <span class="badge" title="Synced from {i.pack.name}"><span class="icon">sync</span></span>
                  {:else if i.shared}
                    <span class="badge" title="Shared with friends"><span class="icon">group</span></span>
                  {/if}
                </span>
                <span class="name title-m">{i.name}</span>
                <span class="meta body-m muted">{loaderText(i)}</span>
              </button>
              {#if !running}
                <!-- Over the picture, but its own button: buttons can't nest. -->
                <span class="over">
                  <button class="quick state" title="Play {i.name}" aria-label="Play {i.name}" onclick={() => app.play(i.id)}>
                    <span class="icon filled">play_arrow</span>
                  </button>
                </span>
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <div class="table group" role="group" aria-label="Instances" in:fade={{ duration: 200 }}>
          <div class="row head" aria-hidden="true">
            <span class="c-name">Name</span>
            <span>Version</span>
            <span>Last played</span>
            <span class="c-num">Playtime</span>
          </div>
          {#each visible as i, n (i.id)}
            {@const selected = inst?.id === i.id}
            {@const running = isActive(app.sessions[i.id])}
            <button
              class="row"
              class:selected
              aria-current={selected ? "true" : undefined}
              animate:flip={{ duration: 300, easing: cubicOut }}
              in:fly={{ y: 8, duration: 300, delay: Math.min(n, 16) * 18, easing: cubicOut }}
              onclick={() => app.open(i)}
              oncontextmenu={(e) => app.openMenu(e, app.instanceMenu(i))}
              onkeydown={(e) => menuKey(e, i)}
            >
              <span class="c-name">
                <span class="thumb" style:background={tileColor(materialFor(i.material, i.name), theme.dark)}><InstanceArt inst={i} size={30} fill radius={10} /></span>
                <span class="full">{i.name}</span>
                {#if running}<span class="live inline">Playing</span>{/if}
              </span>
              <span class="muted">{loaderText(i)}</span>
              <span class="muted">{i.last_played ? relative(i.last_played) : "Never"}</span>
              <span class="muted c-num">{i.playtime_secs ? duration(i.playtime_secs) : "None"}</span>
            </button>
          {/each}
        </div>
      {/if}
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
    margin-top: 24px;
    padding: 64px 440px 56px 64px;
    min-height: 400px;
    border-radius: 32px;
    background:
      radial-gradient(ellipse 44% 72% at 83% 44%, var(--glow), transparent 72%),
      var(--card);
    box-shadow: var(--shadow-card);
  }
  .hero.raised { margin-top: 92px; }
  /* With a picture there's no block on the right, so the copy may use the space. */
  .hero:not(.raised) { padding-right: 64px; }
  .copy { max-width: 680px; display: flex; flex-direction: column; }
  .btn.lg { height: 48px; padding: 0 22px 0 18px; font-size: 16px; font-weight: 650; gap: 8px; }
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
  .diagnosis {
    padding: 14px 18px;
    border-radius: 16px;
    background: color-mix(in srgb, #ff9500 12%, var(--card));
    color: var(--label);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .fixes { display: flex; gap: 8px; flex-wrap: wrap; margin-top: 8px; }
  .lead { margin: 22px 0 36px; max-width: 34ch; color: var(--label-2); }
  .actions { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .art {
    position: absolute;
    right: 44px;
    top: -84px;
    filter: drop-shadow(0 36px 32px rgb(0 0 0 / 0.25));
  }
  .wash {
    position: absolute;
    inset: 0;
    border-radius: 32px;
    overflow: hidden;
    pointer-events: none;
  }
  .wash img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    filter: blur(48px) saturate(1.4);
    will-change: transform;
    opacity: 0.32;
    transform: scale(1.2);
    -webkit-mask-image: linear-gradient(to right, transparent 10%, #000 70%);
    mask-image: linear-gradient(to right, transparent 10%, #000 70%);
  }
  .backdrop {
    position: absolute;
    inset: 0 0 0 auto;
    width: min(52%, 820px);
    border-radius: 0 32px 32px 0;
    overflow: hidden;
    pointer-events: none;
  }
  .backdrop img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: top center;
    display: block;
    -webkit-mask-image: linear-gradient(to right, transparent, #000 55%), linear-gradient(to top, transparent, #000 30%);
    -webkit-mask-composite: source-in;
    mask-image: linear-gradient(to right, transparent, #000 55%), linear-gradient(to top, transparent, #000 30%);
    mask-composite: intersect;
  }
  .copy, .gallery { position: relative; z-index: 1; }
  @media (max-width: 1100px) {
    .hero { padding-right: 300px; }
    .art { right: 20px; top: -52px; transform: scale(0.72); transform-origin: top right; }
  }

  .stats {
    margin: 40px 0 0;
    padding-top: 22px;
    border-top: 1px solid var(--separator);
    display: flex;
    flex-wrap: wrap;
    gap: 16px 40px;
  }
  .stats div { display: flex; flex-direction: column; gap: 2px; }
  .stats dt { font-size: 13px; line-height: 18px; color: var(--label-2); }
  .stats dd { margin: 0; font-size: 22px; line-height: 28px; font-weight: 600; letter-spacing: -0.015em; font-variant-numeric: tabular-nums; }
  .stats div[title] { cursor: help; }

  /* Hero layout: copy on the left, the screenshot viewer filling the middle. */
  .hero { display: flex; align-items: center; gap: 56px; }
  .copy { flex: none; }
  .gallery { flex: 1 1 0; min-width: 0; max-width: 780px; margin: -24px auto; display: flex; flex-direction: column; gap: 10px; }
  .frame {
    position: relative;
    aspect-ratio: 16 / 9;
    padding: 6px;
    border-radius: 24px;
    background: color-mix(in srgb, var(--label) 8%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--label) 10%, transparent), 0 24px 48px rgb(0 0 0 / 0.28);
  }
  .pic {
    appearance: none;
    border: 0;
    padding: 0;
    position: absolute;
    inset: 6px;
    border-radius: 18px;
    overflow: hidden;
    background: #000;
    cursor: zoom-in;
  }
  .pic img { width: 100%; height: 100%; object-fit: cover; display: block; transition: transform 0.6s cubic-bezier(0.2, 0.8, 0.2, 1); }
  .frame:hover .pic img { transform: scale(1.025); }
  .nav {
    position: absolute;
    top: 50%;
    translate: 0 -50%;
    background: rgb(0 0 0 / 0.55);
    color: #fff;
    opacity: 0;
    transition: opacity 0.2s ease;
  }
  .prev { left: 16px; }
  .next { right: 16px; }
  .frame:hover .nav, .nav:focus-visible { opacity: 1; }
  figcaption { display: flex; justify-content: space-between; padding: 0 10px; font-size: 13px; line-height: 18px; color: var(--label-2); }
  /* Too narrow for both side by side: the viewer moves under the stats. */
  @media (max-width: 1500px) {
    .hero { flex-wrap: wrap; }
    .gallery { flex-basis: 100%; max-width: 560px; margin: 0; }
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
    /* Tiles stretch so each row reaches both edges. */
    grid-template-columns: repeat(auto-fill, minmax(164px, 1fr));
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
    box-shadow: var(--shadow-card);
    transition: transform 560ms var(--spring), box-shadow 300ms var(--ease), filter 150ms linear;
  }
  .tile:hover .swatch { transform: translateY(-6px) scale(1.02); box-shadow: 0 14px 30px rgb(0 0 0 / 0.14); }
  .tile.selected .swatch { box-shadow: 0 0 0 3px var(--bg), 0 0 0 6px var(--accent); }
  .tile:focus-visible { outline: none; }
  .tile:focus-visible .swatch { outline: 3px solid var(--accent); outline-offset: 6px; }
  /* Two lines before cutting off; the full name is in the tooltip and the list view. */
  .name {
    font-weight: 700;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
    transition: color 200ms linear;
  }
  .tile.selected .name { color: var(--accent); }
  .live {
    position: absolute;
    top: 12px;
    right: 12px;
    padding: 2px 9px 2px 20px;
    border-radius: var(--shape-full);
    font-size: 12px;
    line-height: 18px;
    font-weight: 700;
    color: #fff;
    background: rgb(0 0 0 / 0.62);
  }
  .live::before {
    content: "";
    position: absolute;
    left: 8px;
    top: 6px;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: #34c759;
    animation: pulse 1.6s ease-in-out infinite;
  }
  .badge {
    position: absolute;
    top: 10px;
    left: 10px;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: #fff;
    background: rgb(0 0 0 / 0.6);
  }
  .badge .icon { font-size: 17px; }
  .cell { position: relative; }
  .over { position: absolute; inset: 0 0 auto 0; aspect-ratio: 1; pointer-events: none; }
  .quick {
    position: absolute;
    right: 12px;
    bottom: 12px;
    width: 48px;
    height: 48px;
    border: 0;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--on-accent);
    background: var(--accent);
    box-shadow: 0 8px 20px rgb(0 0 0 / 0.35);
    cursor: pointer;
    pointer-events: auto;
    opacity: 0;
    translate: 0 8px;
    transition: opacity 180ms var(--ease), translate 420ms var(--spring), transform 420ms var(--spring);
  }
  .quick .icon { font-size: 28px; }
  .cell:hover .quick, .quick:focus-visible { opacity: 1; translate: 0 -6px; }
  .quick:hover { transform: scale(1.08); }
  @media (prefers-reduced-motion: reduce) { .quick { transition: opacity 180ms linear; translate: none; } .cell:hover .quick { translate: none; } }
  .live.inline { position: relative; top: auto; right: auto; margin-left: 4px; flex: none; }
  @keyframes pulse { 50% { opacity: 0.35; } }

  .count { color: var(--label-3); font-variant-numeric: tabular-nums; }
  header .segmented button { padding: 0 10px; }

  /* List view: full names in columns. */
  .table { padding: 6px 8px; display: flex; flex-direction: column; }
  .row {
    appearance: none;
    border: 0;
    margin: 0;
    font: inherit;
    color: inherit;
    text-align: left;
    background: none;
    display: grid;
    grid-template-columns: minmax(0, 2.4fr) minmax(0, 1.3fr) minmax(0, 1fr) 110px;
    align-items: center;
    gap: 16px;
    min-height: 52px;
    padding: 8px 12px;
    border-radius: 12px;
    cursor: pointer;
    transition: background-color 150ms linear;
  }
  .row:not(.head):hover { background: var(--fill); }
  .row.selected { background: var(--accent-soft); }
  .row.selected .full { color: var(--accent); }
  .row.head { min-height: 32px; cursor: default; font-size: 13px; font-weight: 600; color: var(--label-3); }
  .c-name { display: flex; align-items: center; gap: 12px; min-width: 0; }
  .full { font-weight: 650; overflow-wrap: anywhere; }
  .c-num { text-align: right; font-variant-numeric: tabular-nums; }
  .thumb { position: relative; flex: none; width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; overflow: hidden; }
</style>
