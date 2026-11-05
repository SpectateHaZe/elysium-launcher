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
    #[serde(rename = "type")]
    pub kind: VersionType,
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
        self.rules.as_deref().is_none_or(|r| is_allowed(r, env))
    }

    /// The main jar that goes on the classpath, with its path relative to `libraries/`.
    pub fn artifact(&self) -> Option<(String, &Artifact)> {
        let artifact = self.downloads.as_ref()?.artifact.as_ref()?;
        let path = artifact.path.clone().or_else(|| maven_path(&self.name))?;
        Some((path, artifact))
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
