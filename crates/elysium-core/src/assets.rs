//! Asset index handling, including the legacy `virtual` and `map_to_resources` layouts
//! used by versions before 1.7.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, IoContext, Result};
use crate::http::DownloadTask;
use crate::java::safe_join;
use crate::paths::Paths;

pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
    /// Pre-1.6: assets are copied into `<game dir>/resources`.
    #[serde(default)]
    pub map_to_resources: bool,
    /// 1.6: assets are copied into `assets/virtual/<id>`.
    #[serde(default, rename = "virtual")]
    pub is_virtual: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

impl AssetIndex {
    pub fn load(path: &Path) -> Result<Self> {
        crate::fsutil::read_json(path)?
            .ok_or_else(|| Error::Invalid(format!("asset index {} is missing", path.display())))
    }

    /// Download tasks for every object in the index.
    pub fn download_tasks(&self, paths: &Paths) -> Vec<DownloadTask> {
        self.objects
            .values()
            .filter(|o| o.hash.len() == 40)
            .map(|o| {
                DownloadTask::new(
                    format!("{RESOURCES_URL}/{}/{}", &o.hash[..2], o.hash),
                    paths.asset_object(&o.hash),
                )
                .sha1(Some(o.hash.clone()))
                .size(Some(o.size))
            })
            .collect()
    }

    /// Directory passed to the game as `${game_assets}` for legacy versions, if any.
    pub fn legacy_dir(&self, paths: &Paths, index_id: &str, game_dir: &Path) -> Option<PathBuf> {
        if self.map_to_resources {
            Some(game_dir.join("resources"))
        } else if self.is_virtual {
            Some(paths.virtual_assets_dir(index_id))
        } else {
            None
        }
    }

    /// Copies objects into the legacy layout (no-op for modern indexes). Files that already
    /// exist with the right size are left alone.
    pub fn materialize_legacy(&self, paths: &Paths, index_id: &str, game_dir: &Path) -> Result<()> {
        let Some(target) = self.legacy_dir(paths, index_id, game_dir) else {
            return Ok(());
        };
        for (name, object) in &self.objects {
            let dest = safe_join(&target, name)?;
            if std::fs::metadata(&dest).is_ok_and(|m| m.len() == object.size) {
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            let src = paths.asset_object(&object.hash);
            std::fs::copy(&src, &dest).at(&src)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_layouts() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let hash = "a9993e364706816aba3e25717850c26c9cd0d89d";
        let obj = paths.asset_object(hash);
        std::fs::create_dir_all(obj.parent().unwrap()).unwrap();
        std::fs::write(&obj, b"abc").unwrap();

        let index: AssetIndex = serde_json::from_str(&format!(
            r#"{{"map_to_resources":true,"objects":{{"sound/a.ogg":{{"hash":"{hash}","size":3}}}}}}"#
        ))
        .unwrap();
        let game_dir = dir.path().join("game");
        index
            .materialize_legacy(&paths, "pre-1.6", &game_dir)
            .unwrap();
        assert_eq!(
            std::fs::read(game_dir.join("resources/sound/a.ogg")).unwrap(),
            b"abc"
        );

        let index: AssetIndex = serde_json::from_str(&format!(
            r#"{{"virtual":true,"objects":{{"x/y.png":{{"hash":"{hash}","size":3}}}}}}"#
        ))
        .unwrap();
        index
            .materialize_legacy(&paths, "legacy", &game_dir)
            .unwrap();
        assert!(paths.virtual_assets_dir("legacy").join("x/y.png").exists());
        assert_eq!(
            index.download_tasks(&paths)[0].url,
            format!("{RESOURCES_URL}/a9/{hash}")
        );
    }
}
