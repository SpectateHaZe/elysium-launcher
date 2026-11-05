<!-- What's installed in an instance: enable, disable, remove, update. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText, LOADER_NAMES, type ContentKind, type ContentUpdate, type InstalledContent } from "../lib/api";
  import { installProject, installedVersion, KIND_LABELS } from "../lib/install.svelte";

  let { id }: { id: string } = $props();

  const tabs: ContentKind[] = ["mod", "resourcepack", "shader"];
  let tab = $state<ContentKind>("mod");
  let items = $state<InstalledContent[]>([]);
  let updates = $state<ContentUpdate[] | null>(null);
  let checking = $state(false);

  const inst = $derived(app.instances.find((i) => i.id === id));
  const shown = $derived(items.filter((i) => i.entry.kind === tab));

  async function load() {
    try {
      items = await api.contentList(id);
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

  function update(u: ContentUpdate) {
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

  {#if updates && updates.length}
    <div class="updates group">
      <p class="title-m">{updates.length} update{updates.length === 1 ? "" : "s"} available</p>
      {#each updates as u (u.file)}
        {@const item = items.find((i) => i.entry.file === u.file)}
        <div class="update-row">
          <span class="body-m">{item?.entry.title ?? u.file}</span>
          <span class="body-s muted">{u.current ?? "current"} to {u.latest.version_number}</span>
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
        <li class:off={!item.enabled}>
          {#if item.entry.icon_url}
            <img src={item.entry.icon_url} alt="" referrerpolicy="no-referrer" />
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
  .page { height: 100%; overflow-y: auto; padding: 32px 40px 64px 8px; max-width: 1000px; }
  header { display: flex; align-items: center; gap: 12px; margin-bottom: 24px; }
  .title { flex: 1; min-width: 0; margin-left: 4px; }
  .title h1 { font-size: 40px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { margin-left: 6px; color: var(--label-3); font-variant-numeric: tabular-nums; }
  .updates { margin-top: 20px; padding: 14px 20px; }
  .update-row { display: flex; align-items: center; gap: 12px; padding: 8px 0; border-top: 1px solid var(--separator); }
  .update-row .body-m { flex: 1; }
  .list { list-style: none; margin: 20px 0 0; padding: 4px 16px; }
  li { display: flex; align-items: center; gap: 14px; padding: 10px 0; border-top: 1px solid var(--separator); }
  li:first-child { border-top: 0; }
  li.off .info { opacity: 0.5; }
  li img, .ph { width: 40px; height: 40px; border-radius: 10px; flex: none; object-fit: cover; background: var(--fill); display: grid; place-items: center; color: var(--label-2); }
  .ph .icon { font-size: 22px; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .info .title-m { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .empty { margin-top: 40px; max-width: 52ch; display: flex; flex-direction: column; gap: 6px; }
</style>
