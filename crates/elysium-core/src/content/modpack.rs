//! Modpacks: Modrinth `.mrpack` and CurseForge zips in, `.mrpack` out.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::install::{ContentEntry, ContentIndex, ContentService, ManualDownload};
use super::{ContentKind, Source, curseforge};
use crate::error::{Error, IoContext, Result};
use crate::fsutil;
use crate::http::{DownloadTask, Verify};
use crate::instance::{Instance, InstanceManager, ModLoader, PackOrigin};
use crate::java::safe_join;
use crate::progress::Progress;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub files: Vec<MrFile>,
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MrFile {
    pub path: String,
    pub hashes: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize")]
    pub file_size: u64,
}

/// Result of importing a pack: the new instance and anything to download by hand.
#[derive(Debug, Clone, Serialize)]
pub struct PackImport {
    pub instance_id: String,
    pub manual: Vec<ManualDownload>,
}

fn loader_from_deps(deps: &HashMap<String, String>) -> (ModLoader, Option<String>) {
    for (key, loader) in [
        ("fabric-loader", ModLoader::Fabric),
        ("quilt-loader", ModLoader::Quilt),
        ("neoforge", ModLoader::NeoForge),
        ("forge", ModLoader::Forge),
    ] {
        if let Some(v) = deps.get(key) {
            return (loader, Some(v.clone()));
        }
    }
    (ModLoader::Vanilla, None)
}

fn host_allowed(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str()
                .is_some_and(|h| super::modrinth::ALLOWED_PACK_HOSTS.contains(&h))
    })
}

fn kind_for_path(path: &str) -> ContentKind {
    match path.split('/').next() {
        Some("resourcepacks") => ContentKind::ResourcePack,
        Some("shaderpacks") => ContentKind::Shader,
        _ => ContentKind::Mod,
    }
}

/// Extracts `prefix/**` from a pack into the game folder (zip-slip safe).
fn extract_overrides(
    archive: &mut zip::ZipArchive<std::fs::File>,
    pack: &Path,
    prefix: &str,
    game_dir: &Path,
) -> Result<()> {
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|source| Error::Zip {
            path: pack.to_owned(),
            source,
        })?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let Ok(rel) = name.strip_prefix(prefix) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = game_dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).at(&dest)?;
            continue;
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data).at(&dest)?;
        fsutil::write_atomic(&dest, &data)?;
    }
    Ok(())
}

fn read_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
    pack: &Path,
    name: &str,
) -> Result<Vec<u8>> {
    let mut e = archive.by_name(name).map_err(|_| {
        Error::Invalid(format!(
            "{} is not a valid pack: {name} is missing",
            pack.display()
        ))
    })?;
    let mut data = Vec::new();
    e.read_to_end(&mut data).at(pack)?;
    Ok(data)
}

