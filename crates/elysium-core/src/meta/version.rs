use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::VersionType;
use crate::rules::{Environment, Rule, is_allowed};

/// A version JSON (`versions/<id>/<id>.json`). Covers both the modern `arguments`
/// format (1.13+) and the legacy `minecraftArguments` string.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    /// Loader profiles (Fabric, Forge, …) build on a vanilla version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    #[serde(rename = "type", default = "default_kind")]
    pub kind: VersionType,
    #[serde(default)]
    pub main_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<VersionDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_time: Option<DateTime<Utc>>,
    /// Id of the version whose client jar goes on the classpath (set by [`Self::merge`]).
    #[serde(skip)]
    pub jar_id: Option<String>,
}

fn default_kind() -> VersionType {
    VersionType::Release
}

impl VersionJson {
    /// Java major version required. Versions that predate the field ran on Java 8.
    pub fn java_major(&self) -> u32 {
        self.java_version.as_ref().map_or(8, |j| j.major_version)
    }

    /// Mojang Java runtime component, e.g. `jre-legacy` or `java-runtime-delta`.
    pub fn java_component(&self) -> &str {
        self.java_version
            .as_ref()
            .and_then(|j| j.component.as_deref())
            .unwrap_or("jre-legacy")
    }

    /// Combines a loader profile (`self`) with the version it inherits from, the way the
    /// official launcher does: the child's main class and arguments win, its libraries
    /// come first and replace the parent's copies of the same artifact.
    pub fn merge(self, parent: VersionJson) -> VersionJson {
        let jar_id = if self.downloads.is_some() {
            Some(self.id.clone())
        } else {
            parent.jar_id.clone().or_else(|| Some(parent.id.clone()))
        };
        let mut libraries = self.libraries;
        let keys: std::collections::HashSet<String> =
            libraries.iter().map(Library::dedupe_key).collect();
        libraries.extend(
            parent
                .libraries
                .into_iter()
                .filter(|l| !keys.contains(&l.dedupe_key())),
        );
        let arguments = match (self.arguments, parent.arguments) {
            (Some(c), Some(p)) => Some(Arguments {
                game: p.game.into_iter().chain(c.game).collect(),
                jvm: p.jvm.into_iter().chain(c.jvm).collect(),
            }),
            (c, p) => c.or(p),
        };
        VersionJson {
            id: self.id,
            inherits_from: None,
            kind: parent.kind,
            main_class: if self.main_class.is_empty() {
                parent.main_class
            } else {
                self.main_class
            },
            arguments,
            minecraft_arguments: self.minecraft_arguments.or(parent.minecraft_arguments),
            asset_index: self.asset_index.or(parent.asset_index),
            assets: self.assets.or(parent.assets),
            downloads: self.downloads.or(parent.downloads),
            java_version: self.java_version.or(parent.java_version),
            libraries,
            logging: self.logging.or(parent.logging),
            release_time: parent.release_time,
            jar_id,
        }
    }

    /// Id of the version providing the client jar.
    pub fn jar_version(&self) -> &str {
        self.jar_id.as_deref().unwrap_or(&self.id)
    }

    /// Asset index id (`assets` field, falling back to the index reference).
    pub fn assets_id(&self) -> &str {
        self.asset_index
            .as_ref()
            .map(|a| a.id.as_str())
            .or(self.assets.as_deref())
            .unwrap_or("legacy")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: ArgValue },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    One(String),
    Many(Vec<String>),
}

