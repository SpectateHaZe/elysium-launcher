//! Playing through the official Minecraft Launcher, the way the CurseForge app does it.
//!
//! Elysium installs everything into its data folder (which has the official launcher's
//! layout: `versions/`, `libraries/`, `assets/`), writes a profile for the instance into
//! `launcher_profiles.json` there, and starts the official launcher with `--workDir` set to
//! that folder. The player signs in there; Elysium never sees their account.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde_json::{Map, Value, json};

use crate::error::{Error, IoContext, Result};
use crate::fsutil;
use crate::http::{DownloadTask, Http, Verify};
use crate::instance::Instance;
use crate::paths::Paths;
use crate::progress::Progress;
use crate::settings::EffectiveSettings;

/// Mojang's standalone launcher downloads.
const WINDOWS_LAUNCHER: &str = "https://launcher.mojang.com/download/Minecraft.exe";
const LINUX_LAUNCHER: &str = "https://launcher.mojang.com/download/Minecraft.tar.gz";

/// An installed official launcher that accepts `--workDir`.
pub fn find_installed() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var) {
                candidates.push(
                    Path::new(&dir)
                        .join("Minecraft Launcher")
                        .join("MinecraftLauncher.exe"),
                );
            }
        }
    } else if cfg!(target_os = "macos") {
        candidates.push("/Applications/Minecraft.app/Contents/MacOS/launcher".into());
    } else {
        candidates.extend([
            "/usr/bin/minecraft-launcher".into(),
            "/opt/minecraft-launcher/minecraft-launcher".into(),
        ]);
    }
    candidates.into_iter().find(|p| p.is_file())
}

fn bundled_path(paths: &Paths) -> PathBuf {
    let dir = paths.root().join("launcher");
    if cfg!(windows) {
        dir.join("Minecraft.exe")
    } else {
        dir.join("minecraft-launcher").join("minecraft-launcher")
    }
}

/// The official launcher to use, downloading Mojang's standalone copy if none is installed.
pub async fn ensure_launcher(
    paths: &Paths,
    http: &Http,
    progress: Arc<Progress>,
) -> Result<PathBuf> {
    if let Some(found) = find_installed() {
        return Ok(found);
    }
    let path = bundled_path(paths);
    if path.is_file() {
        return Ok(path);
    }
    progress.set_stage("Downloading the Minecraft Launcher", 0);
    if cfg!(windows) {
        http.download_all(
            vec![DownloadTask::new(WINDOWS_LAUNCHER, &path)],
            1,
            Verify::Size,
            progress,
        )
        .await?;
        Ok(path)
    } else if cfg!(target_os = "linux") {
        let archive = paths.root().join("launcher").join("Minecraft.tar.gz");
        http.download_all(
            vec![DownloadTask::new(LINUX_LAUNCHER, &archive)],
            1,
            Verify::Size,
            progress,
        )
        .await?;
        let dest = paths.root().join("launcher");
        let file = std::fs::File::open(&archive).at(&archive)?;
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        for entry in tar.entries().at(&archive)? {
            entry.at(&archive)?.unpack_in(&dest).at(&dest)?;
        }
        let _ = std::fs::remove_file(&archive);
        if path.is_file() {
            Ok(path)
        } else {
            Err(Error::Invalid(
                "the Minecraft Launcher download had an unexpected layout".into(),
            ))
        }
    } else {
        Err(Error::Invalid(
            "Install the Minecraft Launcher from minecraft.net, then try again".into(),
        ))
    }
}

fn profile_key(instance: &Instance) -> String {
    let id: String = instance
        .id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("elysium-{id}")
}

