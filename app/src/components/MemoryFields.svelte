<script lang="ts">
  import { memory } from "../lib/format";

  let {
    min,
    max,
    onchange,
  }: { min: number; max: number; onchange: (min: number, max: number) => void } = $props();

  const MAX_LIMIT = 32768;
  const pct = (v: number, lo: number, hi: number) => `${((v - lo) / (hi - lo)) * 100}%`;

  function setMax(v: number) {
    onchange(Math.min(min, v), v);
  }
  function setMin(v: number) {
    onchange(v, Math.max(max, v));
  }
</script>

<div class="memory">
  <label>
    <span class="head"><span class="label-l">Maximum</span><span class="value title-m">{memory(max)}</span></span>
    <input
      class="slider"
      type="range"
      min="1024"
      max={MAX_LIMIT}
      step="512"
      value={max}
      style:--pct={pct(max, 1024, MAX_LIMIT)}
      oninput={(e) => setMax(+e.currentTarget.value)}
    />
  </label>
  <label>
    <span class="head"><span class="label-l">Minimum</span><span class="value title-m">{memory(min)}</span></span>
    <input
      class="slider"
      type="range"
      min="256"
      max="8192"
      step="256"
      value={min}
      style:--pct={pct(min, 256, 8192)}
      oninput={(e) => setMin(+e.currentTarget.value)}
    />
  </label>
</div>

<style>
  .memory { display: grid; gap: 8px; }
  label { display: block; }
  .head { display: flex; justify-content: space-between; align-items: baseline; }
  .value { font-variant-numeric: tabular-nums; }
</style>