impl Argument {
    /// Pushes this argument's raw (unsubstituted) values if its rules allow it.
    pub fn collect_into(&self, env: &Environment, out: &mut Vec<String>) {
        match self {
            Self::Plain(s) => out.push(s.clone()),
            Self::Conditional { rules, value } => {
                if is_allowed(rules, env) {
                    match value {
                        ArgValue::One(s) => out.push(s.clone()),
                        ArgValue::Many(v) => out.extend(v.iter().cloned()),
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    #[serde(default)]
    pub total_size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    #[serde(default)]
    pub component: Option<String>,
    pub major_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default)]
    pub client: Option<LoggingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// e.g. `-Dlog4j.configurationFile=${path}`
    pub argument: String,
    pub file: LoggingFile,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    /// Maven coordinate `group:artifact:version[:classifier]`.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    /// Legacy natives: OS name -> classifier (may contain `${arch}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,
    /// Maven repository base for libraries without `downloads` (Fabric, Quilt, old Forge).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Old Forge profiles mark libraries the client needs; `false` means server only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clientreq: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Library {
    pub fn is_allowed(&self, env: &Environment) -> bool {
        self.clientreq != Some(false) && self.rules.as_deref().is_none_or(|r| is_allowed(r, env))
    }

    /// `group:artifact[:classifier]`, used to let a loader's library replace vanilla's.
    pub fn dedupe_key(&self) -> String {
        let mut parts = self.name.split('@').next().unwrap_or_default().split(':');
        let (g, a) = (
            parts.next().unwrap_or_default(),
            parts.next().unwrap_or_default(),
        );
        let classifier = parts.nth(1).unwrap_or_default();
        format!("{g}:{a}:{classifier}")
    }

    /// The main jar that goes on the classpath, with its path relative to `libraries/`.
    /// Libraries without a `downloads` block are resolved against their Maven repository.
    pub fn artifact(&self) -> Option<(String, Artifact)> {
        if let Some(downloads) = &self.downloads {
            let artifact = downloads.artifact.as_ref()?;
            let path = artifact.path.clone().or_else(|| maven_path(&self.name))?;
            return Some((path, artifact.clone()));
        }
        if self.natives.is_some() {
            return None;
        }
        let path = maven_path(&self.name)?;
        let base = self
            .url
            .as_deref()
            .filter(|u| !u.is_empty())
            .unwrap_or(MOJANG_LIBRARIES);
        let url = format!("{}/{path}", base.trim_end_matches('/'));
        Some((
            path.clone(),
            Artifact {
                path: Some(path),
                sha1: self.sha1.clone(),
                size: self.size,
                url,
            },
        ))
    }

    /// Legacy native jar for `env` (to be extracted, not put on the classpath).
    pub fn native_artifact(&self, env: &Environment) -> Option<(String, &Artifact)> {
        let classifier = self
            .natives
            .as_ref()?
            .get(env.os.as_str())?
            .replace("${arch}", env.arch.bits());
        let artifact = self
            .downloads
            .as_ref()?
            .classifiers
            .as_ref()?
            .get(&classifier)?;
        let path = artifact
            .path
            .clone()
            .or_else(|| maven_path(&format!("{}:{classifier}", self.name)))?;
        Some((path, artifact))
    }
}

/// Default Maven repository for libraries that name none.
pub const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

/// Converts a Maven coordinate (`group:artifact:version[:classifier][@ext]`) into a
/// repository-relative path.
pub fn maven_path(coord: &str) -> Option<String> {
    let (coord, ext) = coord.split_once('@').unwrap_or((coord, "jar"));
    let mut parts = coord.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next();
    if group.is_empty() || artifact.is_empty() || version.is_empty() || parts.next().is_some() {
        return None;
    }
    let file = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!(
        "{}/{artifact}/{version}/{file}",
        group.replace('.', "/")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{Arch, OsName};

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.2:natives-windows").as_deref(),
            Some("org/lwjgl/lwjgl/3.3.2/lwjgl-3.3.2-natives-windows.jar")
        );
        assert_eq!(
            maven_path("a.b:c:1.0@zip").as_deref(),
            Some("a/b/c/1.0/c-1.0.zip")
        );
        assert_eq!(maven_path("bad"), None);
    }

    #[test]
    fn maven_libraries_and_merge() {
        let child: VersionJson = serde_json::from_str(
            r#"{"id":"fabric-loader-0.16.9-1.21","inheritsFrom":"1.21","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
                "arguments":{"game":[],"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main "]},
                "libraries":[{"name":"org.ow2.asm:asm:9.7","url":"https://maven.fabricmc.net/","sha1":"abc","size":5},
                             {"name":"net.fabricmc:fabric-loader:0.16.9","url":"https://maven.fabricmc.net/"}]}"#,
        )
        .unwrap();
        let parent: VersionJson = serde_json::from_str(
            r#"{"id":"1.21","type":"release","mainClass":"net.minecraft.client.main.Main",
                "arguments":{"game":["--username","${auth_player_name}"],"jvm":["-cp","${classpath}"]},
                "downloads":{"client":{"url":"c","sha1":"s","size":1}},
                "libraries":[{"name":"org.ow2.asm:asm:9.3","downloads":{"artifact":{"path":"old.jar","url":"u"}}},
                             {"name":"com.mojang:brigadier:1.0"}]}"#,
        )
        .unwrap();
        let merged = child.merge(parent);
        assert_eq!(
            merged.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(merged.jar_version(), "1.21");
        let names: Vec<_> = merged.libraries.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "org.ow2.asm:asm:9.7",
                "net.fabricmc:fabric-loader:0.16.9",
                "com.mojang:brigadier:1.0"
            ]
        );
        assert_eq!(
            merged.arguments.unwrap().jvm.len(),
            3,
            "parent jvm args first, then the loader's"
        );
        let (path, art) = merged.libraries[0].artifact().unwrap();
        assert_eq!(path, "org/ow2/asm/asm/9.7/asm-9.7.jar");
        assert_eq!(
            art.url,
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.7/asm-9.7.jar"
        );
        assert_eq!(art.sha1.as_deref(), Some("abc"));
        let (_, mojang) = merged.libraries[2].artifact().unwrap();
        assert_eq!(
            mojang.url,
            "https://libraries.minecraft.net/com/mojang/brigadier/1.0/brigadier-1.0.jar"
        );
    }

    #[test]
    fn legacy_native_classifier_with_arch() {
        let lib: Library = serde_json::from_str(
            r#"{"name":"tv.twitch:twitch-platform:5.16",
                "natives":{"windows":"natives-windows-${arch}"},
                "downloads":{"classifiers":{"natives-windows-64":{"path":"p64.jar","sha1":"x","size":1,"url":"u"}}}}"#,
        )
        .unwrap();
        let env = Environment {
            os: OsName::Windows,
            os_version: String::new(),
            arch: Arch::X86_64,
            features: Default::default(),
        };
        assert_eq!(lib.native_artifact(&env).unwrap().0, "p64.jar");
        let linux = Environment {
            os: OsName::Linux,
            ..env
        };
        assert!(lib.native_artifact(&linux).is_none());
    }
}
