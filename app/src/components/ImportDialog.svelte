<!-- Bring instances over from Prism, MultiMC, CurseForge and the official launcher. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText, LOADER_NAMES, type Foreign } from "../lib/api";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";

  const SOURCE_NAMES = { prism: "Prism Launcher", multimc: "MultiMC", curseforge: "CurseForge", official: "Minecraft Launcher" } as const;

  let found = $state<Foreign[] | null>(null);
  let picked = $state<Set<string>>(new Set());
  let busy = $state(false);
  let linked = $state(false);

  const key = (f: Foreign) => `${f.source}:${f.game_dir}:${f.name}`;

  $effect(() => {
    api.importScan().then((f) => {
      found = f;
      picked = new Set(f.filter((x) => x.source !== "official").map(key));
    }).catch((e) => app.toast(errorText(e)));
  });

  async function chooseFolder() {
    const path = await openDialog({ title: "Choose a launcher's instances folder", directory: true, multiple: false });
    if (typeof path !== "string") return;
    const more = await api.importScanFolder(path);
    if (more.length === 0) {
      app.toast("No instances found in that folder");
      return;
    }
    const known = new Set((found ?? []).map(key));
    found = [...(found ?? []), ...more.filter((m) => !known.has(key(m)))];
    picked = new Set([...picked, ...more.map(key)]);
  }

  function toggle(f: Foreign) {
    const k = key(f);
    const next = new Set(picked);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    picked = next;
  }

  async function run() {
    const items = (found ?? []).filter((f) => picked.has(key(f)));
    busy = true;
    try {
      const job = await api.importRun(items, linked);
      app.dialog = null;
      app.watchJob(job, (j) => {
        void app.dismissJob(j.id);
        if (j.state.kind === "failed") {
          app.toast(j.state.message);
          return;
        }
        if (j.state.kind !== "done") return;
        const r = j.state.result as { imported: string[]; failed: string[] };
        void app.refreshInstances();
        app.toast(r.failed.length ? `Imported ${r.imported.length}. Couldn't import: ${r.failed.join("; ")}` : `Imported ${r.imported.length} instance${r.imported.length === 1 ? "" : "s"}`);
      });
    } catch (e) {
      app.toast(errorText(e));
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && (app.dialog = null)} />
<div class="scrim" role="presentation" onclick={() => (app.dialog = null)}></div>
<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="import-title">
  <h2 class="headline" id="import-title">Import instances</h2>
  <p class="body-m muted">
    {linked
      ? "Elysium uses the other launcher's folders directly. Worlds, mods and settings are shared, so a change in one launcher shows up in the other."
      : "Worlds, mods, resource packs and settings are copied. The originals stay where they are."}
  </p>

  {#if found === null}
    <p class="body-m muted">Looking for other launchers</p>
  {:else if found.length === 0}
    <p class="body-m">Nothing found in the usual places. If you use MultiMC or keep instances elsewhere, choose the folder.</p>
  {:else}
    <ul class="group list">
      {#each found as f (key(f))}
        <li>
          <label>
            <input class="switch" type="checkbox" checked={picked.has(key(f))} onchange={() => toggle(f)} />
            <span class="info">
              <span class="title-m">{f.name}</span>
              <span class="body-s muted">{SOURCE_NAMES[f.source]}, Minecraft {f.game_version}{f.loader !== "vanilla" ? ` with ${LOADER_NAMES[f.loader]}` : ""}</span>
            </span>
          </label>
        </li>
      {/each}
    </ul>
    <label class="group share">
      <span class="info">
        <span class="title-m">Share files instead of copying</span>
        <span class="body-s muted">Instant and uses no extra space. Don't play the same instance in both launchers at once.</span>
      </span>
      <input class="switch" type="checkbox" bind:checked={linked} />
    </label>
  {/if}

  <div class="buttons">
    <button class="btn text state folder" onclick={chooseFolder}><span class="icon">folder_open</span>Choose folder</button>
    <button class="btn text state" onclick={() => (app.dialog = null)}>Cancel</button>
    <button class="btn filled state" disabled={busy || picked.size === 0} onclick={run}>Import{picked.size ? ` ${picked.size}` : ""}</button>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.32); z-index: 40; animation: fade 200ms linear; }
  @keyframes fade { from { opacity: 0; } }
  @keyframes rise { from { opacity: 0; transform: translate(-50%, calc(-50% + 24px)) scale(0.98); } }
  .dialog {
    position: fixed; z-index: 41; left: 50%; top: 50%; transform: translate(-50%, -50%);
    width: min(600px, calc(100vw - 48px)); max-height: calc(100vh - 48px); overflow-y: auto;
    padding: 28px; border-radius: 28px; background: var(--card); box-shadow: var(--shadow-float);
    display: flex; flex-direction: column; gap: 14px;
    animation: rise 350ms var(--ease-out);
  }
  .headline { font-size: 34px; }
  .list { list-style: none; margin: 0; padding: 4px 16px; background: var(--bg); max-height: 360px; overflow-y: auto; }
  li { border-top: 1px solid var(--separator); }
  li:first-child { border-top: 0; }
  label { display: flex; align-items: center; gap: 14px; padding: 12px 0; cursor: pointer; }
  .info { display: flex; flex-direction: column; min-width: 0; }
  .share { justify-content: space-between; padding: 12px 16px; background: var(--bg); }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; }
  .folder { margin-right: auto; }
</style>
