// Appearance: Apple-style palettes, system accent colours and user customisation.
import type { Appearance } from "./api";
import { hexToHsl, hslToHex, isHex, mix, onColor, withAlpha } from "./color";

interface Tokens {
  bg: string;
  card: string;
  fill: string;
  fillStrong: string;
}

export interface Palette {
  id: string;
  name: string;
  accent: string;
  light: Tokens;
  dark: Tokens;
}

export const PALETTES: Palette[] = [
  {
    id: "graphite", name: "Graphite", accent: "graphite",
    light: { bg: "#eceef1", card: "#fbfbfc", fill: "#e1e4e8", fillStrong: "#c9cdd3" },
    dark: { bg: "#121315", card: "#1e1f22", fill: "#2a2c30", fillStrong: "#3a3d42" },
  },
  {
    // Id kept as `default` so existing settings files still pick it.
    id: "default", name: "Classic", accent: "blue",
    light: { bg: "#f5f5f7", card: "#ffffff", fill: "#e8e8ed", fillStrong: "#d2d2d7" },
    dark: { bg: "#000000", card: "#1c1c1e", fill: "#2c2c2e", fillStrong: "#3a3a3c" },
  },
  {
    id: "midnight", name: "Midnight", accent: "indigo",
    light: { bg: "#eef1f8", card: "#ffffff", fill: "#e2e7f2", fillStrong: "#cbd3e4" },
    dark: { bg: "#070b16", card: "#121a2c", fill: "#1d2740", fillStrong: "#2b3756" },
  },
  {
    id: "ocean", name: "Ocean", accent: "teal",
    light: { bg: "#edf5f8", card: "#ffffff", fill: "#dfecf1", fillStrong: "#c5dce5" },
    dark: { bg: "#04131a", card: "#0e212a", fill: "#17313c", fillStrong: "#22414f" },
  },
  {
    id: "forest", name: "Forest", accent: "green",
    light: { bg: "#f0f4ef", card: "#ffffff", fill: "#e2eae0", fillStrong: "#cbd8c8" },
    dark: { bg: "#0a120c", card: "#152018", fill: "#1f2e23", fillStrong: "#2b3d2f" },
  },
  {
    id: "rose", name: "Rose", accent: "pink",
    light: { bg: "#f8f1f3", card: "#ffffff", fill: "#f0e2e6", fillStrong: "#e2cad1" },
    dark: { bg: "#160c10", card: "#24151b", fill: "#311e25", fillStrong: "#432a33" },
  },
  {
    id: "sand", name: "Sand", accent: "orange",
    light: { bg: "#f5f2ec", card: "#fffdf9", fill: "#ebe5da", fillStrong: "#dbd2c2" },
    dark: { bg: "#15120e", card: "#221e18", fill: "#2f2a22", fillStrong: "#3f382d" },
  },
];

/** Builds a palette from one tint colour, keeping Apple's neutral contrast levels. */
export function customPalette(tint: string): Palette {
  const t = isHex(tint) ? tint : "#5e5ce6";
  return {
    id: "custom",
    name: "Custom",
    accent: "custom-tint",
    light: { bg: mix(t, "#f5f5f7", 0.07), card: mix(t, "#ffffff", 0.02), fill: mix(t, "#e8e8ed", 0.1), fillStrong: mix(t, "#d2d2d7", 0.14) },
    dark: { bg: mix(t, "#000000", 0.1), card: mix(t, "#1c1c1e", 0.12), fill: mix(t, "#2c2c2e", 0.14), fillStrong: mix(t, "#3a3a3c", 0.16) },
  };
}

export interface Accent {
  id: string;
  name: string;
  light: string;
  dark: string;
}

