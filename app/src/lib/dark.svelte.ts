// Resolves the theme preference against the OS setting, reactively.
import { app } from "./store.svelte";

const media = window.matchMedia("(prefers-color-scheme: dark)");
let systemDark = $state(media.matches);
media.addEventListener("change", (e) => (systemDark = e.matches));

export const theme = {
  get dark() {
    const pref = app.themePreview?.theme ?? app.settings?.theme ?? "system";
    return pref === "dark" || (pref === "system" && systemDark);
  },
};
