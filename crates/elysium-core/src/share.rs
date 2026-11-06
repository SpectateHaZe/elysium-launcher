//! Shared storage (experimental): identical mod jars and pack zips across instances are
//! stored once, as hard links to the same data. Jars and zips are replaced, never edited
//! in place, so each instance still behaves as if it had its own copy; deleting one copy
//! leaves the others intact.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::Result;
use crate::progress::Progress;

/// Files smaller than this aren't worth the bookkeeping.
const MIN_SIZE: u64 = 16 * 1024;
const FOLDERS: &[&str] = &["mods", "resourcepacks", "shaderpacks"];

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ShareReport {
    /// Files turned into shared copies this run.
    pub linked: usize,
    /// Space all shared copies save, this run's and earlier ones.
    pub saved: u64,
}

fn shareable(name: &str) -> bool {
    let name = name.strip_suffix(".disabled").unwrap_or(name);
    name.ends_with(".jar") || name.ends_with(".zip")
}

fn candidates(game_dirs: &[PathBuf]) -> Vec<(u64, PathBuf)> {
    let mut out = Vec::new();
    for g in game_dirs {
        for f in FOLDERS {
            let Ok(read) = std::fs::read_dir(g.join(f)) else {
                continue;
            };
            for e in read.flatten() {
                let Ok(meta) = e.metadata() else { continue };
                if meta.is_file()
                    && meta.len() >= MIN_SIZE
                    && shareable(&e.file_name().to_string_lossy())
                {
                    out.push((meta.len(), e.path()));
                }
            }
        }
    }
    out
}

/// Replaces `dup` with a hard link to `keep`. The link is made under a temporary name and
/// renamed over the file, so `dup` is never missing; on any failure it's left as it was.
fn link(keep: &Path, dup: &Path) -> std::io::Result<()> {
    let tmp = dup.with_file_name(format!(
        "{}.elysium-share",
        dup.file_name().unwrap_or_default().to_string_lossy()
    ));
    let _ = std::fs::remove_file(&tmp);
    std::fs::hard_link(keep, &tmp)?;
    std::fs::rename(&tmp, dup).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Shares identical files across the given game folders. Only files of equal size are
/// hashed, so a run over many instances stays quick.
pub fn share(game_dirs: &[PathBuf], progress: &Progress) -> Result<ShareReport> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for (size, path) in candidates(game_dirs) {
        by_size.entry(size).or_default().push(path);
    }
    let groups: Vec<(u64, Vec<PathBuf>)> =
        by_size.into_iter().filter(|(_, v)| v.len() > 1).collect();
    progress.set_stage(
        "Finding identical files",
        groups.iter().map(|(_, v)| v.len() as u64).sum(),
    );
    let mut report = ShareReport::default();
    for (size, paths) in groups {
        let mut by_hash: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for p in paths {
            progress.check()?;
            progress.advance(1);
            match crate::fsutil::sha1_file(&p) {
                Ok(h) => by_hash.entry(h).or_default().push(p),
                Err(e) => tracing::warn!("not sharing {}: {e}", p.display()),
            }
        }
        for same in by_hash.into_values().filter(|v| v.len() > 1) {
            let keep = &same[0];
            for dup in &same[1..] {
                if same_file::is_same_file(keep, dup).unwrap_or(false) {
                    report.saved += size;
                    continue;
                }
                match link(keep, dup) {
                    Ok(()) => {
                        report.linked += 1;
                        report.saved += size;
                    }
                    // Different drives, a file in use, or NTFS's link limit: keep the copy.
                    Err(e) => tracing::info!("kept a separate copy of {}: {e}", dup.display()),
                }
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_identical_files_once() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        let jar = vec![7u8; MIN_SIZE as usize];
        for g in [&a, &b] {
            std::fs::create_dir_all(g.join("mods")).unwrap();
            std::fs::write(g.join("mods/sodium.jar"), &jar).unwrap();
            // Same size, different content: never linked.
            std::fs::write(
                g.join("mods/other.jar"),
                vec![g.ends_with("a") as u8; MIN_SIZE as usize],
            )
            .unwrap();
        }
        std::fs::create_dir_all(b.join("config")).unwrap();
        std::fs::write(b.join("config/x.jar"), &jar).unwrap();

        let dirs = [a.clone(), b.clone()];
        let r = share(&dirs, &Progress::new()).unwrap();
        assert_eq!(
            r,
            ShareReport {
                linked: 1,
                saved: MIN_SIZE
            }
        );
        assert!(
            same_file::is_same_file(a.join("mods/sodium.jar"), b.join("mods/sodium.jar")).unwrap()
        );
        assert!(
            !same_file::is_same_file(a.join("mods/other.jar"), b.join("mods/other.jar")).unwrap()
        );
        assert!(
            !same_file::is_same_file(a.join("mods/sodium.jar"), b.join("config/x.jar")).unwrap()
        );

        // A second run finds them already shared.
        assert_eq!(
            share(&dirs, &Progress::new()).unwrap(),
            ShareReport {
                linked: 0,
                saved: MIN_SIZE
            }
        );
        // Deleting one copy leaves the other.
        std::fs::remove_file(a.join("mods/sodium.jar")).unwrap();
        assert_eq!(std::fs::read(b.join("mods/sodium.jar")).unwrap(), jar);
    }
}
