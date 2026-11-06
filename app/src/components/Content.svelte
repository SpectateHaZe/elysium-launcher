<!-- What's installed in an instance: enable, disable, remove, update. -->
<script lang="ts">
  import { app, type MenuItem } from "../lib/store.svelte";
  import { api, errorText, fileSrc, LOADER_NAMES, type ContentKind, type ContentUpdate, type InstalledContent, type Snapshot, type StartupReport } from "../lib/api";
  import { span } from "../lib/format";
  import { installProject, installedVersion, KIND_LABELS } from "../lib/install.svelte";

  let { id }: { id: string } = $props();

  const tabs: ContentKind[] = ["mod", "resourcepack", "shader"];
  let tab = $state<ContentKind>("mod");
  let items = $state<InstalledContent[]>([]);
  let updates = $state<ContentUpdate[] | null>(null);
  let checking = $state(false);

  const inst = $derived(app.instances.find((i) => i.id === id));
  const shown = $derived(items.filter((i) => i.entry.kind === tab));

  let pending = $state<Snapshot | null>(null);
  let startup = $state<StartupReport | null>(null);

  async function load() {
    try {
      items = await api.contentList(id);
      pending = await api.snapshotPending(id);
    } catch (e) {
      app.toast(errorText(e));
    }
  }

  $effect(() => {
    api.startupReport(id).then((r) => (startup = r)).catch(() => (startup = null));
  });

  async function undo() {
    if (!pending) return;
    try {
      await api.snapshotRestore(id, pending.id);
      app.toast("Mods and settings are back to how they were before the update");
      updates = null;
      await load();
    } catch (e) {
      app.toast(errorText(e));
    }
  }

  $effect(() => {
    void installedVersion.n;
    void load();
  });

  async function toggle(item: InstalledContent) {
    try {
      await api.contentSetEnabled(id, item.entry.file, !item.enabled);
      await load();
    } catch (e) {
      app.toast(errorText(e));
    }
  }

  async function remove(item: InstalledContent) {
    try {
      await api.contentRemove(id, item.entry.file);
      app.toast(`Removed ${item.entry.title}`);
      await load();
    } catch (e) {
      app.toast(errorText(e));
    }
  }

  function pageUrl(e: InstalledContent["entry"]): string | null {
    if (!e.project_id) return null;
    if (e.source === "modrinth") return `https://modrinth.com/project/${encodeURIComponent(e.project_id)}`;
    if (e.source === "curseforge") return `https://www.curseforge.com/projects/${encodeURIComponent(e.project_id)}`;
    return null;
  }

  function menuFor(item: InstalledContent): MenuItem[] {
    const url = pageUrl(item.entry);
    return [
      { label: item.enabled ? "Disable" : "Enable", icon: item.enabled ? "toggle_off" : "toggle_on", run: () => void toggle(item) },
      ...(url ? [{ label: "Open page", icon: "open_in_new", run: () => void api.openUrl(url).catch((e) => app.toast(errorText(e))) }] : []),
      "separator",
      { label: "Remove", icon: "delete", danger: true, run: () => void remove(item) },
    ];
  }

  async function checkUpdates() {
    checking = true;
    try {
      updates = await api.contentUpdates(id);
      if (updates.length === 0) app.toast("Everything is up to date");
    } catch (e) {
      app.toast(errorText(e));
    } finally {
      checking = false;
    }
  }

  async function update(u: ContentUpdate) {
    const item = items.find((i) => i.entry.file === u.file);
    if (await restorePoint(`Updating ${item?.entry.title ?? u.latest.name}`)) start(u);
  }

  /** Updates everything listed, with one restore point covering all of it. */
  async function updateAll() {
    const all = updates ?? [];
    if (!all.length || !(await restorePoint(`Updating ${all.length} item${all.length === 1 ? "" : "s"}`))) return;
    for (const u of all) start(u);
  }

  // Save the working mods and configs first, so a bad update can be undone.
  async function restorePoint(label: string) {
    try {
      pending = await api.snapshotTake(id, label);
      return true;
    } catch (e) {
      app.toast(`Couldn't save a restore point, so nothing was updated. ${errorText(e)}`);
      return false;
    }
  }

  function start(u: ContentUpdate) {
    const item = items.find((i) => i.entry.file === u.file);
    app.targetId = id;
    void installProject(
      {
        source: "modrinth",
        id: u.latest.project_id,
        slug: u.latest.project_id,
        title: item?.entry.title ?? u.latest.name,
        summary: "",
        author: "",
        icon_url: item?.entry.icon_url ?? null,
        downloads: 0,
        updated: null,
        kind: item?.entry.kind ?? "mod",
        categories: [],
      },
      u.latest.id,
    );
    updates = (updates ?? []).filter((x) => x.file !== u.file);
  }

  function size(bytes: number) {
    return bytes > 1048576 ? `${(bytes / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`;
  }
