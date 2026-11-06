//! Installing content into an instance, with dependencies, the CurseForge fallback
//! chain, and management of what's installed.
//!
//! Fallback for CurseForge files whose author disabled third-party downloads:
//! 1. the identical file on Modrinth, matched by SHA-1;
//! 2. otherwise a manual download: the player downloads it in the browser and the
//!    launcher picks it up from their Downloads folder, checking the hash.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::curseforge::{self, CurseForge};
use super::modrinth::Modrinth;
use super::{
    ContentKind, ProjectDetails, ProjectVersion, SearchPage, SearchQuery, Source, best_version,
    merge_results,
};
use crate::error::{Error, IoContext, Result};
use crate::fsutil;
use crate::http::{DownloadTask, Http, Verify};
use crate::instance::Instance;
use crate::progress::Progress;

const INDEX_FILE: &str = "content.json";
const DISABLED: &str = ".disabled";

/// What the launcher knows about a file it installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentEntry {
    /// Path relative to the game folder, without any `.disabled` suffix.
    pub file: String,
    pub kind: ContentKind,
    pub source: Option<Source>,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub title: String,
    pub version_number: Option<String>,
    pub sha1: Option<String>,
    pub url: Option<String>,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentIndex {
    #[serde(default)]
    pub entries: Vec<ContentEntry>,
}

impl ContentIndex {
    pub fn load(instance: &Instance) -> Result<Self> {
        Ok(fsutil::read_json(&instance.dir.join(INDEX_FILE))?.unwrap_or_default())
    }

    pub fn save(&self, instance: &Instance) -> Result<()> {
        fsutil::write_json_atomic(&instance.dir.join(INDEX_FILE), self)
    }

    fn upsert(&mut self, entry: ContentEntry) {
        self.entries.retain(|e| e.file != entry.file);
        self.entries.push(entry);
    }

    fn find_project(&self, source: Source, project_id: &str) -> Option<&ContentEntry> {
        self.entries
            .iter()
            .find(|e| e.source == Some(source) && e.project_id.as_deref() == Some(project_id))
    }
}

/// A file the player has to download in the browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualDownload {
    pub title: String,
    pub file_name: String,
    pub page_url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
    pub entry: ContentEntry,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstallReport {
    pub installed: Vec<ContentEntry>,
    /// Installed from Modrinth because CurseForge doesn't allow the download.
    pub via_modrinth: Vec<String>,
    pub manual: Vec<ManualDownload>,
    /// Optional or unavailable dependencies that were left out.
    pub skipped: Vec<String>,
}

/// Content in an instance's folders, joined with what the launcher knows about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledContent {
    pub entry: ContentEntry,
    pub enabled: bool,
    pub size: u64,
    /// Picture read from the file itself, for content without one from Modrinth or CurseForge.
    pub local_icon: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentUpdate {
    pub file: String,
    pub current: Option<String>,
    pub latest: ProjectVersion,
}

#[derive(Debug, Clone)]
pub struct ContentService {
    http: Http,
    pub modrinth: Modrinth,
    pub curseforge: Option<CurseForge>,
}

/// Which source(s) to search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceFilter {
    All,
    Modrinth,
    CurseForge,
}

impl ContentService {
    pub fn new(http: Http, curseforge_key: &str) -> Self {
        Self {
            modrinth: Modrinth::new(http.clone()),
            curseforge: CurseForge::new(http.clone(), curseforge_key),
            http,
        }
    }

    pub fn with_clients(http: Http, modrinth: Modrinth, curseforge: Option<CurseForge>) -> Self {
        Self {
            http,
            modrinth,
            curseforge,
        }
    }

    pub(crate) fn http_ref(&self) -> &Http {
        &self.http
    }

    fn cf(&self) -> Result<&CurseForge> {
        self.curseforge.as_ref().ok_or_else(|| {
            Error::Invalid("add a CurseForge API key in Settings to use CurseForge".into())
        })
    }

    /// Searches one or both sources. With `All`, a CurseForge failure (e.g. no key)
    /// still returns Modrinth's results.
    pub async fn search(&self, filter: SourceFilter, q: &SearchQuery) -> Result<SearchPage> {
        match filter {
            SourceFilter::Modrinth => self.modrinth.search(q).await,
            SourceFilter::CurseForge => self.cf()?.search(q).await,
            SourceFilter::All => {
                let cf = async {
                    match &self.curseforge {
                        Some(c) => c.search(q).await.ok(),
                        None => None,
                    }
                };
                let (m, c) = tokio::join!(self.modrinth.search(q), cf);
                let m = m?;
                let (c_hits, c_total) = c.map(|p| (p.hits, p.total)).unwrap_or_default();
                Ok(SearchPage {
                    total: m.total + c_total,
                    hits: merge_results(m.hits, c_hits),
                })
            }
        }
    }

