<script lang="ts">
  import { app } from "../lib/store.svelte";
</script>

{#if app.snack}
  {#key app.snack.id}
    <div class="snackbar" role="status">
      <span class="body-m">{app.snack.text}</span>
      {#if app.snack.action}
        {@const action = app.snack.action}
        <button class="btn text state" onclick={() => { action.run(); app.snack = null; }}>{action.label}</button>
      {/if}
    </div>
  {/key}
{/if}

<style>
  .snackbar {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    z-index: 30;
    min-width: 280px;
    max-width: 560px;
    min-height: 48px;
    padding: 6px 8px 6px 16px;
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: space-between;
    padding: 8px 10px 8px 22px;
    border-radius: var(--shape-full);
    background: rgb(29 29 31 / 0.88);
    backdrop-filter: saturate(180%) blur(20px);
    color: #f5f5f7;
    font-weight: 600;
    box-shadow: var(--shadow-float);
    animation: in 250ms var(--ease-out);
  }
  .btn { color: var(--md-inverse-primary); }
  .snackbar :global(.body-m) { font-weight: 600; }
  @keyframes in { from { opacity: 0; transform: translate(-50%, 12px); } }
</style>
