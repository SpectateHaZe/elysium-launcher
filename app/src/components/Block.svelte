<!-- Isometric block with procedurally generated pixel faces in the material's palette. -->
<script lang="ts">
  import { blockFaces, type Material } from "../lib/theme";

  let { material, key, size = 96 }: { material: Material; key: string; size?: number } = $props();

  const faces = $derived(blockFaces(material, key));

  // Unit cube of edge 330 centred at (512, 512): three affine-mapped 16×16 faces.
  const S = 330;
  const k = S / 16;
  const c = Math.cos(Math.PI / 6);
  const top = `matrix(${c * k} ${0.5 * k} ${-c * k} ${0.5 * k} 512 ${512 - S})`;
  const left = `matrix(${c * k} ${0.5 * k} 0 ${k} ${512 - c * S} ${512 - S / 2})`;
  const right = `matrix(${c * k} ${-0.5 * k} 0 ${k} 512 512)`;
</script>

<svg
  class="block"
  width={size}
  height={size}
  viewBox="170 170 684 684"
  role="img"
  aria-label="{material.name} block"
>
  <image href={faces.top} width="16.04" height="16.04" transform={top} image-rendering="pixelated" style="image-rendering: pixelated" />
  <image href={faces.left} width="16.04" height="16.04" transform={left} image-rendering="pixelated" style="image-rendering: pixelated" />
  <image href={faces.right} width="16.04" height="16.04" transform={right} image-rendering="pixelated" style="image-rendering: pixelated" />
</svg>

<style>
  .block {
    display: block;
    flex: none;
    image-rendering: pixelated;
    shape-rendering: crispEdges;
    overflow: visible;
  }
</style>