/// An empty jar: the official launcher expects `versions/<id>/<id>.jar` even for loader
/// profiles whose code comes from libraries and the parent version.
const EMPTY_JAR: &[u8] = &[
    0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// Adds or updates the instance's profile in `launcher_profiles.json`, keeping everything
/// else in the file (the official launcher's own settings and profiles) untouched.
pub fn write_profile(
    paths: &Paths,
    instance: &Instance,
    version_id: &str,
    settings: &EffectiveSettings,
) -> Result<String> {
    if version_id != instance.config.game_version {
        let jar = paths.version_jar(version_id);
        if !jar.exists() {
            fsutil::write_atomic(&jar, EMPTY_JAR)?;
        }
    }
    let file = paths.root().join("launcher_profiles.json");
    let mut root: Value = fsutil::read_json(&file)?.unwrap_or_else(|| json!({}));
    if !root.is_object() {
        root = json!({});
    }
    let obj = root.as_object_mut().expect("checked above");
    let profiles = obj
        .entry("profiles")
        .or_insert_with(|| Value::Object(Map::new()));
    if !profiles.is_object() {
        *profiles = Value::Object(Map::new());
    }
    let key = profile_key(instance);
    let now = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut java_args = vec![
        format!("-Xmx{}M", settings.max_memory_mb),
        format!("-Xms{}M", settings.min_memory_mb),
    ];
    java_args.extend(settings.jvm_args.iter().cloned());
    let mut profile = json!({
        "name": instance.config.name,
        "type": "custom",
        "created": instance.config.created.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "lastUsed": now,
        "lastVersionId": version_id,
        "gameDir": instance.game_dir().to_string_lossy(),
        "javaArgs": java_args.join(" "),
        "icon": "Grass",
    });
    if let Some(r) = settings.resolution {
        profile["resolution"] = json!({ "width": r.width, "height": r.height });
    }
    if let Some(java) = &settings.java_path {
        profile["javaDir"] = json!(java.to_string_lossy());
    }
    profiles
        .as_object_mut()
        .expect("checked above")
        .insert(key.clone(), profile);
    fsutil::write_json_atomic(&file, &root)?;
    Ok(key)
}

/// Starts the official launcher on Elysium's data folder. It keeps running on its own.
pub fn open(launcher: &Path, paths: &Paths) -> Result<()> {
    let mut cmd = std::process::Command::new(launcher);
    cmd.arg("--workDir")
        .arg(paths.root())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(dir) = launcher.parent() {
        cmd.current_dir(dir);
    }
    cmd.spawn().at(launcher)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{InstanceManager, ModLoader};
    use crate::settings::{Resolution, Settings};

    #[test]
    fn writes_profile_and_keeps_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        std::fs::write(
            paths.root().join("launcher_profiles.json"),
            r#"{"profiles":{"mine":{"name":"Keep me","type":"custom","lastVersionId":"1.20.1"}},"settings":{"crashAssistance":true},"version":3}"#,
        )
        .unwrap();
        let inst = InstanceManager::new(paths.instances_dir())
            .create_with(
                "My Pack",
                "1.21.1",
                ModLoader::Fabric,
                Some("0.16.9".into()),
            )
            .unwrap();
        let mut s = Settings {
            optimized_java_flags: false,
            jvm_args: "-Dfoo=1".into(),
            ..Default::default()
        }
        .effective(&inst.config.overrides);
        s.resolution = Some(Resolution {
            width: 1600,
            height: 900,
        });

        let key = write_profile(&paths, &inst, "fabric-loader-0.16.9-1.21.1", &s).unwrap();
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(paths.root().join("launcher_profiles.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["profiles"]["mine"]["name"], "Keep me");
        assert_eq!(v["settings"]["crashAssistance"], true);
        let p = &v["profiles"][&key];
        assert_eq!(p["lastVersionId"], "fabric-loader-0.16.9-1.21.1");
        assert_eq!(p["gameDir"], inst.game_dir().to_string_lossy().as_ref());
        assert_eq!(p["javaArgs"], "-Xmx4096M -Xms512M -Dfoo=1");
        assert_eq!(p["resolution"]["width"], 1600);
        assert!(
            paths.version_jar("fabric-loader-0.16.9-1.21.1").exists(),
            "loader profiles get the jar the launcher expects"
        );

        // Writing again updates the same profile rather than adding another.
        write_profile(&paths, &inst, "fabric-loader-0.16.9-1.21.1", &s).unwrap();
        let v: Value = serde_json::from_str(
            &std::fs::read_to_string(paths.root().join("launcher_profiles.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(v["profiles"].as_object().unwrap().len(), 2);
    }
}