/** Apple's system colours, with their light and dark appearances. */
export const ACCENTS: Accent[] = [
  { id: "blue", name: "Blue", light: "#007aff", dark: "#0a84ff" },
  { id: "purple", name: "Purple", light: "#af52de", dark: "#bf5af2" },
  { id: "pink", name: "Pink", light: "#ff2d55", dark: "#ff375f" },
  { id: "red", name: "Red", light: "#ff3b30", dark: "#ff453a" },
  { id: "orange", name: "Orange", light: "#ff9500", dark: "#ff9f0a" },
  { id: "yellow", name: "Yellow", light: "#ffcc00", dark: "#ffd60a" },
  { id: "green", name: "Green", light: "#34c759", dark: "#30d158" },
  { id: "teal", name: "Teal", light: "#30b0c7", dark: "#40c8e0" },
  { id: "indigo", name: "Indigo", light: "#5856d6", dark: "#5e5ce6" },
  // Monochrome rather than Apple's mid-gray, so filled buttons don't read as disabled.
  { id: "graphite", name: "Graphite", light: "#2c2c2e", dark: "#e5e5ea" },
];

export function paletteFor(a: Appearance): Palette {
  if (a.palette === "custom") return customPalette(a.custom_tint);
  return PALETTES.find((p) => p.id === a.palette) ?? PALETTES[0]!;
}

export function accentColor(a: Appearance, dark: boolean): string {
  if (a.accent === "custom" && isHex(a.custom_accent)) return a.custom_accent;
  const palette = paletteFor(a);
  const id = a.accent === "palette" ? palette.accent : a.accent;
  if (id === "custom-tint") return a.custom_tint;
  const accent = ACCENTS.find((x) => x.id === id) ?? ACCENTS[0]!;
  return dark ? accent.dark : accent.light;
}

export interface Gradient {
  id: string;
  name: string;
  from: string;
  to: string;
}

/** Background gradient presets. Each is washed into the theme's background so text
 *  and cards keep their contrast. */
export const GRADIENTS: Gradient[] = [
  { id: "aurora", name: "Aurora", from: "#30d158", to: "#5e5ce6" },
  { id: "sunset", name: "Sunset", from: "#ff9500", to: "#ff2d55" },
  { id: "ocean", name: "Ocean", from: "#40c8e0", to: "#0a84ff" },
  { id: "dusk", name: "Dusk", from: "#5e5ce6", to: "#bf5af2" },
  { id: "blossom", name: "Blossom", from: "#ff375f", to: "#ffd60a" },
  { id: "nether", name: "Nether", from: "#c0281e", to: "#7a1fa2" },
  { id: "end", name: "End", from: "#d9d27a", to: "#2b1f4f" },
];

/** The two colours of the chosen gradient, or null for none. */
export function gradientColors(a: Appearance): [string, string] | null {
  if (a.gradient === "custom") return isHex(a.gradient_from) && isHex(a.gradient_to) ? [a.gradient_from, a.gradient_to] : null;
  const g = GRADIENTS.find((g) => g.id === a.gradient);
  return g ? [g.from, g.to] : null;
}

/** A soft diagonal wash of the gradient over the theme background. */
export function backdrop(colors: [string, string] | null, bg: string, dark: boolean): string {
  if (!colors) return "none";
  const k = dark ? 0.3 : 0.26;
  return `radial-gradient(120% 90% at 0% 0%, ${mix(colors[0], bg, k)}, transparent 70%), radial-gradient(120% 90% at 100% 100%, ${mix(colors[1], bg, k)}, transparent 70%)`;
}

const TEXT_ZOOM = { small: 0.92, default: 1, large: 1.1 } as const;

/** Writes the resolved appearance to CSS custom properties on `<html>`. */
export function applyAppearance(a: Appearance, dark: boolean) {
  const root = document.documentElement;
  const t = paletteFor(a)[dark ? "dark" : "light"];
  const accent = accentColor(a, dark);
  const vars: Record<string, string> = {
    "--bg": t.bg,
    "--card": t.card,
    "--fill": t.fill,
    "--fill-strong": t.fillStrong,
    "--label": dark ? "#f5f5f7" : "#1d1d1f",
    "--label-2": dark ? "#a1a1a6" : "#6e6e73",
    "--label-3": dark ? "#6e6e73" : "#86868b",
    "--separator": dark ? "rgb(255 255 255 / 0.12)" : "rgb(0 0 0 / 0.08)",
    "--chrome": a.reduce_transparency ? t.bg : withAlpha(t.bg, 0.72),
    "--accent": accent,
    "--on-accent": onColor(accent),
    "--accent-soft": mix(accent, t.card, dark ? 0.24 : 0.14),
  };
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
  root.style.setProperty("--backdrop", backdrop(gradientColors(a), t.bg, dark));
  root.style.colorScheme = dark ? "dark" : "light";
  root.dataset.theme = dark ? "dark" : "light";
  root.dataset.reduceTransparency = String(a.reduce_transparency);
  const zoom = TEXT_ZOOM[a.text_size] ?? 1;
  // Only zoom when needed: WebKit drops pixelated image scaling under any zoom.
  if (zoom === 1) root.style.removeProperty("zoom");
  else root.style.setProperty("zoom", String(zoom));
}