impl ContentService {
    /// Creates a new instance from a `.mrpack` file.
    pub async fn import_mrpack(
        &self,
        instances: &InstanceManager,
        pack: &Path,
        origin: Option<PackOrigin>,
        concurrency: usize,
        progress: Arc<Progress>,
    ) -> Result<PackImport> {
        let file = std::fs::File::open(pack).at(pack)?;
        let mut archive = zip::ZipArchive::new(file).map_err(|source| Error::Zip {
            path: pack.to_owned(),
            source,
        })?;
        let index: MrIndex =
            serde_json::from_slice(&read_entry(&mut archive, pack, "modrinth.index.json")?)
                .map_err(|source| Error::Json {
                    context: "modrinth.index.json".into(),
                    source,
                })?;
        if index.game != "minecraft" {
            return Err(Error::Invalid(format!(
                "this pack is for {}, not Minecraft",
                index.game
            )));
        }
        let mc = index.dependencies.get("minecraft").ok_or_else(|| {
            Error::Invalid("the pack doesn't say which Minecraft version it needs".into())
        })?;
        let (loader, loader_version) = loader_from_deps(&index.dependencies);
        let instance = instances.create_with(&index.name, mc, loader, loader_version)?;
        let instance = instances.update(&instance.id, |c| {
            c.pack = Some(origin.unwrap_or(PackOrigin {
                source: "file".into(),
                project_id: None,
                version_id: Some(index.version_id.clone()),
                name: index.name.clone(),
            }));
        })?;

        let result = async {
            let game_dir = instance.game_dir();
            let mut tasks = Vec::new();
            let mut entries = Vec::new();
            for f in &index.files {
                if f.env
                    .as_ref()
                    .and_then(|e| e.get("client"))
                    .is_some_and(|c| c == "unsupported")
                {
                    continue;
                }
                let dest = safe_join(&game_dir, &f.path)?;
                let url = f
                    .downloads
                    .iter()
                    .find(|u| host_allowed(u))
                    .ok_or_else(|| {
                        Error::Invalid(format!(
                            "{} downloads from a host Modrinth packs may not use",
                            f.path
                        ))
                    })?;
                let sha1 = f.hashes.get("sha1").cloned();
                tasks.push(
                    DownloadTask::new(url, &dest)
                        .sha1(sha1.clone())
                        .size(Some(f.file_size)),
                );
                entries.push(ContentEntry {
                    file: f.path.clone(),
                    kind: kind_for_path(&f.path),
                    source: Some(Source::Modrinth),
                    project_id: None,
                    version_id: None,
                    title: Path::new(&f.path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    version_number: None,
                    sha1,
                    url: Some(url.clone()),
                    icon_url: None,
                });
            }
            progress.set_stage(format!("Downloading {}", index.name), 0);
            self.http_client()
                .download_all(tasks, concurrency, Verify::Hash, Arc::clone(&progress))
                .await?;

            progress.set_stage("Copying pack files", 0);
            extract_overrides(&mut archive, pack, "overrides", &game_dir)?;
            extract_overrides(&mut archive, pack, "client-overrides", &game_dir)?;

            // Fill in project names and ids so updates and the content list work.
            let hashes: Vec<String> = entries.iter().filter_map(|e| e.sha1.clone()).collect();
            if let Ok(known) = self.modrinth.versions_by_hash(&hashes).await {
                for e in &mut entries {
                    if let Some(v) = e.sha1.as_ref().and_then(|h| known.get(h)) {
                        e.project_id = Some(v.project_id.clone());
                        e.version_id = Some(v.id.clone());
                        e.version_number = Some(v.version_number.clone());
                        e.title = v
                            .name
                            .split(' ')
                            .next()
                            .filter(|_| false)
                            .map(str::to_owned)
                            .unwrap_or(e.title.clone());
                    }
                }
            }
            ContentIndex { entries }.save(&instance)?;
            Ok::<_, Error>(())
        }
        .await;
        if let Err(e) = result {
            // Don't leave a half-installed instance behind.
            let _ = instances.delete(&instance.id);
            return Err(e);
        }
        Ok(PackImport {
            instance_id: instance.id,
            manual: Vec::new(),
        })
    }

    /// Downloads a Modrinth modpack version and imports it.
    pub async fn install_modrinth_pack(
        &self,
        instances: &InstanceManager,
        cache_dir: &Path,
        project_id: &str,
        version_id: &str,
        concurrency: usize,
        progress: Arc<Progress>,
    ) -> Result<PackImport> {
        let version = self.modrinth.version(version_id).await?;
        let file = version
            .primary_file()
            .ok_or_else(|| Error::Invalid("the pack has no file".into()))?;
        let url = file
            .url
            .clone()
            .ok_or_else(|| Error::Invalid("the pack has no download".into()))?;
        let dest = cache_dir.join(format!("{version_id}.mrpack"));
        progress.set_stage("Downloading pack", file.size.unwrap_or(0));
        self.http_client()
            .download_all(
                vec![
                    DownloadTask::new(url, &dest)
                        .sha1(file.sha1.clone())
                        .size(file.size),
                ],
                1,
                Verify::Hash,
                Arc::clone(&progress),
            )
            .await?;
        let details = self.modrinth.project(project_id).await.ok();
        let origin = PackOrigin {
            source: "modrinth".into(),
            project_id: Some(project_id.to_owned()),
            version_id: Some(version_id.to_owned()),
            name: details.map_or_else(|| version.name.clone(), |d| d.hit.title),
        };
        let result = self
            .import_mrpack(instances, &dest, Some(origin), concurrency, progress)
            .await;
        let _ = std::fs::remove_file(&dest);
        result
    }

    /// Creates a new instance from a CurseForge modpack zip (needs an API key).
    pub async fn import_curseforge_pack(
        &self,
        instances: &InstanceManager,
        pack: &Path,
        origin: Option<PackOrigin>,
        concurrency: usize,
        progress: Arc<Progress>,
    ) -> Result<PackImport> {
        let cf = self.curseforge.as_ref().ok_or_else(|| {
            Error::Invalid("add a CurseForge API key in Settings to import CurseForge packs".into())
        })?;
        let file = std::fs::File::open(pack).at(pack)?;
        let mut archive = zip::ZipArchive::new(file).map_err(|source| Error::Zip {
            path: pack.to_owned(),
            source,
        })?;
        let manifest: Value =
            serde_json::from_slice(&read_entry(&mut archive, pack, "manifest.json")?).map_err(
                |source| Error::Json {
                    context: "manifest.json".into(),
                    source,
                },
            )?;
        let mc = manifest
            .pointer("/minecraft/version")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Error::Invalid("the pack doesn't say which Minecraft version it needs".into())
            })?;
        let loader_id = manifest
            .pointer("/minecraft/modLoaders")
            .and_then(Value::as_array)
            .and_then(|l| {
                l.iter()
                    .find(|x| x.get("primary").and_then(Value::as_bool).unwrap_or(true))
                    .or(l.first())
            })
            .and_then(|l| l.get("id")?.as_str())
            .unwrap_or_default();
        let (loader, loader_version) = parse_cf_loader(loader_id);
        let name = manifest
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("CurseForge pack")
            .to_owned();
        let overrides = manifest
            .get("overrides")
            .and_then(Value::as_str)
            .unwrap_or("overrides")
            .to_owned();
        let file_ids: Vec<u64> = manifest
            .get("files")
            .and_then(Value::as_array)
            .map(|fs| {
                fs.iter()
                    .filter_map(|f| f.get("fileID")?.as_u64())
                    .collect()
            })
            .unwrap_or_default();

