//! World backups: each world zipped into `<instance>/backups/<world>/<time>.zip`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, TimeZone};
use serde::Serialize;

use crate::Progress;
use crate::error::{Error, IoContext, Result};
use crate::instance::Instance;

#[derive(Debug, Clone, Serialize)]
pub struct Backup {
    pub world: String,
    pub file: String,
    pub created: DateTime<Local>,
    pub size: u64,
}

fn backups_dir(instance: &Instance) -> PathBuf {
    instance.dir.join("backups")
}

fn check_name(name: &str) -> Result<&str> {
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(Error::Invalid(format!("invalid name {name:?}")));
    }
    Ok(name)
}

pub fn worlds(instance: &Instance) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(instance.game_dir().join("saves"))
        .map(|r| {
            r.flatten()
                .filter(|e| e.path().join("level.dat").exists())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn zip_dir(src: &Path, out: &Path, progress: &Progress) -> Result<()> {
    if let Some(p) = out.parent() {
        std::fs::create_dir_all(p).at(p)?;
    }
    let tmp = out.with_extension("partial");
    let r = write_zip(src, &tmp, out, progress);
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r?;
    std::fs::rename(&tmp, out).at(out)
}

fn write_zip(src: &Path, tmp: &Path, out: &Path, progress: &Progress) -> Result<()> {
    let file = std::fs::File::create(tmp).at(tmp)?;
    let mut zip = zip::ZipWriter::new(file);
    let deflate = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    // Region files are already compressed chunk by chunk; deflating them again costs
    // most of the backup time and saves almost nothing.
    let store = deflate.compression_method(zip::CompressionMethod::Stored);
    let zerr = |e| Error::Zip {
        path: out.to_owned(),
        source: e,
    };
    let mut stack = vec![src.to_owned()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).at(&dir)?.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(src)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if path.is_dir() {
                stack.push(path);
            } else if rel != "session.lock" {
                progress.check()?;
                let opts = if rel.ends_with(".mca") {
                    store
                } else {
                    deflate
                };
                zip.start_file(rel, opts).map_err(zerr)?;
                let mut data = Vec::new();
                std::fs::File::open(&path)
                    .and_then(|mut f| f.read_to_end(&mut data))
                    .at(&path)?;
                zip.write_all(&data).at(out)?;
            }
        }
    }
    zip.finish().map_err(zerr)?;
    Ok(())
}

/// Backs up one world and keeps only the newest `keep` backups of it. Stops with
/// `Error::Cancelled` (leaving no partial file) when `progress` is cancelled.
pub fn backup_world(
    instance: &Instance,
    world: &str,
    keep: u32,
    progress: &Progress,
) -> Result<Backup> {
    let world = check_name(world)?;
    let src = instance.game_dir().join("saves").join(world);
    if !src.join("level.dat").exists() {
        return Err(Error::Invalid(format!("{world} isn't a world")));
    }
    let now = Local::now();
    let stamp = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    // Two backups in the same second (e.g. the safety copy a restore makes) get a suffix.
    let mut out = backups_dir(instance)
        .join(world)
        .join(format!("{stamp}.zip"));
    let mut n = 1;
    while out.exists() {
        n += 1;
        out = backups_dir(instance)
            .join(world)
            .join(format!("{stamp}_{n}.zip"));
    }
    zip_dir(&src, &out, progress)?;
    prune(instance, world, keep)?;
    Ok(Backup {
        world: world.to_owned(),
        file: out
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        created: now,
        size: std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0),
    })
}

fn prune(instance: &Instance, world: &str, keep: u32) -> Result<()> {
    let mut all: Vec<Backup> = list(instance)?
        .into_iter()
        .filter(|b| b.world == world)
        .collect();
    all.sort_by(|a, b| b.created.cmp(&a.created).then_with(|| b.file.cmp(&a.file)));
    for old in all.into_iter().skip(keep.max(1) as usize) {
        let path = backups_dir(instance).join(&old.world).join(&old.file);
        std::fs::remove_file(&path).at(&path)?;
    }
    Ok(())
}

