//! Java runtime management: Mojang's official runtimes plus user-provided installs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, IoContext, Result};
use crate::http::{DownloadTask, Http, Verify};
use crate::paths::Paths;
use crate::progress::Progress;
use crate::rules::{Arch, OsName};

const RUNTIME_INDEX_URL: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const MARKER_FILE: &str = ".elysium-runtime.json";

type RuntimeIndex = HashMap<String, HashMap<String, Vec<RuntimeEntry>>>;

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
struct ManifestRef {
    sha1: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    File {
        downloads: RuntimeDownloads,
        #[serde(default)]
        executable: bool,
    },
    Directory,
    Link {
        #[cfg_attr(not(unix), allow(dead_code))]
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct RuntimeDownloads {
    raw: RuntimeDownload,
}

#[derive(Debug, Deserialize)]
struct RuntimeDownload {
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Marker {
    manifest_sha1: String,
    version: String,
}

/// Mojang's platform keys to try, most specific first. ARM machines fall back to the
/// x64 build (run through the OS's emulation layer) when no native build exists.
fn platform_candidates(os: OsName, arch: Arch) -> &'static [&'static str] {
    match (os, arch) {
        (OsName::Windows, Arch::X86_64) => &["windows-x64"],
        (OsName::Windows, Arch::X86) => &["windows-x86"],
        (OsName::Windows, Arch::Arm64) => &["windows-arm64", "windows-x64"],
        (OsName::Osx, Arch::Arm64) => &["mac-os-arm64", "mac-os"],
        (OsName::Osx, _) => &["mac-os"],
        (OsName::Linux, Arch::X86) => &["linux-i386"],
        (OsName::Linux, _) => &["linux"],
    }
}

/// Path of the Java executable inside a runtime directory. On Windows this is
/// `javaw.exe`, which does not open a console window.
pub fn java_executable(runtime_dir: &Path) -> PathBuf {
    if cfg!(target_os = "windows") {
        runtime_dir.join("bin").join("javaw.exe")
    } else if cfg!(target_os = "macos") {
        runtime_dir.join("jre.bundle/Contents/Home/bin/java")
    } else {
        runtime_dir.join("bin").join("java")
    }
}

/// Ensures Mojang's runtime `component` is installed and returns its Java executable.
pub async fn ensure_runtime(
    paths: &Paths,
    http: &Http,
    component: &str,
    concurrency: usize,
    progress: Arc<Progress>,
) -> Result<PathBuf> {
    let dir = paths.java_dir().join(component);
    let marker_path = dir.join(MARKER_FILE);
    let exe = java_executable(&dir);
    let index: RuntimeIndex = match http.get_json(RUNTIME_INDEX_URL).await {
        Ok(index) => index,
        // Offline: an already-installed runtime is good enough.
        Err(e) if exe.exists() && marker_path.exists() => {
            tracing::warn!(
                "using installed Java runtime {component}, could not check for updates: {e}"
            );
            return Ok(exe);
        }
        Err(e) => return Err(e),
    };
    let (os, arch) = (OsName::current(), Arch::current());
    let entry = platform_candidates(os, arch)
        .iter()
        .find_map(|p| index.get(*p)?.get(component)?.first())
        .ok_or_else(|| Error::JavaUnavailable {
            component: component.to_owned(),
            platform: format!("{}-{}", os.as_str(), arch.as_str()),
        })?;

    let marker = Marker {
        manifest_sha1: entry.manifest.sha1.clone(),
        version: entry.version.name.clone(),
    };
    if crate::fsutil::read_json::<Marker>(&marker_path)?.as_ref() == Some(&marker) && exe.exists() {
        return Ok(exe);
    }

    progress.set_stage(format!("Installing Java {}", entry.version.name), 0);
    let manifest: RuntimeManifest = http.get_json(&entry.manifest.url).await?;
    let mut tasks = Vec::new();
    let mut links = Vec::new();
    for (name, file) in &manifest.files {
        let path = safe_join(&dir, name)?;
        match file {
            RuntimeFile::Directory => std::fs::create_dir_all(&path).at(&path)?,
            RuntimeFile::File {
                downloads,
                executable,
            } => tasks.push(DownloadTask {
                url: downloads.raw.url.clone(),
                path,
                sha1: Some(downloads.raw.sha1.clone()),
                size: Some(downloads.raw.size),
                executable: *executable,
            }),
            RuntimeFile::Link { target } => links.push((path, target)),
        }
    }
    http.download_all(tasks, concurrency, Verify::Hash, progress)
        .await?;

    #[cfg(unix)]
    for (path, target) in links {
        if path.symlink_metadata().is_err() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            std::os::unix::fs::symlink(target, &path).at(&path)?;
        }
    }
    #[cfg(not(unix))]
    drop(links);

    if !exe.exists() {
        return Err(Error::Invalid(format!(
            "Java runtime {component} installed but {} is missing",
            exe.display()
        )));
    }
    crate::fsutil::write_json_atomic(&marker_path, &marker)?;
    Ok(exe)
}

/// Joins a manifest-supplied relative path, rejecting anything that escapes `base`.
pub(crate) fn safe_join(base: &Path, relative: &str) -> Result<PathBuf> {
    use std::path::Component;
    let rel = Path::new(relative);
    if rel
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(Error::Invalid(format!(
            "unsafe path in manifest: {relative}"
        )));
    }
    Ok(base.join(rel))
}

