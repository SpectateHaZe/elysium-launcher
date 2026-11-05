<!-- Mode, theme palette, accent colour and display options. -->
<script lang="ts">
  import { app } from "../lib/store.svelte";
  import { theme } from "../lib/dark.svelte";
  import { ACCENTS, PALETTES, accentColor, customPalette, paletteFor, type Palette } from "../lib/theme";
  import { isHex } from "../lib/color";
  import type { Appearance, TextSize, ThemePref } from "../lib/api";
  import Row from "./Row.svelte";

  const s = $derived(app.settings!);
  const a = $derived(s.appearance);
  const dark = $derived(theme.dark);
  const palettes = $derived<Palette[]>([...PALETTES, customPalette(a.custom_tint)]);

  const modes: { value: ThemePref; label: string; icon: string }[] = [
    { value: "system", label: "Auto", icon: "contrast" },
    { value: "light", label: "Light", icon: "light_mode" },
    { value: "dark", label: "Dark", icon: "dark_mode" },
  ];
  const sizes: { value: TextSize; label: string }[] = [
    { value: "small", label: "Small" },
    { value: "default", label: "Default" },
    { value: "large", label: "Large" },
  ];

  function setAppearance(patch: Partial<Appearance>) {
    void app.saveSettings({ ...s, appearance: { ...a, ...patch } });
  }

  // Native colour pickers fire `input` continuously; save at most every 120 ms.
  let pending: ReturnType<typeof setTimeout> | null = null;
  function setColor(key: "custom_accent" | "custom_tint", value: string, extra: Partial<Appearance> = {}) {
    if (!isHex(value)) return;
    if (pending) clearTimeout(pending);
    pending = setTimeout(() => setAppearance({ [key]: value.toLowerCase(), ...extra }), 120);
  }

  const themeAccent = $derived(accentColor({ ...a, accent: "palette" }, dark));
</script>

