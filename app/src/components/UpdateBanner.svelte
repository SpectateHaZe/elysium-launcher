<script lang="ts">
  import { updater } from "../lib/updater.svelte";
</script>

{#if updater.available}
  <div class="banner" role="status">
    <span class="icon">system_update_alt</span>
    <div class="text">
      <span class="title-m">Elysium {updater.available.version} is ready</span>
      <span class="body-s muted">
        {#if updater.error}Couldn't update: {updater.error}{:else if updater.installing}Downloading{updater.progress !== null ? ` ${Math.round(updater.progress * 100)}%` : ""}, Elysium restarts when it's done{:else}Restart to get the latest version.{/if}
      </span>
    </div>
    <button class="btn filled state" disabled={updater.installing} onclick={() => updater.install()}>
      {updater.installing ? "Updating" : "Restart to update"}
    </button>
    {#if !updater.installing}
      <button class="icon-btn state" aria-label="Later" title="Later" onclick={() => (updater.available = null)}><span class="icon">close</span></button>
    {/if}
  </div>
{/if}

<style>
  .banner {
    position: fixed;
    top: 16px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 30;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 10px 10px 18px;
    border-radius: 20px;
    background: var(--chrome);
    backdrop-filter: saturate(180%) blur(20px);
    border: 1px solid var(--separator);
    box-shadow: var(--shadow-float);
    max-width: calc(100vw - 140px);
    animation: drop 420ms var(--ease-out);
  }
  .banner > .icon { color: var(--accent); }
  .text { display: flex; flex-direction: column; min-width: 0; }
  @keyframes drop { from { opacity: 0; transform: translate(-50%, -16px); } }
</style>
