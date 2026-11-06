// Minecraft (1.13+) key names, as options.txt stores them, and readable labels for them.

const SPECIAL: Record<string, string> = {
  Space: "space", Tab: "tab", Enter: "enter", Backspace: "backspace", CapsLock: "caps.lock",
  ShiftLeft: "left.shift", ShiftRight: "right.shift", ControlLeft: "left.control", ControlRight: "right.control",
  AltLeft: "left.alt", AltRight: "right.alt", MetaLeft: "left.win", MetaRight: "right.win",
  ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right",
  Insert: "insert", Delete: "delete", Home: "home", End: "end", PageUp: "page.up", PageDown: "page.down",
  Backquote: "grave.accent", Minus: "minus", Equal: "equal", BracketLeft: "left.bracket", BracketRight: "right.bracket",
  Backslash: "backslash", Semicolon: "semicolon", Quote: "apostrophe", Comma: "comma", Period: "period", Slash: "slash",
  NumpadAdd: "keypad.add", NumpadSubtract: "keypad.subtract", NumpadMultiply: "keypad.multiply",
  NumpadDivide: "keypad.divide", NumpadDecimal: "keypad.decimal", NumpadEnter: "keypad.enter",
  PrintScreen: "print.screen", ScrollLock: "scroll.lock", Pause: "pause", NumLock: "num.lock", ContextMenu: "menu",
};

/** The Minecraft name for a pressed key, or null for keys it can't bind. */
export function keyName(e: KeyboardEvent): string | null {
  const c = e.code;
  let name: string | undefined = SPECIAL[c];
  if (!name && /^Key[A-Z]$/.test(c)) name = c.slice(3).toLowerCase();
  else if (!name && /^Digit\d$/.test(c)) name = c.slice(5);
  else if (!name && /^F\d{1,2}$/.test(c)) name = c.toLowerCase();
  else if (!name && /^Numpad\d$/.test(c)) name = `keypad.${c.slice(6)}`;
  return name ? `key.keyboard.${name}` : null;
}

/** The Minecraft name for a mouse button (0 left, 1 middle, 2 right, 3 and 4 side). */
export function mouseName(button: number): string {
  return ["key.mouse.left", "key.mouse.middle", "key.mouse.right"][button] ?? `key.mouse.${button + 1}`;
}

export const UNBOUND = "key.keyboard.unknown";

/** "key.keyboard.left.shift" → "Left shift", "key.mouse.right" → "Right click". */
export function keyLabel(name: string): string {
  if (name === UNBOUND) return "Not bound";
  const mouse = name.match(/^key\.mouse\.(.+)$/);
  if (mouse) {
    const b = mouse[1] ?? "";
    return { left: "Left click", right: "Right click", middle: "Middle click" }[b] ?? `Mouse ${b}`;
  }
  const rest = name.replace(/^key\.keyboard\./, "");
  if (rest.length === 1) return rest.toUpperCase();
  if (/^f\d+$/.test(rest)) return rest.toUpperCase();
  const words = rest.replace(/^keypad\./, "numpad ").replaceAll(".", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Vanilla keybinds, in the order Minecraft's controls screen shows them. */
export const VANILLA_KEYS: { id: string; label: string; def: string }[] = [
  { id: "key.forward", label: "Walk forward", def: "key.keyboard.w" },
  { id: "key.left", label: "Walk left", def: "key.keyboard.a" },
  { id: "key.back", label: "Walk back", def: "key.keyboard.s" },
  { id: "key.right", label: "Walk right", def: "key.keyboard.d" },
  { id: "key.jump", label: "Jump", def: "key.keyboard.space" },
  { id: "key.sneak", label: "Sneak", def: "key.keyboard.left.shift" },
  { id: "key.sprint", label: "Sprint", def: "key.keyboard.left.control" },
  { id: "key.attack", label: "Attack", def: "key.mouse.left" },
  { id: "key.use", label: "Use item", def: "key.mouse.right" },
  { id: "key.pickItem", label: "Pick block", def: "key.mouse.middle" },
  { id: "key.drop", label: "Drop item", def: "key.keyboard.q" },
  { id: "key.swapOffhand", label: "Swap with offhand", def: "key.keyboard.f" },
  { id: "key.inventory", label: "Inventory", def: "key.keyboard.e" },
  { id: "key.chat", label: "Chat", def: "key.keyboard.t" },
  { id: "key.command", label: "Command", def: "key.keyboard.slash" },
  { id: "key.playerlist", label: "Player list", def: "key.keyboard.tab" },
  { id: "key.advancements", label: "Advancements", def: "key.keyboard.l" },
  { id: "key.socialInteractions", label: "Social interactions", def: "key.keyboard.p" },
  { id: "key.screenshot", label: "Screenshot", def: "key.keyboard.f2" },
  { id: "key.togglePerspective", label: "Change perspective", def: "key.keyboard.f5" },
  { id: "key.fullscreen", label: "Fullscreen", def: "key.keyboard.f11" },
  ...Array.from({ length: 9 }, (_, n) => ({ id: `key.hotbar.${n + 1}`, label: `Hotbar slot ${n + 1}`, def: `key.keyboard.${n + 1}` })),
];
