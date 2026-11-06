//! How long the game took to reach the main menu, and which mods that time went to.
//!
//! Loaders don't log per-mod timings, so this is an estimate from the log itself: a pause
//! between two lines from the same mod is that mod working, and lines like "loaded in
//! 1200 ms" are taken at their word. Pauses between lines from different sources are left
//! unattributed rather than guessed.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::content::local::ModInfo;
use crate::instance::Instance;

/// One game log line, timed from when the game process started.
#[derive(Debug, Clone)]
pub struct Mark {
    pub ms: u64,
    pub logger: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlowMod {
    pub name: String,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartupReport {
    pub at: DateTime<Utc>,
    /// Process start to the main menu.
    pub total_ms: u64,
    /// Slowest first, only mods that took noticeable time.
    pub mods: Vec<SlowMod>,
}

/// Reports kept per instance, newest last.
const HISTORY: usize = 10;
/// Mods below this aren't worth listing.
const NOTICEABLE_MS: u64 = 400;

/// True for the line printed as the main menu is about to show.
pub fn is_ready(text: &str) -> bool {
    text.contains("Sound engine started") || text.contains("OpenAL initialized")
}

fn mod_of<'a>(m: &Mark, mods: &'a [ModInfo], paren: &Regex) -> Option<&'a ModInfo> {
    let logger = m
        .logger
        .clone()
        .or_else(|| paren.captures(&m.text).map(|c| c[1].to_owned()))?;
    if logger.contains('.')
        && let Some(owner) = ModInfo::owner(mods, &logger)
    {
        return Some(owner);
    }
    ModInfo::by_id(mods, &logger)
        .or_else(|| ModInfo::by_id(mods, logger.rsplit('.').next().unwrap_or_default()))
}

pub fn analyze(marks: &[Mark], mods: &[ModInfo]) -> Option<StartupReport> {
    let end = marks.iter().position(|m| is_ready(&m.text))?;
    let marks = &marks[..=end];
    // Fabric's plain format: "[12:00:00] [main/INFO] (sodium) message".
    let paren = Regex::new(r"^\[[^\]]*\] \[[^\]]*\] \(([^)]+)\)").expect("valid regex");
    let said = Regex::new(r"(?i)\b(?:took|in) (\d+(?:\.\d+)?) ?(ms|milliseconds|s|seconds)\b")
        .expect("valid regex");

    let owners: Vec<Option<&ModInfo>> = marks.iter().map(|m| mod_of(m, mods, &paren)).collect();
    let mut gaps: HashMap<&str, u64> = HashMap::new();
    let mut stated: HashMap<&str, u64> = HashMap::new();
    for (i, m) in marks.iter().enumerate() {
        let Some(owner) = owners[i] else { continue };
        if i > 0 && owners[i - 1].is_some_and(|p| p.file == owner.file) {
            *gaps.entry(&owner.name).or_default() += m.ms.saturating_sub(marks[i - 1].ms);
        }
        if let Some(c) = said.captures(&m.text) {
            let n: f64 = c[1].parse().unwrap_or(0.0);
            let ms = if c[2].starts_with('m') { n } else { n * 1000.0 };
            *stated.entry(&owner.name).or_default() += ms as u64;
        }
    }
    let mut slow: Vec<SlowMod> = gaps
        .keys()
        .chain(stated.keys())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .map(|name| SlowMod {
            name: (*name).to_owned(),
            ms: gaps
                .get(name)
                .copied()
                .unwrap_or(0)
                .max(stated.get(name).copied().unwrap_or(0)),
        })
        .filter(|s| s.ms >= NOTICEABLE_MS)
        .collect();
    slow.sort_by(|a, b| b.ms.cmp(&a.ms).then_with(|| a.name.cmp(&b.name)));
    slow.truncate(8);
    Some(StartupReport {
        at: Utc::now(),
        total_ms: marks[end].ms,
        mods: slow,
    })
}

fn file(instance: &Instance) -> std::path::PathBuf {
    instance.dir.join("startup.json")
}

pub fn history(instance: &Instance) -> Vec<StartupReport> {
    std::fs::read(file(instance))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn record(instance: &Instance, report: StartupReport) -> crate::Result<()> {
    let mut all = history(instance);
    all.push(report);
    let skip = all.len().saturating_sub(HISTORY);
    all.drain(..skip);
    crate::fsutil::write_json_atomic(&file(instance), &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_pauses_within_one_mod_and_stated_times() {
        let mods = vec![
            ModInfo {
                file: "mods/iris.jar".into(),
                ids: vec!["iris".into()],
                name: "Iris".into(),
                packages: vec!["net.irisshaders.iris".into()],
            },
            ModInfo {
                file: "mods/create.jar".into(),
                ids: vec!["create".into()],
                name: "Create".into(),
                packages: vec!["com.simibubi.create".into()],
            },
        ];
        let m = |ms, logger: Option<&str>, text: &str| Mark {
            ms,
            logger: logger.map(str::to_owned),
            text: text.into(),
        };
        let marks = [
            m(0, Some("net.minecraft.Main"), "starting"),
            m(1000, Some("net.irisshaders.iris.Iris"), "Loading shaders"),
            m(4000, Some("net.irisshaders.iris.Iris"), "Shaders ready"),
            // A pause between different sources isn't pinned on anyone.
            m(
                9000,
                None,
                "[12:00:00] [main/INFO] (create) Registered blocks in 1500 ms",
            ),
            m(9500, Some("net.minecraft.Sound"), "Sound engine started"),
            m(20000, Some("net.irisshaders.iris.Iris"), "after the menu"),
        ];
        let r = analyze(&marks, &mods).unwrap();
        assert_eq!(r.total_ms, 9500);
        assert_eq!(
            r.mods,
            [
                SlowMod {
                    name: "Iris".into(),
                    ms: 3000
                },
                SlowMod {
                    name: "Create".into(),
                    ms: 1500
                },
            ]
        );
        assert!(
            analyze(&marks[..3], &mods).is_none(),
            "never reached the menu"
        );
    }

    #[test]
    fn keeps_the_last_ten() {
        let dir = tempfile::tempdir().unwrap();
        let inst = crate::instance::InstanceManager::new(dir.path())
            .create("P", "1.21.1")
            .unwrap();
        for n in 0..12 {
            record(
                &inst,
                StartupReport {
                    at: Utc::now(),
                    total_ms: n,
                    mods: Vec::new(),
                },
            )
            .unwrap();
        }
        let h = history(&inst);
        assert_eq!((h.len(), h[0].total_ms, h[9].total_ms), (10, 2, 11));
    }
}
