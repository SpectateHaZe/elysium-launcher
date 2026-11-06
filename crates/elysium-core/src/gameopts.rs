//! Global game options: keybinds and vanilla settings the user sets once in Elysium and
//! that new and imported instances start with. Stored as raw `options.txt` entries
//! (`key_key.jump` = `key.keyboard.space`, `fov` = `0.25`), so whatever Minecraft
//! understands can be carried over, and only the entries the user set are written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::{IoContext, Result};
use crate::instance::Instance;
use crate::paths::Paths;

pub type GameOptions = BTreeMap<String, String>;

fn file(paths: &Paths) -> PathBuf {
    paths.root().join("game-options.json")
}

pub fn load(paths: &Paths) -> Result<GameOptions> {
    Ok(crate::fsutil::read_json(&file(paths))?.unwrap_or_default())
}

pub fn save(paths: &Paths, opts: &GameOptions) -> Result<()> {
    crate::fsutil::write_json_atomic(&file(paths), opts)
}

/// Keys that make no sense to carry between instances (they describe this one install).
fn portable(key: &str) -> bool {
    !matches!(
        key,
        "version" | "resourcePacks" | "incompatibleResourcePacks" | "lastServer"
    ) && !key.contains(['\n', '\r', ':'])
}

/// Parses `options.txt` into its entries, keeping only ones worth carrying over.
pub fn parse(text: &str) -> GameOptions {
    text.lines()
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| portable(k))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// The options of an existing instance, to start the global ones from.
pub fn from_instance(instance: &Instance) -> Result<GameOptions> {
    let path = instance.game_dir().join("options.txt");
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(parse(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(GameOptions::new()),
        Err(e) => Err(e).at(&path),
    }
}

/// Keybinds as Minecraft 1.13+ names them (`key.keyboard.w`); older versions use numeric
/// codes, so they don't get them.
fn modern_keys(game_version: &str) -> bool {
    let mut parts = game_version.split('.');
    match (
        parts.next(),
        parts.next().and_then(|m| m.parse::<u32>().ok()),
    ) {
        (Some("1"), Some(minor)) => minor >= 13,
        // Alpha and beta versions ("a1.2.6", "b1.7.3") are old; snapshots ("24w10a") new.
        (Some(first), _) => !first.starts_with(['a', 'b', 'c', 'r', 'i']),
        _ => false,
    }
}

/// Writes the global options into a new instance's `options.txt`, overwriting only those
/// entries and keeping everything else (a modpack's own defaults included).
pub fn apply(instance: &Instance, opts: &GameOptions) -> Result<()> {
    let keys = modern_keys(&instance.config.game_version);
    let wanted: Vec<(&String, &String)> = opts
        .iter()
        .filter(|(k, _)| portable(k) && (keys || !k.starts_with("key_")))
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let path = instance.game_dir().join("options.txt");
    let current = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).at(&path),
    };
    let mut done = std::collections::HashSet::new();
    let mut out: Vec<String> = current
        .lines()
        .map(|l| match l.split_once(':') {
            Some((k, _)) => match opts.get(k).filter(|_| wanted.iter().any(|(w, _)| *w == k)) {
                Some(v) => {
                    done.insert(k.to_owned());
                    format!("{k}:{v}")
                }
                None => l.to_owned(),
            },
            None => l.to_owned(),
        })
        .collect();
    out.extend(
        wanted
            .iter()
            .filter(|(k, _)| !done.contains(k.as_str()))
            .map(|(k, v)| format!("{k}:{v}")),
    );
    write(&path, &(out.join("\n") + "\n"))
}

fn write(path: &Path, text: &str) -> Result<()> {
    crate::fsutil::write_atomic(path, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_only_set_entries_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = crate::instance::InstanceManager::new(dir.path());
        let inst = mgr.create("P", "1.21.1").unwrap();
        let g = inst.game_dir();
        std::fs::create_dir_all(&g).unwrap();
        std::fs::write(g.join("options.txt"), "version:3955\nfov:0.0\nkey_key.jump:key.keyboard.space\nkey_key.mymod.dash:key.keyboard.r\n").unwrap();

        let opts = parse(
            "fov:0.5\nkey_key.jump:key.keyboard.v\nrenderDistance:16\nversion:1\nlastServer:x\n",
        );
        assert!(!opts.contains_key("version") && !opts.contains_key("lastServer"));
        apply(&inst, &opts).unwrap();
        let text = std::fs::read_to_string(g.join("options.txt")).unwrap();
        assert_eq!(
            text,
            "version:3955\nfov:0.5\nkey_key.jump:key.keyboard.v\nkey_key.mymod.dash:key.keyboard.r\nrenderDistance:16\n"
        );

        // A new instance with no options file yet gets one with just these.
        let fresh = mgr.create("Q", "1.20.4").unwrap();
        apply(&fresh, &opts).unwrap();
        assert!(
            std::fs::read_to_string(fresh.game_dir().join("options.txt"))
                .unwrap()
                .contains("renderDistance:16")
        );

        // Old versions use numeric key codes: keybinds are left out, other options aren't.
        let old = mgr.create("Old", "1.12.2").unwrap();
        apply(&old, &opts).unwrap();
        let text = std::fs::read_to_string(old.game_dir().join("options.txt")).unwrap();
        assert!(text.contains("fov:0.5") && !text.contains("key_"));
        assert!(modern_keys("24w10a") && !modern_keys("b1.7.3") && modern_keys("1.13"));
    }
}
