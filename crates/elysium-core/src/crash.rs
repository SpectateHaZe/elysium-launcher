//! Reads a crash log and explains, in plain words, what went wrong and how to fix it.

use regex::Regex;
use serde::Serialize;

use crate::content::local::ModInfo;
use crate::instance::Instance;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fix {
    /// Raise the memory limit.
    MoreMemory,
    /// Install a missing mod (Modrinth id or slug when known).
    InstallMod { id: String, name: String },
    /// Update or remove the named mod.
    CheckMod { name: String },
    /// Use the Java the version expects (turn automatic Java back on).
    AutomaticJava { major: Option<u32> },
    /// Update the graphics driver.
    UpdateDrivers,
    /// Remove one of two copies of a mod.
    RemoveDuplicate,
    /// Turn the named mod off (`file` is `mods/<name>.jar`).
    DisableMod { file: String, name: String },
    /// Go back to the mods and configs saved before an update.
    UndoUpdate { snapshot: String, label: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnosis {
    pub title: String,
    pub detail: String,
    pub fixes: Vec<Fix>,
}

fn d(title: &str, detail: impl Into<String>, fixes: Vec<Fix>) -> Option<Diagnosis> {
    Some(Diagnosis {
        title: title.into(),
        detail: detail.into(),
        fixes,
    })
}

/// The most likely explanation for a crash, from the game's output.
pub fn diagnose<S: AsRef<str>>(lines: &[S]) -> Option<Diagnosis> {
    let text: Vec<&str> = lines.iter().map(AsRef::as_ref).collect();
    let has = |needle: &str| text.iter().any(|l| l.contains(needle));

    // Missing dependencies (Fabric/Quilt). Newer loaders print "- Install <id>, ...".
    let install =
        Regex::new(r"- Install ([A-Za-z0-9_\-]+), (?:any version|version)").expect("valid regex");
    let requires = Regex::new(r"requires (?:any version|version [^ ]+(?: or later)?) of (?:mod )?'([^']+)' \(([A-Za-z0-9_\-]+)\), which is missing").expect("valid regex");
    let mut missing: Vec<Fix> = Vec::new();
    for l in &text {
        if let Some(c) = requires.captures(l) {
            missing.push(Fix::InstallMod {
                id: c[2].to_owned(),
                name: c[1].to_owned(),
            });
        } else if let Some(c) = install.captures(l) {
            missing.push(Fix::InstallMod {
                id: c[1].to_owned(),
                name: c[1].to_owned(),
            });
        }
    }
    // Forge/NeoForge list mandatory dependencies with "[MISSING]".
    let forge = Regex::new(r"Mod ID: '([A-Za-z0-9_\-]+)'.*Actual version: '\[MISSING\]'")
        .expect("valid regex");
    for l in &text {
        if let Some(c) = forge.captures(l) {
            missing.push(Fix::InstallMod {
                id: c[1].to_owned(),
                name: c[1].to_owned(),
            });
        }
    }
    missing.dedup();
    if !missing.is_empty() {
        let names: Vec<String> = missing
            .iter()
            .filter_map(|f| match f {
                Fix::InstallMod { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        return d(
            "A mod needs another mod that isn't installed",
            format!("Install {} and play again.", names.join(", ")),
            missing,
        );
    }

    if has("java.lang.OutOfMemoryError") || has("Could not reserve enough space for") {
        return d(
            "Minecraft ran out of memory",
            "Give the game more memory in this instance's settings. Large modpacks usually need 6 to 8 GB.",
            vec![Fix::MoreMemory],
        );
    }

    let class_version = Regex::new(r"class file version (\d+)").expect("valid regex");
    if has("UnsupportedClassVersionError")
        || has("has been compiled by a more recent version of the Java Runtime")
    {
        let major = text
            .iter()
            .find_map(|l| class_version.captures(l))
            .and_then(|c| c[1].parse::<u32>().ok())
            .map(|v| v.saturating_sub(44));
        return d(
            "The game needs a newer Java",
            match major {
                Some(m) => format!(
                    "Something here needs Java {m}. Turn automatic Java back on, or point the instance at Java {m}."
                ),
                None => "Turn automatic Java back on so Elysium picks the right Java.".into(),
            },
            vec![Fix::AutomaticJava { major }],
        );
    }

    if has("DuplicateModsFoundException")
        || has("Found duplicate mods")
        || has("Duplicate mods found")
        || has("Mod ID '") && has("has multiple")
    {
        return d(
            "The same mod is installed twice",
            "Remove one of the copies from the instance's mods.",
            vec![Fix::RemoveDuplicate],
        );
    }

    let mixin_mod = Regex::new(r"(?:from mod|mixin config|in config) \[?([A-Za-z0-9_\-]+)")
        .expect("valid regex");
    if has("Mixin apply failed") || has("MixinApplyError") || has("MixinTransformerError") {
        let name = text
            .iter()
            .find_map(|l| mixin_mod.captures(l))
            .map(|c| c[1].trim_end_matches(".mixins").to_owned());
        return d(
            "A mod couldn't load into the game",
            match &name {
                Some(n) => format!("{n} failed to patch Minecraft. It may not support this Minecraft or loader version, or it clashes with another mod. Update or remove it."),
                None => "A mod failed to patch Minecraft. Update your mods, or remove the most recently added one.".into(),
            },
            name.map(|name| vec![Fix::CheckMod { name }]).unwrap_or_default(),
        );
    }

    if has("Incompatible mods found") || has("is incompatible with") {
        return d(
            "Two mods don't work together",
            "The log names the pair. Remove one of them.",
            Vec::new(),
        );
    }

    if has("GLFW error 65542")
        || has("Pixel format not accelerated")
        || has("does not appear to support OpenGL")
        || has("No OpenGL context")
    {
        return d(
            "Your graphics driver couldn't start the game",
            "Update your graphics driver from NVIDIA, AMD or Intel. On laptops, make sure Minecraft uses the dedicated GPU.",
            vec![Fix::UpdateDrivers],
        );
    }
    None
}

/// Everything Elysium can say about a crash: known causes first, then the mod the
/// crash happened in, using the game's crash report when it wrote one.
pub fn explain(instance: &Instance, mut lines: Vec<String>) -> Option<Diagnosis> {
    let report = crash_report(instance, &lines);
    let known = diagnose(&lines).or_else(|| report.as_deref().and_then(diagnose));
    let found = known.or_else(|| {
        lines.extend(report.unwrap_or_default());
        blame(&lines, &crate::content::local::scan_mods(instance))
    });
    match crate::snapshot::pending(instance) {
        None => found,
        // Crashing right after an update: going back is the surest fix.
        Some(s) => {
            let undo = Fix::UndoUpdate {
                snapshot: s.id,
                label: s.label.clone(),
            };
            Some(match found {
                Some(mut d) => {
                    d.fixes.insert(0, undo);
                    d
                }
                None => Diagnosis {
                    title: "The game crashed after an update".into(),
                    detail: format!(
                        "Elysium saved your mods and settings before \"{}\". Go back to them and play again.",
                        s.label
                    ),
                    fixes: vec![undo],
                },
            })
        }
    }
}

/// The crash report the game just wrote: the file the log names, or else one written in
/// the last two minutes.
fn crash_report(instance: &Instance, lines: &[String]) -> Option<Vec<String>> {
    let dir = instance.game_dir().join("crash-reports");
    let named = lines.iter().rev().find_map(|l| {
        let (_, rest) = l.split_once("Crash report saved to:")?;
        let p = rest.replace("#@!@#", "").replace("#@?@#", "");
        let name = std::path::Path::new(p.trim()).file_name()?.to_owned();
        Some(dir.join(name))
    });
    let path = named.filter(|p| p.is_file()).or_else(|| {
        std::fs::read_dir(&dir)
            .ok()?
            .flatten()
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .filter(|(t, _)| t.elapsed().is_ok_and(|a| a.as_secs() < 120))
            .max_by_key(|(t, _)| *t)
            .map(|(_, p)| p)
    })?;
    let text = std::fs::read_to_string(path).ok()?;
    Some(text.lines().map(str::to_owned).collect())
}

/// Names the mod a crash happened in, from the crash's stack trace, when [`diagnose`]
/// found no known cause. Evidence, strongest first: the loader's own "Suspected Mods"
/// line, mod ids NeoForge prints on each frame, mod ids Mixin bakes into injected method
/// names, and finally whose package each frame's class is in.
pub fn blame<S: AsRef<str>>(lines: &[S], mods: &[ModInfo]) -> Option<Diagnosis> {
    let text: Vec<&str> = lines.iter().map(AsRef::as_ref).collect();
    // Only the last crash counts; earlier caught exceptions in the log are noise.
    let start = text
        .iter()
        .rposition(|l| l.contains("Minecraft Crash Report"))
        .or_else(|| text.len().checked_sub(400))
        .unwrap_or(0);
    let text = &text[start..];

    let suspected = Regex::new(r"Suspected Mods?: (.+)").expect("valid regex");
    let paren_id = Regex::new(r"\(([A-Za-z0-9_\-]+)\)").expect("valid regex");
    let frame =
        Regex::new(r"^\s*at (?:(\S+)/)?([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)+)\.[\w$<>]+\(")
            .expect("valid regex");
    let module_id = Regex::new(r"(?:^|/)([a-z][a-z0-9_]*)@[^/]+$").expect("valid regex");
    let mixin_id = Regex::new(
        r"(?:handler|redirect|modify[A-Za-z]*|wrap[A-Za-z]*|localvar|constant|inject)\$[a-zA-Z0-9]{3,}\$([a-z][a-z0-9_]*)\$",
    )
    .expect("valid regex");
    const NOT_MODS: &[&str] = &[
        "minecraft",
        "neoforge",
        "forge",
        "fml",
        "fabricloader",
        "java",
        "mixin",
        "mixinextras",
    ];

    let mut culprit: Option<&ModInfo> = None;
    for l in text {
        if let Some(c) = suspected.captures(l)
            && !c[1].trim().eq_ignore_ascii_case("none")
        {
            culprit = paren_id
                .captures_iter(&c[1])
                .find_map(|id| ModInfo::by_id(mods, &id[1]))
                .or_else(|| {
                    ModInfo::by_id(mods, c[1].split(',').next().unwrap_or_default().trim())
                });
            if culprit.is_some() {
                break;
            }
        }
    }
    if culprit.is_none() {
        // The root cause is the last "Caused by:" section; look there first.
        let cause = text
            .iter()
            .rposition(|l| l.trim_start().starts_with("Caused by:"))
            .unwrap_or(0);
        let ordered = text[cause..].iter().chain(text[..cause].iter());
        let mut by_package = None;
        for l in ordered {
            let Some(f) = frame.captures(l) else { continue };
            let by_id = f
                .get(1)
                .and_then(|m| module_id.captures(m.as_str()))
                .map(|c| c[1].to_owned())
                .or_else(|| mixin_id.captures(l).map(|c| c[1].to_owned()))
                .filter(|id| !NOT_MODS.contains(&id.as_str()))
                .and_then(|id| ModInfo::by_id(mods, &id));
            if let Some(m) = by_id {
                culprit = Some(m);
                break;
            }
            if by_package.is_none() {
                by_package = ModInfo::owner(mods, &f[2]);
            }
        }
        culprit = culprit.or(by_package);
    }
    let m = culprit?;
    d(
        &format!("{} probably caused the crash", m.name),
        format!(
            "The crash happened inside {}. Update it, or turn it off and play again to check. Other mods that need it may stop working while it's off.",
            m.name
        ),
        vec![
            Fix::DisableMod {
                file: m.file.clone(),
                name: m.name.clone(),
            },
            Fix::CheckMod {
                name: m.name.clone(),
            },
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods() -> Vec<ModInfo> {
        let m = |id: &str, name: &str, pkg: &str| ModInfo {
            file: format!("mods/{id}.jar"),
            ids: vec![id.to_owned()],
            name: name.to_owned(),
            packages: vec![pkg.to_owned()],
        };
        vec![
            m("iris", "Iris", "net.irisshaders.iris"),
            m("sodium", "Sodium", "net.caffeinemc.mods"),
            m("create", "Create", "com.simibubi.create"),
        ]
    }

    fn blamed(log: &[&str]) -> Option<String> {
        blame(log, &mods()).map(|d| match &d.fixes[0] {
            Fix::DisableMod { name, .. } => name.clone(),
            f => panic!("unexpected {f:?}"),
        })
    }

    #[test]
    fn offers_undo_after_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let inst = crate::instance::InstanceManager::new(dir.path())
            .create("P", "1.21.1")
            .unwrap();
        assert!(explain(&inst, vec!["boom".into()]).is_none());
        let s = crate::snapshot::take(&inst, "Updating Sodium").unwrap();
        let d = explain(&inst, vec!["java.lang.OutOfMemoryError".into()]).unwrap();
        assert_eq!(
            d.fixes,
            [
                Fix::UndoUpdate {
                    snapshot: s.id,
                    label: "Updating Sodium".into()
                },
                Fix::MoreMemory
            ]
        );
    }

    #[test]
    fn blames_the_mod_in_the_trace() {
        // Plain frames: the deepest cause wins over the outer exception.
        let log = [
            "java.lang.RuntimeException: boom",
            "	at net.minecraft.client.Minecraft.run(Minecraft.java:10)",
            "	at knot//net.caffeinemc.mods.sodium.Render.draw(Render.java:5)",
            "Caused by: java.lang.NullPointerException",
            "	at knot//com.simibubi.create.content.Foo.tick(Foo.java:3)",
        ];
        assert_eq!(blamed(&log).as_deref(), Some("Create"));
        // NeoForge names the mod on each frame; Mixin names it in injected methods.
        let neo =
            ["	at TRANSFORMER/iris@1.8.0/net.minecraft.client.Renderer.render(Renderer.java:1)"];
        assert_eq!(blamed(&neo).as_deref(), Some("Iris"));
        let mixin = ["	at net.minecraft.world.Level.handler$zbc000$sodium$onTick(Level.java:4)"];
        assert_eq!(blamed(&mixin).as_deref(), Some("Sodium"));
        // Forge's own verdict comes first.
        let forge = [
            "Suspected Mods: Iris (iris)",
            "	at com.simibubi.create.X.y(X.java:1)",
        ];
        assert_eq!(blamed(&forge).as_deref(), Some("Iris"));
        // Nothing to go on: no blame.
        assert_eq!(
            blamed(&["	at net.minecraft.client.Main.main(Main.java:1)"]),
            None
        );
        // Only the last crash report counts.
        let two = [
            "	at com.simibubi.create.X.y(X.java:1)",
            "---- Minecraft Crash Report ----",
            "	at net.irisshaders.iris.Iris.z(Iris.java:1)",
        ];
        assert_eq!(blamed(&two).as_deref(), Some("Iris"));
    }

    #[test]
    fn fabric_missing_dependency() {
        let log = [
            "Incompatible mods found!",
            " - Mod 'Sodium' (sodium) 0.5.8 requires version 0.15.0 or later of mod 'Fabric API' (fabric-api), which is missing!",
        ];
        let d = diagnose(&log).unwrap();
        assert_eq!(
            d.fixes,
            [Fix::InstallMod {
                id: "fabric-api".into(),
                name: "Fabric API".into()
            }]
        );
        let newer = [
            "A potential solution has been determined:",
            "\t - Install fabric-api, version 0.92.0 or later.",
        ];
        assert!(
            matches!(&diagnose(&newer).unwrap().fixes[0], Fix::InstallMod { id, .. } if id == "fabric-api")
        );
    }

    #[test]
    fn forge_missing_dependency() {
        let log = [
            "Missing or unsupported mandatory dependencies:",
            "\tMod ID: 'architectury', Requested by: 'rei', Expected range: '[9.1,)', Actual version: '[MISSING]'",
        ];
        assert!(
            matches!(&diagnose(&log).unwrap().fixes[0], Fix::InstallMod { id, .. } if id == "architectury")
        );
    }

    #[test]
    fn memory_java_drivers_mixins() {
        assert_eq!(
            diagnose(&["java.lang.OutOfMemoryError: Java heap space"])
                .unwrap()
                .fixes,
            [Fix::MoreMemory]
        );
        let java = [
            "java.lang.UnsupportedClassVersionError: x has been compiled by a more recent version of the Java Runtime (class file version 65.0)",
        ];
        assert_eq!(
            diagnose(&java).unwrap().fixes,
            [Fix::AutomaticJava { major: Some(21) }]
        );
        assert_eq!(
            diagnose(&["GLFW error 65542: WGL: The driver does not appear to support OpenGL"])
                .unwrap()
                .fixes,
            [Fix::UpdateDrivers]
        );
        let mixin = [
            "Mixin apply failed sodium.mixins.json:core.MixinFoo -> net.minecraft.Bar",
            "from mod sodium",
        ];
        assert_eq!(
            diagnose(&mixin).unwrap().fixes,
            [Fix::CheckMod {
                name: "sodium".into()
            }]
        );
        assert!(diagnose(&["[main/INFO]: everything is fine"]).is_none());
    }
}
