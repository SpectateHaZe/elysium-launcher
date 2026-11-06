//! Reads a crash log and explains, in plain words, what went wrong and how to fix it.

use regex::Regex;
use serde::Serialize;

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

#[cfg(test)]
mod tests {
    use super::*;

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
