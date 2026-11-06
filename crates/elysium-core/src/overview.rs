//! At-a-glance facts about an instance for the home screen: mods, worlds, size on disk
//! and the latest screenshots.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use crate::instance::Instance;

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    /// Enabled mods (`mods/*.jar`).
    pub mods: usize,
    pub worlds: usize,
    /// Bytes in the game folder, worlds included.
    pub size: u64,
    /// Process start to main menu, last time it was measured.
    pub startup_ms: Option<u64>,
    /// Newest first, at most [`SCREENSHOTS`].
    pub screenshots: Vec<Screenshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Screenshot {
    pub path: PathBuf,
    /// Milliseconds since the Unix epoch.
    pub taken: u64,
}

pub const SCREENSHOTS: usize = 12;

pub fn overview(instance: &Instance) -> Overview {
    let game = instance.game_dir();
    let mods = std::fs::read_dir(game.join("mods"))
        .map(|r| {
            r.flatten()
                .filter(|e| has_ext(&e.path(), &["jar"]) && e.path().is_file())
                .count()
        })
        .unwrap_or(0);
    Overview {
        mods,
        worlds: crate::backup::worlds(instance).len(),
        size: dir_size(&game),
        startup_ms: crate::startup::history(instance).last().map(|r| r.total_ms),
        screenshots: screenshots(&game.join("screenshots")),
    }
}

fn has_ext(p: &Path, exts: &[&str]) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn millis(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn screenshots(dir: &Path) -> Vec<Screenshot> {
    let mut shots: Vec<Screenshot> = std::fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .filter(|e| has_ext(&e.path(), &["png", "jpg", "jpeg"]))
                .filter_map(|e| {
                    let meta = e.metadata().ok().filter(|m| m.is_file())?;
                    Some(Screenshot {
                        path: e.path(),
                        taken: millis(meta.modified().ok()?),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    shots.sort_by_key(|s| std::cmp::Reverse(s.taken));
    shots.truncate(SCREENSHOTS);
    shots
}

/// Total size of the files under `dir`. Symlinks aren't followed, so a link can't make
/// the walk loop or count another folder.
fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                total += e.metadata().map_or(0, |m| m.len());
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::InstanceManager;

    #[test]
    fn counts_mods_worlds_size_and_screenshots() {
        let dir = tempfile::tempdir().unwrap();
        let inst = InstanceManager::new(dir.path())
            .create("Pack", "1.21.1")
            .unwrap();
        let g = inst.game_dir();
        for p in [
            "mods",
            "saves/A",
            "saves/B",
            "saves/not-a-world",
            "screenshots",
        ] {
            std::fs::create_dir_all(g.join(p)).unwrap();
        }
        std::fs::write(g.join("mods/a.jar"), b"12345").unwrap();
        std::fs::write(g.join("mods/b.jar.disabled"), b"1").unwrap();
        std::fs::write(g.join("saves/A/level.dat"), b"1").unwrap();
        std::fs::write(g.join("saves/B/level.dat"), b"1").unwrap();
        for n in 0..(SCREENSHOTS + 2) {
            std::fs::write(g.join(format!("screenshots/{n}.png")), b"").unwrap();
        }
        std::fs::write(g.join("screenshots/notes.txt"), b"").unwrap();

        let o = overview(&inst);
        assert_eq!((o.mods, o.worlds), (1, 2));
        assert_eq!(o.size, 5 + 1 + 1 + 1);
        assert_eq!(o.screenshots.len(), SCREENSHOTS);
        assert!(o.screenshots.windows(2).all(|w| w[0].taken >= w[1].taken));
    }
}
