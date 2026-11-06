// Global app state (Svelte 5 runes). Components read fields directly and call methods.
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  api,
  errorText,
  type AccountStore,
  type Appearance,
  type ThemePref,
  type Instance,
  type Job,
  type ManualDownload,
  type Overrides,
  type SessionView,
  type Settings,
  type VersionManifest,
} from "./api";

export type Route =
  | { name: "home" }
  | { name: "logs"; id: string }
  | { name: "content"; id: string }
  | { name: "discover" }
  | { name: "accounts" }
  | { name: "game" }
  | { name: "settings" };

export type MenuItem =
  | { label: string; icon?: string; run: () => void; danger?: boolean; disabled?: boolean }
  | "separator";

export interface Snack {
  id: number;
  text: string;
  action?: { label: string; run: () => void };
}

class AppState {
  ready = $state(false);
  fatal = $state<string | null>(null);
  version = $state("");
  dataDir = $state("");
  settings = $state<Settings | null>(null);
  accounts = $state<AccountStore>({ accounts: [], active: null });
  instances = $state<Instance[]>([]);
  sessions = $state<Record<string, SessionView>>({});
  selectedId = $state<string | null>(null);
  #route = $state<Route>({ name: "home" });
  #back: Route[] = [];
  #forward: Route[] = [];