    pub async fn project(&self, source: Source, id: &str) -> Result<ProjectDetails> {
        match source {
            Source::Modrinth => self.modrinth.project(id).await,
            Source::CurseForge => self.cf()?.project(id).await,
        }
    }

    /// Versions compatible with the instance (game version, and loader for mods).
    pub async fn versions(
        &self,
        source: Source,
        id: &str,
        kind: ContentKind,
        instance: &Instance,
    ) -> Result<Vec<ProjectVersion>> {
        let gv = Some(instance.config.game_version.as_str());
        let loader = Some(instance.config.loader);
        match source {
            Source::Modrinth => self.modrinth.versions(id, kind, gv, loader).await,
            Source::CurseForge => self.cf()?.versions(id, kind, gv, loader).await,
        }
    }

    /// Installs a project (and its required dependencies) into `instance`.
    /// Data packs need the `world` they go into.
    #[allow(clippy::too_many_arguments)]
    pub async fn install(
        &self,
        instance: &Instance,
        source: Source,
        project_id: &str,
        version_id: Option<&str>,
        kind: ContentKind,
        world: Option<&str>,
        progress: Arc<Progress>,
    ) -> Result<InstallReport> {
        if kind == ContentKind::Modpack {
            return Err(Error::Invalid(
                "modpacks are installed as new instances".into(),
            ));
        }
        let folder = match kind {
            ContentKind::DataPack => {
                let world = world
                    .ok_or_else(|| Error::Invalid("choose a world for the data pack".into()))?;
                if world.is_empty() || world.contains(['/', '\\']) || world == ".." {
                    return Err(Error::Invalid("invalid world name".into()));
                }
                format!("saves/{world}/datapacks")
            }
            k => k.folder().to_owned(),
        };

        let mut index = ContentIndex::load(instance)?;
        let mut report = InstallReport::default();
        let mut queue: VecDeque<(String, Option<String>, bool)> =
            VecDeque::from([(project_id.to_owned(), version_id.map(str::to_owned), true)]);
        let mut seen = HashSet::new();

        while let Some((pid, vid, top)) = queue.pop_front() {
            progress.check()?;
            if !seen.insert(pid.clone()) {
                continue;
            }
            // Dependencies already present are left alone.
            if !top && index.find_project(source, &pid).is_some() {
                continue;
            }
            let version = match &vid {
                Some(v) => match source {
                    Source::Modrinth => Some(self.modrinth.version(v).await?),
                    Source::CurseForge => {
                        let id: u64 = v
                            .parse()
                            .map_err(|_| Error::Invalid(format!("bad file id {v}")))?;
                        self.cf()?
                            .files_by_id(&[id])
                            .await?
                            .first()
                            .map(|f| f.version())
                    }
                },
                None => {
                    let dep_kind = if top { kind } else { ContentKind::Mod };
                    let list = self.versions(source, &pid, dep_kind, instance).await?;
                    best_version(&list).cloned()
                }
            };
            let Some(version) = version else {
                if top {
                    return Err(Error::Invalid(format!(
                        "no version for Minecraft {} {}",
                        instance.config.game_version,
                        if kind.uses_loader() {
                            instance.config.loader.name()
                        } else {
                            ""
                        }
                    )));
                }
                report.skipped.push(pid);
                continue;
            };
            let details = self.project(source, &pid).await.ok();
            let title = details
                .as_ref()
                .map_or_else(|| pid.clone(), |d| d.hit.title.clone());
            let file = version
                .primary_file()
                .ok_or_else(|| Error::Invalid(format!("{title} has no files")))?
                .clone();
            let dest_folder = if top {
                folder.clone()
            } else {
                ContentKind::Mod.folder().to_owned()
            };
            let rel = format!("{dest_folder}/{}", sanitize_file_name(&file.filename)?);
            let entry = ContentEntry {
                file: rel.clone(),
                kind: if top { kind } else { ContentKind::Mod },
                source: Some(source),
                project_id: Some(pid.clone()),
                version_id: Some(version.id.clone()),
                title: title.clone(),
                version_number: Some(version.version_number.clone()),
                sha1: file.sha1.clone(),
                url: file.url.clone(),
                icon_url: details.as_ref().and_then(|d| d.hit.icon_url.clone()),
            };

            progress.set_stage(format!("Downloading {title}"), file.size.unwrap_or(0));
            let url = match &file.url {
                Some(u) => Some(u.clone()),
                None => {
                    let alt = self.modrinth_mirror(file.sha1.as_deref()).await;
                    if alt.is_some() {
                        report.via_modrinth.push(title.clone());
                    }
                    alt
                }
            };
            match url {
                Some(url) => {
                    let dest = instance.game_dir().join(&rel);
                    let task = DownloadTask::new(&url, &dest)
                        .sha1(file.sha1.clone())
                        .size(file.size);
                    self.http
                        .download_all(vec![task], 1, Verify::Hash, Arc::clone(&progress))
                        .await?;
                    replace_previous(instance, &mut index, source, &pid, &rel)?;
                    let mut entry = entry;
                    entry.url = Some(url);
                    index.upsert(entry.clone());
                    index.save(instance)?;
                    report.installed.push(entry);
                }
                None => {
                    let slug = details
                        .as_ref()
                        .map_or_else(|| pid.clone(), |d| d.hit.slug.clone());
                    report.manual.push(ManualDownload {
                        title: title.clone(),
                        file_name: file.filename.clone(),
                        page_url: curseforge::manual_download_url(entry.kind, &slug, &version.id),
                        sha1: file.sha1.clone(),
                        size: file.size,
                        entry,
                    });
                }
            }
            for dep in version.dependencies.iter().filter(|d| d.required) {
                match (&dep.project_id, &dep.version_id) {
                    (Some(p), v) => queue.push_back((p.clone(), v.clone(), false)),
                    (None, Some(v)) if source == Source::Modrinth => {
                        if let Ok(dv) = self.modrinth.version(v).await {
                            queue.push_back((dv.project_id, Some(dv.id), false));
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(report)
    }

    /// The same file on Modrinth, by SHA-1, if it exists there.
    pub async fn modrinth_mirror(&self, sha1: Option<&str>) -> Option<String> {
        let sha1 = sha1?.to_lowercase();
        let found = self
            .modrinth
            .versions_by_hash(std::slice::from_ref(&sha1))
            .await
            .ok()?;
        found
            .get(&sha1)?
            .files
            .iter()
            .find(|f| f.sha1.as_deref().map(str::to_lowercase).as_deref() == Some(sha1.as_str()))
            .and_then(|f| f.url.clone())
    }

    /// Newer versions (from Modrinth) of the instance's installed files.
    pub async fn check_updates(&self, instance: &Instance) -> Result<Vec<ContentUpdate>> {
        let items = list(instance)?;
        let mut by_hash = Vec::new();
        for item in items.iter().filter(|i| {
            i.entry.kind == ContentKind::Mod
                || i.entry.kind == ContentKind::ResourcePack
                || i.entry.kind == ContentKind::Shader
        }) {
            let sha1 = match &item.entry.sha1 {
                Some(s) => s.clone(),
                None => {
                    let path = current_path(instance, &item.entry.file, item.enabled);
                    // Unpacked resource and shader packs are folders: nothing to look up.
                    if path.is_dir() {
                        continue;
                    }
                    match fsutil::sha1_file(&path) {
                        Ok(s) => s,
                        Err(e) => {
                            // One locked or unreadable file shouldn't stop the whole check.
                            tracing::warn!("skipping {} in update check: {e}", item.entry.file);
                            continue;
                        }
                    }
                }
            };
            by_hash.push((sha1, item));
        }
        let mut updates = Vec::new();
        let groups: [(ContentKind, Vec<&str>); 3] = [
            (
                ContentKind::Mod,
                instance.config.loader.modrinth_id().into_iter().collect(),
            ),
            (ContentKind::ResourcePack, vec!["minecraft"]),
            (
                ContentKind::Shader,
                vec!["iris", "optifine", "canvas", "vanilla"],
            ),
        ];
        for (kind, loaders) in groups {
            let hashes: Vec<String> = by_hash
                .iter()
                .filter(|(_, i)| i.entry.kind == kind)
                .map(|(h, _)| h.clone())
                .collect();
            if hashes.is_empty() || loaders.is_empty() {
                continue;
            }
            let latest = self
                .modrinth
                .updates_by_hash(&hashes, &instance.config.game_version, &loaders)
                .await?;
            for (hash, item) in by_hash.iter().filter(|(_, i)| i.entry.kind == kind) {
                if let Some(v) = latest.get(hash)
                    && v.primary_file().and_then(|f| f.sha1.as_deref()) != Some(hash.as_str())
                {
                    updates.push(ContentUpdate {
                        file: item.entry.file.clone(),
                        current: item.entry.version_number.clone(),
                        latest: v.clone(),
                    });
                }
            }
        }
        Ok(updates)
    }
}

fn sanitize_file_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(Error::Invalid(format!(
            "refusing unsafe file name {name:?}"
        )));
    }
    Ok(name.to_owned())
}

/// Removes the file of an older version of the same project.
fn replace_previous(
    instance: &Instance,
    index: &mut ContentIndex,
    source: Source,
    project_id: &str,
    new_rel: &str,
) -> Result<()> {
    let old: Vec<String> = index
        .entries
        .iter()
        .filter(|e| {
            e.source == Some(source)
                && e.project_id.as_deref() == Some(project_id)
                && e.file != new_rel
        })
        .map(|e| e.file.clone())
        .collect();
    for rel in old {
        for path in [
            instance.game_dir().join(&rel),
            instance.game_dir().join(format!("{rel}{DISABLED}")),
        ] {
            if path.exists() {
                std::fs::remove_file(&path).at(&path)?;
            }
        }
        index.entries.retain(|e| e.file != rel);
    }
    Ok(())
}

fn current_path(instance: &Instance, rel: &str, enabled: bool) -> PathBuf {
    let rel = if enabled {
        rel.to_owned()
    } else {
        format!("{rel}{DISABLED}")
    };
    instance.game_dir().join(rel)
}

/// Mods, resource packs and shaders in the instance, known or not.
pub fn list(instance: &Instance) -> Result<Vec<InstalledContent>> {
    let index = ContentIndex::load(instance)?;
    let mut icons = super::local::IconCache::open(instance);
    let mut out = Vec::new();
    for kind in [
        ContentKind::Mod,
        ContentKind::ResourcePack,
        ContentKind::Shader,
    ] {
        let dir = instance.game_dir().join(kind.folder());
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let (base, enabled) = match name.strip_suffix(DISABLED) {
                Some(b) => (b.to_owned(), false),
                None => (name.clone(), true),
            };
            let wanted = match kind {
                ContentKind::Mod => base.ends_with(".jar"),
                // Shader packs and resource packs can be folders or zips.
                _ => meta.is_dir() || base.ends_with(".zip"),
            };
            if !wanted {
                continue;
            }
            let rel = format!("{}/{base}", kind.folder());
            let known = index.entries.iter().find(|e| e.file == rel).cloned();
            let (local_name, local_icon) = if known.as_ref().is_some_and(|k| k.icon_url.is_some()) {
                (None, None)
            } else {
                icons.get(&entry.path(), kind, &meta)
            };
            out.push(InstalledContent {
                local_icon,
                entry: known.unwrap_or(ContentEntry {
                    file: rel,
                    kind,
                    source: None,
                    project_id: None,
                    version_id: None,
                    title: local_name.unwrap_or_else(|| {
                        base.trim_end_matches(".jar")
                            .trim_end_matches(".zip")
                            .to_owned()
                    }),
                    version_number: None,
                    sha1: None,
                    url: None,
                    icon_url: None,
                }),
                enabled,
                size: meta.len(),
            });
        }
    }
    icons.finish();
    out.sort_by_key(|a| a.entry.title.to_lowercase());
    Ok(out)
}

fn checked_rel(rel: &str) -> Result<&str> {
    let ok = ["mods/", "resourcepacks/", "shaderpacks/"]
        .iter()
        .any(|p| rel.starts_with(p))
        && !rel.contains("..")
        && !rel[rel.find('/').unwrap_or(0) + 1..].contains(['/', '\\']);
    if ok {
        Ok(rel)
    } else {
        Err(Error::Invalid(format!("not a content file: {rel}")))
    }
}

/// Enables or disables a file by adding/removing the `.disabled` suffix.
pub fn set_enabled(instance: &Instance, rel: &str, enabled: bool) -> Result<()> {
    let rel = checked_rel(rel)?;
    let on = instance.game_dir().join(rel);
    let off = instance.game_dir().join(format!("{rel}{DISABLED}"));
    let (from, to) = if enabled { (off, on) } else { (on, off) };
    if from.exists() {
        std::fs::rename(&from, &to).at(&from)?;
    }
    Ok(())
}

/// Deletes a content file and forgets it.
pub fn remove(instance: &Instance, rel: &str) -> Result<()> {
    let rel = checked_rel(rel)?;
    for path in [
        instance.game_dir().join(rel),
        instance.game_dir().join(format!("{rel}{DISABLED}")),
    ] {
        if path.is_dir() {
            std::fs::remove_dir_all(&path).at(&path)?;
        } else if path.exists() {
            std::fs::remove_file(&path).at(&path)?;
        }
    }
    let mut index = ContentIndex::load(instance)?;
    index.entries.retain(|e| e.file != rel);
    index.save(instance)
}

/// Picks up manual downloads from `downloads_dir` (by name, then by hash), moving them
/// into place. Returns the titles that were found.
pub fn collect_manual(
    instance: &Instance,
    items: &[ManualDownload],
    downloads_dir: &Path,
) -> Result<Vec<String>> {
    let mut found = Vec::new();
    let mut index = ContentIndex::load(instance)?;
    let candidates: Vec<PathBuf> = std::fs::read_dir(downloads_dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect()
        })
        .unwrap_or_default();
    for item in items {
        let dest = instance.game_dir().join(&item.entry.file);
        if dest.exists() {
            found.push(item.title.clone());
            continue;
        }
        let by_name = candidates.iter().find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy() == item.file_name)
        });
        let matching = |p: &&PathBuf| match &item.sha1 {
            Some(h) => fsutil::sha1_file(p).is_ok_and(|s| s.eq_ignore_ascii_case(h)),
            None => true,
        };
        let hit = by_name.filter(matching).or_else(|| {
            // Browsers may rename duplicates ("file (1).jar"); fall back to size + hash.
            item.sha1.as_ref()?;
            candidates
                .iter()
                .filter(|p| {
                    item.size
                        .is_none_or(|s| p.metadata().is_ok_and(|m| m.len() == s))
                })
                .find(matching)
        });
        if let Some(src) = hit {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            if std::fs::rename(src, &dest).is_err() {
                std::fs::copy(src, &dest).at(src)?;
                let _ = std::fs::remove_file(src);
            }
            index.upsert(item.entry.clone());
            found.push(item.title.clone());
        }
    }
    index.save(instance)?;
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::InstanceManager;

    #[test]
    fn list_toggle_remove_and_manual_pickup() {
        let dir = tempfile::tempdir().unwrap();
        let inst = InstanceManager::new(dir.path().join("instances"))
            .create("Pack", "1.21.1")
            .unwrap();
        let mods = inst.game_dir().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("sodium.jar"), b"abc").unwrap();
        std::fs::write(mods.join("readme.txt"), b"x").unwrap();

        let items = list(&inst).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].enabled);

        set_enabled(&inst, "mods/sodium.jar", false).unwrap();
        assert!(mods.join("sodium.jar.disabled").exists());
        assert!(!list(&inst).unwrap()[0].enabled);
        set_enabled(&inst, "mods/sodium.jar", true).unwrap();
        assert!(set_enabled(&inst, "../evil.jar", true).is_err());
        assert!(remove(&inst, "mods/../../x").is_err());

        // Manual pickup: a renamed duplicate is still found by hash.
        let downloads = dir.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::write(downloads.join("jei (1).jar"), b"abc").unwrap();
        let entry = ContentEntry {
            file: "mods/jei.jar".into(),
            kind: ContentKind::Mod,
            source: Some(Source::CurseForge),
            project_id: Some("238222".into()),
            version_id: Some("1".into()),
            title: "JEI".into(),
            version_number: None,
            sha1: Some("a9993e364706816aba3e25717850c26c9cd0d89d".into()),
            url: None,
            icon_url: None,
        };
        let manual = ManualDownload {
            title: "JEI".into(),
            file_name: "jei.jar".into(),
            page_url: String::new(),
            sha1: entry.sha1.clone(),
            size: Some(3),
            entry,
        };
        assert_eq!(
            collect_manual(&inst, &[manual], &downloads).unwrap(),
            ["JEI"]
        );
        assert!(mods.join("jei.jar").exists());
        let listed = list(&inst).unwrap();
        assert_eq!(
            listed
                .iter()
                .find(|i| i.entry.file == "mods/jei.jar")
                .unwrap()
                .entry
                .source,
            Some(Source::CurseForge)
        );

        remove(&inst, "mods/sodium.jar").unwrap();
        assert!(!mods.join("sodium.jar").exists());
    }
}
