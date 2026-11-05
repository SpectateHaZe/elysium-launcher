//! Preparing a version for launch: metadata, Java, libraries, natives, assets.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::assets::AssetIndex;
use crate::error::{Error, IoContext, Result};
use crate::http::{DownloadTask, Http, Verify};
use crate::instance::Instance;
use crate::java;
use crate::meta::VersionJson;
use crate::paths::Paths;
use crate::progress::Progress;
use crate::rules::Environment;
use crate::versions;

/// Everything needed to build the launch command for an instance.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub version: VersionJson,
    pub java: PathBuf,
    /// Library jars followed by the client jar.
    pub classpath: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub assets_root: PathBuf,
    pub assets_index: String,
    /// `${game_assets}` for legacy versions (virtual or resources dir).
    pub legacy_assets_dir: Option<PathBuf>,
    /// The log4j argument with `${path}` already resolved.
    pub logging_arg: Option<String>,
    pub libraries_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    /// Custom Java executable; `None` installs the Mojang runtime the version wants.
    pub java_path: Option<PathBuf>,
    pub concurrency: usize,
    pub verify: Verify,
}

/// A version whose shared files (jar, libraries, assets, Java) are on disk.
#[derive(Debug, Clone)]
pub struct InstalledVersion {
    pub version: VersionJson,
    pub java: PathBuf,
    pub classpath: Vec<PathBuf>,
    natives: Vec<(PathBuf, Vec<String>)>,
    pub asset_index: Option<AssetIndex>,
    pub logging_arg: Option<String>,
}

/// Picks the Java to use for `version`: the user's override, or the Mojang runtime.
pub async fn java_for(
    paths: &Paths,
    http: &Http,
    version: &VersionJson,
    options: &InstallOptions,
    progress: Arc<Progress>,
) -> Result<PathBuf> {
    match &options.java_path {
        Some(path) if path.is_file() => Ok(path.clone()),
        Some(path) => Err(Error::Invalid(format!(
            "custom Java not found at {}",
            path.display()
        ))),
        None => {
            java::ensure_runtime(
                paths,
                http,
                version.java_component(),
                options.concurrency,
                progress,
            )
            .await
        }
    }
}

/// Downloads and verifies everything version `id` (and what it inherits from) needs.
pub async fn install_version(
    paths: &Paths,
    http: &Http,
    id: &str,
    options: &InstallOptions,
    progress: Arc<Progress>,
) -> Result<InstalledVersion> {
    let env = Environment::current();

    progress.set_stage("Fetching version info", 0);
    let manifest = versions::fetch_manifest(paths, http).await?;
    let version = versions::resolve(paths, http, &manifest, id).await?;
    progress.check()?;

    let java = java_for(paths, http, &version, options, Arc::clone(&progress)).await?;
    progress.check()?;

    progress.set_stage("Downloading game files", 0);
    let mut tasks = Vec::new();
    let mut classpath = Vec::new();
    let mut natives = Vec::new();

    for lib in version.libraries.iter().filter(|l| l.is_allowed(&env)) {
        if let Some((rel, artifact)) = lib.artifact() {
            let path = paths.libraries_dir().join(&rel);
            // Loader installers generate some libraries locally; they have no URL.
            if !artifact.url.is_empty() {
                tasks.push(artifact_task(&artifact, path.clone()));
            }
            if !classpath.contains(&path) {
                classpath.push(path);
            }
        }
        if let Some((rel, artifact)) = lib.native_artifact(&env) {
            let path = paths.libraries_dir().join(&rel);
            tasks.push(artifact_task(artifact, path.clone()));
            let exclude = lib
                .extract
                .as_ref()
                .map(|e| e.exclude.clone())
                .unwrap_or_default();
            natives.push((path, exclude));
        }
    }

    let downloads = version
        .downloads
        .as_ref()
        .ok_or_else(|| Error::Invalid(format!("version {id} has no client download")))?;
    let client_jar = paths.version_jar(version.jar_version());
    tasks.push(artifact_task(&downloads.client, client_jar.clone()));
    classpath.push(client_jar);

    let logging_arg = match version.logging.as_ref().and_then(|l| l.client.as_ref()) {
        Some(cfg) => {
            let path = paths.log_config_file(&cfg.file.id);
            tasks.push(
                DownloadTask::new(&cfg.file.url, &path)
                    .sha1(Some(&cfg.file.sha1))
                    .size(Some(cfg.file.size)),
            );
            Some(cfg.argument.replace("${path}", &path.to_string_lossy()))
        }
        None => None,
    };

    let asset_index = match &version.asset_index {
        Some(r) => {
            let index_path = paths.asset_index_file(&r.id);
            let task = DownloadTask::new(&r.url, &index_path)
                .sha1(Some(&r.sha1))
                .size(Some(r.size));
            http.download_all(vec![task], 1, Verify::Hash, Arc::clone(&progress))
                .await?;
            let index = AssetIndex::load(&index_path)?;
            tasks.extend(index.download_tasks(paths));
            Some(index)
        }
        None => None,
    };

    http.download_all(
        tasks,
        options.concurrency,
        options.verify,
        Arc::clone(&progress),
    )
    .await?;
    progress.check()?;

    // Libraries generated by a loader installer must exist even though we can't fetch them.
    if let Some(missing) = classpath.iter().find(|p| !p.exists()) {
        return Err(Error::Invalid(format!(
            "{} is missing; repair the instance to reinstall its loader",
            missing.display()
        )));
    }

    Ok(InstalledVersion {
        version,
        java,
        classpath,
        natives,
        asset_index,
        logging_arg,
    })
}

