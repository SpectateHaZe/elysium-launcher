use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Environment variable that overrides the data directory (portable installs, tests).
pub const HOME_ENV: &str = "ELYSIUM_HOME";

/// Layout of the launcher's data directory.
///
/// Game files that are identical between instances (libraries, assets, client jars,
/// Java runtimes) are shared; everything instance-specific lives under `instances/`.
#[derive(Debug, Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `$ELYSIUM_HOME`, or the platform data directory (`%APPDATA%\ElysiumLauncher` on Windows).
    pub fn from_env() -> Result<Self> {
        if let Some(dir) = std::env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
            return Ok(Self::new(dir));
        }
        let base = dirs::data_dir()
            .ok_or_else(|| Error::Invalid("could not determine the user data directory".into()))?;
        Ok(Self::new(base.join("ElysiumLauncher")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }
    pub fn meta_dir(&self) -> PathBuf {
        self.root.join("meta")
    }
    pub fn libraries_dir(&self) -> PathBuf {
        self.root.join("libraries")
    }
    pub fn assets_dir(&self) -> PathBuf {
        self.root.join("assets")
    }
    pub fn asset_index_file(&self, id: &str) -> PathBuf {
        self.assets_dir().join("indexes").join(format!("{id}.json"))
    }
    pub fn asset_object(&self, hash: &str) -> PathBuf {
        self.assets_dir()
            .join("objects")
            .join(&hash[..2])
            .join(hash)
    }
    pub fn virtual_assets_dir(&self, id: &str) -> PathBuf {
        self.assets_dir().join("virtual").join(id)
    }
    pub fn log_config_file(&self, id: &str) -> PathBuf {
        self.assets_dir().join("log_configs").join(id)
    }
    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.root.join("versions").join(id)
    }
    pub fn version_json(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.json"))
    }
    pub fn version_jar(&self, id: &str) -> PathBuf {
        self.version_dir(id).join(format!("{id}.jar"))
    }
    pub fn java_dir(&self) -> PathBuf {
        self.root.join("java")
    }
    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }
}
