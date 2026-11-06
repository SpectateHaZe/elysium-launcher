<!-- An instance's picture, or its block when it has none (or the picture won't load). -->
<script lang="ts">
  import { iconSrc, type Instance } from "../lib/api";
  import { materialFor } from "../lib/theme";
  import Block from "./Block.svelte";

  /** `fill` stretches a picture over its container; the block keeps `size`. */
  let { inst, size, radius = 0, fill = false }: { inst: Instance; size: number; radius?: number; fill?: boolean } = $props();

  const src = $derived(iconSrc(inst));
  let broken = $state<string | null>(null);
</script>

{#if src && broken !== src}
  <img {src} alt="" width={size} height={size} class:fill style:border-radius="{radius}px" onerror={() => (broken = src)} />
{:else}
  <Block material={materialFor(inst.material, inst.name)} key={inst.id} {size} />
{/if}

<style>
  img { display: block; flex: none; object-fit: cover; }
  img.fill { position: absolute; inset: 0; width: 100%; height: 100%; }
</style>
