//! Instances: isolated game directories with their own version and settings.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, IoContext, Result};
use crate::fsutil;
use crate::settings::Resolution;

const CONFIG_FILE: &str = "instance.json";

/// Serialises read-modify-write cycles on instance configs within this process, so a
/// playtime update from a finishing game can't race with a settings save from the UI.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModLoader {
    #[default]
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    NeoForge,
    /// Horizon, the Rust reimplementation of the client (not a Java mod loader).
    Horizon,
}

impl ModLoader {
    pub fn name(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::Forge => "Forge",
            Self::NeoForge => "NeoForge",
            Self::Horizon => "Horizon",
        }
    }

    /// Loader id used by Modrinth and in `.mrpack` files.
    pub fn modrinth_id(self) -> Option<&'static str> {
        match self {
            Self::Fabric => Some("fabric"),
            Self::Quilt => Some("quilt"),
            Self::Forge => Some("forge"),
            Self::NeoForge => Some("neoforge"),
            Self::Vanilla | Self::Horizon => None,
        }
    }
}

/// Per-instance overrides of the global settings. `None` means "use global".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_memory_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_memory_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_args: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub name: String,
    pub game_version: String,
    #[serde(default)]
    pub loader: ModLoader,
    /// Loader version (e.g. Fabric `0.16.9`, Forge `47.2.0`). `None` for vanilla.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    /// Id of the installed loader profile under `versions/`, set once installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Where this instance came from (e.g. a modpack), for display and updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackOrigin>,
    pub created: DateTime<Utc>,
    #[serde(default)]
    pub last_played: Option<DateTime<Utc>>,
    #[serde(default)]
    pub playtime_secs: u64,
    /// Theme material picked by the user (e.g. `amethyst`); drives the instance's colours
    /// in the UI. `None` lets the UI pick one from the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    #[serde(default)]
    pub overrides: InstanceOverrides,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackOrigin {
    /// `modrinth`, `curseforge` or `file`.
    pub source: String,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// Directory name; stable identifier even if the display name changes.
    pub id: String,
    pub dir: PathBuf,
    pub config: InstanceConfig,
}

impl Instance {
    /// The game directory (`.minecraft`) of this instance.
    pub fn game_dir(&self) -> PathBuf {
        self.dir.join(".minecraft")
    }

    pub fn natives_dir(&self) -> PathBuf {
        self.dir.join("natives")
    }

