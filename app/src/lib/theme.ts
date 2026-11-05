// Dynamic colour: every instance has a block material whose colour seeds a full
// Material 3 scheme (HCT tonal-spot), so the launcher re-tints to the instance in focus.
import {
  Hct,
  MaterialDynamicColors,
  SchemeTonalSpot,
  TonalPalette,
  argbFromHex,
  hexFromArgb,
  type DynamicScheme,
} from "@material/material-color-utilities";

export interface Material {
  id: string;
  name: string;
  seed: string;
}

export const MATERIALS: Material[] = [
  { id: "amethyst", name: "Amethyst", seed: "#9A5CD0" },
  { id: "grass", name: "Grass", seed: "#5D9A3C" },
  { id: "prismarine", name: "Prismarine", seed: "#3A9C93" },
  { id: "lapis", name: "Lapis", seed: "#2B4BB0" },
  { id: "sculk", name: "Sculk", seed: "#0D7380" },
  { id: "copper", name: "Copper", seed: "#C2683C" },
  { id: "gold", name: "Gold", seed: "#D9A623" },
  { id: "redstone", name: "Redstone", seed: "#C0281E" },
  { id: "cherry", name: "Cherry", seed: "#E58BB6" },
  { id: "deepslate", name: "Deepslate", seed: "#5B5F6B" },
];

const DEFAULT = MATERIALS[0]!;

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
  if (!name) return DEFAULT;
  return MATERIALS[hash(name) % MATERIALS.length] ?? DEFAULT;
}

const ROLES = [
  "primary", "onPrimary", "primaryContainer", "onPrimaryContainer",
  "secondary", "onSecondary", "secondaryContainer", "onSecondaryContainer",
  "tertiary", "tertiaryContainer", "onTertiaryContainer",
  "error", "onError", "errorContainer", "onErrorContainer",
  "surface", "surfaceDim", "surfaceBright", "onSurface", "onSurfaceVariant",
  "surfaceContainerLowest", "surfaceContainerLow", "surfaceContainer",
  "surfaceContainerHigh", "surfaceContainerHighest",
  "outline", "outlineVariant", "inverseSurface", "inverseOnSurface", "inversePrimary", "scrim",
] as const;

function kebab(s: string): string {
  return s.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase());
}

export function scheme(material: Material, dark: boolean): DynamicScheme {
  return new SchemeTonalSpot(Hct.fromInt(argbFromHex(material.seed)), dark, 0);
}

/** Writes the scheme's colour roles to `--md-<role>` custom properties. */
export function applyScheme(material: Material, dark: boolean, el: HTMLElement = document.documentElement) {
  const s = scheme(material, dark);
  const colors = MaterialDynamicColors as unknown as Record<string, { getArgb(s: DynamicScheme): number }>;
  for (const role of ROLES) {
    const c = colors[role];
    if (c) el.style.setProperty(`--md-${kebab(role)}`, hexFromArgb(c.getArgb(s)));
  }
  el.style.colorScheme = dark ? "dark" : "light";
  el.dataset.theme = dark ? "dark" : "light";
}

// ---------------------------------------------------------------------------
// Block textures: three 16×16 faces of tonal "pixel noise" from the material's
// own palette, cached as data URLs.

export interface BlockFaces {
  top: string;
  left: string;
  right: string;
}

const cache = new Map<string, BlockFaces>();

function rng(seed: number) {
  let a = seed || 1;
  return () => {
    a ^= a << 13;
    a ^= a >>> 17;
    a ^= a << 5;
    return ((a >>> 0) % 10000) / 10000;
  };
}

function face(palette: TonalPalette, tones: number[], rand: () => number): string {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 16;
  const ctx = canvas.getContext("2d");
  if (!ctx) return "";
  for (let y = 0; y < 16; y++) {
    for (let x = 0; x < 16; x++) {
      // Weighted toward the middle tones, with occasional highlights/shadows.
      const r = rand();
      const i = r < 0.35 ? 1 : r < 0.65 ? 2 : r < 0.85 ? 0 : r < 0.95 ? 3 : 4;
      ctx.fillStyle = hexFromArgb(palette.tone(tones[i]!));
      ctx.fillRect(x, y, 1, 1);
    }
  }
  return canvas.toDataURL();
}

export function blockFaces(material: Material, key: string): BlockFaces {
  const k = `${material.id}:${key}`;
  const hit = cache.get(k);
  if (hit) return hit;
  const hct = Hct.fromInt(argbFromHex(material.seed));
  // Keep some chroma even for muted materials so the block never looks grey.
  const palette = TonalPalette.fromHueAndChroma(hct.hue, Math.max(hct.chroma, 24));
  const rand = rng(hash(k));
  const faces = {
    top: face(palette, [84, 80, 76, 90, 70], rand),
    left: face(palette, [64, 60, 56, 70, 50], rand),
    right: face(palette, [44, 40, 36, 50, 30], rand),
  };
  cache.set(k, faces);
  return faces;
}

const tintCache = new Map<string, { container: string; onContainer: string }>();

/** A material's own container colours, for previews that aren't the focused instance. */
export function tint(material: Material, dark: boolean) {
  const k = `${material.id}:${dark}`;
  let t = tintCache.get(k);
  if (!t) {
    const s = scheme(material, dark);
    t = {
      container: hexFromArgb(MaterialDynamicColors.primaryContainer.getArgb(s)),
      onContainer: hexFromArgb(MaterialDynamicColors.onPrimaryContainer.getArgb(s)),
    };
    tintCache.set(k, t);
  }
  return t;
}
