<!-- Browse and install mods, modpacks, resource packs, shaders and data packs. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText, LOADER_NAMES, type ContentKind, type ProjectHit, type SearchQuery, type SortOrder, type SourceFilter } from "../lib/api";
  import { installProject, pending, key, installedVersion, KIND_LABELS, compactNumber } from "../lib/install.svelte";
  import ProjectSheet from "./ProjectSheet.svelte";

  const kinds: ContentKind[] = ["mod", "modpack", "resourcepack", "shader", "datapack"];
  let kind = $state<ContentKind>("mod");
  let source = $state<SourceFilter>("all");
  let sort = $state<SortOrder>("relevance");
  let query = $state("");
  let hits = $state<ProjectHit[]>([]);
  let total = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let open = $state<ProjectHit | null>(null);
  let installed = $state<Set<string>>(new Set());
  let worldPick = $state<{ hit: ProjectHit; worlds: string[] } | null>(null);

  // Java instances only; Horizon has its own mod system.
  const javaInstances = $derived(app.instances.filter((i) => i.loader !== "horizon"));
  const target = $derived(javaInstances.find((i) => i.id === app.targetId) ?? null);

  $effect(() => {
    if (!app.targetId || !javaInstances.some((i) => i.id === app.targetId)) {
      const sel = app.selected && app.selected.loader !== "horizon" ? app.selected.id : javaInstances[0]?.id;
      app.targetId = sel ?? null;
    }
  });

  // Which projects the target already has.
  $effect(() => {
    void installedVersion.n;
    const id = app.targetId;
    if (!id) return;
    api.contentList(id).then((items) => {
      installed = new Set(items.filter((i) => i.entry.source && i.entry.project_id).map((i) => `${i.entry.source}:${i.entry.project_id}`));
    }).catch(() => {});
  });

  let seq = 0;
  async function run(append = false) {
    const mine = ++seq;
    loading = true;
    error = null;
    const q: SearchQuery = {
      query: query.trim(),
      kind,
      game_version: kind === "modpack" ? null : target?.game_version ?? null,
      loader: kind === "modpack" || !target ? null : target.loader,
      sort,
      offset: append ? hits.length : 0,
      limit: 24,
    };
    try {
      const page = await api.search(source, q);
      if (mine !== seq) return;
      hits = append ? [...hits, ...page.hits] : page.hits;
      total = page.total;
    } catch (e) {
      if (mine === seq) error = errorText(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  // Search as you type (debounced), and whenever the filters change.
  $effect(() => {
    void [kind, source, sort, app.targetId];
    const _q = query;
    const t = setTimeout(() => void run(), _q ? 300 : 0);
    return () => clearTimeout(t);
  });

  async function get(hit: ProjectHit) {
    if (hit.kind === "datapack") {
      if (!app.targetId) return;
      const worlds = await api.worlds(app.targetId);
      if (worlds.length === 0) {
        app.toast("Data packs go into a world. Play this instance once and create a world first.");
        return;
      }
      worldPick = { hit, worlds };
      return;
    }
    void installProject(hit);
  }
</script>

<div class="scroll">
  <div class="page">
    <h1 class="headline big">Discover</h1>

    <div class="toolbar">
      <label class="search wide">
        <span class="icon">search</span>
        <span class="sr-only">Search {KIND_LABELS[kind].toLowerCase()}</span>
        <input bind:value={query} placeholder="Search {KIND_LABELS[kind].toLowerCase()}" />
      </label>
      {#if kind !== "modpack"}
        <label class="target">
          <span class="body-s muted">Install into</span>
          <select bind:value={app.targetId} aria-label="Install into">
            {#each javaInstances as i (i.id)}
              <option value={i.id}>{i.name}</option>
            {/each}
          </select>
        </label>
      {/if}
    </div>

    <div class="filters">
      <div class="segmented" role="group" aria-label="Content type">
        {#each kinds as k (k)}
          <button class="state" aria-pressed={kind === k} onclick={() => (kind = k)}>{KIND_LABELS[k]}</button>
        {/each}
      </div>
      <div class="right">
        <select bind:value={source} aria-label="Source" class="pill-select">
          <option value="all">All sources</option>
          <option value="curseforge">CurseForge</option>
          <option value="modrinth">Modrinth</option>
        </select>
        <select bind:value={sort} aria-label="Sort" class="pill-select">
          <option value="relevance">Relevance</option>
          <option value="downloads">Downloads</option>
          <option value="updated">Recently updated</option>
          <option value="newest">Newest</option>
        </select>
      </div>
    </div>

    {#if kind !== "modpack" && target}
      <p class="body-m muted context">
        Showing {KIND_LABELS[kind].toLowerCase()} for Minecraft {target.game_version}{kind === "mod" && target.loader !== "vanilla" ? ` with ${LOADER_NAMES[target.loader]}` : ""}.
        {#if kind === "mod" && target.loader === "vanilla"}This instance has no mod loader, so mods won't load in it.{/if}
      </p>
    {:else if kind !== "modpack" && !target}
      <p class="body-m muted context">Create an instance to install {KIND_LABELS[kind].toLowerCase()} into.</p>
    {/if}
    {#if source !== "modrinth" && !app.curseforgeConfigured}
      <p class="body-m muted context">
        CurseForge results need an API key.
        <button class="link" onclick={() => (app.route = { name: "settings" })}>Add one in Settings</button>
      </p>
    {/if}

    {#if error}
      <p class="body-l error">{error}</p>
    {:else if !loading && hits.length === 0}
      <p class="body-l muted empty">{query.trim() ? `Nothing matches “${query.trim()}”.` : "Nothing to show yet."}</p>
    {/if}

    <ul class="results">
      {#each hits as hit (key(hit))}
        {@const busy = pending.has(key(hit))}
        {@const have = installed.has(key(hit))}
        <li>
          <button class="row" onclick={() => (open = hit)}>
            {#if hit.icon_url}
              <img class="icon-img" src={hit.icon_url} alt="" loading="lazy" referrerpolicy="no-referrer" />
            {:else}
              <span class="icon-img placeholder" aria-hidden="true">{hit.title.slice(0, 1)}</span>
            {/if}
            <span class="text">
              <span class="title-m name">{hit.title}</span>
              <span class="body-s muted meta">{hit.author ? `${hit.author}, ` : ""}{compactNumber(hit.downloads)} downloads</span>
              <span class="body-m summary">{hit.summary}</span>
            </span>
          </button>
          <div class="side">
            <span class="badge {hit.source}">{hit.source === "modrinth" ? "Modrinth" : "CurseForge"}</span>
            {#if busy}
              <span class="get busy" aria-label="Installing"><span class="spinner"></span></span>
            {:else if have}
              <span class="get done">Installed</span>
            {:else}
              <button class="get state" disabled={hit.kind !== "modpack" && !target} onclick={() => get(hit)}>Get</button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>

    {#if hits.length < total && hits.length > 0}
      <div class="more">
        <button class="btn tonal state" disabled={loading} onclick={() => run(true)}>{loading ? "Loading" : "Show more"}</button>
      </div>
    {/if}
  </div>
</div>

{#if open}
  <ProjectSheet hit={open} onclose={() => (open = null)} onget={get} installed={installed.has(key(open))} />
{/if}

{#if worldPick}
  {@const wp = worldPick}
  <div class="scrim" role="presentation" onclick={() => (worldPick = null)}></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="world-title">
    <h2 class="title-l" id="world-title">Add {wp.hit.title} to which world?</h2>
    <ul class="worlds">
      {#each wp.worlds as w (w)}
        <li>
          <button class="btn tonal state" onclick={() => { worldPick = null; void installProject(wp.hit, null, w); }}>{w}</button>
        </li>
      {/each}
    </ul>
    <div class="buttons"><button class="btn text state" onclick={() => (worldPick = null)}>Cancel</button></div>
  </div>
{/if}

<style>
  .scroll { height: 100%; overflow-y: auto; }
  .page { padding: 48px 40px 64px 8px; max-width: 1000px; }
  .big { font-size: 56px; margin-bottom: 24px; }
  .toolbar { display: flex; gap: 16px; align-items: center; }
  .search.wide { flex: 1; height: 48px; border-radius: 14px; }
  .search.wide input { font-size: 17px; }
  .target { display: flex; flex-direction: column; gap: 2px; min-width: 220px; }
  select {
    appearance: none;
    height: 36px;
    padding: 0 34px 0 14px;
    border: 0;
    border-radius: 10px;
    background: var(--fill) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 12 12'%3E%3Cpath d='M3 4.5l3 3 3-3' stroke='%2386868b' stroke-width='1.6' fill='none' stroke-linecap='round'/%3E%3C/svg%3E") no-repeat right 12px center;
    color: var(--label);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .filters { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin: 18px 0 8px; flex-wrap: wrap; }
  .right { display: flex; gap: 8px; }
  .context { margin: 6px 0 0; }
  .link { appearance: none; border: 0; background: none; padding: 0; color: var(--accent); font: inherit; font-weight: 600; cursor: pointer; }
  .error { color: #ff3b30; margin-top: 24px; }
  .empty { margin-top: 32px; }

  .results { list-style: none; margin: 20px 0 0; padding: 0; }
  li { display: flex; align-items: center; gap: 16px; padding: 14px 0; border-top: 1px solid var(--separator); }
  li:first-child { border-top: 0; }
  .row {
    appearance: none;
    border: 0;
    padding: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 16px;
    align-items: center;
    cursor: pointer;
  }
  .icon-img {
    width: 64px;
    height: 64px;
    flex: none;
    border-radius: 15px;
    object-fit: cover;
    background: var(--fill);
    box-shadow: inset 0 0 0 1px var(--separator);
  }
  .placeholder { display: grid; place-items: center; font-size: 26px; font-weight: 700; color: var(--label-2); }
  .text { display: flex; flex-direction: column; min-width: 0; gap: 2px; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .summary { color: var(--label-2); display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .row:hover .name { color: var(--accent); }
  .side { display: flex; flex-direction: column; align-items: flex-end; gap: 8px; flex: none; }
  .badge { font-size: 11px; font-weight: 700; letter-spacing: 0.01em; color: var(--label-3); }
  .badge.modrinth { color: #1bd96a; }
  .badge.curseforge { color: #f16436; }
  /* The App Store's "Get" button. */
  .get {
    appearance: none;
    border: 0;
    min-width: 76px;
    height: 30px;
    padding: 0 16px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--on-accent);
    font: inherit;
    font-size: 15px;
    font-weight: 700;
    display: grid;
    place-items: center;
    cursor: pointer;
    box-shadow: 0 2px 10px color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .get:disabled { opacity: 0.4; cursor: default; }
  .get.done { background: none; color: var(--label-3); cursor: default; font-weight: 600; }
  .spinner {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 2.5px solid var(--fill-strong);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .more { display: flex; justify-content: center; margin-top: 24px; }

  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.32); z-index: 40; }
  .dialog {
    position: fixed; z-index: 41; left: 50%; top: 50%; transform: translate(-50%, -50%);
    width: min(420px, calc(100vw - 48px)); padding: 24px; border-radius: 28px;
    background: var(--card); box-shadow: var(--shadow-float);
  }
  .worlds { list-style: none; padding: 0; margin: 16px 0; display: flex; flex-direction: column; gap: 8px; }
  .worlds li { border: 0; padding: 0; }
  .worlds .btn { width: 100%; justify-content: flex-start; }
  .buttons { display: flex; justify-content: flex-end; }
</style>
