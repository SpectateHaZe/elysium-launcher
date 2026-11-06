//! Bringing instances over from Prism Launcher, MultiMC, the CurseForge app and the
//! official Minecraft Launcher.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, IoContext, Result};
use crate::instance::{Instance, InstanceManager, ModLoader};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForeignSource {
    Prism,
    MultiMc,
    CurseForge,
    Official,
}

/// An instance found in another launcher.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub struct Foreign {
    pub source: ForeignSource,
    pub name: String,
    pub game_version: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub game_dir: PathBuf,
}

/// Folders the other launcher manages itself; never copied.
const SKIP: &[&str] = &[
    "versions",
    "libraries",
    "assets",
    "runtime",
    "natives",
    "logs",
    "crash-reports",
    "webcache2",
    ".fabric",
    ".cache",
    "launcher_accounts.json",
    "launcher_profiles.json",
    "launcher_log.txt",
];

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Default places other launchers keep their instances on this OS.
fn default_roots() -> Vec<(ForeignSource, PathBuf)> {
    let mut out = Vec::new();
    if let Some(data) = dirs::data_dir() {
        out.push((
            ForeignSource::Prism,
            data.join("PrismLauncher").join("instances"),
        ));
        out.push((
            ForeignSource::MultiMc,
            data.join("MultiMC").join("instances"),
        ));
    }
    if let Some(h) = home() {
        out.push((
            ForeignSource::CurseForge,
            h.join("curseforge").join("minecraft").join("Instances"),
        ));
        out.push((
            ForeignSource::Prism,
            h.join(".local/share/PrismLauncher/instances"),
        ));
    }
    let official = if cfg!(target_os = "macos") {
        dirs::data_dir().map(|d| d.join("minecraft"))
    } else if cfg!(windows) {
        dirs::data_dir().map(|d| d.join(".minecraft"))
    } else {
        home().map(|h| h.join(".minecraft"))
    };
    if let Some(o) = official {
        out.push((ForeignSource::Official, o));
    }
    out.sort();
    out.dedup();
    out
}

/// Every instance found in the default locations.
pub fn scan() -> Vec<Foreign> {
    let mut found = Vec::new();
    for (source, root) in default_roots() {
        found.extend(scan_root(source, &root));
    }
    found
}