// ---------------------------------------------------------------------------
// Instance blocks. Each instance has a block material used for its artwork.

export interface Material {
  id: string;
  name: string;
  seed: string;
}

export const MATERIALS: Material[] = [
  { id: "amethyst", name: "Amethyst", seed: "#9a5cd0" },
  { id: "grass", name: "Grass", seed: "#5d9a3c" },
  { id: "prismarine", name: "Prismarine", seed: "#3a9c93" },
  { id: "lapis", name: "Lapis", seed: "#2b4bb0" },
  { id: "sculk", name: "Sculk", seed: "#0d7380" },
  { id: "copper", name: "Copper", seed: "#c2683c" },
  { id: "gold", name: "Gold", seed: "#d9a623" },
  { id: "redstone", name: "Redstone", seed: "#c0281e" },
  { id: "cherry", name: "Cherry", seed: "#e58bb6" },
  { id: "deepslate", name: "Deepslate", seed: "#5b5f6b" },
];

const DEFAULT_MATERIAL = MATERIALS[0]!;

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

/** The instance's chosen material, or a stable pick from its name. */
export function materialFor(id: string | null | undefined, name = ""): Material {
  if (id) {
    const m = MATERIALS.find((m) => m.id === id);
    if (m) return m;
  }
  if (!name) return DEFAULT_MATERIAL;
  return MATERIALS[hash(name) % MATERIALS.length] ?? DEFAULT_MATERIAL;
}

/** Background for a block tile: a soft wash of the material colour. */
export function tileColor(m: Material, dark: boolean): string {
  return dark ? mix(m.seed, "#1c1c1e", 0.34) : mix(m.seed, "#ffffff", 0.24);
}

/** Light cast behind the block on the home stage. */
export function glowColor(m: Material, dark: boolean): string {
  return dark ? withAlpha(m.seed, 0.32) : mix(m.seed, "#ffffff", 0.3);
}

export interface BlockFaces {
  top: string;
  left: string;
  right: string;
}

const faceCache = new Map<string, BlockFaces>();

function rng(seed: number) {
  let a = seed || 1;
  return () => {
    a ^= a << 13;
    a ^= a >>> 17;
    a ^= a << 5;
    return ((a >>> 0) % 10000) / 10000;
  };
}

function face(h: number, s: number, lightness: number[], rand: () => number): string {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 16;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  for (let y = 0; y < 16; y++) {
    for (let x = 0; x < 16; x++) {
      // Mostly mid tones, with occasional highlights and shadows.
      const r = rand();
      const i = r < 0.35 ? 1 : r < 0.65 ? 2 : r < 0.85 ? 0 : r < 0.95 ? 3 : 4;
      ctx.fillStyle = hslToHex(h, s, lightness[i]!);
      ctx.fillRect(x, y, 1, 1);
    }
  }
  return canvas.toDataURL();
}

/** Three 16×16 pixel-noise faces in the material's hue, lit from the top-left. */
export function blockFaces(material: Material, key: string): BlockFaces {
  const k = `${material.id}:${key}`;
  const hit = faceCache.get(k);
  if (hit) return hit;
  const [h, sat] = hexToHsl(material.seed);
  const s = Math.max(0.3, Math.min(0.75, sat));
  const rand = rng(hash(k));
  const faces = {
    top: face(h, s, [0.76, 0.72, 0.68, 0.82, 0.63], rand),
    left: face(h, s, [0.56, 0.52, 0.48, 0.61, 0.43], rand),
    right: face(h, s, [0.38, 0.34, 0.31, 0.43, 0.27], rand),
  };
  faceCache.set(k, faces);
  return faces;
}
