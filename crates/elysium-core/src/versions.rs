//! Fetching and caching the version manifest and version JSONs.

use std::sync::Arc;

use crate::error::{Error, Result};
use crate::fsutil;
use crate::http::{DownloadTask, Http, Verify};
use crate::meta::manifest::MANIFEST_URL;
use crate::meta::{VersionJson, VersionManifest};
use crate::paths::Paths;
use crate::progress::Progress;

fn manifest_cache(paths: &Paths) -> std::path::PathBuf {
    paths.meta_dir().join("version_manifest_v2.json")
}

/// Fetches the version manifest, updating the local cache. Falls back to the cache
/// when offline.
pub async fn fetch_manifest(paths: &Paths, http: &Http) -> Result<VersionManifest> {
    match http.get_json::<VersionManifest>(MANIFEST_URL).await {
        Ok(manifest) => {
            if let Err(e) = fsutil::write_json_atomic(&manifest_cache(paths), &manifest) {
                tracing::warn!("could not cache version manifest: {e}");
            }
            Ok(manifest)
        }
        Err(e) => match cached_manifest(paths)? {
            Some(cached) => {
                tracing::warn!("using cached version manifest: {e}");
                Ok(cached)
            }
            None => Err(e),
        },
    }
}

pub fn cached_manifest(paths: &Paths) -> Result<Option<VersionManifest>> {
    fsutil::read_json(&manifest_cache(paths))
}

/// Loads `id` and everything it inherits from, merged into one launchable version.
pub async fn resolve(
    paths: &Paths,
    http: &Http,
    manifest: &VersionManifest,
    id: &str,
) -> Result<VersionJson> {
    let mut version = load_version(paths, http, manifest, id).await?;
    let mut depth = 0;
    while let Some(parent_id) = version.inherits_from.clone() {
        depth += 1;
        if depth > 8 {
            return Err(Error::Invalid(format!("{id} inherits too deeply")));
        }
        let parent = load_version(paths, http, manifest, &parent_id).await?;
        let parent_inherits = parent.inherits_from.clone();
        version = version.merge(parent);
        version.inherits_from = parent_inherits;
    }
    if version.jar_id.is_none() {
        version.jar_id = Some(version.id.clone());
    }
    Ok(version)
}

/// Loads the version JSON for `id`, downloading (and verifying) it when needed.
pub async fn load_version(
    paths: &Paths,
    http: &Http,
    manifest: &VersionManifest,
    id: &str,
) -> Result<VersionJson> {
    let path = paths.version_json(id);
    if let Some(entry) = manifest.find(id) {
        let task = DownloadTask::new(&entry.url, &path).sha1(entry.sha1.clone());
        http.download_all(vec![task], 1, Verify::Hash, Arc::new(Progress::new()))
            .await?;
    }
    fsutil::read_json(&path)?.ok_or_else(|| Error::UnknownVersion(id.to_owned()))
}