  /** The page shown. Setting it records history for the mouse back and forward buttons. */
  get route(): Route {
    return this.#route;
  }
  set route(next: Route) {
    if (JSON.stringify(next) === JSON.stringify(this.#route)) return;
    this.#back.push(this.#route);
    if (this.#back.length > 50) this.#back.shift();
    this.#forward = [];
    this.#route = next;
  }

  back() {
    const prev = this.#back.pop();
    if (!prev) return;
    this.#forward.push(this.#route);
    this.#route = prev;
  }

  forward() {
    const next = this.#forward.pop();
    if (!next) return;
    this.#back.push(this.#route);
    this.#route = next;
  }
  manifest = $state<VersionManifest | null>(null);
  manifestError = $state<string | null>(null);
  snack = $state<Snack | null>(null);
  dialog = $state<"new-instance" | "quit" | "msa" | "import" | { delete: string } | null>(null);
  sheetOpen = $state(false);
  curseforgeConfigured = $state(false);
  curseforgeBuiltin = $state(false);
  horizonSupported = $state(false);
  jobs = $state<Job[]>([]);
  /** Instance the Discover page installs into. */
  targetId = $state<string | null>(null);
  /** The open right-click menu. */
  /** The theme editor's unsaved theme, shown live while it's open. */
  themePreview = $state<{ appearance: Appearance; theme: ThemePref } | null>(null);
  menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  /** Files waiting to be downloaded by hand, per instance. */
  manual = $state<{ instanceId: string; items: ManualDownload[] } | null>(null);
  #jobPoll: ReturnType<typeof setTimeout> | null = null;
  #jobHandlers = new Map<number, (job: Job) => void>();

  #poll: ReturnType<typeof setTimeout> | null = null;
  #snackTimer: ReturnType<typeof setTimeout> | null = null;

  selected = $derived(this.instances.find((i) => i.id === this.selectedId) ?? this.instances[0] ?? null);
  activeAccount = $derived(this.accounts.accounts.find((a) => a.uuid === this.accounts.active) ?? null);

  async init() {
    try {
      const b = await api.bootstrap();
      this.settings = b.settings;
      this.accounts = b.accounts;
      this.instances = b.instances;
      this.dataDir = b.data_dir;
      this.version = b.version;
      this.curseforgeConfigured = b.curseforge_configured;
      this.curseforgeBuiltin = b.curseforge_builtin;
      this.horizonSupported = b.horizon_supported;
      this.selectedId = b.instances[0]?.id ?? null;
      this.ready = true;
    } catch (e) {
      this.fatal = errorText(e);
      return;
    }
    await listen<string>("session-ended", () => {
      void this.refreshInstances();
      void this.pollSessions();
    });
    await listen("confirm-quit", () => (this.dialog = "quit"));
    void this.loadVersions();
    void this.pollSessions();
  }

  async loadVersions() {
    this.manifestError = null;
    try {
      this.manifest = await api.versions();
    } catch (e) {
      this.manifestError = errorText(e);
    }
  }

  async refreshInstances() {
    try {
      this.instances = await api.instances();
    } catch (e) {
      this.toast(`Couldn't read instances: ${errorText(e)}`);
    }
  }

  /** Polls session progress while anything is installing or running. */
  async pollSessions() {
    if (this.#poll) clearTimeout(this.#poll);
    try {
      this.sessions = await api.sessions();
    } catch {
      // Transient; the next poll will retry.
    }
    const active = Object.values(this.sessions).some((s) => isActive(s));
    if (active) this.#poll = setTimeout(() => void this.pollSessions(), 300);
  }

  /** Tracks a background job; `onDone` runs once when it finishes. */
  watchJob(id: number, onDone?: (job: Job) => void) {
    if (onDone) this.#jobHandlers.set(id, onDone);
    void this.pollJobs();
  }

  async pollJobs() {
    if (this.#jobPoll) clearTimeout(this.#jobPoll);
    try {
      this.jobs = await api.jobs();
    } catch {
      return;
    }
    for (const job of this.jobs) {
      if (job.state.kind === "running") continue;
      const handler = this.#jobHandlers.get(job.id);
      if (handler) {
        this.#jobHandlers.delete(job.id);
        handler(job);
      }
    }
    if (this.jobs.some((j) => j.state.kind === "running")) {
      this.#jobPoll = setTimeout(() => void this.pollJobs(), 400);
    }
  }

  async dismissJob(id: number) {
    await api.jobDismiss(id);
    this.jobs = this.jobs.filter((j) => j.id !== id);
  }

  toast(text: string, action?: Snack["action"]) {
    if (this.#snackTimer) clearTimeout(this.#snackTimer);
    this.snack = { id: Date.now(), text, action };
    this.#snackTimer = setTimeout(() => (this.snack = null), action ? 8000 : 4000);
  }

  select(id: string) {
    this.selectedId = id;
  }

  /** A click on an instance: select it and open its mods and packs (Horizon has none). */
  /** First click selects an instance; clicking the selected one opens its mods. */
  open(i: Instance) {
    if (this.selectedId !== i.id) this.selectedId = i.id;
    else if (i.loader !== "horizon") this.route = { name: "content", id: i.id };
  }

  /** Opens a right-click menu at the pointer (or under the focused element). */
  openMenu(e: MouseEvent | KeyboardEvent, items: MenuItem[]) {
    e.preventDefault();
    e.stopPropagation();
    if (e instanceof MouseEvent && e.clientX + e.clientY > 0) {
      this.menu = { x: e.clientX, y: e.clientY, items };
    } else {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      this.menu = { x: r.left + 12, y: r.bottom - 8, items };
    }
  }

  /** Menu for an instance, wherever it's shown. */
  instanceMenu(i: Instance): MenuItem[] {
    const running = isActive(this.sessions[i.id]);
    return [
      running
        ? { label: "Stop", icon: "stop", run: () => void this.stop(i.id) }
        : { label: "Play", icon: "play_arrow", run: () => void this.play(i.id) },
      { label: "Instance settings", icon: "tune", run: () => { this.select(i.id); this.sheetOpen = true; } },
      ...(i.loader === "horizon" ? [] : [{ label: "Mods and packs", icon: "extension", run: () => (this.route = { name: "content", id: i.id }) }]),
      { label: "Game log", icon: "terminal", run: () => (this.route = { name: "logs", id: i.id }) },
      { label: "Open game folder", icon: "folder_open", run: () => void api.openFolder("instance", i.id).catch((e) => this.toast(errorText(e))) },
      "separator",
      { label: i.icon ? "Change picture" : "Choose picture", icon: "image", run: () => void this.chooseIcon(i.id) },
      ...(i.icon ? [{ label: "Use block", icon: "deployed_code", run: () => void this.clearIcon(i.id) }] : []),
      { label: "Back up worlds now", icon: "backup", disabled: running, run: () => void this.backupNow(i.id) },
      "separator",
      { label: "Delete", icon: "delete", danger: true, disabled: running, run: () => this.askDelete(i.id) },
    ];
  }

  async backupNow(id: string) {
    this.toast("Backing up worlds");
    try {
      const n = await api.backupNow(id);
      this.toast(n === 0 ? "There are no worlds to back up yet" : `Backed up ${n} world${n === 1 ? "" : "s"}`);
    } catch (e) {
      this.toast(errorText(e));
    }
  }

  /** Plays `id`. Without an account the game opens in the official Minecraft Launcher. */
  async play(id: string, repair = false) {
    try {
      await api.launch(id, repair);
      void this.pollSessions();
    } catch (e) {
      this.toast(errorText(e));
    }
  }

  async stop(id: string) {
    await api.stop(id);
    void this.pollSessions();
  }

  async createInstance(name: string, version: string, material: string | null, loader: Instance["loader"] = "vanilla", loaderVersion: string | null = null) {
    const inst = await api.createInstance(name, version, material, loader, loaderVersion);
    await this.refreshInstances();
    this.selectedId = inst.id;
    this.route = { name: "home" };
    this.toast(`Created ${inst.name}`);
  }

  async updateInstance(inst: Instance, patch: { name?: string; material?: string | null; overrides?: Overrides }) {
    try {
      const updated = await api.updateInstance(
        inst.id,
        patch.name ?? inst.name,
        patch.material === undefined ? inst.material : patch.material,
        patch.overrides ?? inst.overrides,
      );
      this.#replace(updated);
    } catch (e) {
      this.toast(errorText(e));
    }
  }

  #replace(updated: Instance) {
    this.instances = this.instances.map((i) => (i.id === updated.id ? updated : i));
  }

  /** Lets the player pick a picture for the instance. */
  async chooseIcon(id: string) {
    const path = await openDialog({
      title: "Choose a picture",
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp"] }],
    });
    if (typeof path !== "string") return;
    try {
      this.#replace(await api.setInstanceIcon(id, path));
    } catch (e) {
      this.toast(errorText(e));
    }
  }

  async clearIcon(id: string) {
    try {
      this.#replace(await api.clearInstanceIcon(id));
    } catch (e) {
      this.toast(errorText(e));
    }
  }

  /** Delete, asking first unless the user turned the question off. */
  askDelete(id: string) {
    if (this.settings?.confirm_delete === false) void this.deleteInstance(id);
    else this.dialog = { delete: id };
  }

  async deleteInstance(id: string) {
    const name = this.instances.find((i) => i.id === id)?.name ?? "Instance";
    try {
      await api.deleteInstance(id);
      this.sheetOpen = false;
      await this.refreshInstances();
      if (this.selectedId === id) this.selectedId = this.instances[0]?.id ?? null;
      this.toast(`Deleted ${name}`);
    } catch (e) {
      this.toast(errorText(e));
    }
  }

  #saveSeq = 0;

  async saveSettings(next: Settings) {
    this.settings = next;
    const seq = ++this.#saveSeq;
    try {
      const saved = await api.saveSettings(next);
      // Ignore responses that a newer change has already superseded.
      if (seq === this.#saveSeq) this.settings = saved;
    } catch (e) {
      this.toast(`Couldn't save settings: ${errorText(e)}`);
    }
  }
}

export function isActive(s: SessionView | undefined): boolean {
  return s?.phase.kind === "preparing" || s?.phase.kind === "running";
}

export const app = new AppState();
