<!-- Material 3 expressive wavy linear progress. `value` null means indeterminate. -->
<script lang="ts">
  let { value, label }: { value: number | null; label: string } = $props();

  let width = $state(320);
  const H = 12;
  const WAVE = 24; // wavelength
  const AMP = 3;

  // A sine path long enough to scroll one wavelength for the travelling animation.
  const wave = $derived.by(() => {
    const len = width + WAVE * 2;
    let d = `M 0 ${H / 2}`;
    for (let x = 0; x <= len; x += 2) {
      d += ` L ${x} ${H / 2 + Math.sin((x / WAVE) * Math.PI * 2) * AMP}`;
    }
    return d;
  });
  const filled = $derived(value === null ? width * 0.35 : Math.max(0, Math.min(1, value)) * width);
</script>

<div
  class="progress"
  class:indeterminate={value === null}
  bind:clientWidth={width}
  role="progressbar"
  aria-label={label}
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={value === null ? undefined : Math.round(value * 100)}
>
  <svg {width} height={H} viewBox="0 0 {width} {H}">
    <defs>
      <clipPath id="wave-clip">
        <rect class="clip" x="0" y="0" width={filled} height={H} />
      </clipPath>
    </defs>
    {#if filled + 8 < width}
      <line class="track" x1={Math.min(width, filled + 8)} x2={width - 2} y1={H / 2} y2={H / 2} />
      <circle class="stop" cx={width - 2} cy={H / 2} r="2" />
    {/if}
    <g clip-path="url(#wave-clip)">
      <path class="wave" d={wave} />
    </g>
  </svg>
</div>

<style>
  .progress { width: 100%; height: 12px; }
  svg { display: block; overflow: visible; }
  .wave {
    fill: none;
    stroke: var(--accent);
    stroke-width: 4;
    stroke-linecap: round;
    animation: travel 1s linear infinite;
  }
  .track { stroke: var(--fill-strong); stroke-width: 4; stroke-linecap: round; }
  .stop { fill: var(--accent); }
  .clip { transition: width 300ms var(--ease); }
  .indeterminate .clip { animation: sweep 1.6s var(--ease) infinite; }
  @keyframes travel {
    to { transform: translateX(-24px); }
  }
  @keyframes sweep {
    0% { transform: translateX(-40%); }
    100% { transform: translateX(110%); }
  }
</style>