/// Downloads and verifies everything `instance` needs to run, installing its mod
/// loader first if needed.
pub async fn prepare(
    paths: &Paths,
    http: &Http,
    instance: &Instance,
    options: &InstallOptions,
    progress: Arc<Progress>,
) -> Result<Prepared> {
    let version_id =
        crate::loaders::ensure_installed(paths, http, instance, options, Arc::clone(&progress))
            .await?;
    let installed =
        install_version(paths, http, &version_id, options, Arc::clone(&progress)).await?;

    progress.set_stage("Preparing game", 0);
    let natives_dir = instance.natives_dir();
    let game_dir = instance.game_dir();
    let assets_index = installed.version.assets_id().to_owned();
    let legacy_assets_dir = {
        let natives_dir = natives_dir.clone();
        let paths = paths.clone();
        let assets_index = assets_index.clone();
        let natives = installed.natives.clone();
        let index = installed.asset_index.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<PathBuf>> {
            extract_natives(&natives, &natives_dir)?;
            std::fs::create_dir_all(&game_dir).at(&game_dir)?;
            match index {
                Some(index) => {
                    index.materialize_legacy(&paths, &assets_index, &game_dir)?;
                    Ok(index.legacy_dir(&paths, &assets_index, &game_dir))
                }
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| Error::Invalid(format!("preparation task failed: {e}")))??
    };

    Ok(Prepared {
        version: installed.version,
        java: installed.java,
        classpath: installed.classpath,
        natives_dir,
        assets_root: paths.assets_dir(),
        assets_index,
        legacy_assets_dir,
        logging_arg: installed.logging_arg,
        libraries_dir: paths.libraries_dir(),
    })
}

fn artifact_task(artifact: &crate::meta::Artifact, path: PathBuf) -> DownloadTask {
    DownloadTask::new(&artifact.url, path)
        .sha1(artifact.sha1.clone())
        .size(artifact.size)
}

/// Extracts legacy native jars into a clean `dir`, honouring `extract.exclude` prefixes.
fn extract_natives(natives: &[(PathBuf, Vec<String>)], dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).at(dir)?;
    }
    std::fs::create_dir_all(dir).at(dir)?;
    for (jar, exclude) in natives {
        let file = std::fs::File::open(jar).at(jar)?;
        let zip_err = |source| Error::Zip {
            path: jar.clone(),
            source,
        };
        let mut archive = zip::ZipArchive::new(file).map_err(zip_err)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(zip_err)?;
            let name = entry.name().map_err(zip_err)?.into_owned();
            if entry.is_dir() || exclude.iter().any(|e| name.starts_with(e.as_str())) {
                continue;
            }
            // `enclosed_name` rejects absolute paths and `..` (zip-slip).
            let Some(rel) = entry.enclosed_name() else {
                continue;
            };
            let dest = dir.join(rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data).at(&dest)?;
            std::fs::write(&dest, data).at(&dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_natives_with_excludes() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("n.jar");
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("lwjgl.dll", opts).unwrap();
            zip.write_all(b"dll").unwrap();
            zip.start_file("META-INF/MANIFEST.MF", opts).unwrap();
            zip.write_all(b"m").unwrap();
            zip.finish().unwrap();
        }
        let out = dir.path().join("natives");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("stale.dll"), b"old").unwrap();
        extract_natives(&[(jar, vec!["META-INF/".into()])], &out).unwrap();
        assert_eq!(std::fs::read(out.join("lwjgl.dll")).unwrap(), b"dll");
        assert!(!out.join("META-INF").exists());
        assert!(!out.join("stale.dll").exists());
    }
}
