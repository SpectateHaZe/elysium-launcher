//! Mod loaders: version lists and installation of Fabric, Quilt, Forge and NeoForge.
//!
//! Fabric and Quilt publish ready-made launcher profiles. Forge and NeoForge ship
//! installers: old ones (up to 1.12.2) only need files copied out of the installer;
//! newer ones patch the game with "processors", so their installer is run headless.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, IoContext, Result};
use crate::fsutil;
use crate::http::{DownloadTask, Http, Verify};
use crate::install::{self, InstallOptions};
use crate::instance::{Instance, ModLoader};
use crate::meta::version::maven_path;
use crate::paths::Paths;
use crate::progress::Progress;

pub const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
pub const QUILT_META: &str = "https://meta.quiltmc.org/v3";
pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
pub const FORGE_PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";
/// NeoForge for 1.20.1 was published under the old Forge coordinates.
pub const NEOFORGE_LEGACY_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/forge";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoaderVersion {
    pub version: String,
    /// Stable/recommended builds; the UI preselects the first stable one.
    pub stable: bool,
}

/// Loader versions available for Minecraft `mc`, newest first.
pub async fn versions(http: &Http, loader: ModLoader, mc: &str) -> Result<Vec<LoaderVersion>> {
    match loader {
        ModLoader::Fabric | ModLoader::Quilt => {
            let base = if loader == ModLoader::Fabric {
                FABRIC_META
            } else {
                QUILT_META
            };
            let list: Vec<Value> = http
                .get_json(&format!("{base}/versions/loader/{mc}"))
                .await?;
            Ok(list
                .iter()
                .filter_map(|v| {
                    let l = v.get("loader")?;
                    let version = l.get("version")?.as_str()?.to_owned();
                    let stable = l
                        .get("stable")
                        .and_then(Value::as_bool)
                        .unwrap_or_else(|| !version.contains('-'));
                    Some(LoaderVersion { version, stable })
                })
                .collect())
        }
        ModLoader::Forge => {
            let xml = String::from_utf8_lossy(
                &http
                    .get_bytes(&format!("{FORGE_MAVEN}/maven-metadata.xml"))
                    .await?,
            )
            .into_owned();
            let promos: Value = http.get_json(FORGE_PROMOTIONS).await.unwrap_or(Value::Null);
            let recommended = promos
                .pointer(&format!("/promos/{mc}-recommended"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            let mut out = forge_versions_for(&maven_versions(&xml), mc);
            for v in &mut out {
                v.stable = recommended
                    .as_deref()
                    .is_some_and(|r| v.version == r || v.version.starts_with(&format!("{r}-")));
            }
            Ok(out)
        }
        ModLoader::NeoForge => {
            if mc == "1.20.1" {
                let xml = String::from_utf8_lossy(
                    &http
                        .get_bytes(&format!("{NEOFORGE_LEGACY_MAVEN}/maven-metadata.xml"))
                        .await?,
                )
                .into_owned();
                return Ok(forge_versions_for(&maven_versions(&xml), mc));
            }
            let xml = String::from_utf8_lossy(
                &http
                    .get_bytes(&format!("{NEOFORGE_MAVEN}/maven-metadata.xml"))
                    .await?,
            )
            .into_owned();
            Ok(neoforge_versions_for(&maven_versions(&xml), mc))
        }
        ModLoader::Vanilla | ModLoader::Horizon => Ok(Vec::new()),
    }
}

/// `<version>` entries of a maven-metadata.xml.
fn maven_versions(xml: &str) -> Vec<String> {
    xml.split("<version>")
        .skip(1)
        .filter_map(|s| s.split("</version>").next())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Compares dotted version strings numerically where possible.
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |s: &str| {
        s.split(['.', '-', '+'])
            .map(|p| p.parse::<u64>().map_err(|_| p.to_owned()))
            .collect::<Vec<_>>()
    };
    let (pa, pb) = (parts(a), parts(b));
    for (x, y) in pa.iter().zip(&pb) {
        let ord = match (x, y) {
            (Ok(x), Ok(y)) => x.cmp(y),
            (Ok(_), Err(_)) => std::cmp::Ordering::Greater,
            (Err(_), Ok(_)) => std::cmp::Ordering::Less,
            (Err(x), Err(y)) => x.cmp(y),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    pa.len().cmp(&pb.len())
}

/// Forge versions are published as `<mc>-<forge>[-<mc>]`; returns the part after `<mc>-`.
fn forge_versions_for(all: &[String], mc: &str) -> Vec<LoaderVersion> {
    let prefix = format!("{mc}-");
    let mut out: Vec<LoaderVersion> = all
        .iter()
        .filter_map(|v| v.strip_prefix(&prefix))
        .map(|v| LoaderVersion {
            version: v.to_owned(),
            stable: false,
        })
        .collect();
    out.sort_by(|a, b| version_cmp(&b.version, &a.version));
    out
}

/// NeoForge numbers versions after Minecraft: 1.21.1 → `21.1.x`, 26.3 → `26.3.0.x`.
fn neoforge_prefix(mc: &str) -> String {
    match mc.strip_prefix("1.") {
        Some(rest) => {
            let mut p: Vec<&str> = rest.split('.').collect();
            if p.len() == 1 {
                p.push("0");
            }
            format!("{}.", p.join("."))
        }
        None => {
            let mut p: Vec<&str> = mc.split('.').collect();
            while p.len() < 3 {
                p.push("0");
            }
            format!("{}.", p.join("."))
        }
    }
}

fn neoforge_versions_for(all: &[String], mc: &str) -> Vec<LoaderVersion> {
    let prefix = neoforge_prefix(mc);
    let mut out: Vec<LoaderVersion> = all
        .iter()
        .filter(|v| v.starts_with(&prefix))
        .map(|v| LoaderVersion {
            stable: !v.contains("beta") && !v.contains("alpha"),
            version: v.clone(),
        })
        .collect();
    out.sort_by(|a, b| version_cmp(&b.version, &a.version));
    out
}

// ---------------------------------------------------------------------------
// Installation

fn index_file(paths: &Paths) -> PathBuf {
    paths.meta_dir().join("loader-profiles.json")
}

fn index_key(loader: ModLoader, mc: &str, version: &str) -> String {
    format!("{}:{mc}:{version}", loader.name().to_lowercase())
}

/// Makes sure the instance's loader profile is installed; returns the version id to launch.
pub async fn ensure_installed(
    paths: &Paths,
    http: &Http,
    instance: &Instance,
    options: &InstallOptions,
    progress: Arc<Progress>,
) -> Result<String> {
    let c = &instance.config;
    let Some(lv) = c.loader_version.as_deref() else {
        return Ok(c.game_version.clone());
    };
    let mc = c.game_version.as_str();
    match c.loader {
        ModLoader::Vanilla | ModLoader::Horizon => Ok(mc.to_owned()),
        ModLoader::Fabric | ModLoader::Quilt => {
            let (base, prefix) = match c.loader {
                ModLoader::Fabric => (FABRIC_META, "fabric-loader"),
                _ => (QUILT_META, "quilt-loader"),
            };
            let id = format!("{prefix}-{lv}-{mc}");
            let json_path = paths.version_json(&id);
            if !json_path.exists() || options.verify == Verify::Hash {
                progress.set_stage(format!("Installing {} {lv}", c.loader.name()), 0);
                let mut profile: Value = http
                    .get_json(&format!("{base}/versions/loader/{mc}/{lv}/profile/json"))
                    .await?;
                // Use our id so the folder name and the JSON always agree.
                profile["id"] = Value::String(id.clone());
                fsutil::write_json_atomic(&json_path, &profile)?;
            }
            Ok(id)
        }
        ModLoader::Forge | ModLoader::NeoForge => {
            let key = index_key(c.loader, mc, lv);
            let mut index: HashMap<String, String> =
                fsutil::read_json(&index_file(paths))?.unwrap_or_default();
            if let Some(id) = index.get(&key)
                && paths.version_json(id).exists()
                && options.verify == Verify::Size
            {
                return Ok(id.clone());
            }
            let id = install_forge_like(paths, http, c.loader, mc, lv, options, progress).await?;
            index.insert(key, id.clone());
            fsutil::write_json_atomic(&index_file(paths), &index)?;
            Ok(id)
        }
    }
}

fn installer_url(loader: ModLoader, mc: &str, lv: &str) -> String {
    match loader {
        ModLoader::NeoForge if mc == "1.20.1" => {
            format!("{NEOFORGE_LEGACY_MAVEN}/{mc}-{lv}/forge-{mc}-{lv}-installer.jar")
        }
        ModLoader::NeoForge => format!("{NEOFORGE_MAVEN}/{lv}/neoforge-{lv}-installer.jar"),
        _ => format!("{FORGE_MAVEN}/{mc}-{lv}/forge-{mc}-{lv}-installer.jar"),
    }
}

/// What an installer's `install_profile.json` asks for.
#[derive(Debug, PartialEq)]
enum InstallerKind {
    /// 1.6–1.11 style: a `versionInfo` profile plus a universal jar inside the installer.
    Legacy {
        id: String,
        version: Value,
        universal_entry: String,
        universal_coord: String,
    },
    /// Version JSON and Maven files inside the installer, nothing to run (e.g. 1.12.2).
    Embedded { id: String, json_entry: String },
    /// Needs the installer's client processors to run.
    Processors { id: String },
}

fn classify(profile: &Value) -> Result<InstallerKind> {
    if let (Some(install), Some(version)) = (profile.get("install"), profile.get("versionInfo")) {
        let s = |k: &str| install.get(k).and_then(Value::as_str).map(str::to_owned);
        let id =
            s("target").ok_or_else(|| Error::Invalid("installer profile has no target".into()))?;
        return Ok(InstallerKind::Legacy {
            id,
            version: version.clone(),
            universal_entry: s("filePath").unwrap_or_default(),
            universal_coord: s("path").unwrap_or_default(),
        });
    }
    let id = profile
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid("installer profile has no version".into()))?
        .to_owned();
    let client_processors = profile
        .get("processors")
        .and_then(Value::as_array)
        .map(|ps| {
            ps.iter()
                .filter(|p| {
                    p.get("sides")
                        .and_then(Value::as_array)
                        .is_none_or(|s| s.iter().any(|x| x == "client"))
                })
                .count()
        })
        .unwrap_or(0);
    if client_processors == 0 {
        let json_entry = profile
            .get("json")
            .and_then(Value::as_str)
            .unwrap_or("/version.json")
            .trim_start_matches('/')
            .to_owned();
        Ok(InstallerKind::Embedded { id, json_entry })
    } else {
        Ok(InstallerKind::Processors { id })
    }
}

fn zip_entry(
    archive: &mut zip::ZipArchive<std::fs::File>,
    jar: &Path,
    name: &str,
) -> Result<Vec<u8>> {
    let mut entry = archive.by_name(name).map_err(|source| Error::Zip {
        path: jar.to_owned(),
        source,
    })?;
    let mut data = Vec::new();
    entry.read_to_end(&mut data).at(jar)?;
    Ok(data)
}

async fn install_forge_like(
    paths: &Paths,
    http: &Http,
    loader: ModLoader,
    mc: &str,
    lv: &str,
    options: &InstallOptions,
    progress: Arc<Progress>,
) -> Result<String> {
    progress.set_stage(
        format!("Downloading the {} {lv} installer", loader.name()),
        0,
    );
    let jar = paths.meta_dir().join("installers").join(format!(
        "{}-{mc}-{lv}-installer.jar",
        loader.name().to_lowercase()
    ));
    http.download_all(
        vec![DownloadTask::new(installer_url(loader, mc, lv), &jar)],
        1,
        Verify::Size,
        Arc::clone(&progress),
    )
    .await
    .map_err(|e| match e {
        Error::Http(ref h) if h.status().is_some_and(|s| s.as_u16() == 404) => Error::Invalid(
            format!("{} {lv} has no installer for Minecraft {mc}", loader.name()),
        ),
        e => e,
    })?;

    let kind = {
        let file = std::fs::File::open(&jar).at(&jar)?;
        let mut archive = zip::ZipArchive::new(file).map_err(|source| Error::Zip {
            path: jar.clone(),
            source,
        })?;
        let profile: Value =
            serde_json::from_slice(&zip_entry(&mut archive, &jar, "install_profile.json")?)
                .map_err(|source| Error::Json {
                    context: "install_profile.json".into(),
                    source,
                })?;
        let kind = classify(&profile)?;
        match &kind {
            InstallerKind::Legacy {
                id,
                version,
                universal_entry,
                universal_coord,
            } => {
                let mut version = version.clone();
                version["id"] = Value::String(id.clone());
                fsutil::write_json_atomic(&paths.version_json(id), &version)?;
                let rel = maven_path(universal_coord).ok_or_else(|| {
                    Error::Invalid(format!("bad Forge coordinate {universal_coord}"))
                })?;
                let dest = paths.libraries_dir().join(rel);
                let data = zip_entry(&mut archive, &jar, universal_entry)?;
                fsutil::write_atomic(&dest, &data)?;
            }
            InstallerKind::Embedded { id, json_entry } => {
                let mut version: Value =
                    serde_json::from_slice(&zip_entry(&mut archive, &jar, json_entry)?).map_err(
                        |source| Error::Json {
                            context: json_entry.clone(),
                            source,
                        },
                    )?;
                version["id"] = Value::String(id.clone());
                fsutil::write_json_atomic(&paths.version_json(id), &version)?;
                extract_maven_dir(&mut archive, &jar, &paths.libraries_dir())?;
            }
            InstallerKind::Processors { .. } => {}
        }
        kind
    };

    match kind {
        InstallerKind::Legacy { id, .. } | InstallerKind::Embedded { id, .. } => Ok(id),
        InstallerKind::Processors { id } => {
            // The installer needs the vanilla game in place and a Java to run with.
            let vanilla =
                install::install_version(paths, http, mc, options, Arc::clone(&progress)).await?;
            progress.set_stage(format!("Running the {} installer", loader.name()), 0);
            run_installer(paths, loader, &vanilla.java, &jar).await?;
            if !paths.version_json(&id).exists() {
                return Err(Error::Invalid(format!(
                    "the {} installer finished but {id} wasn't created",
                    loader.name()
                )));
            }
            Ok(id)
        }
    }
}

/// Copies `maven/**` from an installer into the libraries folder.
fn extract_maven_dir(
    archive: &mut zip::ZipArchive<std::fs::File>,
    jar: &Path,
    libraries: &Path,
) -> Result<()> {
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|source| Error::Zip {
            path: jar.to_owned(),
            source,
        })?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let Ok(rel) = rel.strip_prefix("maven") else {
            continue;
        };
        let dest = libraries.join(rel);
        let mut data = Vec::new();
        entry.read_to_end(&mut data).at(&dest)?;
        fsutil::write_atomic(&dest, &data)?;
    }
    Ok(())
}

/// Runs a Forge/NeoForge installer headless against the launcher's data folder, which
/// uses the same `versions/` + `libraries/` layout as the official launcher.
async fn run_installer(paths: &Paths, loader: ModLoader, java: &Path, jar: &Path) -> Result<()> {
    let profiles = paths.root().join("launcher_profiles.json");
    if !profiles.exists() {
        // The installers refuse to run without this file.
        fsutil::write_atomic(&profiles, br#"{"profiles":{}}"#)?;
    }
    let flag = if loader == ModLoader::NeoForge {
        "--install-client"
    } else {
        "--installClient"
    };
    let mut cmd = tokio::process::Command::new(java);
    cmd.arg("-jar")
        .arg(jar)
        .arg(flag)
        .arg(paths.root())
        .current_dir(paths.meta_dir())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    crate::launch::hide_console(&mut cmd);
    let out = cmd.output().await.at(java)?;
    if out.status.success() {
        return Ok(());
    }
    let log = String::from_utf8_lossy(&out.stdout);
    let tail: Vec<&str> = log.lines().rev().take(6).collect();
    Err(Error::Invalid(format!(
        "the {} installer failed ({}): {}",
        loader.name(),
        out.status,
        tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neoforge_prefixes() {
        assert_eq!(neoforge_prefix("1.21.1"), "21.1.");
        assert_eq!(neoforge_prefix("1.21"), "21.0.");
        assert_eq!(neoforge_prefix("1.20.4"), "20.4.");
        assert_eq!(neoforge_prefix("26.3"), "26.3.0.");
        assert_eq!(neoforge_prefix("26.1.2"), "26.1.2.");
    }

    #[test]
    fn parses_and_sorts_maven_versions() {
        let xml = "<metadata><versioning><versions><version>1.20.1-47.1.0</version><version>1.20.1-47.10.2</version>\
                   <version>1.20.1-47.3.0</version><version>1.7.10-10.13.4.1614-1.7.10</version><version>1.20-46.0.1</version>\
                   </versions></versioning></metadata>";
        let all = maven_versions(xml);
        assert_eq!(all.len(), 5);
        let v: Vec<_> = forge_versions_for(&all, "1.20.1")
            .into_iter()
            .map(|v| v.version)
            .collect();
        assert_eq!(v, ["47.10.2", "47.3.0", "47.1.0"]);
        let old: Vec<_> = forge_versions_for(&all, "1.7.10")
            .into_iter()
            .map(|v| v.version)
            .collect();
        assert_eq!(old, ["10.13.4.1614-1.7.10"]);
        let neo = neoforge_versions_for(
            &[
                "21.1.9".into(),
                "21.1.77".into(),
                "21.10.1-beta".into(),
                "20.4.1".into(),
            ],
            "1.21.1",
        );
        assert_eq!(
            neo.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(),
            ["21.1.77", "21.1.9"]
        );
    }

    #[test]
    fn classifies_installer_profiles() {
        let legacy = serde_json::json!({
            "install": {"target": "1.7.10-Forge10.13.4.1614-1.7.10", "filePath": "forge-universal.jar", "path": "net.minecraftforge:forge:1.7.10-10.13.4.1614-1.7.10"},
            "versionInfo": {"id": "x", "inheritsFrom": "1.7.10"}
        });
        assert!(
            matches!(classify(&legacy).unwrap(), InstallerKind::Legacy { id, .. } if id == "1.7.10-Forge10.13.4.1614-1.7.10")
        );
        let embedded = serde_json::json!({"spec": 0, "version": "1.12.2-forge-14.23.5.2860", "json": "/version.json", "processors": []});
        assert_eq!(
            classify(&embedded).unwrap(),
            InstallerKind::Embedded {
                id: "1.12.2-forge-14.23.5.2860".into(),
                json_entry: "version.json".into()
            }
        );
        let modern = serde_json::json!({"spec": 1, "version": "neoforge-21.1.77", "processors": [
            {"sides": ["server"], "jar": "a"}, {"jar": "b"}]});
        assert_eq!(
            classify(&modern).unwrap(),
            InstallerKind::Processors {
                id: "neoforge-21.1.77".into()
            }
        );
        let server_only = serde_json::json!({"spec": 1, "version": "v", "processors": [{"sides": ["server"], "jar": "a"}]});
        assert!(matches!(
            classify(&server_only).unwrap(),
            InstallerKind::Embedded { .. }
        ));
    }

    #[test]
    fn version_ordering() {
        assert_eq!(
            version_cmp("0.16.10", "0.16.9"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(version_cmp("21.1.77", "21.1.100"), std::cmp::Ordering::Less);
    }
}
