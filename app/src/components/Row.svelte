<!-- A settings row: title and supporting text on the left, a control on the right. -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, description = "", children, below }: { title: string; description?: string; children?: Snippet; below?: Snippet } = $props();
</script>

<div class="row">
  <div class="text">
    <p class="title-m">{title}</p>
    {#if description}<p class="body-m muted">{description}</p>{/if}
  </div>
  {#if children}<div class="control">{@render children()}</div>{/if}
</div>
{#if below}<div class="below">{@render below()}</div>{/if}

<style>
  .row { display: flex; align-items: center; gap: 24px; min-height: 60px; padding: 12px 0; border-top: 1px solid var(--separator); }
  .row:first-child { border-top: 0; }
  .text { flex: 1; min-width: 0; }
  .text p + p { margin-top: 2px; }
  .text .muted { font-size: 13px; line-height: 18px; }
  .control { flex: none; display: flex; align-items: center; gap: 8px; }
  .below { padding: 0 0 16px; }
</style>