</script>

<div class="page">
  <header>
    <button class="icon-btn state" aria-label="Back" onclick={() => (app.route = { name: "home" })}><span class="icon">arrow_back</span></button>
    <div class="title">
      <h1 class="headline">{inst?.name ?? "Instance"}</h1>
      {#if inst}
        <p class="body-m muted">Minecraft {inst.game_version}{inst.loader !== "vanilla" ? ` with ${LOADER_NAMES[inst.loader]} ${inst.loader_version ?? ""}` : ""}</p>
      {/if}
    </div>
    {#if pending}
      <button class="btn text state" title="Go back to the mods and settings from before “{pending.label}”" onclick={undo}>
        <span class="icon">undo</span>Undo update
      </button>
    {/if}
    <button class="btn tonal state" disabled={checking} onclick={checkUpdates}>
      <span class="icon">update</span>{checking ? "Checking" : "Check for updates"}
    </button>
    <button class="btn filled state" onclick={() => { app.targetId = id; app.route = { name: "discover" }; }}>
      <span class="icon">add</span>Add content
    </button>
  </header>

  <div class="segmented" role="group" aria-label="Content type">
    {#each tabs as t (t)}
      <button class="state" aria-pressed={tab === t} onclick={() => (tab = t)}>
        {KIND_LABELS[t]}<span class="count">{items.filter((i) => i.entry.kind === t).length}</span>
      </button>
    {/each}
  </div>

  {#if tab === "mod" && startup}
    <div class="startup group">
      <p class="title-m">Reached the main menu in {span(startup.total_ms)}</p>
      {#if startup.mods.length}
        <p class="body-s muted">Mods that took the longest to load, estimated from the game log</p>
        <ol>
          {#each startup.mods as m (m.name)}
            <li><span class="body-m">{m.name}</span><span class="bar" style:--w="{Math.round((m.ms / (startup.mods[0]?.ms ?? m.ms)) * 100)}%"></span><span class="body-s muted num">{span(m.ms)}</span></li>
          {/each}
        </ol>
      {:else}
        <p class="body-s muted">No single mod stood out.</p>
      {/if}
    </div>
  {/if}

  {#if updates && updates.length}
    <div class="updates group">
      <div class="updates-head">
        <p class="title-m">{updates.length} update{updates.length === 1 ? "" : "s"} available</p>
        <button class="btn filled state" onclick={updateAll}>Update all</button>
      </div>
      {#each updates as u (u.file)}
        {@const item = items.find((i) => i.entry.file === u.file)}
        <div class="update-row">
          <span class="body-m">{item?.entry.title ?? u.file}</span>
          <span class="body-s muted">{u.current ? `${u.current} to ${u.latest.version_number}` : `New version ${u.latest.version_number}`}</span>
          <button class="btn text state" onclick={() => update(u)}>Update</button>
        </div>
      {/each}
    </div>
  {/if}

  {#if shown.length === 0}
    <div class="empty">
      <p class="title-m">No {KIND_LABELS[tab].toLowerCase()} yet</p>
      <p class="body-m muted">
        {tab === "mod" && inst?.loader === "vanilla"
          ? "This instance has no mod loader. Create a Fabric, Quilt, Forge or NeoForge instance to use mods."
          : `Find ${KIND_LABELS[tab].toLowerCase()} in Discover, or drop files into the instance's ${tab === "mod" ? "mods" : tab === "shader" ? "shaderpacks" : "resourcepacks"} folder.`}
      </p>
    </div>
  {:else}
    <ul class="list group">
      {#each shown as item (item.entry.file)}
        <li class:off={!item.enabled} oncontextmenu={(e) => app.openMenu(e, menuFor(item))}>
          {#if item.entry.icon_url || item.local_icon}
            <img src={item.entry.icon_url ?? fileSrc(item.local_icon ?? "")} alt="" referrerpolicy="no-referrer" loading="lazy" class:pixel={!item.entry.icon_url} />
          {:else}
            <span class="ph" aria-hidden="true"><span class="icon">{tab === "mod" ? "extension" : tab === "shader" ? "wb_sunny" : "palette"}</span></span>
          {/if}
          <div class="info">
            <span class="title-m">{item.entry.title}</span>
            <span class="body-s muted">
              {item.entry.version_number ? `${item.entry.version_number}, ` : ""}{size(item.size)}{item.entry.source ? `, from ${item.entry.source === "modrinth" ? "Modrinth" : "CurseForge"}` : ""}
            </span>
          </div>
          <input class="switch" type="checkbox" checked={item.enabled} aria-label="Enable {item.entry.title}" onchange={() => toggle(item)} />
          <button class="icon-btn state" title="Remove" aria-label="Remove {item.entry.title}" onclick={() => remove(item)}><span class="icon">delete</span></button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .page { height: 100%; overflow-y: auto; padding: 32px 40px 64px 8px; }
  header { display: flex; align-items: center; gap: 12px; margin-bottom: 24px; }
  .title { flex: 1; min-width: 0; margin-left: 4px; }
  .title h1 { font-size: 40px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { margin-left: 6px; color: var(--label-3); font-variant-numeric: tabular-nums; }
  .updates { margin-top: 20px; padding: 14px 20px; }
  .startup { margin-top: 20px; padding: 14px 20px; display: flex; flex-direction: column; gap: 4px; }
  .startup ol { list-style: none; margin: 8px 0 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .startup ol li { display: grid; grid-template-columns: minmax(120px, 220px) 1fr 76px; align-items: center; gap: 12px; padding: 0; border: 0; }
  .startup .bar { height: 6px; border-radius: 3px; background: linear-gradient(var(--accent), var(--accent)) 0 0 / var(--w) 100% no-repeat, var(--fill); }
  .startup .num { text-align: right; font-variant-numeric: tabular-nums; }
  .updates-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding-bottom: 8px; }
  .update-row { display: flex; align-items: center; gap: 12px; padding: 8px 0; border-top: 1px solid var(--separator); }
  .update-row .body-m { flex: 1; }
  .list { list-style: none; margin: 20px 0 0; padding: 4px 16px; }
  li { display: flex; align-items: center; gap: 14px; padding: 10px 0; border-top: 1px solid var(--separator); }
  li:first-child { border-top: 0; }
  li.off .info { opacity: 0.5; }
  li img, .ph { width: 40px; height: 40px; border-radius: 10px; flex: none; object-fit: cover; background: var(--fill); display: grid; place-items: center; color: var(--label-2); }
  .ph .icon { font-size: 22px; }
  /* Mod logos inside jars are often tiny pixel art. */
  li img.pixel { image-rendering: pixelated; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .info .title-m { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { margin-top: 40px; max-width: 52ch; display: flex; flex-direction: column; gap: 6px; }
</style>