/// Instances under one launcher's folder (for MultiMC, which is portable, the player picks it).
pub fn scan_root(source: ForeignSource, root: &Path) -> Vec<Foreign> {
    match source {
        ForeignSource::Official => read_official(root),
        _ => std::fs::read_dir(root)
            .map(|r| {
                r.flatten()
                    .filter(|e| e.path().is_dir())
                    .filter_map(|e| match source {
                        ForeignSource::CurseForge => read_curseforge(&e.path()),
                        _ => read_mmc(source, &e.path()),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Prism and MultiMC: `instance.cfg` (name) and `mmc-pack.json` (components).
fn read_mmc(source: ForeignSource, dir: &Path) -> Option<Foreign> {
    let pack: Value =
        serde_json::from_slice(&std::fs::read(dir.join("mmc-pack.json")).ok()?).ok()?;
    let cfg = std::fs::read_to_string(dir.join("instance.cfg")).unwrap_or_default();
    let name = cfg
        .lines()
        .find_map(|l| l.strip_prefix("name="))
        .map(str::to_owned)
        .unwrap_or_else(|| {
            dir.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    let mut game_version = None;
    let (mut loader, mut loader_version) = (ModLoader::Vanilla, None);
    for c in pack.get("components")?.as_array()? {
        let uid = c.get("uid").and_then(Value::as_str).unwrap_or_default();
        let version = c.get("version").and_then(Value::as_str).map(str::to_owned);
        let l = match uid {
            "net.minecraft" => {
                game_version = version;
                continue;
            }
            "net.fabricmc.fabric-loader" => ModLoader::Fabric,
            "org.quiltmc.quilt-loader" => ModLoader::Quilt,
            "net.minecraftforge" => ModLoader::Forge,
            "net.neoforged" => ModLoader::NeoForge,
            _ => continue,
        };
        loader = l;
        loader_version = version;
    }
    let game_dir = [".minecraft", "minecraft"]
        .iter()
        .map(|d| dir.join(d))
        .find(|d| d.is_dir())
        .unwrap_or_else(|| dir.join(".minecraft"));
    Some(Foreign {
        source,
        name,
        game_version: game_version?,
        loader,
        loader_version,
        game_dir,
    })
}

/// CurseForge app: `minecraftinstance.json`; the instance folder is the game folder.
fn read_curseforge(dir: &Path) -> Option<Foreign> {
    let v: Value =
        serde_json::from_slice(&std::fs::read(dir.join("minecraftinstance.json")).ok()?).ok()?;
    let game_version = v.get("gameVersion").and_then(Value::as_str)?.to_owned();
    let loader_name = v
        .pointer("/baseModLoader/name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (loader, loader_version) = crate::content::modpack::parse_cf_loader(loader_name);
    Some(Foreign {
        source: ForeignSource::CurseForge,
        name: v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("CurseForge instance")
            .to_owned(),
        game_version,
        loader,
        loader_version,
        game_dir: dir.to_owned(),
    })
}

/// Splits the official launcher's version ids, e.g. `fabric-loader-0.16.9-1.21.1`.
fn parse_version_id(id: &str) -> (String, ModLoader, Option<String>) {
    for (prefix, loader) in [
        ("fabric-loader-", ModLoader::Fabric),
        ("quilt-loader-", ModLoader::Quilt),
    ] {
        if let Some(rest) = id.strip_prefix(prefix)
            && let Some((lv, mc)) = rest.split_once('-')
        {
            return (mc.to_owned(), loader, Some(lv.to_owned()));
        }
    }
    if let Some(lv) = id.strip_prefix("neoforge-") {
        return (String::new(), ModLoader::NeoForge, Some(lv.to_owned()));
    }
    if let Some((mc, lv)) = id.split_once("-forge-") {
        return (mc.to_owned(), ModLoader::Forge, Some(lv.to_owned()));
    }
    (id.to_owned(), ModLoader::Vanilla, None)
}

fn read_official(root: &Path) -> Vec<Foreign> {
    let Ok(data) = std::fs::read(root.join("launcher_profiles.json")) else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_slice::<Value>(&data) else {
        return Vec::new();
    };
    let Some(profiles) = v.get("profiles").and_then(Value::as_object) else {
        return Vec::new();
    };
    profiles
        .values()
        .filter_map(|p| {
            let id = p.get("lastVersionId").and_then(Value::as_str)?;
            // "latest-release" and similar don't name a version.
            if id.starts_with("latest-") {
                return None;
            }
            let (mut mc, loader, loader_version) = parse_version_id(id);
            if mc.is_empty() {
                // NeoForge ids don't include the Minecraft version; read it from the profile JSON.
                let json: Value = serde_json::from_slice(
                    &std::fs::read(root.join("versions").join(id).join(format!("{id}.json")))
                        .ok()?,
                )
                .ok()?;
                mc = json.get("inheritsFrom")?.as_str()?.to_owned();
            }
            let game_dir = p
                .get("gameDir")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| root.to_owned());
            let name = p
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Minecraft {mc}"));
            Some(Foreign {
                source: ForeignSource::Official,
                name,
                game_version: mc,
                loader,
                loader_version,
                game_dir,
            })
        })
        .collect()
}

fn copy_tree(src: &Path, dest: &Path, top: bool) -> Result<u64> {
    let mut copied = 0;
    std::fs::create_dir_all(dest).at(dest)?;
    for entry in std::fs::read_dir(src).at(src)?.flatten() {
        let name = entry.file_name();
        if top && SKIP.iter().any(|s| name.to_string_lossy() == *s) {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        let ft = entry.file_type().at(&from)?;
        if ft.is_dir() {
            copied += copy_tree(&from, &to, false)?;
        } else if ft.is_file() {
            copied += std::fs::copy(&from, &to).at(&from)?;
        }
    }
    Ok(copied)
}

/// Creates an Elysium instance from a foreign one and copies its game folder over
/// (worlds, mods, configs, packs, options), leaving the original untouched.
pub fn import(instances: &InstanceManager, foreign: &Foreign) -> Result<Instance> {
    if !foreign.game_dir.is_dir() {
        return Err(Error::Invalid(format!(
            "{} has no game folder to copy",
            foreign.name
        )));
    }
    let instance = instances.create_with(
        &foreign.name,
        &foreign.game_version,
        foreign.loader,
        foreign.loader_version.clone(),
    )?;
    if let Err(e) = copy_tree(&foreign.game_dir, &instance.game_dir(), true) {
        let _ = instances.delete(&instance.id);
        return Err(e);
    }
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_prism_curseforge_and_official() {
        let dir = tempfile::tempdir().unwrap();
        let prism = dir.path().join("prism/Better Fabric");
        std::fs::create_dir_all(prism.join(".minecraft/mods")).unwrap();
        std::fs::write(
            prism.join("instance.cfg"),
            "InstanceType=OneSix\nname=Better Fabric\n",
        )
        .unwrap();
        std::fs::write(
            prism.join("mmc-pack.json"),
            r#"{"components":[{"uid":"org.lwjgl3","version":"3.3.3"},{"uid":"net.minecraft","version":"1.21.1"},{"uid":"net.fabricmc.intermediary","version":"1.21.1"},{"uid":"net.fabricmc.fabric-loader","version":"0.16.9"}],"formatVersion":1}"#,
        )
        .unwrap();
        std::fs::write(prism.join(".minecraft/mods/sodium.jar"), b"jar").unwrap();
        let found = scan_root(ForeignSource::Prism, &dir.path().join("prism"));
        assert_eq!(found.len(), 1);
        assert_eq!(
            (
                found[0].game_version.as_str(),
                found[0].loader,
                found[0].loader_version.as_deref()
            ),
            ("1.21.1", ModLoader::Fabric, Some("0.16.9"))
        );

        let cf = dir.path().join("cf/All the Mods");
        std::fs::create_dir_all(&cf).unwrap();
        std::fs::write(cf.join("minecraftinstance.json"), r#"{"name":"All the Mods","gameVersion":"1.20.1","baseModLoader":{"name":"forge-47.2.0"}}"#).unwrap();
        let found = scan_root(ForeignSource::CurseForge, &dir.path().join("cf"));
        assert_eq!(
            (found[0].loader, found[0].loader_version.as_deref()),
            (ModLoader::Forge, Some("47.2.0"))
        );

        let official = dir.path().join("official");
        std::fs::create_dir_all(&official).unwrap();
        std::fs::write(
            official.join("launcher_profiles.json"),
            r#"{"profiles":{"a":{"name":"","lastVersionId":"latest-release"},"b":{"name":"Fabric","lastVersionId":"fabric-loader-0.16.9-1.21.1","gameDir":"/somewhere"},"c":{"name":"Old","lastVersionId":"1.8.9"}}}"#,
        )
        .unwrap();
        let mut found = scan_root(ForeignSource::Official, &official);
        found.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(found.len(), 2);
        assert_eq!(
            (
                found[0].name.as_str(),
                found[0].loader,
                found[0].game_version.as_str()
            ),
            ("Fabric", ModLoader::Fabric, "1.21.1")
        );
        assert_eq!(found[1].game_dir, official);
    }

    #[test]
    fn version_ids() {
        assert_eq!(
            parse_version_id("1.20.1-forge-47.2.0"),
            ("1.20.1".into(), ModLoader::Forge, Some("47.2.0".into()))
        );
        assert_eq!(
            parse_version_id("quilt-loader-0.26.0-1.20.4"),
            ("1.20.4".into(), ModLoader::Quilt, Some("0.26.0".into()))
        );
        assert_eq!(
            parse_version_id("1.12.2"),
            ("1.12.2".into(), ModLoader::Vanilla, None)
        );
    }

    #[test]
    fn import_copies_content_not_launcher_files() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        for p in [
            "saves/World/region",
            "mods",
            "config",
            "versions/1.21.1",
            "libraries/x",
            "logs",
        ] {
            std::fs::create_dir_all(src.join(p)).unwrap();
        }
        std::fs::write(src.join("saves/World/level.dat"), b"lvl").unwrap();
        std::fs::write(src.join("mods/a.jar"), b"a").unwrap();
        std::fs::write(src.join("options.txt"), b"fov:70").unwrap();
        std::fs::write(src.join("versions/1.21.1/1.21.1.jar"), b"big").unwrap();
        let foreign = Foreign {
            source: ForeignSource::Official,
            name: "Main".into(),
            game_version: "1.21.1".into(),
            loader: ModLoader::Vanilla,
            loader_version: None,
            game_dir: src.clone(),
        };
        let mgr = InstanceManager::new(dir.path().join("instances"));
        let inst = import(&mgr, &foreign).unwrap();
        let g = inst.game_dir();
        assert!(
            g.join("saves/World/level.dat").exists()
                && g.join("mods/a.jar").exists()
                && g.join("options.txt").exists()
        );
        assert!(
            !g.join("versions").exists()
                && !g.join("libraries").exists()
                && !g.join("logs").exists()
        );
        assert!(
            src.join("mods/a.jar").exists(),
            "the original is left alone"
        );
    }
}
