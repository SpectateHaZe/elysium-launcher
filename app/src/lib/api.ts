// Typed wrappers around the Tauri commands in src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export interface Resolution {
  width: number;
  height: number;
}

export interface Overrides {
  java_path?: string | null;
  min_memory_mb?: number | null;
  max_memory_mb?: number | null;
  jvm_args?: string | null;
  resolution?: Resolution | null;
}

export type ThemePref = "system" | "light" | "dark";
export type TextSize = "small" | "default" | "large";

export interface Appearance {
  palette: string;
  accent: string;
  custom_accent: string;
  custom_tint: string;
  text_size: TextSize;
  reduce_transparency: boolean;
}

export interface Settings {
  java_path: string | null;
  min_memory_mb: number;
  max_memory_mb: number;
  jvm_args: string;
  resolution: Resolution | null;
  concurrent_downloads: number;
  theme: ThemePref;
  appearance: Appearance;
  hide_on_launch: boolean;
}

export interface Account {
  uuid: string;
  username: string;
  kind: "offline";
}

export interface AccountStore {
  accounts: Account[];
  active: string | null;
}

export interface Instance {
  id: string;
  name: string;
  game_version: string;
  material: string | null;
  created: string;
  last_played: string | null;
  playtime_secs: number;
  overrides: Overrides;
}

export type Phase =
  | { kind: "preparing" }
  | { kind: "running"; since: string }
  | { kind: "finished" }
  | { kind: "crashed"; code: number | null }
  | { kind: "failed"; message: string }
  | { kind: "stopped" };

export interface SessionView {
  phase: Phase;
  stage: string;
  done: number;
  total: number;
  log_seq: number;
}

export interface LogEntry {
  seq: number;
  level: "debug" | "info" | "warn" | "error";
  source: "game" | "launcher";
  text: string;
}

export type VersionType = "release" | "snapshot" | "old_beta" | "old_alpha" | "other";

export interface ManifestEntry {
  id: string;
  type: VersionType;
  releaseTime: string;
}

export interface VersionManifest {
  latest: { release: string; snapshot: string };
  versions: ManifestEntry[];
}

export interface Bootstrap {
  settings: Settings;
  accounts: AccountStore;
  instances: Instance[];
  data_dir: string;
  version: string;
}

export interface JavaProbe {
  version: string;
  major: number;
  arch: string;
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  instances: () => invoke<Instance[]>("instances"),
  versions: () => invoke<VersionManifest>("versions"),
  createInstance: (name: string, version: string, material: string | null) =>
    invoke<Instance>("create_instance", { name, version, material }),
  updateInstance: (id: string, name: string, material: string | null, overrides: Overrides) =>
    invoke<Instance>("update_instance", { id, name, material, overrides }),
  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  addOfflineAccount: (username: string) => invoke<AccountStore>("add_offline_account", { username }),
  removeAccount: (uuid: string) => invoke<AccountStore>("remove_account", { uuid }),
  setActiveAccount: (uuid: string) => invoke<AccountStore>("set_active_account", { uuid }),
  probeJava: (path: string) => invoke<JavaProbe>("probe_java", { path }),
  openFolder: (target: "instance" | "logs" | "data", id?: string) =>
    invoke<void>("open_folder", { target, id: id ?? null }),
  sessions: () => invoke<Record<string, SessionView>>("sessions"),
  logs: (id: string, after: number) => invoke<LogEntry[]>("logs", { id, after }),
  clearLogs: (id: string) => invoke<void>("clear_logs", { id }),
  launch: (id: string, repair: boolean) => invoke<void>("launch", { id, repair }),
  stop: (id: string) => invoke<void>("stop", { id }),
  quit: () => invoke<void>("quit"),
};

/** Tauri rejects with the Rust error string; normalise anything else. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Something went wrong";
}
