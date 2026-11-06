<!-- Project details in tabs: description, screenshots and compatible versions. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText, type ProjectDetails, type ProjectHit, type ProjectVersion } from "../lib/api";
  import { installProject, pending, key, compactNumber } from "../lib/install.svelte";
  import { date } from "../lib/format";

  let { hit, installed, onclose, onget }: { hit: ProjectHit; installed: boolean; onclose: () => void; onget: (h: ProjectHit) => void } = $props();

  let details = $state<ProjectDetails | null>(null);
  let versions = $state<ProjectVersion[] | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    const h = hit;
    details = null;
    versions = null;
    error = null;
    api.project(h.source, h.id).then((d) => (details = d)).catch((e) => (error = errorText(e)));
    const instance = h.kind === "modpack" ? null : app.targetId;
    api.projectVersions(h.source, h.id, h.kind, instance).then((v) => (versions = v.slice(0, 30))).catch(() => (versions = []));
  });

  /** Descriptions come from the internet: show them as plain text, never as markup. */
  function plain(d: ProjectDetails): string {
    if (d.body_format === "html") {
      const doc = new DOMParser().parseFromString(d.body, "text/html");
      doc.querySelectorAll("p, br, li, h1, h2, h3, h4, div").forEach((el) => el.append("\n"));
      return (doc.body.textContent ?? "").replace(/\n{3,}/g, "\n\n").trim();
    }
    return d.body
      .replace(/<[^>]+>/g, "")
      .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
      .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/^#{1,6}\s*/gm, "")
      .replace(/[*_`~]{1,3}/g, "")
      .replace(/\n{3,}/g, "\n\n")
      .trim();
  }

  const busy = $derived(pending.has(key(hit)));

  type Tab = "about" | "gallery" | "versions";
  let tab = $state<Tab>("about");
  $effect(() => {
    void hit;
    tab = "about";
  });
  const tabs = $derived<{ id: Tab; label: string }[]>([
    { id: "about", label: "About" },
    ...(details?.gallery.length ? [{ id: "gallery" as const, label: "Gallery" }] : []),
    { id: "versions", label: versions?.length ? `Versions (${versions.length})` : "Versions" },
  ]);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="scrim" onclick={onclose} role="presentation"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-labelledby="project-title">
  <header>
    <button class="icon-btn state close" aria-label="Close" onclick={onclose}><span class="icon">close</span></button>
  </header>
  <div class="content">
    <div class="top">
      {#if hit.icon_url}
        <img class="app-icon" src={hit.icon_url} alt="" referrerpolicy="no-referrer" />
      {/if}
      <div class="who">
        <h2 class="title-l" id="project-title">{hit.title}</h2>
        <p class="body-m muted">{hit.author || (hit.source === "modrinth" ? "Modrinth" : "CurseForge")}</p>
        <div class="actions">
          {#if busy}
            <span class="btn tonal">Installing</span>
          {:else if installed}
            <span class="btn tonal">Installed</span>
          {:else}
            <button class="btn filled state" onclick={() => onget(hit)}>{hit.kind === "modpack" ? "Install as new instance" : "Get"}</button>
          {/if}
          {#if details?.website}
            <button class="btn text state" onclick={() => api.openUrl(details!.website!)}>Open website</button>
          {/if}
        </div>
      </div>
    </div>

    <dl class="facts">
      <div><dt class="body-s muted">Downloads</dt><dd class="title-m">{compactNumber(hit.downloads)}</dd></div>
      <div><dt class="body-s muted">Source</dt><dd class="title-m">{hit.source === "modrinth" ? "Modrinth" : "CurseForge"}</dd></div>
      {#if hit.updated}<div><dt class="body-s muted">Updated</dt><dd class="title-m">{date(hit.updated)}</dd></div>{/if}
    </dl>

    <div class="segmented tabs" role="group" aria-label="Project details">
      {#each tabs as t (t.id)}
        <button class="state" aria-pressed={tab === t.id} onclick={() => (tab = t.id)}>{t.label}</button>
      {/each}
    </div>

    {#key tab}
    <div class="pane">
    {#if tab === "about"}
      {#if error}
        <p class="body-m error">{error}</p>
      {:else if details}
        <p class="body-l summary">{hit.summary}</p>
        <p class="body-m description">{plain(details)}</p>
      {:else}
        <p class="body-m muted">Loading details</p>
      {/if}
    {:else if tab === "gallery" && details}
      <div class="gallery">
        {#each details.gallery.slice(0, 12) as src (src)}
          <img {src} alt="" loading="lazy" referrerpolicy="no-referrer" />
        {/each}
      </div>
    {:else}
      {#if hit.kind !== "modpack" && app.targetId}<p class="body-s muted">Versions that work with this instance</p>{/if}
      {#if versions === null}
        <p class="body-m muted">Loading versions</p>
      {:else if versions.length === 0}
        <p class="body-m muted">No versions match this instance's Minecraft version and loader.</p>
      {:else}
        <ul class="versions group">
          {#each versions as v (v.id)}
            <li>
              <div class="v">
                <span class="title-m">{v.version_number}</span>
                <span class="body-s muted">{v.release_type === "release" ? "" : `${v.release_type[0]!.toUpperCase()}${v.release_type.slice(1)}, `}{date(v.published)}, {v.game_versions.slice(0, 3).join(", ")}</span>
              </div>
              <button class="btn text state" disabled={busy} onclick={() => installProject(hit, v.id)}>Install</button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
    </div>
    {/key}
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.28); z-index: 10; animation: fade 250ms linear; }
  .sheet {
    position: fixed;
    top: 8px;
    right: 8px;
    bottom: 8px;
    width: min(560px, calc(100vw - 120px));
    display: flex;
    flex-direction: column;
    border-radius: 28px;
    background: var(--bg);
    box-shadow: var(--shadow-float);
    z-index: 11;
    animation: slide 600ms var(--spring);
  }
  header { display: flex; justify-content: flex-end; padding: 12px 12px 0; }
  .content { overflow-y: auto; padding: 0 32px 32px; display: flex; flex-direction: column; gap: 22px; }
  .top { display: flex; gap: 20px; align-items: flex-start; }
  .app-icon { width: 112px; height: 112px; border-radius: 26px; object-fit: cover; box-shadow: inset 0 0 0 1px var(--separator), var(--shadow-card); flex: none; }
  .who { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
  .who h2 { font-size: 28px; line-height: 32px; }
  .actions { display: flex; gap: 8px; margin-top: 10px; flex-wrap: wrap; }
  .facts { display: flex; gap: 32px; margin: 0; padding: 14px 0; border-top: 1px solid var(--separator); border-bottom: 1px solid var(--separator); }
  .facts dt { margin: 0; }
  .facts dd { margin: 0; }
  .tabs { align-self: flex-start; }
  .tabs button { padding: 0 14px; }
  .pane { display: flex; flex-direction: column; gap: 12px; animation: pane 360ms var(--spring); }
  @keyframes pane { from { opacity: 0; translate: 0 6px; } }
  .gallery { display: flex; flex-direction: column; gap: 12px; }
  .gallery img { width: 100%; border-radius: 14px; object-fit: cover; background: var(--fill); }
  .summary { font-weight: 600; }
  .description { white-space: pre-wrap; color: var(--label-2); user-select: text; }
  .error { color: #ff3b30; }
  .versions { list-style: none; margin: 0; padding: 4px 16px; }
  .versions li { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 0; border-top: 1px solid var(--separator); }
  .versions li:first-child { border-top: 0; }
  .v { display: flex; flex-direction: column; min-width: 0; }
  @keyframes slide { from { transform: translateX(calc(100% + 16px)); } }
  @keyframes fade { from { opacity: 0; } }
</style>
