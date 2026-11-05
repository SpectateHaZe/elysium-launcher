<!-- Files a CurseForge author only allows from their website. The player downloads each
     in the browser; the launcher watches the Downloads folder and moves them in place. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api, errorText } from "../lib/api";
  import { installedVersion } from "../lib/install.svelte";

  const m = $derived(app.manual!);
  let found = $state<Set<string>>(new Set());
  let folder = $state<string | null>(null);

  $effect(() => {
    const current = m;
    const tick = async () => {
      try {
        const r = await api.manualCheck(current.instanceId, current.items);
        folder = r.downloads_dir;
        if (r.found.length !== found.size) installedVersion.bump();
        found = new Set(r.found);
      } catch (e) {
        app.toast(errorText(e));
      }
    };
    void tick();
    const t = setInterval(tick, 2000);
    return () => clearInterval(t);
  });

  const remaining = $derived(m.items.filter((i) => !found.has(i.title)).length);
</script>

<div class="scrim" role="presentation"></div>
<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="manual-title">
  <h2 class="title-l" id="manual-title">{remaining === 0 ? "All files are in place" : `Download ${remaining} file${remaining === 1 ? "" : "s"} from CurseForge`}</h2>
  <p class="body-m muted">
    {#if remaining === 0}
      Everything this instance needs is installed.
    {:else}
      These authors only allow downloads from their CurseForge page, and the files aren't on Modrinth. Open each page and download the file; Elysium picks it up from {folder ?? "your Downloads folder"} automatically.
    {/if}
  </p>
  <ul class="group">
    {#each m.items as item (item.title)}
      {@const done = found.has(item.title)}
      <li>
        <span class="icon status" class:done>{done ? "check_circle" : "download"}</span>
        <div class="info">
          <span class="title-m">{item.title}</span>
          <span class="body-s muted">{item.file_name}</span>
        </div>
        {#if !done}
          <button class="btn tonal state" onclick={() => api.openUrl(item.page_url)}>Open page</button>
        {/if}
      </li>
    {/each}
  </ul>
  <div class="buttons">
    <button class="btn {remaining === 0 ? 'filled' : 'text'} state" onclick={() => (app.manual = null)}>{remaining === 0 ? "Done" : "Finish later"}</button>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.32); z-index: 40; }
  .dialog {
    position: fixed; z-index: 41; left: 50%; top: 50%; transform: translate(-50%, -50%);
    width: min(560px, calc(100vw - 48px)); max-height: calc(100vh - 48px); overflow-y: auto;
    padding: 28px; border-radius: 28px; background: var(--card); box-shadow: var(--shadow-float);
    display: flex; flex-direction: column; gap: 14px;
  }
  ul { list-style: none; margin: 0; padding: 4px 16px; background: var(--bg); }
  li { display: flex; align-items: center; gap: 12px; padding: 12px 0; border-top: 1px solid var(--separator); }
  li:first-child { border-top: 0; }
  .status { color: var(--label-3); }
  .status.done { color: #34c759; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .buttons { display: flex; justify-content: flex-end; }
</style>
