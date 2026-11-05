<!-- A player's face cut from their skin texture (face plus hat layer). -->
<script lang="ts">
  let { url, size = 44, name }: { url: string | null | undefined; size?: number; name: string } = $props();
  const scale = $derived(size / 8);
</script>

{#if url}
  <span class="face" style:width="{size}px" style:height="{size}px" role="img" aria-label="{name}'s skin">
    <span class="layer" style:background-image="url({url})" style:background-size="{64 * scale}px auto" style:background-position="-{8 * scale}px -{8 * scale}px"></span>
    <span class="layer" style:background-image="url({url})" style:background-size="{64 * scale}px auto" style:background-position="-{40 * scale}px -{8 * scale}px"></span>
  </span>
{:else}
  <span class="face letter" style:width="{size}px" style:height="{size}px" aria-hidden="true">{name.slice(0, 1).toUpperCase()}</span>
{/if}

<style>
  .face { position: relative; display: inline-block; flex: none; border-radius: 10px; overflow: hidden; }
  .layer { position: absolute; inset: 0; image-rendering: pixelated; background-repeat: no-repeat; }
  .letter { display: grid; place-items: center; background: var(--fill); color: var(--label); font-weight: 800; font-size: 18px; border-radius: 50%; }
</style>