        let instance = instances.create_with(&name, mc, loader, loader_version)?;
        let instance = instances.update(&instance.id, |c| {
            c.pack = Some(origin.unwrap_or(PackOrigin {
                source: "file".into(),
                project_id: None,
                version_id: None,
                name: name.clone(),
            }));
        })?;

        let result: Result<Vec<ManualDownload>> = async {
            progress.set_stage("Looking up pack files", 0);
            let files = cf.files_by_id(&file_ids).await?;
            let mod_ids: Vec<u64> = files.iter().map(|f| f.mod_id).collect();
            let mods: HashMap<u64, curseforge::CfMod> = cf
                .mods(&mod_ids)
                .await?
                .into_iter()
                .map(|m| (m.id, m))
                .collect();
            let mut tasks = Vec::new();
            let mut entries = Vec::new();
            let mut manual = Vec::new();
            for f in &files {
                let m = mods.get(&f.mod_id);
                let kind = m
                    .and_then(|m| m.class_id)
                    .map_or(ContentKind::Mod, curseforge::kind_of);
                let folder = if kind == ContentKind::Modpack {
                    "mods"
                } else {
                    kind.folder()
                };
                let rel = format!("{folder}/{}", f.file_name);
                let entry = ContentEntry {
                    file: rel.clone(),
                    kind,
                    source: Some(Source::CurseForge),
                    project_id: Some(f.mod_id.to_string()),
                    version_id: Some(f.id.to_string()),
                    title: m.map_or_else(|| f.display_name.clone(), |m| m.name.clone()),
                    version_number: Some(f.display_name.clone()),
                    sha1: f.sha1().map(str::to_owned),
                    url: f.download_url.clone(),
                    icon_url: m.and_then(|m| m.hit().icon_url),
                };
                let url = match &f.download_url {
                    Some(u) => Some(u.clone()),
                    None => self.modrinth_mirror(f.sha1()).await,
                };
                match url {
                    Some(url) => {
                        tasks.push(
                            DownloadTask::new(&url, safe_join(&instance.game_dir(), &rel)?)
                                .sha1(entry.sha1.clone())
                                .size(f.file_length),
                        );
                        entries.push(entry);
                    }
                    None => manual.push(ManualDownload {
                        title: entry.title.clone(),
                        file_name: f.file_name.clone(),
                        page_url: curseforge::manual_download_url(
                            kind,
                            m.map_or("", |m| m.slug.as_str()),
                            &f.id.to_string(),
                        ),
                        sha1: entry.sha1.clone(),
                        size: f.file_length,
                        entry,
                    }),
                }
            }
            progress.set_stage(format!("Downloading {name}"), 0);
            self.http_client()
                .download_all(tasks, concurrency, Verify::Hash, Arc::clone(&progress))
                .await?;
            extract_overrides(&mut archive, pack, &overrides, &instance.game_dir())?;
            ContentIndex { entries }.save(&instance)?;
            Ok(manual)
        }
        .await;
        match result {
            Ok(manual) => Ok(PackImport {
                instance_id: instance.id,
                manual,
            }),
            Err(e) => {
                let _ = instances.delete(&instance.id);
                Err(e)
            }
        }
    }

    fn http_client(&self) -> &crate::http::Http {
        self.http_ref()
    }
}

