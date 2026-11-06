// Self-updates from GitHub releases (signed; the public key is in tauri.conf.json).
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

class Updater {
  available = $state<Update | null>(null);
  installing = $state(false);
  progress = $state<number | null>(null);
  error = $state<string | null>(null);

  async check() {
    try {
      this.available = await check();
    } catch (e) {
      // Offline or no release yet: try again next launch.
      console.warn("update check failed", e);
    }
  }

  async install() {
    const update = this.available;
    if (!update || this.installing) return;
    this.installing = true;
    this.error = null;
    let total = 0;
    let done = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        else if (event.event === "Progress") {
          done += event.data.chunkLength;
          this.progress = total ? done / total : null;
        }
      });
      await relaunch();
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      this.installing = false;
    }
  }
}

export const updater = new Updater();