/// Every backup of every world, newest first.
pub fn list(instance: &Instance) -> Result<Vec<Backup>> {
    let mut out = Vec::new();
    let Ok(worlds) = std::fs::read_dir(backups_dir(instance)) else {
        return Ok(out);
    };
    for w in worlds.flatten().filter(|e| e.path().is_dir()) {
        let world = w.file_name().to_string_lossy().into_owned();
        for f in std::fs::read_dir(w.path()).at(w.path())?.flatten() {
            let file = f.file_name().to_string_lossy().into_owned();
            let Some(stem) = file.strip_suffix(".zip") else {
                continue;
            };
            let stamp = stem.get(..19).unwrap_or(stem);
            let Ok(naive) = chrono::NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d_%H-%M-%S")
            else {
                continue;
            };
            let Some(created) = Local.from_local_datetime(&naive).single() else {
                continue;
            };
            out.push(Backup {
                world: world.clone(),
                file,
                created,
                size: f.metadata().map(|m| m.len()).unwrap_or(0),
            });
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.created));
    Ok(out)
}

/// Backs up every world whose newest backup is older than a day (or missing).
pub fn auto_backup(instance: &Instance, keep: u32, progress: &Progress) -> Result<Vec<Backup>> {
    let existing = list(instance)?;
    let due: Vec<String> = worlds(instance)
        .into_iter()
        .filter(|world| {
            !existing
                .iter()
                .find(|b| &b.world == world)
                .is_some_and(|b| Local::now() - b.created < chrono::Duration::hours(24))
        })
        .collect();
    let mut made = Vec::new();
    for (n, world) in due.iter().enumerate() {
        progress.set_stage(
            format!("Backing up {world} ({} of {})", n + 1, due.len()),
            0,
        );
        made.push(backup_world(instance, world, keep, progress)?);
    }
    Ok(made)
}

/// Restores a backup. The world as it is now is backed up first, so nothing is lost.
pub fn restore(instance: &Instance, world: &str, file: &str, keep: u32) -> Result<()> {
    let (world, file) = (check_name(world)?, check_name(file)?);
    let zip_path = backups_dir(instance).join(world).join(file);
    if !zip_path.is_file() {
        return Err(Error::Invalid(format!("backup {file} not found")));
    }
    let dest = instance.game_dir().join("saves").join(world);
    if dest.join("level.dat").exists() {
        backup_world(instance, world, keep.saturating_add(1), &Progress::new())?;
        std::fs::remove_dir_all(&dest).at(&dest)?;
    }
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(&zip_path).at(&zip_path)?).map_err(|source| {
            Error::Zip {
                path: zip_path.clone(),
                source,
            }
        })?;
    archive.extract(&dest).map_err(|source| Error::Zip {
        path: zip_path,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::InstanceManager;

    #[test]
    fn backup_prune_restore() {
        let dir = tempfile::tempdir().unwrap();
        let inst = InstanceManager::new(dir.path())
            .create("Survival", "1.21.1")
            .unwrap();
        let world = inst.game_dir().join("saves/My World");
        std::fs::create_dir_all(world.join("region")).unwrap();
        std::fs::write(world.join("level.dat"), b"v1").unwrap();
        std::fs::write(world.join("region/r.0.0.mca"), b"chunks").unwrap();
        std::fs::write(world.join("session.lock"), b"lock").unwrap();

        let cancelled = Progress::new();
        cancelled.cancel();
        assert!(matches!(
            auto_backup(&inst, 5, &cancelled),
            Err(Error::Cancelled)
        ));
        assert!(list(&inst).unwrap().is_empty(), "no partial backup left");
        let p = Progress::new();
        assert_eq!(auto_backup(&inst, 5, &p).unwrap().len(), 1);
        assert!(
            auto_backup(&inst, 5, &p).unwrap().is_empty(),
            "only once a day"
        );
        let first = list(&inst).unwrap().remove(0);

        std::fs::write(world.join("level.dat"), b"v2").unwrap();
        restore(&inst, "My World", &first.file, 5).unwrap();
        assert_eq!(std::fs::read(world.join("level.dat")).unwrap(), b"v1");
        assert_eq!(
            std::fs::read(world.join("region/r.0.0.mca")).unwrap(),
            b"chunks"
        );
        assert!(!world.join("session.lock").exists());
        assert!(restore(&inst, "../x", "a.zip", 5).is_err());
    }
}