/// Information about a Java installation, from `java -XshowSettings:properties -version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInfo {
    pub version: String,
    pub major: u32,
    pub arch: String,
}

/// Runs the given Java binary to find its version. Used to validate custom Java paths.
pub async fn probe(java: &Path) -> Result<JavaInfo> {
    let mut cmd = tokio::process::Command::new(java);
    cmd.args(["-XshowSettings:properties", "-version"])
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    crate::launch::hide_console(&mut cmd);
    let out = tokio::time::timeout(std::time::Duration::from_secs(15), cmd.output())
        .await
        .map_err(|_| Error::Invalid(format!("{} did not respond", java.display())))?
        .at(java)?;
    let text = String::from_utf8_lossy(&out.stderr);
    parse_properties(&text).ok_or_else(|| {
        Error::Invalid(format!(
            "{} does not look like a Java runtime",
            java.display()
        ))
    })
}

fn parse_properties(text: &str) -> Option<JavaInfo> {
    let prop = |key: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(key)?.trim_start().strip_prefix('='))
            .map(|v| v.trim().to_owned())
    };
    let version = prop("java.version")?;
    Some(JavaInfo {
        major: major_from_version(&version)?,
        arch: prop("os.arch").unwrap_or_default(),
        version,
    })
}

/// `1.8.0_412` -> 8, `17.0.2` -> 17, `21` -> 21.
pub fn major_from_version(version: &str) -> Option<u32> {
    let mut parts = version.split(|c: char| !c.is_ascii_digit());
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_versions() {
        assert_eq!(major_from_version("1.8.0_412"), Some(8));
        assert_eq!(major_from_version("17.0.2"), Some(17));
        assert_eq!(major_from_version("21"), Some(21));
        assert_eq!(major_from_version("25-ea"), Some(25));
        assert_eq!(major_from_version("x"), None);
    }

    #[test]
    fn parses_property_dump() {
        let text = "Property settings:\n    java.version = 21.0.2\n    os.arch = amd64\nopenjdk version \"21.0.2\"";
        assert_eq!(
            parse_properties(text),
            Some(JavaInfo {
                version: "21.0.2".into(),
                major: 21,
                arch: "amd64".into()
            })
        );
    }

    #[test]
    fn rejects_path_traversal() {
        let base = Path::new("/base");
        assert!(safe_join(base, "bin/java").is_ok());
        assert!(safe_join(base, "../evil").is_err());
        assert!(safe_join(base, "/etc/passwd").is_err());
    }

    #[test]
    fn arm_falls_back_to_x64() {
        assert_eq!(
            platform_candidates(OsName::Windows, Arch::Arm64),
            &["windows-arm64", "windows-x64"]
        );
    }
}