<Row title="Mode">
  <div class="segmented" role="group" aria-label="Appearance">
    {#each modes as m (m.value)}
      <button class="state" aria-pressed={s.theme === m.value} onclick={() => void app.saveSettings({ ...s, theme: m.value })}>
        <span class="icon">{m.icon}</span>{m.label}
      </button>
    {/each}
  </div>
</Row>

<div class="block">
  <p class="title-m">Theme</p>
  <div class="themes" role="radiogroup" aria-label="Theme">
    {#each palettes as p (p.id)}
      {@const t = p[dark ? "dark" : "light"]}
      {@const selected = paletteFor(a).id === p.id}
      {@const accent = accentColor({ ...a, palette: p.id, accent: "palette" }, dark)}
      <button class="theme" role="radio" aria-checked={selected} onclick={() => setAppearance({ palette: p.id })}>
        <span class="preview" class:selected style:background={t.bg}>
          <span class="side" style:background={t.fill}></span>
          <span class="win" style:background={t.card}>
            <span class="line" style:background={t.fillStrong}></span>
            <span class="line short" style:background={t.fill}></span>
            <span class="pill" style:background={accent}></span>
          </span>
        </span>
        <span class="name" class:selected>{p.name}</span>
      </button>
    {/each}
  </div>
</div>

{#if a.palette === "custom"}
  <Row title="Theme tint" description="The custom theme's surfaces are built from this color">
    <label class="color-field">
      <span class="dot" style:background={a.custom_tint}></span>
      <input
        type="color"
        value={a.custom_tint}
        aria-label="Theme tint"
        oninput={(e) => setColor("custom_tint", e.currentTarget.value)}
      />
      <span class="hex">{a.custom_tint.toUpperCase()}</span>
    </label>
  </Row>
{/if}

<div class="block">
  <p class="title-m">Accent color</p>
  <div class="accents" role="radiogroup" aria-label="Accent color">
    <button
      class="accent multi"
      role="radio"
      aria-checked={a.accent === "palette"}
      title="Theme default"
      aria-label="Theme default"
      style:--swatch={themeAccent}
      onclick={() => setAppearance({ accent: "palette" })}
    ></button>
    {#each ACCENTS as c (c.id)}
      <button
        class="accent"
        role="radio"
        aria-checked={a.accent === c.id}
        title={c.name}
        aria-label={c.name}
        style:--swatch={dark ? c.dark : c.light}
        onclick={() => setAppearance({ accent: c.id })}
      ></button>
    {/each}
    <label class="accent custom" class:checked={a.accent === "custom"} title="Custom color" style:--swatch={a.custom_accent}>
      <input
        type="color"
        value={a.custom_accent}
        aria-label="Custom accent color"
        onclick={() => a.accent !== "custom" && setAppearance({ accent: "custom" })}
        oninput={(e) => setColor("custom_accent", e.currentTarget.value, { accent: "custom" })}
      />
      <span class="icon">colorize</span>
    </label>
  </div>
  <p class="body-s muted caption">
    {a.accent === "palette" ? "Using the theme's own accent." : a.accent === "custom" ? `Custom ${a.custom_accent.toUpperCase()}.` : (ACCENTS.find((c) => c.id === a.accent)?.name ?? "")}
  </p>
</div>

<Row title="Text size">
  <div class="segmented" role="group" aria-label="Text size">
    {#each sizes as t (t.value)}
      <button class="state" aria-pressed={a.text_size === t.value} onclick={() => setAppearance({ text_size: t.value })}>{t.label}</button>
    {/each}
  </div>
</Row>

<Row title="Reduce transparency" description="Uses solid backgrounds instead of frosted glass">
  <input class="switch" type="checkbox" checked={a.reduce_transparency} onchange={(e) => setAppearance({ reduce_transparency: e.currentTarget.checked })} />
</Row>

<style>
  .block { padding: 14px 0 18px; border-top: 1px solid var(--separator); }
  .block .title-m { margin-bottom: 14px; }

  .themes { display: grid; grid-template-columns: repeat(auto-fill, minmax(96px, 1fr)); gap: 18px 14px; }
  .theme {
    appearance: none;
    border: 0;
    padding: 0;
    background: none;
    font: inherit;
    color: var(--label-2);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .preview {
    position: relative;
    width: 100%;
    aspect-ratio: 4 / 3;
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 0 0 1px var(--separator);
    transition: box-shadow 200ms var(--ease);
  }
  .preview.selected { box-shadow: 0 0 0 3px var(--accent); }
  .theme:focus-visible { outline: none; }
  .theme:focus-visible .preview { outline: 3px solid var(--accent); outline-offset: 3px; }
  .side { position: absolute; left: 0; top: 0; bottom: 0; width: 18%; }
  .win {
    position: absolute;
    left: 26%;
    top: 16%;
    right: 10%;
    bottom: 16%;
    border-radius: 6px;
    box-shadow: 0 2px 6px rgb(0 0 0 / 0.12);
    padding: 10% 10%;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .line { height: 5px; border-radius: 3px; width: 80%; }
  .line.short { width: 55%; }
  .pill { margin-top: auto; height: 9px; width: 42%; border-radius: 6px; }
  .name { font-size: 13px; font-weight: 600; }
  .name.selected { color: var(--label); font-weight: 700; }

  .accents { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; }
  .accent {
    appearance: none;
    position: relative;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--swatch);
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.12);
    cursor: pointer;
    display: grid;
    place-items: center;
    transition: transform 150ms var(--ease);
  }
  .accent:hover { transform: scale(1.08); }
  .accent.multi { background: conic-gradient(#ff3b30, #ff9500, #ffcc00, #34c759, #30b0c7, #007aff, #af52de, #ff2d55, #ff3b30); }
  /* macOS marks the chosen accent with a white dot. */
  .accent[aria-checked="true"]::after,
  .accent.custom.checked::after {
    content: "";
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
  }
  .accent[aria-checked="true"],
  .accent.custom.checked { box-shadow: 0 0 0 2px var(--card), 0 0 0 4px var(--swatch); }
  .accent.custom { margin-left: 6px; }
  .accent.custom .icon { position: absolute; font-size: 16px; color: #fff; text-shadow: 0 1px 2px rgb(0 0 0 / 0.4); pointer-events: none; }
  .accent.custom.checked .icon { display: none; }
  .accent.custom input,
  .color-field input { position: absolute; inset: 0; opacity: 0; width: 100%; height: 100%; cursor: pointer; border: 0; padding: 0; }
  .caption { margin-top: 10px; }

  .color-field {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 14px 0 6px;
    border-radius: var(--shape-full);
    background: var(--fill);
    cursor: pointer;
  }
  .dot { width: 26px; height: 26px; border-radius: 50%; box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.12); }
  .hex { font-weight: 600; font-variant-numeric: tabular-nums; }
</style>
