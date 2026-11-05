<!-- Downloads in progress, bottom right, like a browser's download list. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { api } from "../lib/api";

  const running = $derived(app.jobs.filter((j) => j.state.kind === "running"));
</script>

{#if running.length}
  <div class="tray" role="status" aria-label="Downloads in progress">
    {#each running as job (job.id)}
      {@const frac = job.total > 0 ? Math.min(1, job.done / job.total) : null}
      <div class="job">
        <div class="head">
          <span class="label-l name">{job.title}</span>
          <button class="icon-btn state small" aria-label="Cancel {job.title}" onclick={() => api.jobCancel(job.id)}><span class="icon">close</span></button>
        </div>
        <span class="body-s muted stage">{job.stage || "Starting"}</span>
        <div class="bar" class:indeterminate={frac === null}><span style:width={frac === null ? "30%" : `${frac * 100}%`}></span></div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .tray {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 25;
    width: 320px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .job {
    padding: 12px 10px 12px 16px;
    border-radius: 16px;
    background: var(--chrome);
    backdrop-filter: saturate(180%) blur(20px);
    box-shadow: var(--shadow-float);
    border: 1px solid var(--separator);
  }
  .head { display: flex; align-items: center; gap: 8px; }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .small { width: 28px; height: 28px; }
  .small .icon { font-size: 18px; }
  .stage { display: block; margin: 0 6px 8px 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bar { height: 4px; border-radius: 2px; background: var(--fill-strong); overflow: hidden; margin-right: 6px; }
  .bar span { display: block; height: 100%; background: var(--accent); border-radius: 2px; transition: width 300ms var(--ease); }
  .indeterminate span { animation: slide 1.2s var(--ease) infinite; }
  @keyframes slide { from { transform: translateX(-100%); } to { transform: translateX(340%); } }
</style>
