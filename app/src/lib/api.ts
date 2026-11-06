// Typed wrappers around the Tauri commands in src-tauri/src/commands.rs.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

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
  /** `none`, a preset id from GRADIENTS, or `custom`. */
  gradient: string;
  gradient_from: string;
  gradient_to: string;
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
  msa_client_id: string;
  horizon_auto_update: boolean;
  onboarded: boolean;
  always_official_launcher: boolean;
  optimized_java_flags: boolean;
  auto_backup: boolean;
  share_files: boolean;
  backups_kept: number;
  discord_presence: boolean;
}

export interface Skin {
  url: string;
  variant: "classic" | "slim";
}

export interface Account {
  uuid: string;
  username: string;
  kind: "offline" | "microsoft";
  owns_game: boolean;
  skin?: Skin | null;
}

export interface AccountStore {
  accounts: Account[];
  active: string | null;
}

export type Loader = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge" | "horizon";

export const LOADER_NAMES: Record<Loader, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  quilt: "Quilt",
  forge: "Forge",
  neoforge: "NeoForge",
  horizon: "Horizon",
};

export interface PackOrigin {
  source: string;
  project_id: string | null;
  version_id: string | null;
  name: string;
}

export interface Instance {
  id: string;
  name: string;
  game_version: string;
  material: string | null;
  loader: Loader;
  loader_version: string | null;
  pack: PackOrigin | null;
  created: string;
  last_played: string | null;
  playtime_secs: number;
  overrides: Overrides;
  /** Another launcher's game folder this instance shares instead of its own copy. */
  linked_dir: string | null;
  icon: string | null;
  icon_version: number;
}

export interface Overview {
  mods: number;
  worlds: number;
  size: number;
  startup_ms: number | null;
  screenshots: { path: string; taken: number }[];
}

export interface StartupReport {
  at: string;
  total_ms: number;
  mods: { name: string; ms: number }[];
}

export interface Snapshot {
  id: string;
  at: string;
  label: string;
  verified: boolean;
}

/** URL for a local image the window is allowed to read. */
export function fileSrc(path: string): string {
  return convertFileSrc(path);
}

/** URL for an instance's picture, or null when it uses its block. */
export function iconSrc(i: Pick<Instance, "icon" | "icon_version">): string | null {
  return i.icon ? `${convertFileSrc(i.icon)}?v=${i.icon_version}` : null;
}

export type Phase =
  | { kind: "preparing" }
  | { kind: "running"; since: string }
  | { kind: "finished" }
  | { kind: "crashed"; code: number | null; diagnosis: Diagnosis | null }
  | { kind: "handed_off" }
  | { kind: "failed"; message: string }
  | { kind: "stopped" };