/// `forge-47.2.0` → (Forge, 47.2.0).
fn parse_cf_loader(id: &str) -> (ModLoader, Option<String>) {
    let Some((name, version)) = id.split_once('-') else {
        return (ModLoader::Vanilla, None);
    };
    let loader = match name {
        "forge" => ModLoader::Forge,
        "neoforge" => ModLoader::NeoForge,
        "fabric" => ModLoader::Fabric,
        "quilt" => ModLoader::Quilt,
        _ => return (ModLoader::Vanilla, None),
    };
    (loader, Some(version.to_owned()))
}

/// Writes the instance as a `.mrpack`. Files known to be on Modrinth are referenced by
/// URL; everything else (configs, unknown mods) goes into `overrides/`.
pub fn export_mrpack(instance: &Instance, out: &Path) -> Result<()> {
    let index = ContentIndex::load(instance)?;
    let game_dir = instance.game_dir();
    let mut files = Vec::new();
    let mut referenced = std::collections::HashSet::new();
    for e in &index.entries {
        let Some(url) = e.url.as_deref().filter(|u| host_allowed(u)) else {
            continue;
        };
        let path = game_dir.join(&e.file);
        if !path.is_file() {
            continue;
        }
        let data = std::fs::read(&path).at(&path)?;
        let sha1 = fsutil::hex(&<sha1::Sha1 as sha1::Digest>::digest(&data));
        let sha512 = fsutil::hex(&<sha2::Sha512 as sha2::Digest>::digest(&data));
        files.push(MrFile {
            path: e.file.clone(),
            hashes: HashMap::from([("sha1".to_owned(), sha1), ("sha512".to_owned(), sha512)]),
            env: Some(HashMap::from([
                ("client".to_owned(), "required".to_owned()),
                ("server".to_owned(), "required".to_owned()),
            ])),
            downloads: vec![url.to_owned()],
            file_size: data.len() as u64,
        });
        referenced.insert(e.file.clone());
    }
    let mut deps = HashMap::from([("minecraft".to_owned(), instance.config.game_version.clone())]);
    if let Some(v) = &instance.config.loader_version {
        let key = match instance.config.loader {
            ModLoader::Fabric => Some("fabric-loader"),
            ModLoader::Quilt => Some("quilt-loader"),
            ModLoader::Forge => Some("forge"),
            ModLoader::NeoForge => Some("neoforge"),
            _ => None,
        };
        if let Some(k) = key {
            deps.insert(k.to_owned(), v.clone());
        }
    }
    let mr = MrIndex {
        format_version: 1,
        game: "minecraft".into(),
        version_id: "1.0.0".into(),
        name: instance.config.name.clone(),
        summary: None,
        files,
        dependencies: deps,
    };

    let file = std::fs::File::create(out).at(out)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let zerr = |e: zip::result::ZipError| Error::Zip {
        path: out.to_owned(),
        source: e,
    };
    zip.start_file("modrinth.index.json", opts).map_err(zerr)?;
    zip.write_all(
        &serde_json::to_vec_pretty(&mr).map_err(|source| Error::Json {
            context: "index".into(),
            source,
        })?,
    )
    .at(out)?;
    // Overrides: player content and configs, but not worlds, logs or caches.
    const SKIP: &[&str] = &[
        "saves",
        "logs",
        "crash-reports",
        "screenshots",
        "natives",
        ".fabric",
        ".cache",
        "downloads",
    ];
    let mut stack = vec![game_dir.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(&game_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if SKIP.iter().any(|s| rel == *s) || referenced.contains(&rel) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                zip.start_file(format!("overrides/{rel}"), opts)
                    .map_err(zerr)?;
                zip.write_all(&std::fs::read(&path).at(&path)?).at(out)?;
            }
        }
    }
    zip.finish().map_err(zerr)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaders_from_pack_metadata() {
        let deps = HashMap::from([
            ("minecraft".into(), "1.21.1".into()),
            ("fabric-loader".into(), "0.16.9".into()),
        ]);
        assert_eq!(
            loader_from_deps(&deps),
            (ModLoader::Fabric, Some("0.16.9".into()))
        );
        assert_eq!(
            parse_cf_loader("neoforge-21.1.77"),
            (ModLoader::NeoForge, Some("21.1.77".into()))
        );
        assert_eq!(
            parse_cf_loader("forge-47.2.0"),
            (ModLoader::Forge, Some("47.2.0".into()))
        );
        assert_eq!(parse_cf_loader("weird"), (ModLoader::Vanilla, None));
    }

    #[test]
    fn pack_hosts() {
        assert!(host_allowed("https://cdn.modrinth.com/data/a/b.jar"));
        assert!(!host_allowed("http://cdn.modrinth.com/data/a/b.jar"));
        assert!(!host_allowed("https://evil.example/x.jar"));
    }

    #[test]
    fn export_then_reimport_index() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = InstanceManager::new(dir.path());
        let inst = mgr
            .create_with("Mine", "1.21.1", ModLoader::Fabric, Some("0.16.9".into()))
            .unwrap();
        let mods = inst.game_dir().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("a.jar"), b"abc").unwrap();
        std::fs::create_dir_all(inst.game_dir().join("config")).unwrap();
        std::fs::write(inst.game_dir().join("config/a.toml"), b"x=1").unwrap();
        std::fs::create_dir_all(inst.game_dir().join("saves/w")).unwrap();
        std::fs::write(inst.game_dir().join("saves/w/level.dat"), b"lvl").unwrap();
        ContentIndex {
            entries: vec![ContentEntry {
                file: "mods/a.jar".into(),
                kind: ContentKind::Mod,
                source: Some(Source::Modrinth),
                project_id: Some("p".into()),
                version_id: Some("v".into()),
                title: "A".into(),
                version_number: None,
                sha1: None,
                url: Some("https://cdn.modrinth.com/data/p/versions/v/a.jar".into()),
                icon_url: None,
            }],
        }
        .save(&inst)
        .unwrap();
        let out = dir.path().join("pack.mrpack");
        export_mrpack(&inst, &out).unwrap();

        let mut archive = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let idx: MrIndex =
            serde_json::from_slice(&read_entry(&mut archive, &out, "modrinth.index.json").unwrap())
                .unwrap();
        assert_eq!(idx.files.len(), 1);
        assert_eq!(
            idx.files[0].hashes["sha1"],
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(idx.dependencies["fabric-loader"], "0.16.9");
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
            .collect();
        assert!(names.contains(&"overrides/config/a.toml".to_string()));
        assert!(!names.iter().any(|n| n.contains("saves")));
        assert!(
            !names.iter().any(|n| n.contains("mods/a.jar")),
            "referenced files are not duplicated"
        );
    }
}
