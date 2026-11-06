//! Safe updates: before mods change, the instance's mods, configs and content list are
//! saved, so a pack that breaks after an update can go back with one click. Mods are
//! hard-linked (they're never edited in place), so a snapshot costs only the configs.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{IoContext, Result};
use crate::instance::Instance;

/// Folders of the game directory a snapshot covers.
const FOLDERS: &[&str] = &["mods", "config"];
const INDEX_FILE: &str = "content.json";
const KEEP: usize = 3;
/// Several updates in a row share the snapshot taken before the first.
const REUSE_MINUTES: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Folder name, also its id.
    pub id: String,
    pub at: DateTime<Utc>,
    /// What was about to change, e.g. "Updating Sodium".
    pub label: String,
    /// Set once the game started fine after the change; no undo is offered then.
    pub verified: bool,
}

fn root(instance: &Instance) -> PathBuf {
    instance.dir.join("snapshots")
}

pub fn list(instance: &Instance) -> Vec<Snapshot> {
    let mut out: Vec<Snapshot> = std::fs::read_dir(root(instance))
        .map(|r| {
            r.flatten()
                .filter_map(|e| {
                    let b = std::fs::read(e.path().join("snapshot.json")).ok()?;
                    serde_json::from_slice(&b).ok()
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|s| s.at);
    out
}

/// The newest snapshot the game hasn't started fine since: the one to offer to undo.
pub fn pending(instance: &Instance) -> Option<Snapshot> {
    list(instance).pop().filter(|s| !s.verified)
}

/// Saves the current mods and configs before a change.
pub fn take(instance: &Instance, label: &str) -> Result<Snapshot> {
    if let Some(p) = pending(instance)
        && (Utc::now() - p.at).num_minutes() < REUSE_MINUTES
    {
        return Ok(p);
    }
    let at = Utc::now();
    let id = at.format("%Y%m%d-%H%M%S%3f").to_string();
    let dir = root(instance).join(&id);
    let game = instance.game_dir();
    let result = (|| -> Result<_> {
        for f in FOLDERS {
            let src = game.join(f);
            if src.is_dir() {
                crate::import::copy_tree(&src, &dir.join(f))?;
            }
        }
        let index = instance.dir.join(INDEX_FILE);
        if index.is_file() {
            std::fs::copy(&index, dir.join(INDEX_FILE)).at(&index)?;
        }
        let snap = Snapshot {
            id: id.clone(),
            at,
            label: label.to_owned(),
            verified: false,
        };
        crate::fsutil::write_json_atomic(&dir.join("snapshot.json"), &snap)?;
        Ok(snap)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    let snap = result?;
    let all = list(instance);
    for old in &all[..all.len().saturating_sub(KEEP)] {
        let _ = std::fs::remove_dir_all(root(instance).join(&old.id));
    }
    Ok(snap)
}

/// Marks every snapshot as fine: the game started after the change.
pub fn verify(instance: &Instance) -> Result<()> {
    for mut s in list(instance).into_iter().filter(|s| !s.verified) {
        s.verified = true;
        crate::fsutil::write_json_atomic(&root(instance).join(&s.id).join("snapshot.json"), &s)?;
    }
    Ok(())
}

/// Puts the instance back the way it was when the snapshot was taken. The current state
/// is moved aside first and put back if anything fails. The snapshot and any newer ones
/// are used up.
pub fn restore(instance: &Instance, id: &str) -> Result<()> {
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return Err(crate::Error::Invalid("not a snapshot".into()));
    }
    let dir = root(instance).join(id);
    if !dir.join("snapshot.json").is_file() {
        return Err(crate::Error::Invalid("that snapshot is gone".into()));
    }
    let game = instance.game_dir();
    let aside = instance.dir.join(format!("restore-{id}"));
    let _ = std::fs::remove_dir_all(&aside);
    std::fs::create_dir_all(&aside).at(&aside)?;

    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    let put_back = |moved: &[(PathBuf, PathBuf)]| {
        for (from, to) in moved.iter().rev() {
            let _ = std::fs::remove_dir_all(from);
            let _ = std::fs::remove_file(from);
            let _ = std::fs::rename(to, from);
        }
    };
    let result = (|| -> Result<_> {
        let mut current: Vec<PathBuf> = FOLDERS.iter().map(|f| game.join(f)).collect();
        current.push(instance.dir.join(INDEX_FILE));
        for path in current.iter().filter(|p| p.exists()) {
            let to = aside.join(path.file_name().unwrap_or_default());
            std::fs::rename(path, &to).at(path)?;
            moved.push((path.clone(), to));
        }
        for f in FOLDERS {
            let src = dir.join(f);
            if src.is_dir() {
                crate::import::copy_tree(&src, &game.join(f))?;
            }
        }
        let index = dir.join(INDEX_FILE);
        if index.is_file() {
            std::fs::copy(&index, instance.dir.join(INDEX_FILE)).at(&index)?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        put_back(&moved);
        let _ = std::fs::remove_dir_all(&aside);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&aside);
    let at = list(instance)
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.at);
    for s in list(instance) {
        if at.is_some_and(|t| s.at >= t) {
            let _ = std::fs::remove_dir_all(root(instance).join(&s.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_verify_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let inst = crate::instance::InstanceManager::new(dir.path())
            .create("P", "1.21.1")
            .unwrap();
        let g = inst.game_dir();
        std::fs::create_dir_all(g.join("mods")).unwrap();
        std::fs::create_dir_all(g.join("config")).unwrap();
        std::fs::write(g.join("mods/a-1.jar"), b"old").unwrap();
        std::fs::write(g.join("config/a.toml"), b"x=1").unwrap();
        std::fs::write(inst.dir.join(INDEX_FILE), b"{}").unwrap();

        let s = take(&inst, "Updating A").unwrap();
        assert_eq!(pending(&inst).unwrap().id, s.id);
        // A second update right after reuses the same snapshot.
        assert_eq!(take(&inst, "Updating B").unwrap().id, s.id);

        // The update: new jar, changed config.
        std::fs::remove_file(g.join("mods/a-1.jar")).unwrap();
        std::fs::write(g.join("mods/a-2.jar"), b"new").unwrap();
        std::fs::write(g.join("config/a.toml"), b"x=2").unwrap();

        restore(&inst, &s.id).unwrap();
        assert!(g.join("mods/a-1.jar").is_file());
        assert!(!g.join("mods/a-2.jar").exists());
        assert_eq!(std::fs::read(g.join("config/a.toml")).unwrap(), b"x=1");
        assert!(list(&inst).is_empty(), "used up");

        take(&inst, "Again").unwrap();
        verify(&inst).unwrap();
        assert!(pending(&inst).is_none());
        assert!(restore(&inst, "../x").is_err());
    }
}