export interface SessionView {
  phase: Phase;
  stage: string;
  done: number;
  total: number;
  log_seq: number;
  diagnosis: Diagnosis | null;
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
  curseforge_configured: boolean;
  curseforge_builtin: boolean;
  horizon_supported: boolean;
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

export type Source = "modrinth" | "curseforge";
export type SourceFilter = "all" | Source;
export type ContentKind = "mod" | "modpack" | "resourcepack" | "shader" | "datapack";
export type SortOrder = "relevance" | "downloads" | "updated" | "newest";

export interface SearchQuery {
  query: string;
  kind: ContentKind;
  game_version: string | null;
  loader: Loader | null;
  sort: SortOrder;
  offset: number;
  limit: number;
}

export interface ProjectHit {
  source: Source;
  id: string;
  slug: string;
  title: string;
  summary: string;
  author: string;
  icon_url: string | null;
  downloads: number;
  updated: string | null;
  kind: ContentKind;
  categories: string[];
}

export interface SearchPage {
  hits: ProjectHit[];
  total: number;
}

export interface ProjectDetails {
  hit: ProjectHit;
  body: string;
  body_format: "markdown" | "html";
  website: string | null;
  gallery: string[];
}

export interface FileRef {
  url: string | null;
  filename: string;
  sha1: string | null;
  size: number | null;
  primary: boolean;
}

export interface ProjectVersion {
  source: Source;
  project_id: string;
  id: string;
  name: string;
  version_number: string;
  game_versions: string[];
  loaders: string[];
  release_type: "release" | "beta" | "alpha";
  published: string;
  files: FileRef[];
}

export interface ContentEntry {
  file: string;
  kind: ContentKind;
  source: Source | null;
  project_id: string | null;
  version_id: string | null;
  title: string;
  version_number: string | null;
  sha1: string | null;
  url: string | null;
  icon_url: string | null;
}

export interface InstalledContent {
  entry: ContentEntry;
  enabled: boolean;
  size: number;
  local_icon: string | null;
}

export interface ContentUpdate {
  file: string;
  current: string | null;
  latest: ProjectVersion;
}

export interface ManualDownload {
  title: string;
  file_name: string;
  page_url: string;
  sha1: string | null;
  size: number | null;
  entry: ContentEntry;
}

export interface InstallReport {
  installed: ContentEntry[];
  via_modrinth: string[];
  manual: ManualDownload[];
  skipped: string[];
}

export interface PackImport {
  instance_id: string;
  manual: ManualDownload[];
}

export type JobState = { kind: "running" } | { kind: "done"; result: unknown } | { kind: "failed"; message: string };

export interface Job {
  id: number;
  title: string;
  stage: string;
  done: number;
  total: number;
  state: JobState;
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
}

export interface HorizonStatus {
  installed: string | null;
  latest: string | null;
  update_available: boolean;
  supported: boolean;
}

export type MsaStatus =
  | { kind: "idle" }
  | { kind: "waiting" }
  | { kind: "done"; accounts: AccountStore }
  | { kind: "failed"; message: string };

export interface InstallRequest {
  instance_id: string;
  source: Source;
  project_id: string;
  version_id: string | null;
  kind: ContentKind;
  world: string | null;
  title: string;
}

export type Fix =
  | { kind: "more_memory" }
  | { kind: "install_mod"; id: string; name: string }
  | { kind: "check_mod"; name: string }
  | { kind: "automatic_java"; major: number | null }
  | { kind: "update_drivers" }
  | { kind: "remove_duplicate" }
  | { kind: "disable_mod"; file: string; name: string }
  | { kind: "undo_update"; snapshot: string; label: string };

export interface Diagnosis {
  title: string;
  detail: string;
  fixes: Fix[];
}

export interface Foreign {
  source: "prism" | "multimc" | "curseforge" | "official";
  name: string;
  game_version: string;
  loader: Loader;
  loader_version: string | null;
  game_dir: string;
  icon: string | null;
}

export interface Backup {
  world: string;
  file: string;
  created: string;
  size: number;
}

export interface SystemInfo {
  total_memory_mb: number;
  recommended_max_mb: number;
  discord_available: boolean;
}

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  instances: () => invoke<Instance[]>("instances"),
  versions: () => invoke<VersionManifest>("versions"),
  createInstance: (name: string, version: string, material: string | null, loader: Loader, loaderVersion: string | null) =>
    invoke<Instance>("create_instance", { name, version, material, loader, loaderVersion }),
  updateInstance: (id: string, name: string, material: string | null, overrides: Overrides) =>
    invoke<Instance>("update_instance", { id, name, material, overrides }),
  setInstanceIcon: (id: string, path: string) => invoke<Instance>("set_instance_icon", { id, path }),
  clearInstanceIcon: (id: string) => invoke<Instance>("clear_instance_icon", { id }),
  instanceOverview: (id: string) => invoke<Overview>("instance_overview", { id }),
  openScreenshot: (id: string, name: string) => invoke<void>("open_screenshot", { id, name }),
  shareFiles: () => invoke<{ linked: number; saved: number }>("share_files"),
  startupReport: (id: string) => invoke<StartupReport | null>("startup_report", { id }),
  snapshotTake: (id: string, label: string) => invoke<Snapshot>("snapshot_take", { id, label }),
  snapshotPending: (id: string) => invoke<Snapshot | null>("snapshot_pending", { id }),
  snapshotRestore: (id: string, snapshot: string) => invoke<void>("snapshot_restore", { id, snapshot }),
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
  loaderVersions: (loader: Loader, mc: string) => invoke<LoaderVersion[]>("loader_versions", { loader, mc }),
  search: (source: SourceFilter, query: SearchQuery) => invoke<SearchPage>("search", { source, query }),
  project: (source: Source, id: string) => invoke<ProjectDetails>("project", { source, id }),
  projectVersions: (source: Source, id: string, kind: ContentKind, instanceId: string | null) =>
    invoke<ProjectVersion[]>("project_versions", { source, id, kind, instanceId }),
  installContent: (request: InstallRequest) => invoke<number>("install_content", { request }),
  installPack: (source: Source, projectId: string, versionId: string, title: string, iconUrl: string | null) =>
    invoke<number>("install_pack", { request: { source, project_id: projectId, version_id: versionId, title, icon_url: iconUrl } }),
  importPackFile: (path: string) => invoke<number>("import_pack_file", { path }),
  exportPack: (id: string, path: string) => invoke<void>("export_pack", { id, path }),
  contentList: (id: string) => invoke<InstalledContent[]>("content_list", { id }),
  contentSetEnabled: (id: string, file: string, enabled: boolean) => invoke<void>("content_set_enabled", { id, file, enabled }),
  contentRemove: (id: string, file: string) => invoke<void>("content_remove", { id, file }),
  contentUpdates: (id: string) => invoke<ContentUpdate[]>("content_updates", { id }),
  worlds: (id: string) => invoke<string[]>("worlds", { id }),
  manualCheck: (id: string, items: ManualDownload[]) =>
    invoke<{ found: string[]; downloads_dir: string | null }>("manual_check", { id, items }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
  jobs: () => invoke<Job[]>("jobs"),
  jobCancel: (id: number) => invoke<void>("job_cancel", { id }),
  jobDismiss: (id: number) => invoke<void>("job_dismiss", { id }),
  setCurseforgeKey: (key: string) => invoke<boolean>("set_curseforge_key", { key }),
  msaStart: () => invoke<{ user_code: string; verification_uri: string; expires_in: number }>("msa_start"),
  msaStatus: () => invoke<MsaStatus>("msa_status"),
  msaCancel: () => invoke<void>("msa_cancel"),
  uploadSkin: (path: string, slim: boolean) => invoke<AccountStore>("upload_skin", { path, slim }),
  horizonStatus: () => invoke<HorizonStatus>("horizon_status"),
  horizonUpdate: () => invoke<number>("horizon_update"),
  horizonMinecraft: () => invoke<string>("horizon_minecraft"),
  importScan: () => invoke<Foreign[]>("import_scan"),
  importScanFolder: (path: string) => invoke<Foreign[]>("import_scan_folder", { path }),
  importRun: (items: Foreign[], linked: boolean) => invoke<number>("import_run", { items, linked }),
  backups: (id: string) => invoke<Backup[]>("backups", { id }),
  backupNow: (id: string) => invoke<number>("backup_now", { id }),
  backupRestore: (id: string, world: string, file: string) => invoke<void>("backup_restore", { id, world, file }),
  installPerformanceMods: (id: string) => invoke<number>("install_performance_mods", { id }),
  systemInfo: () => invoke<SystemInfo>("system_info"),
};

/** Tauri rejects with the Rust error string; normalise anything else. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Something went wrong";
}
