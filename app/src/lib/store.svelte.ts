// Global app state (Svelte 5 runes). Components read fields directly and call methods.
import { listen } from "@tauri-apps/api/event";
import {
  api,
  errorText,
  type AccountStore,
  type Instance,
  type Overrides,
  type SessionView,
  type Settings,
  type VersionManifest,
} from "./api";

export type Route =
  | { name: "home" }
  | { name: "logs"; id: string }
  | { name: "accounts" }
  | { name: "settings" };

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
  route = $state<Route>({ name: "home" });
  manifest = $state<VersionManifest | null>(null);
  manifestError = $state<string | null>(null);
  snack = $state<Snack | null>(null);
  dialog = $state<"new-instance" | "quit" | { delete: string } | null>(null);
  sheetOpen = $state(false);

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

  toast(text: string, action?: Snack["action"]) {
    if (this.#snackTimer) clearTimeout(this.#snackTimer);
    this.snack = { id: Date.now(), text, action };
    this.#snackTimer = setTimeout(() => (this.snack = null), action ? 8000 : 4000);
  }

  select(id: string) {
    this.selectedId = id;
  }

  async play(id: string, repair = false) {
    if (!this.activeAccount) {
      this.route = { name: "accounts" };
      this.toast("Add an account to start playing");
      return;
    }
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

  async createInstance(name: string, version: string, material: string | null) {
    const inst = await api.createInstance(name, version, material);
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
      this.instances = this.instances.map((i) => (i.id === updated.id ? updated : i));
    } catch (e) {
      this.toast(errorText(e));
    }
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
