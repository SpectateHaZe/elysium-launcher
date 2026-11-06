<!-- Custom theme editor: a side panel that previews the theme live while you edit it. -->
<script lang="ts">
  import { fly } from "svelte/transition";
  import { cubicOut } from "svelte/easing";
  import { app } from "../lib/store.svelte";
  import { theme } from "../lib/dark.svelte";
  import { hexToHsv, hsvToHex, isHex } from "../lib/color";

  const DEFAULTS = { custom_colors: ["#ff8cc8", "#a98bff"], custom_angle: 235, custom_intensity: 40 };
  const MAX = 5;

  const draft = $derived(app.themePreview!);
  const a = $derived(draft.appearance);
  const colors = $derived(a.custom_colors);

  let sel = $state(0);
  // Kept apart from the hex so dragging through grey or black doesn't lose the hue.
  let hsv = $state<[number, number, number]>(hexToHsv(app.themePreview!.appearance.custom_colors[0] ?? "#ff8cc8"));
  let hexText = $state("");
  $effect(() => {
    hexText = (colors[sel] ?? "").toUpperCase().slice(1);
  });

  function patch(p: Partial<typeof a>) {
    app.themePreview = { ...draft, appearance: { ...a, ...p } };
  }

  function select(i: number) {
    sel = i;
    hsv = hexToHsv(colors[i]!);
  }

  function setColor(hex: string, fromPicker = false) {
    const next = [...colors];
    next[sel] = hex.toLowerCase();
    patch({ custom_colors: next });
    if (!fromPicker) hsv = hexToHsv(hex);
  }

  function setHsv(h: number, s: number, v: number) {
    hsv = [h, s, v];
    setColor(hsvToHex(h, s, v), true);
  }

  const clamp = (n: number) => Math.min(1, Math.max(0, n));

  // Dragging on the saturation square and the hue bar.
  function drag(e: PointerEvent, apply: (x: number, y: number) => void) {
    const el = e.currentTarget as HTMLElement;
    const at = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      apply(clamp((ev.clientX - r.left) / r.width), clamp((ev.clientY - r.top) / r.height));
    };
    el.setPointerCapture(e.pointerId);
    at(e);
    const up = () => {
      el.removeEventListener("pointermove", at);
      el.removeEventListener("pointerup", up);
    };
    el.addEventListener("pointermove", at);
    el.addEventListener("pointerup", up);
  }

  // Arrow keys move the pickers too (Shift for bigger steps).
  function arrows(e: KeyboardEvent, apply: (dx: number, dy: number) => void) {
    const d = ({ ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] } as Record<string, [number, number]>)[e.key];
    if (!d) return;
    e.preventDefault();
    const step = e.shiftKey ? 10 : 1;
    apply(d[0] * step, d[1] * step);
  }

  function typedHex(value: string) {
    hexText = value;
    const hex = `#${value.replace(/^#/, "")}`;
    if (isHex(hex)) setColor(hex);
  }

  const canPick = "EyeDropper" in window;
  async function pick() {
    try {
      // @ts-expect-error EyeDropper isn't in TypeScript's DOM types yet.
      const r: { sRGBHex: string } = await new window.EyeDropper().open();
      if (isHex(r.sRGBHex)) setColor(r.sRGBHex);
    } catch {
      // Cancelled with Escape.
    }
  }

  function addColor() {
    if (colors.length >= MAX) return;
    const [h, s, v] = hexToHsv(colors[colors.length - 1]!);
    const hex = hsvToHex((h + 40) % 360, Math.max(s, 0.35), Math.max(v, 0.75));
    patch({ custom_colors: [...colors, hex] });
    sel = colors.length;
    hsv = hexToHsv(hex);
  }

  function removeColor() {
    if (colors.length <= 1) return;
    const next = colors.filter((_, i) => i !== sel);
    const i = Math.min(sel, next.length - 1);
    patch({ custom_colors: next });
    sel = i;
    hsv = hexToHsv(next[i]!);
  }

  function surprise() {
    const n = 2 + Math.floor(Math.random() * 2);
    const start = Math.random() * 360;
    const next = Array.from({ length: n }, (_, i) =>
      hsvToHex((start + i * (40 + Math.random() * 50)) % 360, 0.45 + Math.random() * 0.35, 0.85 + Math.random() * 0.15),
    );
    patch({ custom_colors: next, custom_angle: Math.round(Math.random() * 359), custom_intensity: 25 + Math.round(Math.random() * 45) });
    sel = 0;
    hsv = hexToHsv(next[0]!);
  }

  const isDefault = $derived(
    a.custom_angle === DEFAULTS.custom_angle && a.custom_intensity === DEFAULTS.custom_intensity && colors.join() === DEFAULTS.custom_colors.join(),
  );
  function reset() {
    patch({ ...DEFAULTS, custom_colors: [...DEFAULTS.custom_colors] });
    sel = 0;
    hsv = hexToHsv(DEFAULTS.custom_colors[0]!);
  }

  function close() {
    app.themePreview = null;
  }
  async function apply() {
    const s = app.settings;
    const d = app.themePreview;
    if (!s || !d) return;
    app.themePreview = null;
    await app.saveSettings({ ...s, theme: d.theme, appearance: d.appearance });
  }

  const stopsBg = $derived(`linear-gradient(90deg, ${colors.length === 1 ? `${colors[0]}, ${colors[0]}` : colors.join(", ")})`);
  const handlePos = (i: number) => (colors.length === 1 ? 0 : i / (colors.length - 1));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<aside class="editor" aria-label="Customize your theme" transition:fly={{ x: 40, duration: 320, easing: cubicOut }}>
  <header>
    <h2>Customize your theme</h2>
    <button class="icon-btn state" title="Close without saving" aria-label="Close without saving" onclick={close}><span class="icon">close</span></button>
  </header>

  <div class="body">
    <section>
      <h3>Appearance</h3>
      <div class="mode" role="group" aria-label="Appearance">
        <button class="state" aria-pressed={theme.dark} title="Dark" aria-label="Dark" onclick={() => (app.themePreview = { ...draft, theme: "dark" })}><span class="icon">dark_mode</span></button>
        <button class="state" aria-pressed={!theme.dark} title="Light" aria-label="Light" onclick={() => (app.themePreview = { ...draft, theme: "light" })}><span class="icon">light_mode</span></button>
      </div>
    </section>

    <section>
      <h3>Colors</h3>
      <div class="stops" style:background={stopsBg}>
        {#each colors as c, i (i)}
          <button
            class="stop"
            class:selected={sel === i}
            style:--pos={handlePos(i)}
            style:--c={c}
            title="Color {i + 1}"
            aria-label="Color {i + 1}, {c.toUpperCase()}"
            aria-pressed={sel === i}
            onclick={() => select(i)}
          ></button>
        {/each}
      </div>

      <div
        class="square"
        role="slider"
        tabindex="0"
        aria-label="Saturation and brightness"
        aria-valuetext="Saturation {Math.round(hsv[1] * 100)}%, brightness {Math.round(hsv[2] * 100)}%"
        aria-valuenow={Math.round(hsv[1] * 100)}
        style:--hue="hsl({hsv[0]} 100% 50%)"
        onpointerdown={(e) => drag(e, (x, y) => setHsv(hsv[0], x, 1 - y))}
        onkeydown={(e) => arrows(e, (dx, dy) => setHsv(hsv[0], clamp(hsv[1] + dx / 100), clamp(hsv[2] - dy / 100)))}
      >
        <span class="knob" style:left="{hsv[1] * 100}%" style:top="{(1 - hsv[2]) * 100}%" style:background={colors[sel]}></span>
      </div>

      <div
        class="hue"
        role="slider"
        tabindex="0"
        aria-label="Hue"
        aria-valuemin={0}
        aria-valuemax={360}
        aria-valuenow={Math.round(hsv[0])}
        onpointerdown={(e) => drag(e, (x) => setHsv(x * 360, hsv[1], hsv[2]))}
        onkeydown={(e) => arrows(e, (dx) => setHsv((hsv[0] + dx + 360) % 360, hsv[1], hsv[2]))}
      >
        <span class="knob" style:left="{(hsv[0] / 360) * 100}%" style:background="hsl({hsv[0]} 100% 50%)"></span>
      </div>

      <div class="hexrow">
        <span class="swatch" style:background={colors[sel]}></span>
        <span class="hash">#</span>
        <input value={hexText} maxlength="7" spellcheck="false" aria-label="Hex color" oninput={(e) => typedHex(e.currentTarget.value)} />
        {#if canPick}
          <button class="icon-btn state" title="Pick a color from the screen" aria-label="Pick a color from the screen" onclick={pick}><span class="icon">colorize</span></button>
        {/if}
        <button class="icon-btn state" title="Remove this color" aria-label="Remove this color" disabled={colors.length <= 1} onclick={removeColor}><span class="icon">remove</span></button>
      </div>

      <button class="wide state" disabled={colors.length >= MAX} onclick={addColor}><span class="icon">add</span>Add color</button>
    </section>

    <section>
      <h3>Controls</h3>
      <label class="control">
        <span class="row"><span>Gradient direction</span><span class="value">{a.custom_angle}°</span></span>
        <input type="range" min="0" max="359" step="1" value={a.custom_angle} oninput={(e) => patch({ custom_angle: +e.currentTarget.value })} />
      </label>
      <label class="control">
        <span class="row"><span>Color intensity</span><span class="value">{a.custom_intensity}%</span></span>
        <input type="range" min="0" max="100" step="1" value={a.custom_intensity} oninput={(e) => patch({ custom_intensity: +e.currentTarget.value })} />
      </label>
    </section>

    <div class="extras">
      <button class="wide state" onclick={surprise}><span class="icon">casino</span>Surprise me</button>
      <button class="wide state" disabled={isDefault} onclick={reset}>Reset</button>
    </div>
  </div>

  <footer>
    <button class="btn tonal state" onclick={close}>Back</button>
    <button class="btn filled state" onclick={apply}>Apply</button>
  </footer>
</aside>

<style>
  .editor {
    position: fixed;
    top: 12px;
    right: 12px;
    bottom: 12px;
    z-index: 60;
    width: 320px;
    display: flex;
    flex-direction: column;
    border-radius: 20px;
    background: var(--chrome);
    backdrop-filter: blur(40px) saturate(1.6);
    box-shadow: 0 0 0 1px var(--separator), var(--shadow-float);
    overflow: hidden;
  }
  header { display: flex; align-items: center; justify-content: space-between; padding: 14px 10px 6px 18px; }
  h2 { margin: 0; font-size: 17px; line-height: 22px; font-weight: 700; }
  .body { flex: 1; overflow-y: auto; padding: 4px 16px 16px; display: flex; flex-direction: column; gap: 20px; }
  section { display: flex; flex-direction: column; gap: 10px; }
  h3 { margin: 0; font-size: 13px; line-height: 18px; font-weight: 600; color: var(--label-2); }

  .mode { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; padding: 4px; border-radius: 12px; background: var(--fill); }
  .mode button { appearance: none; border: 0; height: 34px; border-radius: 9px; background: none; color: var(--label-2); cursor: pointer; display: grid; place-items: center; }
  .mode button[aria-pressed="true"] { background: var(--card); color: var(--label); box-shadow: 0 1px 3px rgb(0 0 0 / 0.2); }

  .stops { position: relative; height: 44px; border-radius: 10px; }
  .stop {
    appearance: none;
    position: absolute;
    top: 8px;
    left: calc(8px + (100% - 44px) * var(--pos));
    width: 28px;
    height: 28px;
    padding: 0;
    border: 2px solid rgb(255 255 255 / 0.85);
    border-radius: 7px;
    background: var(--c);
    box-shadow: 0 1px 4px rgb(0 0 0 / 0.3);
    cursor: pointer;
  }
  .stop.selected { border: 3px solid #fff; }
  /* A small notch under the colour being edited. */
  .stop.selected::after {
    content: "";
    position: absolute;
    left: 50%;
    bottom: -13px;
    translate: -50% 0;
    border: 5px solid transparent;
    border-top-color: #fff;
  }
  .stop:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

  .square {
    position: relative;
    height: 180px;
    border-radius: 10px;
    background: linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, var(--hue));
    cursor: crosshair;
    touch-action: none;
  }
  .hue {
    position: relative;
    height: 14px;
    border-radius: 7px;
    background: linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00);
    cursor: pointer;
    touch-action: none;
  }
  .knob {
    position: absolute;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: 3px solid #fff;
    box-shadow: 0 0 0 1px rgb(0 0 0 / 0.25), 0 2px 4px rgb(0 0 0 / 0.3);
    translate: -50% -50%;
    pointer-events: none;
  }
  .hue .knob { top: 50%; }
  .square:focus-visible, .hue:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

  .hexrow { display: flex; align-items: center; gap: 4px; height: 44px; padding: 0 4px 0 8px; border-radius: 10px; background: var(--fill); }
  .swatch { width: 28px; height: 28px; border-radius: 6px; flex: none; box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.15); margin-right: 6px; }
  .hash { color: var(--label-2); font-weight: 600; }
  .hexrow input { flex: 1; min-width: 0; border: 0; background: none; color: var(--label); font: inherit; font-weight: 600; font-variant-numeric: tabular-nums; text-transform: uppercase; outline: none; }

  .wide {
    appearance: none;
    border: 0;
    height: 40px;
    border-radius: 10px;
    background: var(--fill);
    color: var(--label);
    font: inherit;
    font-weight: 600;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    cursor: pointer;
  }
  .wide:disabled { opacity: 0.45; cursor: default; }
  .wide .icon { font-size: 20px; }

  .control { display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; justify-content: space-between; font-weight: 600; }
  .value { font-variant-numeric: tabular-nums; }
  .control input { width: 100%; accent-color: var(--accent); }
  .extras { display: flex; flex-direction: column; gap: 8px; }

  footer { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; padding: 12px 16px 16px; border-top: 1px solid var(--separator); }
  footer .btn { width: 100%; justify-content: center; }
</style>