    /// The version JSON id to launch: the loader profile once installed, else vanilla.
    pub fn version_id(&self) -> &str {
        match self.config.loader {
            ModLoader::Vanilla | ModLoader::Horizon => &self.config.game_version,
            _ => self
                .config
                .profile_id
                .as_deref()
                .unwrap_or(&self.config.game_version),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstanceManager {
    root: PathBuf,
}

impl InstanceManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// All valid instances, most recently played first. Broken instance folders are
    /// logged and skipped rather than failing the whole list.
    pub fn list(&self) -> Result<Vec<Instance>> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).at(&self.root),
        };
        let mut out = Vec::new();
        for entry in entries {
            let entry = entry.at(&self.root)?;
            if !entry.file_type().at(entry.path())?.is_dir() {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            match self.get(&id) {
                Ok(instance) => out.push(instance),
                Err(Error::InstanceNotFound(_)) => {}
                Err(e) => tracing::warn!("skipping instance '{id}': {e}"),
            }
        }
        out.sort_by(|a, b| {
            b.config
                .last_played
                .cmp(&a.config.last_played)
                .then_with(|| {
                    a.config
                        .name
                        .to_lowercase()
                        .cmp(&b.config.name.to_lowercase())
                })
        });
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Result<Instance> {
        let dir = self.dir_for(id)?;
        let config: InstanceConfig = fsutil::read_json(&dir.join(CONFIG_FILE))?
            .ok_or_else(|| Error::InstanceNotFound(id.to_owned()))?;
        Ok(Instance {
            id: id.to_owned(),
            dir,
            config,
        })
    }

    pub fn create(&self, name: &str, game_version: &str) -> Result<Instance> {
        self.create_with(name, game_version, ModLoader::Vanilla, None)
    }

    pub fn create_with(
        &self,
        name: &str,
        game_version: &str,
        loader: ModLoader,
        loader_version: Option<String>,
    ) -> Result<Instance> {
        if loader != ModLoader::Vanilla && loader != ModLoader::Horizon && loader_version.is_none()
        {
            return Err(Error::Invalid(format!("pick a {} version", loader.name())));
        }
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Invalid("instance name cannot be empty".into()));
        }
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let base = sanitize_dir_name(name);
        let mut id = base.clone();
        let mut n = 2;
        while self.root.join(&id).exists() {
            id = format!("{base} ({n})");
            n += 1;
        }
        let dir = self.root.join(&id);
        let instance = Instance {
            config: InstanceConfig {
                name: name.to_owned(),
                game_version: game_version.to_owned(),
                loader,
                loader_version,
                profile_id: None,
                pack: None,
                created: Utc::now(),
                last_played: None,
                playtime_secs: 0,
                material: None,
                overrides: InstanceOverrides::default(),
            },
            id,
            dir,
        };
        std::fs::create_dir_all(instance.game_dir()).at(instance.game_dir())?;
        fsutil::write_json_atomic(&instance.dir.join(CONFIG_FILE), &instance.config)?;
        Ok(instance)
    }

    /// Reads the latest config from disk, applies `f`, and writes it back.
    pub fn update(&self, id: &str, f: impl FnOnce(&mut InstanceConfig)) -> Result<Instance> {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut instance = self.get(id)?;
        f(&mut instance.config);
        fsutil::write_json_atomic(&instance.dir.join(CONFIG_FILE), &instance.config)?;
        Ok(instance)
    }

    /// Adds a finished play session to the instance's statistics.
    pub fn record_session(
        &self,
        id: &str,
        started: DateTime<Utc>,
        seconds: u64,
    ) -> Result<Instance> {
        self.update(id, |c| {
            c.playtime_secs = c.playtime_secs.saturating_add(seconds);
            c.last_played = Some(started);
        })
    }

    /// Permanently deletes the instance folder, including worlds.
    pub fn delete(&self, id: &str) -> Result<()> {
        let dir = self.dir_for(id)?;
        if !dir.join(CONFIG_FILE).exists() {
            return Err(Error::InstanceNotFound(id.to_owned()));
        }
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::remove_dir_all(&dir).at(&dir)
    }

    fn dir_for(&self, id: &str) -> Result<PathBuf> {
        if id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\']) {
            return Err(Error::Invalid(format!("invalid instance id '{id}'")));
        }
        Ok(self.root.join(id))
    }
}

/// Makes a display name safe as a folder name on every OS.
fn sanitize_dir_name(name: &str) -> String {
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .take(64)
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim().to_owned();
    if cleaned.is_empty() || RESERVED.contains(&cleaned.to_uppercase().as_str()) {
        format!("instance_{cleaned}")
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_dir_name("My: World?"), "My_ World_");
        assert_eq!(sanitize_dir_name("con"), "instance_con");
        assert_eq!(sanitize_dir_name("..."), "instance_");
        assert_eq!(sanitize_dir_name("a/b\\c"), "a_b_c");
    }

    #[test]
    fn create_list_update_delete() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = InstanceManager::new(dir.path());
        let a = mgr.create("Survival", "1.21").unwrap();
        let b = mgr.create("Survival", "1.8.9").unwrap();
        assert_eq!(a.id, "Survival");
        assert_eq!(b.id, "Survival (2)");
        assert!(a.game_dir().is_dir());

        mgr.record_session(&b.id, Utc::now(), 90).unwrap();
        mgr.record_session(&b.id, Utc::now(), 30).unwrap();
        let list = mgr.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, b.id, "recently played first");
        assert_eq!(list[0].config.playtime_secs, 120);

        mgr.update(&a.id, |c| c.overrides.max_memory_mb = Some(6144))
            .unwrap();
        assert_eq!(
            mgr.get(&a.id).unwrap().config.overrides.max_memory_mb,
            Some(6144)
        );

        mgr.delete(&a.id).unwrap();
        assert!(matches!(mgr.get(&a.id), Err(Error::InstanceNotFound(_))));
        assert!(mgr.get("../x").is_err());
        assert!(mgr.create("  ", "1.21").is_err());
    }
}
