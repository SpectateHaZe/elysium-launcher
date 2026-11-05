<script lang="ts">
  import { MATERIALS, tileColor } from "../lib/theme";
  import { theme } from "../lib/dark.svelte";
  import Block from "./Block.svelte";

  let { value, onchange }: { value: string | null; onchange: (id: string) => void } = $props();
</script>

<div class="picker" role="radiogroup" aria-label="Block">
  {#each MATERIALS as m (m.id)}
    {@const selected = value === m.id}
    <button
      class="swatch state"
      class:selected
      role="radio"
      aria-checked={selected}
      aria-label={m.name}
      title={m.name}
      style:background={tileColor(m, theme.dark)}
      onclick={() => onchange(m.id)}
    >
      <Block material={m} key="swatch" size={30} />
    </button>
  {/each}
</div>

<style>
  .picker { display: flex; flex-wrap: wrap; gap: 10px; }
  .swatch {
    appearance: none;
    border: 0;
    width: 48px;
    height: 48px;
    border-radius: var(--shape-full);
    display: grid;
    place-items: center;
    cursor: pointer;
    transition: border-radius 250ms var(--ease);
  }
  .swatch.selected {
    border-radius: 14px;
    box-shadow: 0 0 0 2px var(--card), 0 0 0 4px var(--label);
  }
</style>
