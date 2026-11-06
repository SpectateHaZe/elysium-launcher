// Starting installs and reacting when they finish (shared by Discover and the sheet).
import { app } from "./store.svelte";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, errorText, type ContentKind, type InstallReport, type PackImport, type ProjectHit } from "./api";

function plural(n: number, word: string) {
  return `${n} ${word}${n === 1 ? "" : "s"}`;
}

/** Installs a project (or a specific version) into the Discover target instance. */
export async function installProject(hit: ProjectHit, versionId: string | null = null, world: string | null = null) {
  if (hit.kind === "modpack") return installModpack(hit, versionId);
  const instanceId = app.targetId;
  if (!instanceId) {
    app.toast("Choose an instance to install into");
    return;
  }
  try {
    const job = await api.installContent({
      instance_id: instanceId,
      source: hit.source,
      project_id: hit.id,
      version_id: versionId,
      kind: hit.kind,
      world,
      title: hit.title,
    });
    pending.add(key(hit));
    app.watchJob(job, (j) => {
      pending.delete(key(hit));
      void app.dismissJob(j.id);
      if (j.state.kind === "failed") {
        app.toast(j.state.message);
        return;
      }
      if (j.state.kind !== "done") return;
      const r = j.state.result as InstallReport;
      const deps = r.installed.length - 1;
      let text = `Installed ${hit.title}${deps > 0 ? ` and ${plural(deps, "dependency")}` : ""}`;
      if (r.via_modrinth.length) text += `. ${r.via_modrinth.join(", ")} came from Modrinth`;
      app.toast(text);
      if (r.manual.length) app.manual = { instanceId, items: r.manual };
      installedVersion.bump();
    });
  } catch (e) {
    app.toast(errorText(e));
  }
}

async function installModpack(hit: ProjectHit, versionId: string | null) {
  try {
    let version = versionId;
    if (!version) {
      const versions = await api.projectVersions(hit.source, hit.id, "modpack", null);
      const best = versions.find((v) => v.release_type === "release") ?? versions[0];
      if (!best) {
        app.toast(`${hit.title} has no downloadable versions`);
        return;
      }
      version = best.id;
    }
    const job = await api.installPack(hit.source, hit.id, version, hit.title, hit.icon_url);
    pending.add(key(hit));
    app.watchJob(job, (j) => {
      pending.delete(key(hit));
      void app.dismissJob(j.id);
      if (j.state.kind === "failed") {
        app.toast(j.state.message);
        return;
      }
      if (j.state.kind !== "done") return;
      void finishPackImport(j.state.result as PackImport, hit.title);
    });
  } catch (e) {
    app.toast(errorText(e));
  }
}

export async function finishPackImport(r: PackImport, title: string) {
  await app.refreshInstances();
  app.selectedId = r.instance_id;
  app.route = { name: "home" };
  app.toast(`${title} is ready to play`);
  if (r.manual.length) app.manual = { instanceId: r.instance_id, items: r.manual };
}

/** Imports a .mrpack or CurseForge export zip, from a picker when no path is given. Throws if starting fails. */
export async function importPackFile(path?: string) {
  if (!path) {
    const picked = await openDialog({ title: "Import a modpack", multiple: false, filters: [{ name: "Modpacks", extensions: ["mrpack", "zip"] }] });
    if (typeof picked !== "string") return false;
    path = picked;
  }
  const job = await api.importPackFile(path);
  app.dialog = null;
  app.watchJob(job, (j) => {
    void app.dismissJob(j.id);
    if (j.state.kind === "failed") app.toast(j.state.message);
    else if (j.state.kind === "done") void finishPackImport(j.state.result as PackImport, "The pack");
  });
  return true;
}

export function key(hit: { source: string; id: string }) {
  return `${hit.source}:${hit.id}`;
}

/** Projects with an install in progress (reactive). */
export const pending = new (class {
  items = $state<string[]>([]);
  add(k: string) {
    this.items = [...this.items, k];
  }
  delete(k: string) {
    this.items = this.items.filter((x) => x !== k);
  }
  has(k: string) {
    return this.items.includes(k);
  }
})();

/** Bumped whenever content changes, so lists reload. */
export const installedVersion = new (class {
  n = $state(0);
  bump() {
    this.n++;
  }
})();

export const KIND_LABELS: Record<ContentKind, string> = {
  mod: "Mods",
  modpack: "Modpacks",
  resourcepack: "Resource packs",
  shader: "Shaders",
  datapack: "Data packs",
};

export function compactNumber(n: number): string {
  return new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(n);
}
